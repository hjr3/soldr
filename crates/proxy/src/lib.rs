pub mod alert;
pub mod cache;
pub mod db;
pub mod error;
pub mod mgmt;
pub mod origin;
pub mod proxy;
pub mod queue;
pub mod request;
pub mod response;
pub mod retry;

use std::result::Result as StdResult;

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::response::IntoResponse;
use axum::{routing::any, Router};
use queue::RetryQueue;
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use tower_http::services::ServeDir;

use crate::cache::OriginCache;
use crate::db::ensure_schema;
use crate::error::AppError;
use crate::mgmt::update_origin_cache;
use crate::proxy::{proxy, Client};
use crate::request::HttpRequest;
use crate::request::State as RequestState;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Tls {
    pub enable: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub database_url: String,
    pub management_listener: String,
    pub ingest_listener: String,
    pub tls: Tls,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // TODO change to a file location
            // maybe $XDG_DATA_DIR ?
            database_url: "sqlite::memory:".to_string(),
            management_listener: "0.0.0.0:3443".to_string(),
            ingest_listener: "0.0.0.0:3000".to_string(),
            tls: Tls {
                enable: false,
                cert_path: None,
                key_path: None,
            },
        }
    }
}

pub async fn app(config: &Config) -> Result<(Router, Router, RetryQueue)> {
    let pool = SqlitePool::connect(&config.database_url).await?;
    ensure_schema(&pool).await?;

    let origin_cache = OriginCache::new();
    update_origin_cache(&pool, &origin_cache).await?;

    let mgmt_router = mgmt::router(pool.clone(), origin_cache.clone());

    let client = Client::new();
    let router = Router::new()
        .nest_service("/.well-known", ServeDir::new("public/.well-known"))
        .route("/", any(handler))
        .route("/*path", any(handler))
        .layer(Extension(pool.clone()))
        .layer(Extension(origin_cache.clone()))
        .with_state(client);

    let retry_queue = RetryQueue::new(pool, origin_cache);

    Ok((router, mgmt_router, retry_queue))
}

#[tracing::instrument(level = "trace", "ingest", skip_all)]
async fn handler(
    State(client): State<Client>,
    Extension(pool): Extension<SqlitePool>,
    Extension(origin_cache): Extension<OriginCache>,
    req: Request<Body>,
) -> StdResult<impl IntoResponse, AppError> {
    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let headers = transform_headers(req.headers());
    let body = req.into_body();
    let body = axum::body::to_bytes(body, 1_000_000).await?;
    let r = HttpRequest {
        method,
        uri,
        headers,
        body: Some(body.to_vec()),
    };

    tracing::debug!("{:?}", &r);

    proxy(&pool, &origin_cache, &client, RequestState::Received(r)).await;

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
