use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqliteQueryResult};
use sqlx::{query_builder::QueryBuilder, FromRow, Row};

use shared_types::{NewOrigin, Origin};

use crate::request::HttpRequest;
use crate::retry::backoff;

#[derive(Debug, Deserialize, Serialize)]
pub struct GetListResponse<T> {
    pub total: i64,
    pub items: Vec<T>,
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
    pub headers: sqlx::types::Json<Vec<(String, String)>>,
    pub body: Option<Vec<u8>>,
    pub state: RequestState,
    pub created_at: i64,
    pub retry_ms_at: i64,
    pub from_request_id: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateRequest {
    pub method: String,
    pub uri: String,
    pub headers: sqlx::types::Json<Vec<(String, String)>>,
    pub body: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Attempt {
    pub id: i64,
    pub request_id: i64,
    pub response_status: i64,
    pub response_body: Vec<u8>,
    pub created_at: i64,
}

pub async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
    let mut conn = pool.acquire().await?;

    tracing::trace!("creating schema");
    sqlx::migrate!().run(&mut conn).await?;

    Ok(())
}

pub async fn insert_request(
    pool: &SqlitePool,
    req: HttpRequest,
    state: RequestState,
) -> Result<QueuedRequest> {
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
        .execute(&mut *conn)
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
        state,
    };

    Ok(r)
}

pub async fn update_request_state(
    pool: &SqlitePool,
    req_id: i64,
    state: RequestState,
) -> Result<u64> {
    tracing::trace!("updating request state to {} for {}", state as i8, req_id);
    let mut conn = pool.acquire().await?;

    let res = sqlx::query("UPDATE requests SET state = ? WHERE id = ?")
        .bind(state)
        .bind(req_id)
        .execute(&mut *conn)
        .await?;

    Ok(res.rows_affected())
}

pub async fn retry_request(pool: &SqlitePool, req_id: i64, state: RequestState) -> Result<()> {
    tracing::trace!("retry_request");
    let mut conn = pool.acquire().await?;

    let query = r#"
    SELECT COUNT(*)
    FROM attempts
    WHERE request_id = ?;
    "#;

    let retries = sqlx::query_scalar(query)
        .bind(req_id)
        .fetch_one(&mut *conn)
        .await?;

    if retries > 19 {
        tracing::warn!(
            "request {} has been retried {} times. skipping",
            req_id,
            retries
        );
        return Ok(());
    }

    let retry_ms = backoff(retries);
    let query = r#"
    UPDATE requests
    SET
        state = ?,
        retry_ms_at = strftime('%s','now') || substr(strftime('%f','now'), 4) + ?
    WHERE id = ?;
    "#;

    sqlx::query(query)
        .bind(state)
        .bind(retry_ms)
        .bind(req_id)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn attempts_reached_threshold(
    pool: &SqlitePool,
    req_id: i64,
    threshold: u16,
) -> Result<bool> {
    tracing::trace!("above_threshold");
    let mut conn = pool.acquire().await?;

    let query = r#"
    SELECT COUNT(*)
    FROM attempts
    WHERE request_id = ?;
    "#;

    let count: i64 = sqlx::query_scalar(query)
        .bind(req_id)
        .fetch_one(&mut *conn)
        .await?;

    Ok(count >= threshold.into())
}

pub async fn list_failed_requests(pool: &SqlitePool) -> Result<Vec<QueuedRequest>> {
    tracing::trace!("list_failed_requests");
    let mut conn = pool.acquire().await?;

    // FIXME - we currently tick the retry queue every second, so this effectively gives a
    // rate limit of 5 requests per second. This should probably be configurable on a per-origin
    // basis.
    let query = r#"
    SELECT *
    FROM requests
    WHERE state IN (?, ?, ?, ?)
        AND retry_ms_at <= strftime('%s','now') || substr(strftime('%f','now'), 4)
    ORDER BY retry_ms_at ASC
    LIMIT 5;
    "#;

    let requests = sqlx::query_as::<_, Request>(query)
        .bind(RequestState::Created)
        .bind(RequestState::Failed)
        .bind(RequestState::Panic)
        .bind(RequestState::Timeout)
        .fetch_all(&mut *conn)
        .await?;

    let queued_requests = requests
        .into_iter()
        .map(|request| QueuedRequest {
            id: request.id,
            method: request.method,
            uri: request.uri,
            headers: request.headers.0,
            body: request.body,
            state: request.state,
        })
        .collect();

    Ok(queued_requests)
}

pub async fn list_requests(
    pool: &SqlitePool,
    start: u32,
    end: u32,
    field: &str,
    order: &str,
    states: Option<Vec<RequestState>>,
    ids: Option<Vec<i64>>,
) -> Result<GetListResponse<Request>> {
    tracing::trace!("list_requests");
    let mut conn = pool.acquire().await?;

    let mut q = QueryBuilder::new("SELECT *, COUNT(*) OVER() AS total FROM requests");

    if let Some(ref states) = states {
        q.push(" WHERE state IN (");
        for (i, state) in states.iter().enumerate() {
            q.push_bind(*state);
            if i < states.len() - 1 {
                q.push(", ");
            }
        }
        q.push(")");
    }

    if let Some(ids) = ids {
        if states.is_some() {
            q.push(" AND ");
        } else {
            q.push(" WHERE ");
        }

        q.push(" id IN (");
        for (i, id) in ids.iter().enumerate() {
            q.push_bind(*id);
            if i < ids.len() - 1 {
                q.push(", ");
            }
        }
        q.push(")");
    }

    q.push(" ORDER BY ");
    q.push_bind(field);

    q.push(&format!(" {} LIMIT ", order));
    q.push_bind(end - start + 1);
    q.push(" OFFSET ");
    q.push_bind(start);

    let q = q.build();
    let rows = q.fetch_all(&mut *conn).await?;

    let total = if rows.is_empty() {
        0
    } else {
        rows.get(0)
            .map(|row| row.try_get("total"))
            .context("Failed to get total count of requests.")??
    };

    let requests: Vec<Request> = rows
        .into_iter()
        .map(|row| Request::from_row(&row))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GetListResponse {
        total,
        items: requests,
    })
}

pub async fn insert_attempt(
    pool: &SqlitePool,
    request_id: i64,
    response_status: u16,
    response_body: Option<&[u8]>,
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
        .bind(response_body)
        .execute(&mut *conn)
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

pub async fn list_attempts(
    pool: &SqlitePool,
    start: u32,
    end: u32,
    field: &str,
    order: &str,
    request_id: Option<i64>,
) -> Result<GetListResponse<Attempt>> {
    tracing::trace!("list_attempts");
    let mut conn = pool.acquire().await?;

    let mut q = QueryBuilder::new("SELECT *, COUNT(*) OVER() AS total FROM attempts");

    if let Some(request_id) = request_id {
        q.push(" WHERE request_id = ");
        q.push_bind(request_id);
    }

    q.push(" ORDER BY ");
    q.push_bind(field);

    q.push(&format!(" {} LIMIT ", order));
    q.push_bind(end - start + 1);
    q.push(" OFFSET ");
    q.push_bind(start);

    let q = q.build();
    let rows = q.fetch_all(&mut *conn).await?;

    let total = if rows.is_empty() {
        0
    } else {
        rows.get(0)
            .map(|row| row.try_get("total"))
            .context("Failed to get total count of attempts.")??
    };

    let attempts: Vec<Attempt> = rows
        .into_iter()
        .map(|row| Attempt::from_row(&row))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GetListResponse {
        total,
        items: attempts,
    })
}

pub async fn insert_origin(pool: &SqlitePool, origin: NewOrigin) -> Result<Origin> {
    tracing::trace!("insert_origin");
    let mut conn = pool.acquire().await?;

    let query = r#"
        INSERT INTO origins
        (
            domain,
            origin_uri,
            timeout,
            alert_threshold,
            alert_email,
            smtp_host,
            smtp_username,
            smtp_password,
            smtp_port,
            smtp_tls,
            created_at,
            updated_at
        )
        VALUES (
            ?,
            ?,
            ?,
            ?,
            ?,
            ?,
            ?,
            ?,
            ?,
            ?,
            strftime('%s','now'),
            strftime('%s','now')
        )
        RETURNING *
    "#;

    let created_origin = sqlx::query_as::<_, Origin>(query)
        .bind(origin.domain)
        .bind(origin.origin_uri)
        .bind(origin.timeout)
        .bind(origin.alert_threshold)
        .bind(origin.alert_email)
        .bind(origin.smtp_host)
        .bind(origin.smtp_username)
        .bind(origin.smtp_password)
        .bind(origin.smtp_port)
        .bind(origin.smtp_tls)
        .fetch_one(&mut *conn)
        .await?;

    Ok(created_origin)
}

pub async fn update_origin(pool: &SqlitePool, id: i64, origin: NewOrigin) -> Result<Origin> {
    tracing::trace!("update_origin");
    let mut conn = pool.acquire().await?;

    let query = r#"
        UPDATE origins
        SET
            domain = ?,
            origin_uri = ?,
            timeout = ?,
            alert_threshold = ?,
            alert_email = ?,
            smtp_host = ?,
            smtp_username = ?,
            smtp_password = ?,
            smtp_port = ?,
            smtp_tls = ?,
            updated_at = strftime('%s','now')
        WHERE id = ?
        RETURNING *
    "#;

    let updated_origin = sqlx::query_as::<_, Origin>(query)
        .bind(origin.domain)
        .bind(origin.origin_uri)
        .bind(origin.timeout)
        .bind(origin.alert_threshold)
        .bind(origin.alert_email)
        .bind(origin.smtp_host)
        .bind(origin.smtp_username)
        .bind(origin.smtp_password)
        .bind(origin.smtp_port)
        .bind(origin.smtp_tls)
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;

    Ok(updated_origin)
}

pub async fn list_origins(
    pool: &SqlitePool,
    start: u32,
    end: u32,
    field: &str,
    order: &str,
) -> Result<GetListResponse<Origin>> {
    tracing::trace!("list_origins");
    let mut conn = pool.acquire().await?;

    let q = format!(
        "SELECT *, COUNT(*) OVER() AS total FROM origins ORDER BY ? {} LIMIT ? OFFSET ?;",
        order
    );
    let rows = sqlx::query(&q)
        .bind(field)
        .bind(end - start + 1)
        .bind(start)
        .fetch_all(&mut *conn)
        .await?;

    let total = if rows.is_empty() {
        0
    } else {
        rows.get(0)
            .map(|row| row.try_get("total"))
            .context("Failed to get total count of origins.")??
    };

    let origins: Vec<Origin> = rows
        .into_iter()
        .map(|row| Origin::from_row(&row))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GetListResponse {
        total,
        items: origins,
    })
}

pub async fn get_origin(pool: &SqlitePool, id: i64) -> Result<Origin> {
    tracing::trace!("get_origin");
    let mut conn = pool.acquire().await?;

    let origin = sqlx::query_as::<_, Origin>("SELECT * FROM origins WHERE id = ?;")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;

    Ok(origin)
}

pub async fn delete_origin(pool: &SqlitePool, id: i64) -> Result<bool> {
    tracing::trace!("delete origin");
    let mut conn = pool.acquire().await?;

    let query = r#"
        DELETE FROM origins
        WHERE id = ?;
    "#;

    let result: SqliteQueryResult = sqlx::query(query).bind(id).execute(&mut *conn).await?;

    Ok(result.rows_affected() > 0)
}

pub async fn purge_completed_requests(pool: &SqlitePool, days: u32) -> Result<()> {
    tracing::trace!("purge_completed_requests");
    let mut conn = pool.acquire().await?;

    let query = r#"
        DELETE FROM requests
        WHERE state = ?
            AND created_at < strftime('%s','now') - 60 * 60 * 24 * ?;
    "#;

    sqlx::query(query)
        .bind(RequestState::Completed)
        .bind(days)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn add_request_to_queue(pool: &SqlitePool, req_id: i64) -> Result<()> {
    tracing::trace!("retry_requests");
    let mut conn = pool.acquire().await?;

    let query = r#"
        UPDATE requests
        SET
            state = ?,
            retry_ms_at = strftime('%s','now') || substr(strftime('%f','now'), 4)
        WHERE id = ?;
        ;
    "#;

    sqlx::query(query)
        .bind(RequestState::Created)
        .bind(req_id)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn get_request(pool: &SqlitePool, id: i64) -> Result<Request> {
    tracing::trace!("get_origin");
    let mut conn = pool.acquire().await?;

    let request = sqlx::query_as::<_, Request>("SELECT * FROM requests WHERE id = ?;")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;

    Ok(request)
}

pub async fn get_attempt(pool: &SqlitePool, id: i64) -> Result<Attempt> {
    tracing::trace!("get_origin");
    let mut conn = pool.acquire().await?;

    let attempt = sqlx::query_as::<_, Attempt>("SELECT * FROM attempts WHERE id = ?;")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;

    Ok(attempt)
}

pub async fn update_request(pool: &SqlitePool, id: i64, request: UpdateRequest) -> Result<Request> {
    tracing::trace!("update_request");
    let mut conn = pool.acquire().await?;

    let query = r#"
        INSERT INTO requests
        (
            method,
            uri,
            headers,
            body,
            state,
            created_at,
            retry_ms_at,
            from_request_id
        )
        VALUES (
            ?,
            ?,
            ?,
            ?,
            ?,
            strftime('%s','now'),
            strftime('%s','now') || substr(strftime('%f','now'), 4),
            ?
        )
        RETURNING *
    "#;

    let updated_request = sqlx::query_as::<_, Request>(query)
        .bind(request.method)
        .bind(request.uri)
        .bind(request.headers)
        .bind(request.body)
        .bind(RequestState::Created)
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;

    Ok(updated_request)
}
