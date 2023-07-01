pub mod db;
pub mod error;
pub mod ingest;
pub mod mgmt;
pub mod proxy;
pub mod queue;

use std::result::Result as StdResult;

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{routing::post, Router};
use hyper::HeaderMap;
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use tracing::Level;

use crate::db::{ensure_schema, insert_request, mark_complete, mark_error};
use crate::error::AppError;
use crate::ingest::HttpRequest;
use crate::proxy::{proxy, Client};

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub database_url: String,
    pub management_listener: String,
    pub ingest_listener: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // TODO change to a file location
            // maybe $XDG_DATA_DIR ?
            database_url: "sqlite::memory:".to_string(),
            management_listener: "0.0.0.0:3443".to_string(),
            ingest_listener: "0.0.0.0:3000".to_string(),
        }
    }
}

pub async fn app(config: &Config) -> Result<(Router, Router, SqlitePool)> {
    let pool = SqlitePool::connect(&config.database_url).await?;
    let pool2 = pool.clone();

    ensure_schema(&pool).await?;

    let mgmt_router = mgmt::router(pool.clone());

    let client = Client::new();
    let router = Router::new()
        .route("/", post(handler))
        .route("/*path", post(handler))
        .layer(Extension(pool))
        .with_state(client);

    Ok((router, mgmt_router, pool2))
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

    let is_success = proxy(&pool, &client, queued_req).await?;

    if is_success {
        mark_complete(&pool, req_id).await?;
    } else {
        mark_error(&pool, req_id).await?;
    }

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
