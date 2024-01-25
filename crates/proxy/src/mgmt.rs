use std::result::Result as StdResult;

use anyhow::{Context, Result};
use axum::extract::{Extension, Json, Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::Level;

use shared_types::{NewOrigin, Origin};

use crate::cache::OriginCache;
use crate::db;
use crate::error::AppError;

#[derive(Debug)]
struct Range {
    start: u32,
    end: u32,
}

#[derive(Deserialize)]
struct RangeParams {
    range: Option<String>,
}

impl RangeParams {
    fn parse_range(&self) -> StdResult<Range, AppError> {
        match self.range {
            Some(ref range) => {
                let values: Vec<&str> = range
                    .trim_matches(|p| p == '[' || p == ']')
                    .split(',')
                    .collect();

                if values.len() != 2 {
                    return Err(anyhow::anyhow!("Invalid range parameter").into());
                }

                let start = values[0].trim().parse()?;
                let end = values[1].trim().parse()?;

                if start > end {
                    return Err(anyhow::anyhow!("Start cannot be greater than end").into());
                }

                // 50 is the max per page that react admin allows
                if end - start > 50 {
                    return Err(anyhow::anyhow!("Range cannot be greater than 50").into());
                }

                // anything past 1,000 should be using a filter instead
                if start > 1000 {
                    return Err(anyhow::anyhow!("Start cannot be greater than 100").into());
                }

                if end > 1000 {
                    return Err(anyhow::anyhow!("End cannot be greater than 100").into());
                }

                Ok(Range { start, end })
            }
            None => Ok(Range { start: 0, end: 9 }),
        }
    }
}

#[derive(Debug)]
enum Order {
    Asc,
    Desc,
}

impl Order {
    fn as_str(&self) -> &str {
        match self {
            Order::Asc => "ASC",
            Order::Desc => "DESC",
        }
    }
}

#[derive(Debug)]
struct Sort {
    field: String,
    order: Order,
}

#[derive(Deserialize)]
struct SortParams {
    sort: Option<String>,
}

impl SortParams {
    fn parse_sort(&self) -> StdResult<Sort, AppError> {
        match self.sort {
            None => Ok(Sort {
                field: "id".to_string(),
                order: Order::Desc,
            }),
            Some(ref sort) => {
                let values: Vec<String> = serde_json::from_str(sort)?;
                if values.len() != 2 {
                    return Err(anyhow::anyhow!("Invalid sort format").into());
                }

                let field = values[0].clone();

                let order = if &values[1] == "ASC" {
                    Order::Asc
                } else if &values[1] == "DESC" {
                    Order::Desc
                } else {
                    return Err(anyhow::anyhow!("Invalid sort order").into());
                };

                Ok(Sort { field, order })
            }
        }
    }
}

pub fn router(pool: SqlitePool, origin_cache: OriginCache) -> Router {
    Router::new()
        .route("/origins", get(list_origins))
        .route("/origins", post(create_origin))
        .route("/origins/:id", get(get_origin))
        .route("/origins/:id", put(update_origin))
        .route("/origins/:id", delete(delete_origin))
        .route("/requests", get(list_requests))
        .route("/requests/:id", get(get_request))
        .route("/requests/:id", put(update_request))
        .route("/attempts", get(list_attempts))
        .route("/attempts/:id", get(get_attempt))
        .route("/queue", post(add_request_to_queue))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(Extension(pool))
        .layer(Extension(origin_cache))
}

#[derive(Debug, Deserialize)]
struct RequestsFilterIr {
    id: Option<Vec<i64>>,
    state: Option<Vec<i8>>,
}

#[derive(Debug, Deserialize)]
struct RequestsFilter {
    id: Option<Vec<i64>>,
    state: Option<Vec<db::RequestState>>,
}

#[derive(Debug, Deserialize)]
struct RequestsFilterQuery {
    filter: Option<String>,
}

impl RequestsFilterQuery {
    fn parse_filter(&self) -> StdResult<RequestsFilter, AppError> {
        let mut requests_filter = RequestsFilter {
            state: None,
            id: None,
        };

        match &self.filter {
            None => return Ok(requests_filter),
            Some(ref filter) => {
                let filter: RequestsFilterIr = serde_json::from_str(filter)?;

                if let Some(state) = &filter.state {
                    let state: Vec<db::RequestState> = state
                        .iter()
                        .filter_map(|&state| match state {
                            0 => Some(db::RequestState::Received),
                            1 => Some(db::RequestState::Created),
                            2 => Some(db::RequestState::Enqueued),
                            3 => Some(db::RequestState::Active),
                            4 => Some(db::RequestState::Completed),
                            5 => Some(db::RequestState::Failed),
                            6 => Some(db::RequestState::Panic),
                            7 => Some(db::RequestState::Timeout),
                            8 => Some(db::RequestState::Skipped),
                            _ => None,
                        })
                        .collect();

                    requests_filter.state = Some(state);
                }

                if let Some(id) = filter.id {
                    requests_filter.id = Some(id);
                }

                Ok(requests_filter)
            }
        }
    }
}

async fn list_requests(
    Extension(pool): Extension<SqlitePool>,
    Query(range_params): Query<RangeParams>,
    Query(sort_params): Query<SortParams>,
    Query(requests_filter): Query<RequestsFilterQuery>,
) -> StdResult<impl IntoResponse, AppError> {
    let span = tracing::span!(Level::TRACE, "list_requests");
    let _enter = span.enter();

    let range = range_params.parse_range()?;
    let sort = sort_params.parse_sort()?;
    let filter = requests_filter.parse_filter()?;

    let list_response = db::list_requests(
        &pool,
        range.start,
        range.end,
        &sort.field,
        sort.order.as_str(),
        filter.state,
        filter.id,
    )
    .await?;
    let reqs = list_response.items;
    tracing::debug!("response = {:?}", &reqs);

    let mut headers = HeaderMap::new();
    headers.insert("Content-Range", content_range(&reqs, list_response.total)?);

    Ok((headers, Json(reqs)))
}

#[derive(Debug, Deserialize)]
struct AttemptsFilter {
    request_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AttemptsFilterQuery {
    filter: String,
}

impl AttemptsFilterQuery {
    fn parse_filter(&self) -> StdResult<AttemptsFilter, AppError> {
        let filter: AttemptsFilter =
            serde_json::from_str(&self.filter).context("Failed to parse attempts filter")?;

        Ok(filter)
    }
}

async fn list_attempts(
    Extension(pool): Extension<SqlitePool>,
    Query(range_params): Query<RangeParams>,
    Query(sort_params): Query<SortParams>,
    Query(attempts_filter): Query<AttemptsFilterQuery>,
) -> StdResult<impl IntoResponse, AppError> {
    let span = tracing::span!(Level::TRACE, "list_attempts");
    let _enter = span.enter();

    let range = range_params.parse_range()?;
    let sort = sort_params.parse_sort()?;
    let filter = attempts_filter.parse_filter()?;

    let list_response = db::list_attempts(
        &pool,
        range.start,
        range.end,
        &sort.field,
        sort.order.as_str(),
        filter.request_id,
    )
    .await?;
    let attempts = list_response.items;
    tracing::debug!("response = {:?}", &attempts);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Range",
        content_range(&attempts, list_response.total)?,
    );

    Ok((headers, Json(attempts)))
}

async fn list_origins(
    Extension(pool): Extension<SqlitePool>,
    Query(range_params): Query<RangeParams>,
    Query(sort_params): Query<SortParams>,
) -> StdResult<impl IntoResponse, AppError> {
    let span = tracing::span!(Level::TRACE, "list_origins");
    let _enter = span.enter();

    let range = range_params.parse_range()?;
    let sort = sort_params.parse_sort()?;
    let list_response = db::list_origins(
        &pool,
        range.start,
        range.end,
        &sort.field,
        sort.order.as_str(),
    )
    .await?;
    let origins = list_response.items;
    tracing::debug!("response = {:?}", &origins);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Range",
        content_range(&origins, list_response.total)?,
    );

    Ok((headers, Json(origins)))
}

fn content_range<T>(list: &Vec<T>, total: i64) -> StdResult<HeaderValue, AppError> {
    let page_max = if list.is_empty() { 0 } else { list.len() - 1 };
    let range = format!("0-{}/{}", page_max, total);
    HeaderValue::from_str(&range).map_err(|e| e.into())
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
    let list_response = db::list_origins(pool, 0, 99, "id", "DESC").await?;
    origin_cache.refresh(list_response.items).unwrap();

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NewQueueRequest {
    pub req_id: i64,
}

#[derive(Debug, Serialize)]
struct NewQueueResponse {
    id: i64,
}

async fn add_request_to_queue(
    Extension(pool): Extension<SqlitePool>,
    Json(payload): Json<NewQueueRequest>,
) -> StdResult<impl IntoResponse, AppError> {
    let span = tracing::span!(Level::TRACE, "add_request_to_queue");
    let _enter = span.enter();

    db::add_request_to_queue(&pool, payload.req_id).await?;

    Ok(Json(NewQueueResponse { id: payload.req_id }))
}

async fn get_request(
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<i64>,
) -> StdResult<Json<db::Request>, AppError> {
    let span = tracing::span!(Level::TRACE, "get_request");
    let _enter = span.enter();

    tracing::debug!("request id = {}", id);
    let request = db::get_request(&pool, id).await?;
    tracing::debug!("response = {:?}", &request);

    Ok(Json(request))
}

// Requests are immutable, so create a new request from this one
async fn update_request(
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<i64>,
    Json(update_request): Json<db::UpdateRequest>,
) -> StdResult<Json<db::Request>, AppError> {
    let span = tracing::span!(Level::TRACE, "update_request");
    let _enter = span.enter();

    tracing::debug!("request payload = {:?}", &update_request);
    let request = db::update_request(&pool, id, update_request).await?;
    tracing::debug!("response = {:?}", &request);

    Ok(Json(request))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Attempt {
    pub id: i64,
    pub request_id: i64,
    pub response_status: i64,
    pub response_body: String,
    pub created_at: i64,
}

async fn get_attempt(
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<i64>,
) -> StdResult<Json<db::Attempt>, AppError> {
    let span = tracing::span!(Level::TRACE, "get_attempt");
    let _enter = span.enter();

    tracing::debug!("attempt id = {}", id);
    let attempt = db::get_attempt(&pool, id).await?;
    tracing::debug!("response = {:?}", &attempt);

    Ok(Json(attempt))
}
