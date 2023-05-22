use std::result::Result as StdResult;

use axum::extract::Extension;
use axum::Json;
use axum::{routing::get, Router};
use sqlx::sqlite::SqlitePool;
use tracing::Level;

use crate::db;
use crate::error::AppError;

pub fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/requests", get(list_requests))
        .layer(Extension(pool))
}

async fn list_requests(
    Extension(pool): Extension<SqlitePool>,
) -> StdResult<Json<Vec<db::Request>>, AppError> {
    let span = tracing::span!(Level::TRACE, "list_requests");
    let _enter = span.enter();

    let reqs = db::list_requests(&pool).await?;

    Ok(Json(reqs))
}
