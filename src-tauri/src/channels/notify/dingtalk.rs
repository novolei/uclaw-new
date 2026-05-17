//! DingTalk outbound webhook sender with optional HMAC-SHA256 signing.
//!
//! Config fields (in `ImChannelInstanceConfig.config`):
//!   - `webhook_url`    — DingTalk robot webhook URL (required)
//!   - `signing_secret` — DingTalk signature secret (optional; omit to disable signing)

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct DingtalkSender {
    client: reqwest::Client,
}

impl DingtalkSender {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Produce the URL-safe HMAC-SHA256 signature required by DingTalk's
    /// security policy when a signing secret is configured.
    ///
    /// Formula: `URLENCODE( BASE64( HMAC-SHA256( key=secret, msg="{ts}\n{secret}" ) ) )`
    fn sign(secret: &str, timestamp: i64) -> String {
        let msg = format!("{}\n{}", timestamp, secret);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(msg.as_bytes());
        let result = mac.finalize().into_bytes();
        let encoded = BASE64.encode(result);
        urlencoding::encode(&encoded).into_owned()
    }
}

#[async_trait]
impl ImChannelSender for DingtalkSender {
    async fn send_text(
        &self,
        _chat_id: &str,
        text: &str,
        ctx: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let ctx = ctx.ok_or("dingtalk: missing config ctx")?;
        let mut url = ctx["webhook_url"]
            .as_str()
            .ok_or("dingtalk: missing webhook_url")?
            .to_string();

        if let Some(secret) = ctx["signing_secret"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            let ts = chrono::Utc::now().timestamp_millis();
            let sign = Self::sign(secret, ts);
            url = format!("{}&timestamp={}&sign={}", url, ts, sign);
        }

        self.client
            .post(&url)
            .json(&serde_json::json!({
                "msgtype": "text",
                "text": { "content": text }
            }))
            .send()
            .await
            .map_err(|e| format!("dingtalk error: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_produces_non_empty_string() {
        let sig = DingtalkSender::sign("mysecret", 1700000000000);
        assert!(!sig.is_empty());
    }

    #[test]
    fn sign_is_deterministic() {
        let ts = 1700000000000i64;
        let a = DingtalkSender::sign("secret", ts);
        let b = DingtalkSender::sign("secret", ts);
        assert_eq!(a, b);
    }

    #[test]
    fn sign_differs_with_different_secrets() {
        let ts = 1700000000000i64;
        let a = DingtalkSender::sign("secret1", ts);
        let b = DingtalkSender::sign("secret2", ts);
        assert_ne!(a, b);
    }
}
