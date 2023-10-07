use crate::common;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use tower::util::ServiceExt;

use shared_types::{NewOrigin, Origin};
use soldr::app;

#[tokio::test]
async fn mgmt_list_requests() {
    let (_, mgmt, _) = app(&common::config()).await.unwrap();

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
    let (_, mgmt, _) = app(&common::config()).await.unwrap();

    let create_origin = NewOrigin {
        domain: "example.wh.soldr.dev".to_string(),
        origin_uri: "https://www.example.com".to_string(),
        timeout: 100,
        ..Default::default()
    };
    let body = serde_json::to_string(&create_origin).unwrap();
    let response = mgmt
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

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let origin: Origin = serde_json::from_slice(&body).unwrap();
    assert_eq!(origin.id, 1);
    assert_eq!(origin.domain, create_origin.domain);
    assert_eq!(origin.origin_uri, create_origin.origin_uri);
}
