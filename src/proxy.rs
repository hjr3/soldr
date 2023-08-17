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
use crate::db::insert_request;
use crate::db::update_request_state;
use crate::db::QueuedRequest;
use crate::db::RequestState;
use crate::origin::Origin;
use crate::request::State;

pub type Client = hyper::client::Client<HttpConnector, Body>;

pub async fn proxy(
    pool: &SqlitePool,
    origin_cache: &OriginCache,
    client: &Client,
    initial_state: State,
) {
    let mut state = initial_state;
    loop {
        match state {
            State::Received(req) => {
                match insert_request(pool, req, RequestState::Received).await {
                    Ok(queue_rec) => {
                        state = State::Created(queue_rec);
                    }
                    Err(error) => {
                        // TODO log in a format that we can recover the dropped request
                        tracing::error!("Error inserting request {:?}", error);
                        return;
                    }
                }
            }
            State::Created(req) => {
                state = State::Enqueued(req);
            }
            State::Enqueued(req) => {
                match update_request_state(pool, req.id, RequestState::Enqueued).await {
                    Ok(_) => {
                        // TODO make this return the value
                        state = State::UnmappedOrigin(req);
                    }
                    Err(error) => {
                        tracing::error!(
                            "Error updating state to {:?} for {:?}: {:?}",
                            RequestState::Enqueued,
                            &req,
                            error
                        );
                        return;
                    }
                }
            }
            State::UnmappedOrigin(req) => match map_origin(origin_cache, &req).await {
                Ok(Some(origin)) => {
                    state = State::Active(req, origin);
                }
                Ok(None) => {
                    state = State::Skipped(req.id);
                }
                Err(error) => {
                    tracing::error!("Error mapping origin for {:?}: {:?}", &req, error);
                    return;
                }
            },
            State::Active(req, origin) => {
                let req_id = req.id;
                match send_request(origin, client, req).await {
                    Ok(response) => {
                        let is_success = response.status().is_success();
                        let is_timeout = response.status() == 504;
                        match record_attempt(pool, req_id, response).await {
                            Ok(attempt_id) => {
                                tracing::debug!(
                                    "Recorded attempt {} for request {}",
                                    attempt_id,
                                    req_id,
                                );
                            }
                            Err(error) => {
                                tracing::error!(
                                    "Error recording attempt for {:?}: {:?}",
                                    req_id,
                                    error
                                );
                                break;
                            }
                        }

                        if is_success {
                            state = State::Completed(req_id);
                        } else if is_timeout {
                            state = State::Timeout(req_id);
                        } else {
                            state = State::Failed(req_id);
                        }
                    }
                    Err(error) => {
                        tracing::error!("Error proxying {:?}: {:?}", req_id, error);
                        state = State::Panic(req_id);
                    }
                }
            }
            State::Completed(req_id) => {
                match update_request_state(pool, req_id, RequestState::Completed).await {
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(
                            "Error updating state to {:?} for {:?}: {:?}",
                            RequestState::Completed,
                            req_id,
                            error
                        );
                    }
                }
                return;
            }
            State::Failed(req_id) => {
                match update_request_state(pool, req_id, RequestState::Failed).await {
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(
                            "Error updating state to {:?} for {:?}: {:?}",
                            RequestState::Failed,
                            req_id,
                            error
                        );
                    }
                }
                return;
            }
            State::Panic(req_id) => {
                match update_request_state(pool, req_id, RequestState::Panic).await {
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(
                            "Error updating state to {:?} for {:?}: {:?}",
                            RequestState::Panic,
                            req_id,
                            error
                        );
                    }
                }
                break;
            }
            State::Timeout(req_id) => {
                match update_request_state(pool, req_id, RequestState::Timeout).await {
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(
                            "Error updating state to {:?} for {:?}: {:?}",
                            RequestState::Timeout,
                            req_id,
                            error
                        );
                    }
                }
                break;
            }
            State::Skipped(req_id) => {
                match update_request_state(pool, req_id, RequestState::Skipped).await {
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(
                            "Error updating state to {:?} for {:?}: {:?}",
                            RequestState::Skipped,
                            req_id,
                            error
                        );
                    }
                }
                break;
            }
        }
    }
}

async fn send_request(
    origin: Origin,
    client: &Client,
    mut req: QueuedRequest,
) -> Result<Response<Body>> {
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

    Ok(response)
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
