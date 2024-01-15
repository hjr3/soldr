use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow, Clone)]
pub struct Origin {
    pub id: i64,
    pub domain: String,
    pub origin_uri: String,
    pub timeout: u32,
    pub alert_threshold: Option<u16>,
    pub alert_email: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_tls: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, Eq, PartialEq)]
#[repr(i8)]
pub enum RequestState {
    // request has been created and is ready to be processed
    Received = 0,
    // request has been created and is ready to be processed
    Created = 1,
    // request to origin is waiting to be processed
    Enqueued = 2,
    // request to origin is in progress
    Active = 3,
    // request completed successfully
    Completed = 4,
    // request to origin had a known error and can be retried
    Failed = 5,
    // unknown error
    Panic = 6,
    // request to origin timed out
    Timeout = 7,
    // no origin was found
    Skipped = 8,
}

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Request {
    pub id: i64,
    pub method: String,
    pub uri: String,
    pub headers: String,
    pub body: Option<Vec<u8>>,
    pub state: RequestState,
    pub created_at: i64,
    pub retry_ms_at: i64,
}

#[derive(Debug)]
pub struct QueuedRequest {
    pub id: i64,
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub state: RequestState,
}

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Attempt {
    pub id: i64,
    pub request_id: i64,
    pub response_status: i64,
    pub response_body: Vec<u8>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct NewOrigin {
    pub domain: String,
    pub origin_uri: String,
    pub timeout: u32,
    #[serde(default)]
    pub alert_threshold: Option<u16>,
    #[serde(default)]
    pub alert_email: Option<String>,
    #[serde(default)]
    pub smtp_host: Option<String>,
    #[serde(default)]
    pub smtp_username: Option<String>,
    #[serde(default)]
    pub smtp_password: Option<String>,
    #[serde(default)]
    pub smtp_port: Option<u16>,
    #[serde(default)]
    pub smtp_tls: bool,
}
