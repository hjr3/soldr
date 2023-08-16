use std::result::Result as StdResult;

use axum::extract::{Extension, Json};
use axum::{
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tracing::Level;

use crate::cache::OriginCache;
use crate::db;
use crate::error::AppError;

pub fn router(pool: SqlitePool, origin_cache: OriginCache) -> Router {
    Router::new()
        .route("/origins", post(create_origin))
        .route("/requests", get(list_requests))
        .route("/attempts", get(list_attempts))
        .layer(Extension(pool))
        .layer(Extension(origin_cache))
}

async fn list_requests(
    Extension(pool): Extension<SqlitePool>,
) -> StdResult<Json<Vec<db::Request>>, AppError> {
    let span = tracing::span!(Level::TRACE, "list_requests");
    let _enter = span.enter();

    let reqs = db::list_requests(&pool).await?;
    tracing::debug!("response = {:?}", &reqs);

    Ok(Json(reqs))
}

async fn list_attempts(
    Extension(pool): Extension<SqlitePool>,
) -> StdResult<Json<Vec<db::Attempt>>, AppError> {
    let span = tracing::span!(Level::TRACE, "list_attempts");
    let _enter = span.enter();

    let attempts = db::list_attempts(&pool).await?;
    tracing::debug!("response = {:?}", &attempts);

    Ok(Json(attempts))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateOrigin {
    pub domain: String,
    pub origin_uri: String,
    pub timeout: Option<u32>,
}

async fn create_origin(
    Extension(pool): Extension<SqlitePool>,
    Extension(origin_cache): Extension<OriginCache>,
    Json(payload): Json<CreateOrigin>,
) -> StdResult<Json<db::Origin>, AppError> {
    let span = tracing::span!(Level::TRACE, "create_origin");
    let _enter = span.enter();

    tracing::debug!("request payload = {:?}", &payload);
    let origin = db::insert_origin(
        &pool,
        &payload.domain,
        &payload.origin_uri,
        payload.timeout.unwrap_or(5000),
    )
    .await?;
    tracing::debug!("response = {:?}", &origin);

    let origins = db::list_origins(&pool).await?;
    origin_cache.refresh(origins).unwrap();

    Ok(Json(origin))
}
