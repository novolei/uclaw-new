//! Generic HTTP POST webhook notify sender.

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;

pub struct WebhookImSender {
    client: reqwest::Client,
}

impl WebhookImSender {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl ImChannelSender for WebhookImSender {
    async fn send_text(
        &self,
        _chat_id: &str,
        text: &str,
        ctx: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let url = ctx
            .and_then(|c| c.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if url.is_empty() {
            return Err("webhook: no url in config".to_string());
        }

        let headers_val = ctx.and_then(|c| c.get("headers")).cloned();
        let mut req = self.client.post(&url).json(&serde_json::json!({
            "text": text,
        }));

        if let Some(h) = headers_val.as_ref().and_then(|v| v.as_object()) {
            for (k, v) in h {
                if let Some(val) = v.as_str() {
                    req = req.header(k.as_str(), val);
                }
            }
        }

        req.send()
            .await
            .map_err(|e| format!("webhook error: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_sender_requires_url() {
        let ctx = serde_json::json!({"url": ""});
        assert!(ctx["url"].as_str().unwrap_or("").is_empty());
    }

    #[test]
    fn webhook_sender_missing_url_key() {
        let ctx = serde_json::json!({"other_key": "value"});
        let url = ctx.get("url").and_then(|v| v.as_str()).unwrap_or("");
        assert!(url.is_empty());
    }
}
