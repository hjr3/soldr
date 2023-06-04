use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use sqlx::Executor;

use crate::ingest::HttpRequest;

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Origin {
    pub id: i64,
    pub domain: String,
    pub origin_uri: String,
}

#[derive(Debug)]
pub struct QueuedRequest {
    pub id: i64,
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize, sqlx::Type, Eq, PartialEq)]
#[repr(i8)]
pub enum RequestState {
    Pending = 0,
    Complete = 1,
    Error = 2,
}

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Request {
    pub id: i64,
    pub method: String,
    pub uri: String,
    pub headers: String,
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

pub async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
    let mut conn = pool.acquire().await?;

    tracing::trace!("creating requests table");
    // States
    //  0 - pending
    //  1 - complete
    //  2 - error
    conn.execute(
        "CREATE TABLE IF NOT EXISTS requests (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             method TEXT NOT NULL,
             uri TEXT NOT NULL,
             headers TEXT NOT NULL,
             body TEXT,
             state INT(1) DEFAULT 0,
             created_at INTEGER NOT NULL
        )",
    )
    .await?;

    tracing::trace!("creating origins table");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS origins (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             domain TEXT NOT NULL,
             origin_uri TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
        )",
    )
    .await?;

    tracing::trace!("creating attempts table");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS attempts (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             request_id INTEGER,
             response_status INTEGER NOT NULL,
             response_body BLOB NOT NULL,
             created_at INTEGER NOT NULL
        )",
    )
    .await?;

    Ok(())
}

pub async fn insert_request(pool: &SqlitePool, req: HttpRequest) -> Result<QueuedRequest> {
    tracing::trace!("insert_request");
    let mut conn = pool.acquire().await?;

    let headers_json = serde_json::to_string(&req.headers)?;

    let query = r#"
        INSERT INTO requests
        (
            method,
            uri,
            headers,
            body,
            created_at
        )
        VALUES (
            ?,
            ?,
            ?,
            ?,
            strftime('%s','now')
        )
    "#;

    let id = sqlx::query(query)
        .bind(&req.method)
        .bind(&req.uri)
        .bind(headers_json)
        .bind(&req.body)
        .execute(&mut conn)
        .await
        .map_err(|err| {
            tracing::error!("Failed to save request. {:?}", &req);
            err
        })?
        .last_insert_rowid();

    let r = QueuedRequest {
        id,
        method: req.method,
        uri: req.uri,
        headers: req.headers,
        body: req.body,
    };

    Ok(r)
}

pub async fn mark_complete(pool: &SqlitePool, req_id: i64) -> Result<()> {
    tracing::trace!("mark_complete");
    let mut conn = pool.acquire().await?;

    sqlx::query("UPDATE requests SET state = 1 WHERE id = ?")
        .bind(req_id)
        .execute(&mut conn)
        .await?;

    Ok(())
}

pub async fn mark_error(pool: &SqlitePool, req_id: i64) -> Result<()> {
    tracing::trace!("mark_error");
    let mut conn = pool.acquire().await?;

    sqlx::query("UPDATE requests SET state = 2 WHERE id = ?")
        .bind(req_id)
        .execute(&mut conn)
        .await?;

    Ok(())
}

pub async fn list_requests(pool: &SqlitePool) -> Result<Vec<Request>> {
    tracing::trace!("list_requests");
    let mut conn = pool.acquire().await?;

    let requests = sqlx::query_as::<_, Request>("SELECT * FROM requests LIMIT 10;")
        .fetch_all(&mut conn)
        .await?;

    Ok(requests)
}

pub async fn insert_attempt(
    pool: &SqlitePool,
    request_id: i64,
    response_status: i64,
    response_body: &[u8],
) -> Result<i64> {
    tracing::trace!("insert_attempt");
    let mut conn = pool.acquire().await?;

    let query = r#"
        INSERT INTO attempts
        (
            request_id,
            response_status,
            response_body,
            created_at
        )
        VALUES (
            ?,
            ?,
            ?,
            strftime('%s','now')
        )
    "#;

    let id = sqlx::query(query)
        .bind(request_id)
        .bind(response_status)
        .bind(&response_body)
        .execute(&mut conn)
        .await
        .map_err(|err| {
            tracing::error!(
                "Failed to save request. {} {} {:?}",
                request_id,
                response_status,
                response_body
            );
            err
        })?
        .last_insert_rowid();

    Ok(id)
}

pub async fn list_attempts(pool: &SqlitePool) -> Result<Vec<Attempt>> {
    tracing::trace!("list_attempts");
    let mut conn = pool.acquire().await?;

    let attempts = sqlx::query_as::<_, Attempt>("SELECT * FROM attempts;")
        .fetch_all(&mut conn)
        .await?;

    Ok(attempts)
}

// TODO consider a stronger type for origin_uri
// TOOD change the types so we can avoid String -> str -> String
pub async fn insert_origin(pool: &SqlitePool, domain: &str, origin_uri: &str) -> Result<Origin> {
    tracing::trace!("insert_origin");
    let mut conn = pool.acquire().await?;

    let query = r#"
        INSERT INTO origins
        (
            domain,
            origin_uri,
            created_at,
            updated_at
        )
        VALUES (
            ?,
            ?,
            strftime('%s','now'),
            strftime('%s','now')
        )
    "#;

    let id = sqlx::query(query)
        .bind(domain)
        .bind(origin_uri)
        .execute(&mut conn)
        .await?
        .last_insert_rowid();

    let origin = Origin {
        id,
        domain: domain.to_string(),
        origin_uri: origin_uri.to_string(),
    };
    Ok(origin)
}

// TODO cache list of origins and only refresh if origins are modified (created, updated, deleted)
pub async fn list_origins(pool: &SqlitePool) -> Result<Vec<Origin>> {
    tracing::trace!("list_origins");
    let mut conn = pool.acquire().await?;

    let origins = sqlx::query_as::<_, Origin>("SELECT * FROM origins;")
        .fetch_all(&mut conn)
        .await?;

    Ok(origins)
}
