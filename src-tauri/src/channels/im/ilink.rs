//! iLink WeChat personal bot HTTP long-poll sender.
//!
//! Protocol: POST https://ilinkai.weixin.qq.com/ilink/bot/getupdates (35s hold)
//! Auth: Authorization: Bearer {bot_token}, X-WECHAT-UIN: random-uint32-base64
//! Reply: POST /ilink/bot/sendmessage with context_token echoed verbatim.
//! context_token has no expiry — valid until next message from same user.
//! errcode/ret -14 = session expired, stop and require re-auth.

use crate::channels::types::{ImChannelSender, InboundMessage, ReplyHandle};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::AbortHandle;

const ILINK_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const CHANNEL_VERSION: &str = "1.0.2";
const SESSION_EXPIRED_CODE: i64 = -14;
const RECONNECT_BASE_MS: u64 = 2_000;
const RECONNECT_MAX_MS: u64 = 30_000;
const MAX_RECONNECT_ATTEMPTS: u32 = 100;

pub struct IlinkSender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    context_tokens: Arc<RwLock<HashMap<String, String>>>,
    client: reqwest::Client,
}

impl IlinkSender {
    pub fn new(instance_id: &str, config: &Value, credentials: &Value) -> Self {
        let bot_token = credentials["bot_token"].as_str().unwrap_or("").to_string();
        let base_url = config["base_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(ILINK_BASE_URL)
            .to_string();
        Self {
            instance_id: instance_id.to_string(),
            bot_token,
            base_url,
            context_tokens: Arc::new(RwLock::new(HashMap::new())),
            client: reqwest::Client::new(),
        }
    }

    pub fn start(
        self: Arc<Self>,
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) -> AbortHandle {
        let sender = self.clone();
        tokio::spawn(async move {
            sender.poll_loop(inbound_tx).await;
        })
        .abort_handle()
    }

    async fn poll_loop(
        &self,
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) {
        if self.bot_token.is_empty() {
            tracing::warn!(
                "[IlinkBot:{}] No bot_token configured — long-poll not started",
                self.instance_id
            );
            return;
        }

        let mut updates_buf = String::new();
        let mut attempt = 0u32;

        loop {
            match self.single_poll(&mut updates_buf, &inbound_tx).await {
                Ok(true) => {
                    attempt = 0;
                }
                Ok(false) => {
                    tracing::error!(
                        "[IlinkBot:{}] Session expired (code -14) — re-auth required",
                        self.instance_id
                    );
                    break;
                }
                Err(e) => {
                    if attempt >= MAX_RECONNECT_ATTEMPTS {
                        tracing::error!(
                            "[IlinkBot:{}] Max reconnect attempts reached",
                            self.instance_id
                        );
                        break;
                    }
                    let delay = std::cmp::min(
                        RECONNECT_BASE_MS * 2u64.saturating_pow(attempt),
                        RECONNECT_MAX_MS,
                    );
                    attempt += 1;
                    tracing::warn!(
                        "[IlinkBot:{}] Poll error (attempt {attempt}): {e}; backoff {delay}ms",
                        self.instance_id
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
        }
    }

    async fn single_poll(
        &self,
        updates_buf: &mut String,
        inbound_tx: &mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) -> Result<bool> {
        let url = format!("{}/ilink/bot/getupdates", self.base_url);
        let headers = self.build_auth_headers();
        let body = json!({
            "get_updates_buf": updates_buf,
            "base_info": { "channel_version": CHANNEL_VERSION }
        });

        let resp: Value = self
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .timeout(std::time::Duration::from_secs(40))
            .send()
            .await?
            .json()
            .await?;

        let ret_code = resp["ret"]
            .as_i64()
            .unwrap_or(resp["errcode"].as_i64().unwrap_or(0));
        if ret_code == SESSION_EXPIRED_CODE {
            return Ok(false);
        }
        if ret_code != 0 {
            return Err(anyhow!("iLink getupdates error code {ret_code}"));
        }

        if let Some(buf) = resp["get_updates_buf"].as_str() {
            *updates_buf = buf.to_string();
        }

        if let Some(msgs) = resp["msgs"].as_array() {
            for msg in msgs {
                if msg["message_type"].as_i64() != Some(1) {
                    continue;
                }
                self.handle_inbound(msg, inbound_tx).await;
            }
        }

        Ok(true)
    }

    async fn handle_inbound(
        &self,
        msg: &Value,
        inbound_tx: &mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) {
        let user_id = match msg["from_user_id"].as_str() {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => return,
        };

        let context_token = msg["context_token"].as_str().map(String::from);
        if let Some(ref ct) = context_token {
            self.context_tokens
                .write()
                .await
                .insert(user_id.clone(), ct.clone());
        }

        let items = &msg["item_list"];
        let text = Self::extract_text(items);

        tracing::info!(
            "[IlinkBot:{}] inbound from={user_id} len={}",
            self.instance_id,
            text.len()
        );

        let channel_ctx = context_token
            .as_ref()
            .map(|ct| json!({ "context_token": ct }));

        let inbound = InboundMessage {
            instance_id: self.instance_id.clone(),
            chat_id: user_id.clone(),
            sender_name: Some(user_id.clone()),
            text,
            timestamp: chrono::Utc::now().timestamp_millis(),
            channel_ctx: channel_ctx.clone(),
        };

        let sender_arc: Arc<dyn ImChannelSender> = Arc::new(IlinkReplySender {
            instance_id: self.instance_id.clone(),
            bot_token: self.bot_token.clone(),
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        });
        let reply = Arc::new(ReplyHandle {
            sender: sender_arc,
            channel_ctx,
            chat_id: user_id,
        });

        let _ = inbound_tx.send((inbound, reply));
    }

    fn build_auth_headers(&self) -> reqwest::header::HeaderMap {
        use base64::Engine;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        let n: u32 = rand::random();
        let uin =
            base64::engine::general_purpose::STANDARD.encode(n.to_string().as_bytes());
        headers.insert("X-WECHAT-UIN", uin.parse().unwrap());
        if !self.bot_token.is_empty() {
            if let Ok(v) = format!("Bearer {}", self.bot_token).parse() {
                headers.insert("Authorization", v);
            }
        }
        headers
    }

    pub fn extract_text(items: &Value) -> String {
        let arr = match items.as_array() {
            Some(a) => a,
            None => return String::new(),
        };
        let mut parts = Vec::new();
        for item in arr {
            let t = item["type"].as_i64().unwrap_or(0);
            let s = match t {
                1 => item["text_item"]["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                2 => "[Image]".to_string(),
                3 => item["voice_item"]["text"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| "[Voice]".to_string()),
                4 => format!(
                    "[File: {}]",
                    item["file_item"]["filename"]
                        .as_str()
                        .unwrap_or("unknown")
                ),
                5 => "[Video]".to_string(),
                _ => format!("[Unknown type: {t}]"),
            };
            if !s.is_empty() {
                parts.push(s);
            }
        }
        parts.join("\n")
    }
}

struct IlinkReplySender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    client: reqwest::Client,
}

#[async_trait]
impl ImChannelSender for IlinkReplySender {
    async fn send_text(
        &self,
        chat_id: &str,
        text: &str,
        ctx: Option<&Value>,
    ) -> Result<(), String> {
        let context_token = ctx
            .and_then(|c| c["context_token"].as_str())
            .ok_or_else(|| {
                format!(
                    "[IlinkBot:{}] Cannot reply to {chat_id}: missing context_token",
                    self.instance_id
                )
            })?;

        let url = format!("{}/ilink/bot/sendmessage", self.base_url);
        let body = json!({
            "to_user_id": chat_id,
            "context_token": context_token,
            "item_list": [{
                "type": 1,
                "text_item": { "text": text }
            }]
        });

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("AuthorizationType", "ilink_bot_token")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .json(&body)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!("iLink sendmessage HTTP {status}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_from_text_item() {
        let items = serde_json::json!([
            { "type": 1, "text_item": { "text": "hello iLink" } }
        ]);
        assert_eq!(IlinkSender::extract_text(&items), "hello iLink");
    }

    #[test]
    fn extract_text_joins_multiple_items() {
        let items = serde_json::json!([
            { "type": 1, "text_item": { "text": "first" } },
            { "type": 1, "text_item": { "text": "second" } }
        ]);
        assert_eq!(IlinkSender::extract_text(&items), "first\nsecond");
    }

    #[test]
    fn extract_text_image_placeholder() {
        let items = serde_json::json!([{ "type": 2 }]);
        assert_eq!(IlinkSender::extract_text(&items), "[Image]");
    }
}
