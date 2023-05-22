mod db;
mod error;
mod ingest;
mod mgmt;
mod proxy;

use std::result::Result as StdResult;

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{routing::post, Router};
use db::ensure_schema;
use hyper::HeaderMap;
use sqlx::sqlite::SqlitePool;
use tracing::Level;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::db::{insert_request, mark_complete};
use crate::error::AppError;
use crate::ingest::HttpRequest;
use crate::proxy::{proxy, Client};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "soldr=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (ingest, mgmt) = app().await?;

    let addr = "0.0.0.0:3443";
    let addr = addr.parse()?;
    tokio::spawn(async move {
        tracing::info!("management API listening on {}", addr);
        axum::Server::bind(&addr)
            .serve(mgmt.into_make_service())
            .await
            .unwrap();
    });

    let addr = "0.0.0.0:3000";
    tracing::info!("ingest listening on {}", addr);
    axum::Server::bind(&addr.parse()?)
        .serve(ingest.into_make_service())
        .await?;

    Ok(())
}

async fn app() -> Result<(Router, Router)> {
    // TODO write to actual database, such as sqlite:soldr.db
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    ensure_schema(&pool).await?;

    let mgmt_router = mgmt::router(pool.clone());

    let client = Client::new();
    let router = Router::new()
        .route("/", post(handler))
        .layer(Extension(pool))
        .with_state(client);

    Ok((router, mgmt_router))
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

        let (ingest, mgmt) = app().await.unwrap();

        // `Router` implements `tower::Service<Request<Body>>` so we can
        // call it like any tower service, no need to run an HTTP server.
        let response = ingest
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

        let response = mgmt
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/requests")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();

        let reqs: Vec<db::Request> = serde_json::from_slice(&body).unwrap();
        assert!(reqs[0].complete);
    }

    #[tokio::test]
    async fn mgmt_hello_world() {
        let (_, mgmt) = app().await.unwrap();

        let response = mgmt
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/requests")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        assert_eq!(&body[..], b"[]");
    }
}
