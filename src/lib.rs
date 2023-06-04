pub mod db;
pub mod error;
pub mod ingest;
pub mod mgmt;
pub mod proxy;

use std::result::Result as StdResult;

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{routing::post, Router};
use hyper::HeaderMap;
use sqlx::sqlite::SqlitePool;
use tracing::Level;

use crate::db::{ensure_schema, insert_request, mark_complete};
use crate::error::AppError;
use crate::ingest::HttpRequest;
use crate::proxy::{proxy, Client};

pub async fn app() -> Result<(Router, Router)> {
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

    tracing::debug!("{:?}", &r);

    let queued_req = insert_request(&pool, r).await?;
    let req_id = queued_req.id;

    proxy(&pool, &client, queued_req).await?;

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
