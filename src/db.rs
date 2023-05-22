use anyhow::Result;
use sqlx::sqlite::SqlitePool;
use sqlx::Executor;

use crate::ingest::HttpRequest;

#[derive(Debug, sqlx::FromRow)]
pub struct QueuedRequest {
    pub id: i64,
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

pub async fn insert_request(pool: &SqlitePool, req: HttpRequest) -> Result<QueuedRequest> {
    let mut conn = pool.acquire().await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS requests (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             method TEXT,
             uri TEXT,
             headers TEXT,
             body TEXT,
             complete INT(1) DEFAULT 0
        )",
    )
    .await?;

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
