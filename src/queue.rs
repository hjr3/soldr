use std::time::Duration;

use anyhow::Result;
use sqlx::sqlite::SqlitePool;
use tokio::time;

use crate::cache::OriginCache;

use crate::{
    db::{list_failed_requests, mark_complete, mark_error, QueuedRequest},
    proxy::{self, Client},
};

pub struct RetryQueue {
    pool: SqlitePool,
    origin_cache: OriginCache,
}

impl RetryQueue {
    pub fn new(pool: SqlitePool, origin_cache: OriginCache) -> Self {
        Self { pool, origin_cache }
    }

    pub async fn start(&self) {
        let mut interval = time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;
            tracing::trace!("retrying failed requests");
            self.tick().await;
        }
    }

    pub async fn tick(&self) {
        if let Err(err) = do_tick(&self.pool, &self.origin_cache).await {
            // TODO flow through the request id
            tracing::error!("tick error {:?}", err);
        }
    }
}

async fn do_tick(pool: &SqlitePool, origin_cache: &OriginCache) -> Result<()> {
    let requests = list_failed_requests(pool).await?;

    let mut tasks = Vec::with_capacity(requests.len());
    for request in requests {
        let pool2 = pool.clone();
        let origin_cache2 = origin_cache.clone();
        tasks.push(tokio::spawn(retry_request(pool2, origin_cache2, request)));
    }

    for task in tasks {
        if let Err(err) = task.await? {
            // TODO flow through the request id
            tracing::error!("error retrying queued request {:?}", err);
        }
    }

    Ok(())
}

async fn retry_request(
    pool: SqlitePool,
    origin_cache: OriginCache,
    request: QueuedRequest,
) -> Result<()> {
    tracing::trace!("retrying {:?}", &request);

    let req_id = request.id;
    let client = Client::new();
    let is_success = proxy::proxy(&pool, &origin_cache, &client, request).await?;

    if is_success {
        mark_complete(&pool, req_id).await?;
    } else {
        mark_error(&pool, req_id).await?;
    }

    Ok(())
}
