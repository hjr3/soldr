mod common;

use std::net::{SocketAddr, TcpListener};

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::{routing::post, Router};
use soldr::db::RequestState;
use tower::util::ServiceExt;

use soldr::mgmt::CreateOrigin;
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

    let (ingest, mgmt, retry_queue) = app(common::config()).await.unwrap();

    // create an origin mapping
    let domain = "example.wh.soldr.dev";
    let create_origin = CreateOrigin {
        domain: domain.to_string(),
        origin_uri: format!("http://localhost:{}", port),
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
    assert_eq!(reqs[0].state, RequestState::Error);

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
}
