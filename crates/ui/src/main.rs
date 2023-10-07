mod error;

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
    Form, Router,
};
use maud::{html, Markup, DOCTYPE};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use shared_types::{Origin, Request};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ui=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let client = reqwest::Client::new();
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/origins", get(origins))
        .route("/origins", post(origin_create))
        .route("/origins/new", get(origin_new))
        .route("/origins/:id", get(origin_detail))
        .route("/origins/:id", put(origin_update))
        .route("/origins/:id", delete(origin_delete))
        .route("/origins/:id/edit", get(origin_edit))
        .route("/requests", get(requests))
        .route("/requests/:id", get(request_detail))
        .route("/requests/:id/edit", get(request_edit))
        .route("/attempts/:id", get(attempt_detail))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(client);

    axum::Server::bind(&"0.0.0.0:8888".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn dashboard() -> Markup {
    html! {
        h1 { "Dashboard" }
    }
}

async fn origins(State(client): State<reqwest::Client>) -> Result<Markup, error::AppError> {
    let response = client.get("http://localhost:3443/origins").send().await?;

    let origins: Vec<Origin> = response.json().await?;

    Ok(page(
        "Origins",
        html! {
            h1 { "Origins" }
            a href="/origins/new" { "New Origin" }
            ul {
                @for origin in origins {
                    @let url = format!("/origins/{}", origin.id);
                    li { (origin.origin_uri)  " - " a href=(url) { "Details" } }
                }
            }
        },
    ))
}

async fn origin_new() -> Markup {
    page(
        "New Origin",
        html! {
            h1 { "New Origin" }
            form action="/origins" method="POST" {
                label for="domain" { "Domain:" }
                input id="domain" name="domain" type="input" required="true";
                label for="origin_uri" { "Origin URI:" }
                input id="origin_uri" name="origin_uri" type="input" required="true";
                label for="timeout" { "Timeout:" }
                input id="timeout" name="timeout" type="input" required="true";
                button { "Create" }
            }
        },
    )
}

#[derive(Debug, Deserialize, Serialize)]
struct CreateOrigin {
    domain: String,
    origin_uri: url::Url,
    timeout: u32,
    #[serde(default)]
    alert_threshold: Option<u16>,
    #[serde(default)]
    alert_email: Option<String>,
    #[serde(default)]
    smtp_host: Option<String>,
    #[serde(default)]
    smtp_username: Option<String>,
    #[serde(default)]
    smtp_password: Option<String>,
    #[serde(default)]
    smtp_port: Option<u16>,
    #[serde(default)]
    smtp_tls: bool,
}

async fn origin_create(
    State(client): State<reqwest::Client>,
    Form(form): Form<CreateOrigin>,
) -> Result<Response, error::AppError> {
    let response = client
        .post("http://localhost:3443/origins")
        .json(&form)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(Redirect::to("/origins").into_response())
    } else if response.status().is_client_error() {
        Ok(Redirect::to("/origins/new?client_error").into_response())
    } else {
        Ok(Redirect::to("/origins/new?server_error").into_response())
    }
}

type UpdateOrigin = CreateOrigin;

async fn origin_update(
    State(client): State<reqwest::Client>,
    Path(id): Path<i64>,
    Form(form): Form<UpdateOrigin>,
) -> Result<Response, error::AppError> {
    let response = client
        .put(format!("http://localhost:3443/origins/{}", id))
        .json(&form)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(Redirect::to(&format!("/origins/{}", id)).into_response())
    } else if response.status().is_client_error() {
        Ok(Redirect::to(&format!("/origins/{}?client_error", id)).into_response())
    } else {
        Ok(Redirect::to(&format!("/origins/{}?server_error", id)).into_response())
    }
}

async fn origin_detail(
    State(client): State<reqwest::Client>,
    Path(id): Path<i64>,
) -> Result<Markup, error::AppError> {
    let response = client
        .get(format!("http://localhost:3443/origins/{}", id))
        .send()
        .await?;

    let origin: Origin = response.json().await?;

    let url = format!("/origins/{}", id);
    let edit_url = format!("/origins/{}/edit", id);
    Ok(page(
        "Origin Detail",
        html! {
            h1 { "Origin Detail" }
            dl {
                dt { "ID" }
                dd { (origin.id) }
                dt { "Domain" }
                dd { (origin.domain) }
                dt { "Origin URI" }
                dd { (origin.origin_uri) }
                dt { "Timeout" }
                dd { (origin.timeout) }
                dt { "Alert Threshold" }
                dd {
                    @if let Some(alert_threshold) = origin.alert_threshold {
                        (alert_threshold)
                    } else {
                        i { "Unset" }
                    }
                }
                dt { "Alert Email" }
                dd {
                    @if let Some(alert_email) = origin.alert_email {
                        (alert_email)
                    } else {
                        i { "Unset" }
                    }
                }
                dt { "SMTP Host" }
                dd {
                    @if let Some(smtp_host) = origin.smtp_host {
                        (smtp_host)
                    } else {
                        i { "Unset" }
                    }
                }
                dt { "SMTP Username" }
                dd {
                    @if let Some(smtp_username) = origin.smtp_username {
                        (smtp_username)
                    } else {
                        i { "Unset" }
                    }
                }
                dt { "SMTP Password" }
                dd {
                    @if let Some(smtp_password) = origin.smtp_password {
                        (smtp_password)
                    } else {
                        i { "Unset" }
                    }
                }
                dt { "SMTP Port" }
                dd {
                    @if let Some(smtp_port) = origin.smtp_port {
                        (smtp_port)
                    } else {
                        i { "Unset" }
                    }
                }
                dt { "SMTP TLS" }
                dd { (origin.smtp_tls) }
            }
            a href=(edit_url) { "Edit Origin" }
            button hx-delete=(url) hx-target="body" hx-push-url="true" { "Delete Origin" }
        },
    ))
}

async fn origin_delete(
    State(client): State<reqwest::Client>,
    Path(id): Path<i64>,
) -> Result<Response, error::AppError> {
    let response = client
        .delete(format!("http://localhost:3443/origins/{}", id))
        .send()
        .await?;

    if response.status().is_success() {
        Ok(Redirect::to("/origins").into_response())
    } else if response.status().is_client_error() {
        Ok(Redirect::to(&format!("/origins/{}?client_error", id)).into_response())
    } else {
        Ok(Redirect::to(&format!("/origins/{}?server_error", id)).into_response())
    }
}

async fn origin_edit(
    State(client): State<reqwest::Client>,
    Path(id): Path<i64>,
) -> Result<Markup, error::AppError> {
    let response = client
        .get(format!("http://localhost:3443/origins/{}", id))
        .send()
        .await?;

    let origin: Origin = response.json().await?;

    let url = format!("/origins/{}", id);
    Ok(page(
        "Edit Origin",
        html! {
            h1 { "Edit Origin" }

            form hx-put=(url) hx-target="body" hx-push-url="true" {
                input name="id" type="hidden" value=(origin.id);
                label for="domain" { "Domain:" }
                input id="domain" name="domain" type="input" required="true" value=(origin.domain);
                label for="origin_uri" { "Origin URI:" }
                input id="origin_uri" name="origin_uri" type="input" required="true" value=(origin.origin_uri);
                label for="timeout" { "Timeout:" }
                input id="timeout" name="timeout" type="input" required="true" value=(origin.timeout);
                button { "Update" }
            }
        },
    ))
}

async fn requests(State(client): State<reqwest::Client>) -> Result<Markup, error::AppError> {
    let response = client.get("http://localhost:3443/requests").send().await?;

    let requests: Vec<Request> = response.json().await?;

    Ok(html! {
        h1 { "Requests" }
        ul {
            @for request in requests {
                @let url = format!("/requests/{}", request.id);
                li { (request.method) " " (request.uri) " - " a href=(url) { "Details" } }
            }
        }
    })
}

async fn request_detail() -> Markup {
    html! {
        h1 { "Request Detail" }
    }
}

async fn request_edit() -> Markup {
    html! {
        h1 { "Request Edit" }
    }
}

async fn attempt_detail() -> Markup {
    html! {
        h1 { "Attempt Detail" }
    }
}
pub fn page(title: &str, content: Markup) -> Markup {
    /// A basic header with a dynamic `page_title`.
    pub(crate) fn head(page_title: &str) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en";
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                link rel="stylesheet" type="text/css" href="/style.css";
                title { (page_title) }
            }
        }
    }

    pub(crate) fn header() -> Markup {
        html! {
            header ."container py-5 flex flex-row place-content-center gap-6 items-center" {
                    div ."uppercase" { "Soldr" }
                    ."" {
                        img src="/favicon.ico" style="image-rendering: pixelated;" alt="soldr's logo";
                    }
            }
        }
    }

    /// A static footer.
    pub(crate) fn footer() -> Markup {
        html! {
            script src="https://unpkg.com/htmx.org@1.9.4" {};
            script src="/script.js" {};
        }
    }

    html! {
        (head(title))
        body ."container relative mx-auto !block" hx-boost="true" {
            (header())

            main ."container" {
                (content)
            }
            (footer())
        }
    }
}
