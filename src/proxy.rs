use anyhow::{anyhow, Result};
use axum::http::Request;
use axum::http::Uri;
use hyper::client::HttpConnector;
use hyper::Body;

use crate::db::QueuedRequest;

pub type Client = hyper::client::Client<HttpConnector, Body>;

pub async fn proxy(client: &Client, mut req: QueuedRequest) -> Result<()> {
    let uri = map_origin(&req)?;

    if uri.is_none() {
        // no origin found, so mark as complete and move on
        return Ok(());
    }

    let uri = uri.unwrap();

    let body = req.body.take();
    let body: hyper::Body = body.map_or(hyper::Body::empty(), |b| b.into());

    let new_req = Request::builder()
        .method(req.method.as_str())
        .uri(uri)
        .body(body)?;

    client.request(new_req).await?;

    Ok(())
}

fn map_origin(req: &QueuedRequest) -> Result<Option<Uri>> {
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

    // FIXME this should be a database lookup
    let domain = match authority.as_str() {
        "localhost:3000" => "http://127.0.0.1:3001",
        _ => return Ok(None),
    };

    let uri = Uri::try_from(domain)?;
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
