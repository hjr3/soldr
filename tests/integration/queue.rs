use crate::common;

use std::net::{SocketAddr, TcpListener};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::{routing::post, Router};
use soldr::db::RequestState;
use tower::util::ServiceExt;

use soldr::db::NewOrigin;
use soldr::mgmt::NewQueueRequest;
use soldr::{app, db};

async fn failure_handler() -> impl axum::response::IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "unexpected error".to_string(),
    )
}

#[tokio::test]
async fn queue_retry_request() {
    common::enable_tracing();

    // set up origin server
    let listener = TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
    let port = listener.local_addr().unwrap().port();
    let client_app = Router::new().route("/failure", post(failure_handler));

    tokio::spawn(async move {
        axum::Server::from_tcp(listener)
            .unwrap()
            .serve(client_app.into_make_service())
            .await
            .unwrap();
    });

    let (ingest, mgmt, retry_queue) = app(&common::config()).await.unwrap();

    // create an origin mapping
    let domain = "example.wh.soldr.dev";
    let create_origin = NewOrigin {
        domain: domain.to_string(),
        origin_uri: format!("http://localhost:{}", port),
        timeout: 100,
        ..Default::default()
    };
    let body = serde_json::to_string(&create_origin).unwrap();
    let response = mgmt
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/origins")
                .header("Content-Type", "application/json")
                .body(body.into())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // send a webhook request
    // `Router` implements `tower::Service<Request<Body>>` so we can
    // call it like any tower service, no need to run an HTTP server.
    let response = ingest
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/failure")
                .header("Host", domain)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // use management API to verify the request is marked error
    let response = mgmt
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/requests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();

    let reqs: Vec<db::Request> = serde_json::from_slice(&body).unwrap();
    assert_eq!(reqs[0].state, RequestState::Failed);

    // use management API to verify an attempt was made
    let response = mgmt
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/attempts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();

    let attempts: Vec<db::Attempt> = serde_json::from_slice(&body).unwrap();
    assert_eq!(attempts[0].id, 1);
    assert_eq!(attempts[0].request_id, 1);
    assert_eq!(attempts[0].response_status, 500);
    assert_eq!(attempts[0].response_body, b"unexpected error");

    // use management API to set the retry_ms_at to be now so our test
    // does not have to sleep
    let new_queue_request = NewQueueRequest {
        req_id: attempts[0].id,
    };
    let body = serde_json::to_string(&new_queue_request).unwrap();
    let response = mgmt
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queue")
                .header("Content-Type", "application/json")
                .body(body.into())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    retry_queue.tick().await;

    // use management API to verify an attempt was made
    let response = mgmt
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/attempts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();

    let attempts: Vec<db::Attempt> = serde_json::from_slice(&body).unwrap();
    assert_eq!(attempts[0].id, 2);
    assert_eq!(attempts[0].request_id, 1);

    // use management API to verify retry_at is set
    let response = mgmt
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/requests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();

    let requests: Vec<db::Request> = serde_json::from_slice(&body).unwrap();

    let current_time = SystemTime::now();
    let since_epoch = current_time
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let milliseconds = since_epoch.as_millis();

    let diff = requests[0].retry_ms_at - milliseconds as i64;
    // the second retry should be less than 3.4 seconds of now
    // rationale: 1.52^(2 attempts) = 2310ms + max(0ms, 1000ms) = 3310ms
    assert!(diff.abs() < 3400);
}
