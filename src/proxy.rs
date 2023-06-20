use anyhow::{anyhow, Result};
use axum::http::Request;
use axum::http::Uri;
use hyper::client::HttpConnector;
use hyper::Body;
use hyper::Response;
use sqlx::SqlitePool;
use tokio::time::{timeout, Duration};

use crate::db::insert_attempt;
use crate::db::list_origins;
use crate::db::QueuedRequest;

pub type Client = hyper::client::Client<HttpConnector, Body>;

const TIMEOUT_DURATION: u64 = 5;

pub async fn proxy(pool: &SqlitePool, client: &Client, mut req: QueuedRequest) -> Result<bool> {
    let uri = map_origin(pool, &req).await?;

    if uri.is_none() {
        // no origin found, so mark as complete and move on
        return Ok(true);
    }

    let uri = uri.unwrap();

    let body = req.body.take();
    let body: hyper::Body = body.map_or(hyper::Body::empty(), |b| b.into());

    let new_req = Request::builder()
        .method(req.method.as_str())
        .uri(&uri)
        .body(body)?;

    let response = timeout(
        Duration::from_secs(TIMEOUT_DURATION),
        client.request(new_req),
    )
    .await??;

    tracing::debug!(
        "Proxy {:?} --> {} with {} response",
        &req,
        &uri,
        response.status()
    );

    let is_success = response.status().is_success();
    let attempt_id = record_attempt(pool, req.id, response).await?;
    tracing::debug!("Recorded attempt {} for request {}", attempt_id, &req.id,);

    Ok(is_success)
}

async fn map_origin(pool: &SqlitePool, req: &QueuedRequest) -> Result<Option<Uri>> {
    let uri = Uri::try_from(&req.uri)?;
    let parts = uri.into_parts();

    let path_and_query = parts
        .path_and_query
        .ok_or(anyhow!("Missing path and query: {}", req.uri))?;

    let authority = if parts.authority.is_some() {
        parts.authority.unwrap()
    } else {
        req.headers
            .iter()
            .find(|header| header.0 == "host")
            .ok_or(anyhow!("Failed to find host header {:?}", req))
            .map(|h| {
                h.1.parse().map_err(|e| {
                    anyhow!(
                        "Failed to parse authority from host header: {} {}",
                        e,
                        req.uri
                    )
                })
            })??
    };
    tracing::debug!("authority = {}", &authority);

    let origins = list_origins(pool).await?;
    tracing::debug!("origins = {:?}", &origins);
    let matching_origin = origins
        .iter()
        .find(|origin| origin.domain == authority.as_str());

    let origin_uri = match matching_origin {
        Some(origin) => &origin.origin_uri,
        None => {
            tracing::trace!("no match found");
            return Ok(None);
        }
    };

    tracing::debug!("{} --> {}", &authority, &origin_uri);

    let uri = Uri::try_from(origin_uri)?;
    let origin_parts = uri.into_parts();
    let scheme = origin_parts.scheme.ok_or(anyhow!("Missing scheme"))?;
    let authority = origin_parts.authority.ok_or(anyhow!("Missing authority"))?;

    let uri = Uri::builder()
        .scheme(scheme)
        .authority(authority)
        .path_and_query(path_and_query)
        .build()?;

    Ok(Some(uri))
}

async fn record_attempt(
    pool: &SqlitePool,
    request_id: i64,
    attempt_req: Response<Body>,
) -> Result<i64> {
    let response_status = attempt_req.status().as_u16() as i64;
    let response_body = attempt_req.into_body();
    let response_body = hyper::body::to_bytes(response_body).await?;

    let attempt_id = insert_attempt(pool, request_id, response_status, &response_body).await?;

    Ok(attempt_id)
}
