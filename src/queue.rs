use anyhow::Result;
use sqlx::sqlite::SqlitePool;

use crate::{
    db::{list_failed_requests, mark_complete, mark_error, QueuedRequest},
    proxy::{self, Client},
};

pub async fn tick(pool: SqlitePool) {
    if let Err(err) = do_tick(pool).await {
        // TODO flow through the request id
        tracing::error!("tick error {:?}", err);
    }
}

async fn do_tick(pool: SqlitePool) -> Result<()> {
    let requests = list_failed_requests(&pool).await?;

    let mut tasks = Vec::with_capacity(requests.len());
    for request in requests {
        let pool2 = pool.clone();
        tasks.push(tokio::spawn(retry_request(pool2, request)));
    }

    for task in tasks {
        if let Err(err) = task.await? {
            // TODO flow through the request id
            tracing::error!("error retrying queued request {:?}", err);
        }
    }

    Ok(())
}

async fn retry_request(pool: SqlitePool, request: QueuedRequest) -> Result<()> {
    tracing::trace!("retrying {:?}", &request);

    let req_id = request.id;
    let client = Client::new();
    let is_success = proxy::proxy(&pool, &client, request).await?;

    if is_success {
        mark_complete(&pool, req_id).await?;
    } else {
        mark_error(&pool, req_id).await?;
    }

    Ok(())
}
