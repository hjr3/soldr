use axum::http::Uri;

pub struct Origin {
    pub uri: Uri,
    pub timeout: u32,
}
