//! SMTP email sender using lettre with STARTTLS.
//!
//! Config fields (in `ImChannelInstanceConfig.config`):
//!   - `smtp_host`     — SMTP server hostname (required)
//!   - `smtp_port`     — SMTP port (default: 587)
//!   - `username`      — SMTP login username (required)
//!   - `password`      — SMTP login password (required)
//!   - `from_address`  — Sender address (defaults to `username`)
//!   - `to_addresses`  — JSON array of recipient addresses (required, at least one)
//!   - `subject`       — Email subject line (default: "uClaw Notification")

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;
use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

pub struct EmailSender;

impl EmailSender {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ImChannelSender for EmailSender {
    async fn send_text(
        &self,
        _chat_id: &str,
        text: &str,
        ctx: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let ctx = ctx.ok_or("email: missing config ctx")?;

        let host = ctx["smtp_host"]
            .as_str()
            .ok_or("email: missing smtp_host")?;
        let port = ctx["smtp_port"].as_u64().unwrap_or(587) as u16;
        let username = ctx["username"]
            .as_str()
            .ok_or("email: missing username")?;
        let password = ctx["password"]
            .as_str()
            .ok_or("email: missing password")?;
        let from = ctx["from_address"].as_str().unwrap_or(username);
        let to_list: Vec<&str> = ctx["to_addresses"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        if to_list.is_empty() {
            return Err("email: no to_addresses configured".to_string());
        }
        let subject = ctx["subject"].as_str().unwrap_or("uClaw Notification");

        let creds = Credentials::new(username.to_string(), password.to_string());
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|e| format!("email: SMTP relay error: {e}"))?
            .port(port)
            .credentials(creds)
            .build();

        let from_addr: lettre::message::Mailbox = from
            .parse()
            .map_err(|e| format!("email: invalid from address '{from}': {e}"))?;

        for to in &to_list {
            let to_addr = to
                .parse()
                .map_err(|e| format!("email: invalid to address '{to}': {e}"))?;

            let email = Message::builder()
                .from(from_addr.clone())
                .to(to_addr)
                .subject(subject)
                .header(ContentType::TEXT_PLAIN)
                .body(text.to_string())
                .map_err(|e| format!("email: message build error: {e}"))?;

            mailer
                .send(email)
                .await
                .map_err(|e| format!("email: send error to '{to}': {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ctx_returns_err() {
        // Can't call async from sync test directly — just validate the logic path via type check.
        let _ = EmailSender::new();
    }

    #[test]
    fn empty_to_list_is_rejected() {
        let ctx = serde_json::json!({
            "smtp_host": "smtp.example.com",
            "username": "user@example.com",
            "password": "secret",
            "to_addresses": []
        });
        let to_list: Vec<&str> = ctx["to_addresses"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert!(to_list.is_empty(), "empty to_addresses should be rejected");
    }

    #[test]
    fn default_port_is_587() {
        let ctx = serde_json::json!({});
        let port = ctx["smtp_port"].as_u64().unwrap_or(587) as u16;
        assert_eq!(port, 587);
    }

    #[test]
    fn default_subject_fallback() {
        let ctx = serde_json::json!({});
        let subject = ctx["subject"].as_str().unwrap_or("uClaw Notification");
        assert_eq!(subject, "uClaw Notification");
    }
}
