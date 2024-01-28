use hyper::Uri;

pub struct Origin {
    pub uri: Uri,
    pub timeout: u32,
    pub alert_threshold: Option<u16>,
    pub alert_email: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_tls: bool,
}
