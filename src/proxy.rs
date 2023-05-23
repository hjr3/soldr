use anyhow::{anyhow, Result};
use axum::http::Request;
use axum::http::Uri;
use hyper::client::HttpConnector;
use hyper::Body;
use sqlx::SqlitePool;

use crate::db::list_origins;
use crate::db::QueuedRequest;

pub type Client = hyper::client::Client<HttpConnector, Body>;

pub async fn proxy(pool: &SqlitePool, client: &Client, mut req: QueuedRequest) -> Result<()> {
    let uri = map_origin(pool, &req).await?;

    if uri.is_none() {
        // no origin found, so mark as complete and move on
        return Ok(());
    }

    let uri = uri.unwrap();

    let body = req.body.take();
    let body: hyper::Body = body.map_or(hyper::Body::empty(), |b| b.into());

    let new_req = Request::builder()
        .method(req.method.as_str())
        .uri(&uri)
        .body(body)?;

    let response = client.request(new_req).await?;
    tracing::debug!(
        "Proxy {:?} --> {} with {} response",
        &req,
        &uri,
        response.status()
    );

    Ok(())
}

async fn map_origin(pool: &SqlitePool, req: &QueuedRequest) -> Result<Option<Uri>> {
    let uri = Uri::try_from(&req.uri)?;
    let parts = uri.into_parts();

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
    let parts = uri.into_parts();
    let scheme = parts.scheme.ok_or(anyhow!("Missing scheme: {}", req.uri))?;
    let authority = parts
        .authority
        .ok_or(anyhow!("Missing authority: {}", req.uri))?;

    let path_and_query = parts
        .path_and_query
        .ok_or(anyhow!("Missing path and query: {}", req.uri))?;

    let uri = Uri::builder()
        .scheme(scheme)
        .authority(authority)
        .path_and_query(path_and_query)
        .build()?;

    Ok(Some(uri))
}
