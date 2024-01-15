use serde::{Deserialize, Serialize};

use crate::db::QueuedRequest;
use crate::origin::Origin;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

pub enum State {
    // request has been received
    Received(HttpRequest),
    // request has been created
    Created(QueuedRequest),
    // request to origin is waiting to be processed
    Enqueued(QueuedRequest),
    // request origin has not been mapped
    UnmappedOrigin(QueuedRequest),
    // request to origin is in progress
    Active(QueuedRequest, Origin),
    // request to origin was successful
    Completed(i64),
    // request to origin had a known error and can be retried
    Failed(i64, Origin),
    // unknown error
    Panic(i64, Origin),
    // request to origin timed out
    Timeout(i64, Origin),
    // no origin was found
    Skipped(i64),
}
