use crate::common;

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::{routing::post, Router};
use soldr::db::RequestState;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tower::util::ServiceExt;

use soldr::db::NewOrigin;
use soldr::{app, db};

type Sentinel = Arc<Mutex<Option<Request<Body>>>>;

async fn success_handler(
    State(sentinal): State<Sentinel>,
    req: Request<Body>,
) -> impl axum::response::IntoResponse {
    let mut lock = sentinal.lock().await;
    *lock = Some(req);
    "Hello, World!"
}

async fn failure_handler() -> impl axum::response::IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "unexpected error".to_string(),
    )
}

async fn timeout_handler() -> impl axum::response::IntoResponse {
    sleep(Duration::from_millis(6)).await;
    "We shouldn't see this"
}

#[tokio::test]
async fn ingest_save_and_proxy() {
    // set up origin server
    let listener = TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
    let port = listener.local_addr().unwrap().port();
    let sentinel: Sentinel = Arc::new(Mutex::new(None));
    let s2 = sentinel.clone();
    let client_app = Router::new().route("/", post(success_handler).with_state(s2));

    tokio::spawn(async move {
        axum::Server::from_tcp(listener)
            .unwrap()
            .serve(client_app.into_make_service())
            .await
            .unwrap();
    });

    let (ingest, mgmt, _) = app(&common::config()).await.unwrap();

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
                .uri("/")
                .header("Host", domain)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let lock = sentinel.lock().await;
    assert!(lock.is_some());

    // use management API to verify the request is marked complete
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
    assert_eq!(reqs[0].state, RequestState::Completed);

    // use management API to verify an attempt was made
    let response = mgmt
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
    assert_eq!(attempts[0].response_status, 200);
    assert_eq!(attempts[0].response_body, b"Hello, World!");
}

// Note: This test will log a failure when it tries to send an email alert
// To test that the email alert works, you can run the following:
// `python3 -m smtpd -n -c DebuggingServer 127.0.0.1:2525`
#[tokio::test]
async fn ingest_proxy_failure() {
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

    let (ingest, mgmt, _) = app(&common::config()).await.unwrap();

    // create an origin mapping
    let domain = "example.wh.soldr.dev";
    let create_origin = NewOrigin {
        domain: domain.to_string(),
        origin_uri: format!("http://localhost:{}", port),
        timeout: 100,
        alert_threshold: Some(1),
        alert_email: Some("error@example.com".to_string()),
        smtp_host: Some("127.0.0.1".to_string()),
        smtp_port: Some(2525),
        smtp_username: None,
        smtp_password: None,
        smtp_tls: false,
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
}

#[tokio::test]
async fn ingest_proxy_timeout() {
    common::enable_tracing();

    // set up origin server
    let listener = TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
    let port = listener.local_addr().unwrap().port();
    let client_app = Router::new().route("/timeout", post(timeout_handler));

    tokio::spawn(async move {
        axum::Server::from_tcp(listener)
            .unwrap()
            .serve(client_app.into_make_service())
            .await
            .unwrap();
    });

    let (ingest, mgmt, _) = app(&common::config()).await.unwrap();

    // create an origin mapping
    let domain = "example.wh.soldr.dev";
    let create_origin = NewOrigin {
        domain: domain.to_string(),
        origin_uri: format!("http://localhost:{}", port),
        timeout: 5,
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
                .uri("/timeout")
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
    assert_eq!(reqs[0].state, RequestState::Timeout);

    // use management API to verify an attempt was made
    let response = mgmt
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
    assert_eq!(attempts[0].response_status, 504);
    assert_eq!(attempts[0].response_body, b"Timeout");
}
