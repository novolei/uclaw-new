//! Feishu (Lark) outbound webhook sender with optional HMAC-SHA256 signing.
//!
//! Config fields (in `ImChannelInstanceConfig.config`):
//!   - `webhook_url`    — Feishu robot webhook URL (required)
//!   - `signing_secret` — Feishu signature secret (optional; omit to disable signing)
//!
//! Feishu signing formula: BASE64( HMAC-SHA256( key=secret, msg="{ts}\n{secret}" ) )

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct FeishuSender {
    client: reqwest::Client,
}

impl FeishuSender {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Feishu signature: HMAC-SHA256 where key = secret and message = "{ts}\n{secret}".
    fn sign(secret: &str, timestamp: i64) -> String {
        let msg = format!("{}\n{}", timestamp, secret);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(msg.as_bytes());
        BASE64.encode(mac.finalize().into_bytes())
    }
}

#[async_trait]
impl ImChannelSender for FeishuSender {
    async fn send_text(
        &self,
        _chat_id: &str,
        text: &str,
        ctx: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let ctx = ctx.ok_or("feishu: missing config ctx")?;
        let url = ctx["webhook_url"]
            .as_str()
            .ok_or("feishu: missing webhook_url")?;

        let mut body = serde_json::json!({
            "msg_type": "text",
            "content": { "text": text }
        });

        if let Some(secret) = ctx["signing_secret"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            let ts = chrono::Utc::now().timestamp();
            let sign = Self::sign(secret, ts);
            body["timestamp"] = serde_json::json!(ts.to_string());
            body["sign"] = serde_json::json!(sign);
        }

        self.client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("feishu error: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_produces_non_empty_string() {
        let sig = FeishuSender::sign("mysecret", 1700000000);
        assert!(!sig.is_empty());
    }

    #[test]
    fn sign_is_deterministic() {
        let ts = 1700000000i64;
        let a = FeishuSender::sign("secret", ts);
        let b = FeishuSender::sign("secret", ts);
        assert_eq!(a, b);
    }

    #[test]
    fn sign_differs_with_different_secrets() {
        let ts = 1700000000i64;
        let a = FeishuSender::sign("secret1", ts);
        let b = FeishuSender::sign("secret2", ts);
        assert_ne!(a, b);
    }
}
