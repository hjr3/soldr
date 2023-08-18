use std::time::Duration;

use anyhow::Result;
use sqlx::sqlite::SqlitePool;
use tokio::time;

use crate::cache::OriginCache;
use crate::db::{list_failed_requests, purge_completed_requests, QueuedRequest};
use crate::proxy::{self, Client};
use crate::request::State;

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
    purge_completed_requests(pool, 30).await?;

    // FIXME mark these as enqueued and then pull them out
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

    let client = Client::new();
    proxy::proxy(&pool, &origin_cache, &client, State::Enqueued(request)).await;

    Ok(())
}
