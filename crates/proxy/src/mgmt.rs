use std::result::Result as StdResult;

use anyhow::Result;
use axum::extract::{Extension, Json};
use axum::http::StatusCode;
use axum::response::IntoResponse;
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
        .route("/queue", post(add_request_to_queue))
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

async fn create_origin(
    Extension(pool): Extension<SqlitePool>,
    Extension(origin_cache): Extension<OriginCache>,
    Json(new_origin): Json<db::NewOrigin>,
) -> StdResult<Json<db::Origin>, AppError> {
    let span = tracing::span!(Level::TRACE, "create_origin");
    let _enter = span.enter();

    tracing::debug!("request payload = {:?}", &new_origin);
    let origin = db::insert_origin(&pool, new_origin).await?;
    tracing::debug!("response = {:?}", &origin);

    update_origin_cache(&pool, &origin_cache).await?;

    Ok(Json(origin))
}

pub async fn update_origin_cache(pool: &SqlitePool, origin_cache: &OriginCache) -> Result<()> {
    let origins = db::list_origins(pool).await?;
    origin_cache.refresh(origins).unwrap();

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NewQueueRequest {
    pub req_id: i64,
}

async fn add_request_to_queue(
    Extension(pool): Extension<SqlitePool>,
    Json(payload): Json<NewQueueRequest>,
) -> StdResult<impl IntoResponse, AppError> {
    let span = tracing::span!(Level::TRACE, "add_request_to_queue");
    let _enter = span.enter();

    db::add_request_to_queue(&pool, payload.req_id).await?;

    Ok(StatusCode::ACCEPTED)
}
