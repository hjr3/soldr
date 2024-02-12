use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Database {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Proxy {
    pub listen: String,
}

#[derive(Debug, Deserialize)]
pub struct Management {
    pub listen: String,
    pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct Tls {
    pub enable: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub database: Database,
    pub management: Management,
    pub proxy: Proxy,
    pub tls: Tls,
}
