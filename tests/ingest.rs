mod common;

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::{routing::post, Router};
use tokio::sync::Mutex;
use tower::util::ServiceExt;

use soldr::mgmt::CreateOrigin;
use soldr::{app, db};

type Sentinel = Arc<Mutex<Option<Request<Body>>>>;

async fn handler(
    State(sentinal): State<Sentinel>,
    req: Request<Body>,
) -> impl axum::response::IntoResponse {
    let mut lock = sentinal.lock().await;
    *lock = Some(req);
    "Hello, World!"
}

#[tokio::test]
async fn ingest_save_and_proxy() {
    common::enable_tracing();

    // set up origin server
    let listener = TcpListener::bind("0.0.0.0:3001".parse::<SocketAddr>().unwrap()).unwrap();
    let sentinel: Sentinel = Arc::new(Mutex::new(None));
    let s2 = sentinel.clone();
    let client_app = Router::new().route("/", post(handler).with_state(s2));

    tokio::spawn(async move {
        axum::Server::from_tcp(listener)
            .unwrap()
            .serve(client_app.into_make_service())
            .await
            .unwrap();
    });

    let (ingest, mgmt) = app().await.unwrap();

    // create an origin mapping
    let domain = "example.wh.soldr.dev";
    let create_origin = CreateOrigin {
        domain: domain.to_string(),
        origin_uri: "http://localhost:3001".to_string(),
    };
    let body = serde_json::to_string(&create_origin).unwrap();
    let response = mgmt
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/origin")
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
    assert!(reqs[0].complete);

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

#[tokio::test]
async fn mgmt_list_requests() {
    let (_, mgmt) = app().await.unwrap();

    let response = mgmt
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
    assert_eq!(&body[..], b"[]");
}

#[tokio::test]
async fn mgmt_create_origin() {
    let (_, mgmt) = app().await.unwrap();

    let create_origin = CreateOrigin {
        domain: "example.wh.soldr.dev".to_string(),
        origin_uri: "https://www.example.com".to_string(),
    };
    let body = serde_json::to_string(&create_origin).unwrap();
    let response = mgmt
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/origin")
                .header("Content-Type", "application/json")
                .body(body.into())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let origin: db::Origin = serde_json::from_slice(&body).unwrap();
    assert_eq!(origin.id, 1);
    assert_eq!(origin.domain, create_origin.domain);
    assert_eq!(origin.origin_uri, create_origin.origin_uri);
}
