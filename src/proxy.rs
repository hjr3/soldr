use anyhow::{anyhow, Result};
use axum::http::Request;
use axum::http::Uri;
use hyper::client::HttpConnector;
use hyper::Body;
use hyper::Response;
use sqlx::SqlitePool;
use tokio::time::{timeout, Duration};

use crate::cache::OriginCache;
use crate::db::insert_attempt;
use crate::db::QueuedRequest;

pub type Client = hyper::client::Client<HttpConnector, Body>;

pub async fn proxy(
    pool: &SqlitePool,
    origin_cache: &OriginCache,
    client: &Client,
    mut req: QueuedRequest,
) -> Result<bool> {
    let maybe_origin = map_origin(origin_cache, &req).await?;

    let origin = match maybe_origin {
        Some(origin) => origin,
        None => {
            return Ok(true);
        }
    };

    let uri = origin.uri;

    let body = req.body.take();
    let body: hyper::Body = body.map_or(hyper::Body::empty(), |b| b.into());

    let new_req = Request::builder()
        .method(req.method.as_str())
        .uri(&uri)
        .body(body)?;

    let maybe_timeout = timeout(
        Duration::from_millis(origin.timeout.into()),
        client.request(new_req),
    )
    .await;

    let response = match maybe_timeout {
        Ok(response) => response?,
        Err(_) => {
            tracing::debug!("Timeout for {:?}", &req);
            Response::builder()
                .status(504)
                .body(Body::from("Timeout"))
                .expect("Failed to build timeout response")
        }
    };

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

struct Origin {
    uri: Uri,
    timeout: u32,
}

async fn map_origin(origin_cache: &OriginCache, req: &QueuedRequest) -> Result<Option<Origin>> {
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

    let matching_origin = origin_cache.get(authority.as_str());

    let matched_origin = match matching_origin {
        Some(origin) => origin,
        None => {
            tracing::trace!("no match found");
            return Ok(None);
        }
    };

    let origin_uri = matched_origin.origin_uri;

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

    let origin = Origin {
        uri,
        timeout: matched_origin.timeout,
    };

    Ok(Some(origin))
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
