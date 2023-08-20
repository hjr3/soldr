use hyper::Body;
use hyper::Response;

pub type HttpResponse = Response<Option<Vec<u8>>>;

pub async fn transform_response(response: Response<Body>) -> HttpResponse {
    let (parts, body) = response.into_parts();
    let body = hyper::body::to_bytes(body).await.unwrap_or_else(|_| {
        tracing::error!("Error reading response body");
        "Unknown body".into()
    });
    Response::from_parts(parts, Some(body.into()))
}
