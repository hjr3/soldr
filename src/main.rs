use std::result::Result as StdResult;

use anyhow::anyhow;
use anyhow::Result;
use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
use axum::{routing::post, Router};
use hyper::client::HttpConnector;
use hyper::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use sqlx::Executor;
use tracing::Level;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

type Client = hyper::client::Client<HttpConnector, hyper::Body>;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

#[derive(Debug, sqlx::FromRow)]
struct QueuedRequest {
    id: i64,
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "soldr=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = app().await?;

    let addr = "0.0.0.0:3000";
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr.parse()?)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn app() -> Result<Router> {
    let client = Client::new();

    // sqlite:soldr.db
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let router = Router::new()
        .route("/", post(handler))
        .layer(Extension(pool))
        .with_state(client);

    Ok(router)
}

async fn handler(
    State(client): State<Client>,
    Extension(pool): Extension<SqlitePool>,
    req: Request<Body>,
) -> StdResult<impl IntoResponse, AppError> {
    let span = tracing::span!(Level::TRACE, "ingest");
    let _enter = span.enter();

    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let headers = transform_headers(req.headers());
    let body = req.into_body();
    let body = hyper::body::to_bytes(body).await?;
    let r = HttpRequest {
        method,
        uri,
        headers,
        body: Some(body.to_vec()),
    };

    tracing::trace!("{:?}", &r);

    let queued_req = insert_request(&pool, r).await?;
    let req_id = queued_req.id;

    proxy(&client, queued_req).await?;

    mark_complete(&pool, req_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn proxy(client: &Client, mut req: QueuedRequest) -> Result<()> {
    let uri = map_origin(&req)?;

    if uri.is_none() {
        // no origin found, so mark as complete and move on
        return Ok(());
    }

    let uri = uri.unwrap();

    let body = req.body.take();
    let body: hyper::Body = body.map_or(hyper::Body::empty(), |b| b.into());

    let new_req = Request::builder()
        .method(req.method.as_str())
        .uri(uri)
        .body(body)?;

    client.request(new_req).await?;

    Ok(())
}

fn map_origin(req: &QueuedRequest) -> Result<Option<Uri>> {
    let uri = Uri::try_from(&req.uri)?;
    let parts = uri.into_parts();

    let authority = if parts.authority.is_some() {
        parts.authority.unwrap()
    } else {
        req.headers
            .iter()
            .find(|header| header.0 == "host")
            .ok_or(anyhow!("Failed to find host header {:?}", req))
            .map(|h| {
                h.1.parse().map_err(|e| {
                    anyhow!(
                        "Failed to parse authority from host header: {} {}",
                        e,
                        req.uri
                    )
                })
            })??
    };

    // FIXME this should be a database lookup
    let domain = match authority.as_str() {
        "localhost:3000" => "http://127.0.0.1:3001",
        _ => return Ok(None),
    };

    let uri = Uri::try_from(domain)?;
    let parts = uri.into_parts();
    let scheme = parts.scheme.ok_or(anyhow!("Missing scheme: {}", req.uri))?;
    let authority = parts
        .authority
        .ok_or(anyhow!("Missing authority: {}", req.uri))?;

    let path_and_query = parts
        .path_and_query
        .ok_or(anyhow!("Missing path and query: {}", req.uri))?;

    let uri = Uri::builder()
        .scheme(scheme)
        .authority(authority)
        .path_and_query(path_and_query)
        .build()?;

    Ok(Some(uri))
}

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Error: {}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected error!".to_string(),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

fn transform_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(key, value)| {
            let key_str = key.as_str().to_string();
            let value_str = match value.to_str() {
                Ok(value) => value.to_string(),
                Err(_) => String::new(), // TODO Handle invalid header values
            };
            (key_str, value_str)
        })
        .collect()
}

async fn insert_request(pool: &SqlitePool, req: HttpRequest) -> Result<QueuedRequest> {
    let mut conn = pool.acquire().await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS requests (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             method TEXT,
             uri TEXT,
             headers TEXT,
             body TEXT,
             complete INT(1) DEFAULT 0
        )",
    )
    .await?;

    let headers_json = serde_json::to_string(&req.headers)?;

    let id = sqlx::query("INSERT INTO requests (method, uri, headers, body) VALUES (?, ?, ?, ?)")
        .bind(&req.method)
        .bind(&req.uri)
        .bind(headers_json)
        .bind(&req.body)
        .execute(&mut conn)
        .await
        .map_err(|err| {
            tracing::error!("Failed to save request. {:?}", &req);
            err
        })?
        .last_insert_rowid();

    let r = QueuedRequest {
        id,
        method: req.method,
        uri: req.uri,
        headers: req.headers,
        body: req.body,
    };

    Ok(r)
}

async fn mark_complete(pool: &SqlitePool, req_id: i64) -> Result<()> {
    let mut conn = pool.acquire().await?;

    sqlx::query("UPDATE requests SET complete = 1 WHERE id = ?")
        .bind(req_id)
        .execute(&mut conn)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{SocketAddr, TcpListener};

    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt; // for `oneshot` and `ready`

    #[tokio::test]
    async fn ingest_save_and_proxy() {
        let listener = TcpListener::bind("0.0.0.0:3001".parse::<SocketAddr>().unwrap()).unwrap();
        let client_app = Router::new().route("/", get(|| async { "Hello, World!" }));

        tokio::spawn(async move {
            axum::Server::from_tcp(listener)
                .unwrap()
                .serve(client_app.into_make_service())
                .await
                .unwrap();
        });

        let app = app().await.unwrap();

        // `Router` implements `tower::Service<Request<Body>>` so we can
        // call it like any tower service, no need to run an HTTP server.
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("Host", "localhost:3000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
