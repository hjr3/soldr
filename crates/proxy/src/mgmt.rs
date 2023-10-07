use std::result::Result as StdResult;

use anyhow::Result;
use axum::extract::{Extension, Json, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tracing::Level;

use shared_types::{NewOrigin, Origin};

use crate::cache::OriginCache;
use crate::db;
use crate::error::AppError;

pub fn router(pool: SqlitePool, origin_cache: OriginCache) -> Router {
    Router::new()
        .route("/origins", get(list_origins))
        .route("/origins", post(create_origin))
        .route("/origins/:id", get(get_origin))
        .route("/origins/:id", put(update_origin))
        .route("/origins/:id", delete(delete_origin))
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

async fn list_origins(
    Extension(pool): Extension<SqlitePool>,
) -> StdResult<Json<Vec<Origin>>, AppError> {
    let span = tracing::span!(Level::TRACE, "list_origins");
    let _enter = span.enter();

    let origins = db::list_origins(&pool).await?;
    tracing::debug!("response = {:?}", &origins);

    Ok(Json(origins))
}

async fn create_origin(
    Extension(pool): Extension<SqlitePool>,
    Extension(origin_cache): Extension<OriginCache>,
    Json(new_origin): Json<NewOrigin>,
) -> StdResult<Json<Origin>, AppError> {
    let span = tracing::span!(Level::TRACE, "create_origin");
    let _enter = span.enter();

    tracing::debug!("request payload = {:?}", &new_origin);
    let origin = db::insert_origin(&pool, new_origin).await?;
    tracing::debug!("response = {:?}", &origin);

    update_origin_cache(&pool, &origin_cache).await?;

    Ok(Json(origin))
}

async fn update_origin(
    Extension(pool): Extension<SqlitePool>,
    Extension(origin_cache): Extension<OriginCache>,
    Path(id): Path<i64>,
    Json(new_origin): Json<NewOrigin>,
) -> StdResult<Json<Origin>, AppError> {
    let span = tracing::span!(Level::TRACE, "update_origin");
    let _enter = span.enter();

    tracing::debug!("request payload = {:?}", &new_origin);
    let origin = db::update_origin(&pool, id, new_origin).await?;
    tracing::debug!("response = {:?}", &origin);

    update_origin_cache(&pool, &origin_cache).await?;

    Ok(Json(origin))
}

async fn get_origin(
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<i64>,
) -> StdResult<Json<Origin>, AppError> {
    let span = tracing::span!(Level::TRACE, "get_origin");
    let _enter = span.enter();

    tracing::debug!("origin id = {}", id);
    let origin = db::get_origin(&pool, id).await?;
    tracing::debug!("response = {:?}", &origin);

    Ok(Json(origin))
}

async fn delete_origin(
    Extension(pool): Extension<SqlitePool>,
    Extension(origin_cache): Extension<OriginCache>,
    Path(id): Path<i64>,
) -> StdResult<impl IntoResponse, AppError> {
    let span = tracing::span!(Level::TRACE, "delete_origin");
    let _enter = span.enter();

    tracing::debug!("origin id = {}", id);
    let found = db::delete_origin(&pool, id).await?;
    tracing::debug!("response = {:?}", found);

    update_origin_cache(&pool, &origin_cache).await?;

    Ok(StatusCode::ACCEPTED)
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
