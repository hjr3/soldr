use crate::common;

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::{routing::post, Router};
use http_auth_basic::Credentials;
use soldr::db::RequestState;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tower::util::ServiceExt;

use shared_types::NewOrigin;
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
    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let sentinel: Sentinel = Arc::new(Mutex::new(None));
    let s2 = sentinel.clone();
    let client_app = Router::new().route("/", post(success_handler).with_state(s2));

    tokio::spawn(async move {
        axum::serve(listener, client_app).await.unwrap();
    });

    let config = common::config();
    let (ingest, mgmt, _) = app(&config).await.unwrap();

    let credentials = Credentials::new(&config.management.secret, "");
    let credentials = credentials.as_http_header();

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
                .header("Authorization", &credentials)
                .header("Content-Type", "application/json")
                .body(body)
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
                .header("Authorization", &credentials)
                // /requests?filter={}&range=[0,9]&sort=["id","ASC"]
                .uri(r#"/requests?filter=%7B%7D&range=%5B0,9%5D&sort=%5B%22id%22,%22ASC%22%5D"#)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();

    let reqs: Vec<db::Request> = serde_json::from_slice(&body).unwrap();
    assert_eq!(reqs[0].state, RequestState::Completed);

    // use management API to verify an attempt was made
    let response = mgmt
        .oneshot(
            Request::builder()
                .method("GET")
                .header("Authorization", &credentials)
                // /attempts?filter={}&range=[0,9]&sort=["id","ASC"]
                .uri("/attempts?filter=%7B%7D&range=%5B0,9%5D&sort=%5B%22id%22,%22ASC%22%5D")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();

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
    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let client_app = Router::new().route("/failure", post(failure_handler));

    tokio::spawn(async move {
        axum::serve(listener, client_app).await.unwrap();
    });

    let config = common::config();
    let (ingest, mgmt, _) = app(&config).await.unwrap();

    let credentials = Credentials::new(&config.management.secret, "");
    let credentials = credentials.as_http_header();

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
                .header("Authorization", &credentials)
                .uri("/origins")
                .header("Content-Type", "application/json")
                .body(body)
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
                .header("Authorization", &credentials)
                // /requests?filter={}&range=[0,9]&sort=["id","ASC"]
                .uri(r#"/requests?filter=%7B%7D&range=%5B0,9%5D&sort=%5B%22id%22,%22ASC%22%5D"#)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();

    let reqs: Vec<db::Request> = serde_json::from_slice(&body).unwrap();
    assert_eq!(reqs[0].state, RequestState::Failed);

    // use management API to verify an attempt was made
    let response = mgmt
        .oneshot(
            Request::builder()
                .method("GET")
                .header("Authorization", &credentials)
                // /attempts?filter={}&range=[0,9]&sort=["id","ASC"]
                .uri("/attempts?filter=%7B%7D&range=%5B0,9%5D&sort=%5B%22id%22,%22ASC%22%5D")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();

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
    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let client_app = Router::new().route("/timeout", post(timeout_handler));

    tokio::spawn(async move {
        axum::serve(listener, client_app).await.unwrap();
    });

    let config = common::config();
    let (ingest, mgmt, _) = app(&config).await.unwrap();

    let credentials = Credentials::new(&config.management.secret, "");
    let credentials = credentials.as_http_header();

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
                .header("Authorization", &credentials)
                .header("Content-Type", "application/json")
                .body(body)
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
                // /requests?filter={}&range=[0,9]&sort=["id","ASC"]
                .uri(r#"/requests?filter=%7B%7D&range=%5B0,9%5D&sort=%5B%22id%22,%22ASC%22%5D"#)
                .header("Authorization", &credentials)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();

    let reqs: Vec<db::Request> = serde_json::from_slice(&body).unwrap();
    assert_eq!(reqs[0].state, RequestState::Timeout);

    // use management API to verify an attempt was made
    let response = mgmt
        .oneshot(
            Request::builder()
                .method("GET")
                // /attempts?filter={}&range=[0,9]&sort=["id","ASC"]
                .uri("/attempts?filter=%7B%7D&range=%5B0,9%5D&sort=%5B%22id%22,%22ASC%22%5D")
                .header("Authorization", &credentials)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();

    let attempts: Vec<db::Attempt> = serde_json::from_slice(&body).unwrap();
    assert_eq!(attempts[0].id, 1);
    assert_eq!(attempts[0].request_id, 1);
    assert_eq!(attempts[0].response_status, 504);
    assert_eq!(attempts[0].response_body, b"Timeout");
}

use soldr::cache::OriginCache;
use soldr::db::ensure_schema;
use soldr::mgmt::update_origin_cache;
use soldr::origin::Origin;
use soldr::proxy::{Client, Proxy};
use soldr::request;
use sqlx::sqlite::SqlitePool;

// FIXME: asbtract this in the lib
async fn bootstrap() -> (SqlitePool, OriginCache, Client) {
    let config = common::config();

    let pool = SqlitePool::connect(&config.database.url)
        .await
        .expect("Failed to connect to sqlite");
    ensure_schema(&pool).await.expect("Failed to ensure schema");

    let origin_cache = OriginCache::new();
    update_origin_cache(&pool, &origin_cache)
        .await
        .expect("Failed to update origin cache");

    let client = Client::new();

    (pool, origin_cache, client)
}

fn random_origin() -> Origin {
    Origin {
        uri: "https://www.example.com".parse().unwrap(),
        timeout: 100,
        alert_threshold: None,
        alert_email: None,
        smtp_host: None,
        smtp_port: None,
        smtp_username: None,
        smtp_password: None,
        smtp_tls: false,
    }
}

#[tokio::test]
async fn test_complete_failed_update_goes_to_panic() {
    common::enable_tracing();

    let (pool, origin_cache, client) = bootstrap().await;

    let proxy = Proxy {
        pool: &pool,
        origin_cache: &origin_cache,
        client: &client,
    };

    let origin = random_origin();

    let state = request::State::Completed(1, origin);
    let next_state = proxy.next(state).await.unwrap().unwrap();
    match next_state {
        request::State::Panic(_, _) => {}
        _ => panic!("Expected panic state"),
    }
}
