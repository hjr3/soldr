use std::net::{SocketAddr, TcpListener};
use std::rc::Rc;
use std::sync::mpsc;

use criterion::Criterion;
use criterion::{criterion_group, criterion_main};

use axum::http::Request;
use axum::http::StatusCode;
use axum::{routing::post, Router};

use tokio::sync::oneshot;
use tower::util::ServiceExt;

use soldr::app;
use soldr::db::NewOrigin;

pub mod common;

async fn success_handler() -> impl axum::response::IntoResponse {
    "Hello, World!"
}

async fn failure_handler() -> impl axum::response::IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "unexpected error".to_string(),
    )
}

async fn do_bench(addr: SocketAddr, client: Rc<reqwest::Client>, path: &str) {
    let domain = "example.wh.soldr.dev";
    let url = format!("http://{}:{}{}", addr.ip(), addr.port(), path);

    let resp = client
        .post(url)
        .header("Host", domain)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
}

async fn setup(port: u16) -> Router {
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

    ingest
}

fn benchmark_proxy(c: &mut Criterion) {
    let (_until_tx, until_rx) = oneshot::channel::<()>();
    let (ingest_addr, origin_addr) = {
        let (addr_tx, addr_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();

            let origin_listener =
                TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
            let origin_addr = origin_listener.local_addr().unwrap();
            let port = origin_addr.port();
            runtime.block_on(async {
                // set up origin server
                let client_app = Router::new().route("/", post(success_handler));

                tokio::spawn(async move {
                    axum::Server::from_tcp(origin_listener)
                        .unwrap()
                        .serve(client_app.into_make_service())
                        .await
                        .unwrap();
                });
            });

            let ingest = runtime.block_on(setup(port));

            let ingest_listener =
                TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
            let ingest_addr = ingest_listener.local_addr().unwrap();

            runtime.spawn(async move {
                axum::Server::from_tcp(ingest_listener)
                    .unwrap()
                    .serve(ingest.into_make_service())
                    .await
                    .unwrap();
            });

            addr_tx.send((ingest_addr, origin_addr)).unwrap();
            runtime.block_on(until_rx).ok();
        });

        addr_rx.recv().unwrap()
    };

    let client = Rc::new(reqwest::Client::new());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("proxy");
    group.bench_function("empty post", |b| {
        b.to_async(&runtime)
            .iter(|| do_bench(origin_addr, client.clone(), "/"));
    });

    let client = Rc::new(reqwest::Client::new());
    group.bench_function("proxy empty post", |b| {
        b.to_async(&runtime)
            .iter(|| do_bench(ingest_addr, client.clone(), "/"));
    });
    group.finish();
}

fn benchmark_outage(c: &mut Criterion) {
    let (_until_tx, until_rx) = oneshot::channel::<()>();
    let (ingest_addr, origin_addr) = {
        let (addr_tx, addr_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();

            let origin_listener =
                TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
            let origin_addr = origin_listener.local_addr().unwrap();
            let port = origin_addr.port();
            runtime.block_on(async {
                // set up origin server
                let client_app = Router::new()
                    .route("/", post(success_handler))
                    .route("/failure", post(failure_handler));

                tokio::spawn(async move {
                    axum::Server::from_tcp(origin_listener)
                        .unwrap()
                        .serve(client_app.into_make_service())
                        .await
                        .unwrap();
                });
            });

            let ingest = runtime.block_on(setup(port));

            let ingest_listener =
                TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
            let ingest_addr = ingest_listener.local_addr().unwrap();

            runtime.spawn(async move {
                axum::Server::from_tcp(ingest_listener)
                    .unwrap()
                    .serve(ingest.into_make_service())
                    .await
                    .unwrap();
            });

            addr_tx.send((ingest_addr, origin_addr)).unwrap();
            runtime.block_on(until_rx).ok();
        });

        addr_rx.recv().unwrap()
    };

    let client = Rc::new(reqwest::Client::new());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("proxy");
    group.bench_function("success", |b| {
        b.to_async(&runtime)
            .iter(|| do_bench(origin_addr, client.clone(), "/"));
    });

    let client = Rc::new(reqwest::Client::new());
    group.bench_function("outage", |b| {
        b.to_async(&runtime)
            .iter(|| do_bench(ingest_addr, client.clone(), "/failure"));
    });

    let client = Rc::new(reqwest::Client::new());
    group.bench_function("success after outage", |b| {
        b.to_async(&runtime)
            .iter(|| do_bench(ingest_addr, client.clone(), "/"));
    });
    group.finish();
}

criterion_group!(benches, benchmark_proxy, benchmark_outage);
criterion_main!(benches);
