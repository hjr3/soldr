use std::result::Result as StdResult;

use axum::extract::{Extension, Json};
use axum::{
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tracing::Level;

use crate::db;
use crate::error::AppError;

pub fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/requests", get(list_requests))
        .route("/origin", post(create_origin))
        .layer(Extension(pool))
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

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateOrigin {
    pub domain: String,
    pub origin_uri: String,
}

async fn create_origin(
    Extension(pool): Extension<SqlitePool>,
    Json(payload): Json<CreateOrigin>,
) -> StdResult<Json<db::Origin>, AppError> {
    let span = tracing::span!(Level::TRACE, "create_origin");
    let _enter = span.enter();

    tracing::debug!("request payload = {:?}", &payload);
    let origin = db::insert_origin(&pool, &payload.domain, &payload.origin_uri).await?;
    tracing::debug!("response = {:?}", &origin);

    Ok(Json(origin))
}
