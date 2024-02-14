use anyhow::{anyhow, Context, Result};
use hyper::client::HttpConnector;
use hyper::{Body, Request, Response, Uri};
use sqlx::SqlitePool;
use tokio::time::{timeout, Duration};

use crate::alert::send_alert;
use crate::cache::OriginCache;
use crate::db::attempts_reached_threshold;
use crate::db::insert_attempt;
use crate::db::insert_request;
use crate::db::retry_request;
use crate::db::update_request_state;
use crate::db::QueuedRequest;
use crate::db::RequestState;
use crate::origin::Origin;
use crate::request::State;
use crate::response::transform_response;
use crate::response::HttpResponse;

pub type Client = hyper::client::Client<HttpConnector, Body>;

pub async fn proxy(
    pool: &SqlitePool,
    origin_cache: &OriginCache,
    client: &Client,
    initial_state: State,
) -> Result<()> {
    let p = Proxy {
        pool,
        origin_cache,
        client,
    };

    let mut state = initial_state;
    loop {
        match p.next(state).await? {
            Some(next_state) => {
                state = next_state;
            }
            None => {
                break;
            }
        }
    }

    Ok(())
}

pub struct Proxy<'a> {
    pub pool: &'a SqlitePool,
    pub origin_cache: &'a OriginCache,
    pub client: &'a Client,
}

impl<'a> Proxy<'a> {
    pub async fn next(&self, state: State) -> Result<Option<State>> {
        match state {
            State::Received(req) => {
                let queued_req = insert_request(self.pool, req, RequestState::Received)
                    .await
                    // TODO log in a format that we can recover the dropped request
                    .context("Error inserting request")?;

                Ok(Some(State::Created(queued_req)))
            }
            State::Created(req) => Ok(Some(State::Enqueued(req))),
            State::Enqueued(req) => {
                update_request_state(self.pool, req.id, RequestState::Enqueued)
                    .await
                    .with_context(|| {
                        format!(
                            "Error updating request state to {:?} for {:?}",
                            RequestState::Enqueued,
                            &req,
                        )
                    })?;

                Ok(Some(State::UnmappedOrigin(req)))
            }
            State::UnmappedOrigin(req) => match map_origin(self.origin_cache, &req)
                .await
                .with_context(|| format!("Error mapping origin for {:?}", &req))?
            {
                Some(origin) => Ok(Some(State::Active(req, origin))),
                None => Ok(Some(State::Skipped(req.id))),
            },
            State::Active(req, origin) => {
                let req_id = req.id;
                match send_request(&origin, self.client, req).await {
                    Ok(response) => {
                        let response = transform_response(response).await;
                        let is_success = response.status().is_success();
                        let is_timeout = response.status() == 504;

                        record_attempt(self.pool, req_id, &response)
                            .await
                            .with_context(
                                || format!("Error recording attempt for {:?}", req_id,),
                            )?;

                        if is_success {
                            Ok(Some(State::Completed(req_id)))
                        } else if is_timeout {
                            Ok(Some(State::Timeout(req_id, origin)))
                        } else {
                            Ok(Some(State::Failed(req_id, origin)))
                        }
                    }
                    Err(error) => {
                        // FIXME: we need to separate fatal errors from recoverable ones
                        // it is expected that a request upstream will fail sometimes
                        tracing::error!("Error proxying {:?}: {:?}", req_id, error);
                        Ok(Some(State::Panic(req_id, origin)))
                    }
                }
            }
            State::Completed(req_id) => {
                update_request_state(self.pool, req_id, RequestState::Completed)
                    .await
                    .with_context(|| {
                        format!(
                            "Error updating state to {:?} for {:?}",
                            RequestState::Completed,
                            req_id,
                        )
                    })?;

                Ok(None)
            }
            State::Failed(req_id, origin) => {
                if let Err(error) = retry_request(self.pool, req_id, RequestState::Failed).await {
                    // FIXME: we need to separate fatal errors from recoverable ones
                    tracing::error!(
                        "Error calling retry_request for state {:?} on req_id {:?}: {:?}",
                        RequestState::Failed,
                        req_id,
                        error
                    );
                }

                if let Some(threshold) = origin.alert_threshold {
                    match attempts_reached_threshold(self.pool, req_id, threshold).await {
                        Ok(true) => {
                            send_alert(&origin, req_id).await;
                        }
                        Ok(false) => { /* do nothing */ }
                        Err(error) => {
                            tracing::error!(
                                "Error calling attempts_reached_threshold for req_id {:?}: {:?}",
                                req_id,
                                error
                            );

                            // err on the side of caution
                            send_alert(&origin, req_id).await;
                        }
                    }
                }

                Ok(None)
            }
            State::Panic(req_id, origin) => {
                // Why are we retrying on unknown errors?
                if let Err(error) = retry_request(self.pool, req_id, RequestState::Panic).await {
                    // FIXME: we need to separate fatal errors from recoverable ones
                    tracing::error!(
                        "Error calling retry_request for state {:?} on req_id {:?}: {:?}",
                        RequestState::Panic,
                        req_id,
                        error
                    );
                }

                send_alert(&origin, req_id).await;

                Ok(None)
            }
            State::Timeout(req_id, origin) => {
                match retry_request(self.pool, req_id, RequestState::Timeout).await {
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(
                            "Error calling retry_request for state {:?} on req_id {:?}: {:?}",
                            RequestState::Timeout,
                            req_id,
                            error
                        );
                    }
                }

                if let Some(threshold) = origin.alert_threshold {
                    match attempts_reached_threshold(self.pool, req_id, threshold).await {
                        Ok(true) => {
                            send_alert(&origin, req_id).await;
                        }
                        Ok(false) => { /* do nothing */ }
                        Err(error) => {
                            tracing::error!(
                                "Error calling attempts_reached_threshold for req_id {:?}: {:?}",
                                req_id,
                                error
                            );

                            // err on the side of caution
                            send_alert(&origin, req_id).await;
                        }
                    }
                }
                Ok(None)
            }
            State::Skipped(req_id) => {
                if let Err(error) =
                    update_request_state(self.pool, req_id, RequestState::Skipped).await
                {
                    tracing::error!(
                        "Error updating state to {:?} for {:?}: {:?}",
                        RequestState::Skipped,
                        req_id,
                        error
                    );
                }

                Ok(None)
            }
        }
    }
}

async fn send_request(
    origin: &Origin,
    client: &Client,
    mut req: QueuedRequest,
) -> Result<Response<Body>> {
    let parts = Uri::try_from(&req.uri)?.into_parts();

    let path_and_query = parts
        .path_and_query
        .ok_or(anyhow!("Missing path and query: {}", req.uri))?;

    let origin_parts = origin.uri.clone().into_parts();
    let scheme = origin_parts.scheme.ok_or(anyhow!("Missing scheme"))?;
    let authority = origin_parts.authority.ok_or(anyhow!("Missing authority"))?;

    let uri = Uri::builder()
        .scheme(scheme)
        .authority(authority)
        .path_and_query(path_and_query)
        .build()?;

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

    tracing::debug!("{} --> {}", &authority, &matched_origin.origin_uri);

    let origin = Origin {
        uri: matched_origin.origin_uri.try_into()?,
        timeout: matched_origin.timeout,
        alert_threshold: matched_origin.alert_threshold,
        alert_email: matched_origin.alert_email,
        smtp_host: matched_origin.smtp_host,
        smtp_port: matched_origin.smtp_port,
        smtp_username: matched_origin.smtp_username,
        smtp_password: matched_origin.smtp_password,
        smtp_tls: matched_origin.smtp_tls,
    };

    Ok(Some(origin))
}

async fn record_attempt(
    pool: &SqlitePool,
    request_id: i64,
    response: &HttpResponse,
) -> Result<i64> {
    let body: Option<&[u8]> = match response.body() {
        Some(inner_vec) => Some(inner_vec.as_slice()),
        None => None,
    };

    let attempt_id = insert_attempt(pool, request_id, response.status().as_u16(), body).await?;

    tracing::debug!("Recorded attempt {} for request {}", attempt_id, request_id);

    Ok(attempt_id)
}
