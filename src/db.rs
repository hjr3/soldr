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

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Request {
    pub id: i64,
    pub method: String,
    pub uri: String,
    pub headers: String,
    pub body: Option<Vec<u8>>,
    pub complete: bool,
}

pub async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
    let mut conn = pool.acquire().await?;

    tracing::trace!("creating requests table");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS requests (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             method TEXT NOT NULL,
             uri TEXT NOT NULL,
             headers TEXT NOT NULL,
             body TEXT,
             complete INT(1) DEFAULT 0
        )",
    )
    .await?;

    tracing::trace!("creating origins table");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS origins (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             domain TEXT NOT NULL,
             origin_uri TEXT NOT NULL
        )",
    )
    .await?;

    // TODO create table track attempts

    Ok(())
}

pub async fn insert_request(pool: &SqlitePool, req: HttpRequest) -> Result<QueuedRequest> {
    let mut conn = pool.acquire().await?;

    let headers_json = serde_json::to_string(&req.headers)?;

    let id = sqlx::query("INSERT INTO requests (method, uri, headers, body) VALUES (?, ?, ?, ?)")
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
    let mut conn = pool.acquire().await?;

    sqlx::query("UPDATE requests SET complete = 1 WHERE id = ?")
        .bind(req_id)
        .execute(&mut conn)
        .await?;

    Ok(())
}

pub async fn list_requests(pool: &SqlitePool) -> Result<Vec<Request>> {
    let mut conn = pool.acquire().await?;

    let requests = sqlx::query_as::<_, Request>("SELECT * FROM requests LIMIT 10;")
        .fetch_all(&mut conn)
        .await?;

    Ok(requests)
}

// TODO consider a stronger type for origin_uri
// TOOD change the types so we can avoid String -> str -> String
pub async fn insert_origin(pool: &SqlitePool, domain: &str, origin_uri: &str) -> Result<Origin> {
    let mut conn = pool.acquire().await?;

    let id = sqlx::query("INSERT INTO origins (domain, origin_uri) VALUES (?, ?)")
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
    let mut conn = pool.acquire().await?;

    let origins = sqlx::query_as::<_, Origin>("SELECT * FROM origins;")
        .fetch_all(&mut conn)
        .await?;

    Ok(origins)
}
