use anyhow::Result;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::origin::Origin;

pub async fn send_alert(origin: &Origin, req_id: i64) {
    if let Err(error) = email_alert(origin, req_id).await {
        tracing::error!("Failed to send alert email: {}", error);
    }
}

async fn email_alert(origin: &Origin, req_id: i64) -> Result<()> {
    let smtp_host = match &origin.smtp_host {
        Some(host) => host,
        None => {
            tracing::debug!("No SMTP host set, skipping alert");
            return Ok(());
        }
    };

    let smtp_port = match origin.smtp_port {
        Some(port) => port,
        None => {
            tracing::debug!("No SMTP port set, skipping alert");
            return Ok(());
        }
    };

    let alert_email = match &origin.alert_email {
        Some(email) => email,
        None => {
            tracing::debug!("No alert email set, skipping alert");
            return Ok(());
        }
    };

    let subject = format!("Error: Request to {} failed", origin.uri);

    let body = format!("Please check the logs for request id {}", req_id);

    let email = Message::builder()
        .from("Soldr <alerts@soldr.dev>".parse()?)
        .to(alert_email.parse()?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body)?;

    let transport = if origin.smtp_tls {
        AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host)?.port(smtp_port)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host).port(smtp_port)
    };

    let transport = if let (Some(smtp_username), Some(smtp_password)) =
        (&origin.smtp_username, &origin.smtp_password)
    {
        let creds = Credentials::from((smtp_username, smtp_password));
        transport.credentials(creds)
    } else {
        transport
    };

    let mailer = transport.build();

    let response = mailer.send(email).await?;
    if !response.is_positive() {
        tracing::error!(
            "Failed to send alert email: {}",
            response
                .message()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join("\n")
        );
    }

    Ok(())
}
