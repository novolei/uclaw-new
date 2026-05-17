//! WeCom Bot (企业微信智能机器人) WebSocket bidirectional channel.
//!
//! Protocol: wss://openws.work.weixin.qq.com
//! - aibot_subscribe {bot_id, secret} for auth
//! - aibot_msg_callback for inbound
//! - aibot_respond_msg {req_id} for passive reply (within REQ_ID_TTL)
//! - aibot_send_msg for proactive push (after TTL expiry)
//! - Heartbeat: {cmd:"ping"} every 30s
//! - Only one WebSocket connection per bot allowed

use crate::channels::types::{ChannelRuntimeStatus, ChannelState, ImChannelSender, InboundMessage, ReplyHandle, StreamingHandle};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::AbortHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const DEFAULT_WS_URL: &str = "wss://openws.work.weixin.qq.com";
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
const RECONNECT_BASE_MS: u64 = 2_000;
const RECONNECT_MAX_MS: u64 = 30_000;
const REQ_ID_TTL_SECS: i64 = 5 * 60; // 5 minutes
const MAX_RECONNECT_ATTEMPTS: u32 = 100;

/// Stored req_id entry for passive reply.
#[derive(Clone)]
struct ReqIdEntry {
    req_id: String,
    expires_at: DateTime<Utc>,
}

/// WecomSender: bidirectional WeCom Bot sender.
pub struct WecomSender {
    instance_id: String,
    bot_id: String,
    secret: String,
    ws_url: String,
    req_ids: Arc<RwLock<std::collections::HashMap<String, ReqIdEntry>>>,
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
    status_tx: mpsc::UnboundedSender<ChannelRuntimeStatus>,
}

impl WecomSender {
    pub fn new(
        instance_id: &str,
        config: &Value,
        credentials: &Value,
        status_tx: mpsc::UnboundedSender<ChannelRuntimeStatus>,
    ) -> Self {
        let bot_id = credentials["bot_id"].as_str().unwrap_or("").to_string();
        let secret = credentials["secret"].as_str().unwrap_or("").to_string();
        let ws_url = config["ws_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_WS_URL)
            .to_string();
        Self {
            instance_id: instance_id.to_string(),
            bot_id,
            secret,
            ws_url,
            req_ids: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tx: Arc::new(Mutex::new(None)),
            status_tx,
        }
    }

    /// Start the WebSocket connection loop in a background tokio task.
    pub fn start(
        self: Arc<Self>,
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) -> AbortHandle {
        let sender = self.clone();
        let handle = tokio::spawn(async move {
            sender.connection_loop(inbound_tx).await;
        });
        handle.abort_handle()
    }

    async fn connection_loop(
        &self,
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) {
        let mut attempt = 0u32;
        loop {
            if attempt >= MAX_RECONNECT_ATTEMPTS {
                tracing::error!(
                    "[WecomBot:{}] giving up after {MAX_RECONNECT_ATTEMPTS} reconnect attempts",
                    self.instance_id
                );
                let _ = self.status_tx.send(ChannelRuntimeStatus {
                    instance_id: self.instance_id.clone(),
                    state: ChannelState::Offline,
                    last_error: Some(format!("已达最大重连次数 ({MAX_RECONNECT_ATTEMPTS})")),
                    connected_since_ms: None,
                    message_count_today: 0,
                });
                break;
            }
            match self.connect_and_run(inbound_tx.clone()).await {
                Ok(()) => break,
                Err(e) => {
                    let _ = self.status_tx.send(ChannelRuntimeStatus {
                        instance_id: self.instance_id.clone(),
                        state: ChannelState::Error,
                        last_error: Some(e.to_string()),
                        connected_since_ms: None,
                        message_count_today: 0,
                    });
                    tracing::warn!(
                        "[WecomBot:{}] connection error: {e}; reconnecting (attempt {attempt}/{})",
                        self.instance_id,
                        MAX_RECONNECT_ATTEMPTS
                    );
                }
            }
            let delay = std::cmp::min(
                RECONNECT_BASE_MS * 2u64.saturating_pow(attempt),
                RECONNECT_MAX_MS,
            );
            attempt += 1;
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
    }

    async fn connect_and_run(
        &self,
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.ws_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let sub_msg = json!({
            "cmd": "aibot_subscribe",
            "headers": { "req_id": format!("sub_{}", Utc::now().timestamp_millis()) },
            "body": { "bot_id": self.bot_id, "secret": self.secret }
        });
        write
            .send(Message::Text(sub_msg.to_string()))
            .await?;
        // Emit Online status — subscribe sent; treat as connected.
        let _ = self.status_tx.send(ChannelRuntimeStatus {
            instance_id: self.instance_id.clone(),
            state: ChannelState::Online,
            last_error: None,
            connected_since_ms: Some(chrono::Utc::now().timestamp_millis()),
            message_count_today: 0,
        });

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
        *self.tx.lock().await = Some(out_tx);

        let hb_tx = {
            let guard = self.tx.lock().await;
            guard.clone()
        };
        let instance_id = self.instance_id.clone();
        let hb_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
                if let Some(ref tx) = hb_tx {
                    let ping = json!({
                        "cmd": "ping",
                        "headers": { "req_id": format!("ping_{}", Utc::now().timestamp_millis()) }
                    });
                    let _ = tx.send(Message::Text(ping.to_string()));
                } else {
                    break;
                }
                tracing::trace!("[WecomBot:{instance_id}] heartbeat sent");
            }
        });

        let result = async {
            loop {
                tokio::select! {
                    msg = read.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Err(e) = self.handle_message(&text, &inbound_tx).await {
                                    tracing::warn!(
                                        "[WecomBot:{}] message handler error: {e}",
                                        self.instance_id
                                    );
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                tracing::info!(
                                    "[WecomBot:{}] WebSocket closed",
                                    self.instance_id
                                );
                                *self.tx.lock().await = None;
                                return Err(anyhow!("WebSocket closed"));
                            }
                            Some(Ok(_)) => {}
                            Some(Err(e)) => {
                                *self.tx.lock().await = None;
                                return Err(e.into());
                            }
                        }
                    }
                    Some(out) = out_rx.recv() => {
                        write.send(out).await?;
                    }
                }
            }
        }
        .await;

        hb_handle.abort();
        result
    }

    async fn handle_message(
        &self,
        text: &str,
        inbound_tx: &mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) -> Result<()> {
        let msg: Value = serde_json::from_str(text)?;
        let cmd = msg["cmd"].as_str().unwrap_or("");

        if cmd == "aibot_msg_callback" {
            let body = &msg["body"];
            let req_id = msg["headers"]["req_id"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let sender_id = body["from"]["userid"].as_str().unwrap_or("").to_string();
            let sender_name = body["from"]["name"].as_str().map(String::from);
            let chat_id = body["chatid"]
                .as_str()
                .unwrap_or(&sender_id)
                .to_string();

            if chat_id.is_empty() {
                return Ok(());
            }

            if !req_id.is_empty() {
                let expires_at = Utc::now() + chrono::Duration::seconds(REQ_ID_TTL_SECS);
                self.req_ids.write().await.insert(
                    chat_id.clone(),
                    ReqIdEntry {
                        req_id: req_id.clone(),
                        expires_at,
                    },
                );
            }

            let text_content = Self::extract_text_from_body(body);
            tracing::info!(
                "[WecomBot:{}] inbound from={sender_id} chat={chat_id} len={}",
                self.instance_id,
                text_content.len()
            );

            let inbound = InboundMessage {
                instance_id: self.instance_id.clone(),
                chat_id: chat_id.clone(),
                sender_name,
                text: text_content,
                timestamp: Utc::now().timestamp_millis(),
                channel_ctx: Some(json!({
                    "req_id": req_id,
                    "expires_at": Utc::now().timestamp_millis() + REQ_ID_TTL_SECS * 1000
                })),
            };

            let sender_arc: Arc<dyn ImChannelSender> = Arc::new(WecomReplySender {
                tx: self.tx.clone(),
                req_ids: self.req_ids.clone(),
                chat_id: chat_id.clone(),
                instance_id: self.instance_id.clone(),
            });
            let reply = Arc::new(ReplyHandle {
                sender: sender_arc,
                channel_ctx: inbound.channel_ctx.clone(),
                chat_id: chat_id.clone(),
            });

            let _ = inbound_tx.send((inbound, reply));
        }
        Ok(())
    }

    pub fn extract_text_from_body(body: &Value) -> String {
        match body["msgtype"].as_str().unwrap_or("") {
            "text" => body["text"]["content"].as_str().unwrap_or("").to_string(),
            "image" => "(image)".to_string(),
            "voice" => "(voice message)".to_string(),
            "file" => format!(
                "(file: {})",
                body["file"]["filename"].as_str().unwrap_or("unknown")
            ),
            "video" => "(video)".to_string(),
            other => format!("({})", other),
        }
    }

    pub fn is_req_id_expired(expires_at: DateTime<Utc>) -> bool {
        Utc::now() > expires_at
    }
}

struct WecomReplySender {
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
    req_ids: Arc<RwLock<std::collections::HashMap<String, ReqIdEntry>>>,
    chat_id: String,
    instance_id: String,
}

#[async_trait]
impl ImChannelSender for WecomReplySender {
    async fn send_text(&self, _chat_id: &str, text: &str, ctx: Option<&Value>) -> Result<(), String> {
        let guard = self.tx.lock().await;
        let tx = guard
            .as_ref()
            .ok_or_else(|| "WecomBot: WebSocket not connected".to_string())?;

        let req_id = ctx
            .and_then(|c| c["req_id"].as_str())
            .map(String::from);
        let req_expires_at: Option<i64> = ctx.and_then(|c| c["expires_at"].as_i64());

        let use_passive_reply = req_id.is_some()
            && req_expires_at
                .map(|ts| ts > Utc::now().timestamp_millis())
                .unwrap_or(false);

        let msg = if use_passive_reply {
            json!({
                "cmd": "aibot_respond_msg",
                "headers": { "req_id": req_id.unwrap() },
                "body": {
                    "msgtype": "markdown",
                    "markdown": { "content": text }
                }
            })
        } else {
            tracing::info!(
                "[WecomBot:{}] req_id expired for chat {}, using proactive push",
                self.instance_id,
                self.chat_id
            );
            json!({
                "cmd": "aibot_send_msg",
                "headers": { "req_id": format!("push_{}", Utc::now().timestamp_millis()) },
                "body": {
                    "chatid": self.chat_id,
                    "chat_type": 1,
                    "msgtype": "markdown",
                    "markdown": { "content": text }
                }
            })
        };

        tx.send(Message::Text(msg.to_string()))
            .map_err(|e| format!("WecomBot send error: {e}"))?;
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        true
    }
}

/// WeCom streaming handle. Sends incremental stream packets via the WebSocket.
pub struct WecomStreamingHandle {
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
    req_id: String,
    stream_id: String,
    instance_id: String,
}

impl WecomStreamingHandle {
    pub fn new(
        tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
        req_id: &str,
        instance_id: &str,
    ) -> Self {
        Self {
            tx,
            req_id: req_id.to_string(),
            stream_id: format!("stream_{}", Utc::now().timestamp_millis()),
            instance_id: instance_id.to_string(),
        }
    }

    async fn send_packet(&self, content: &str, finish: bool) -> Result<()> {
        let guard = self.tx.lock().await;
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("WecomStreamingHandle: WebSocket not connected"))?;
        let packet = json!({
            "cmd": "aibot_respond_msg",
            "headers": { "req_id": self.req_id },
            "body": {
                "msgtype": "stream",
                "stream": {
                    "id": self.stream_id,
                    "finish": finish,
                    "content": content
                }
            }
        });
        tx.send(Message::Text(packet.to_string()))
            .map_err(|e| anyhow!("WecomStreamingHandle send error: {e}"))?;
        Ok(())
    }
}

#[async_trait]
impl StreamingHandle for WecomStreamingHandle {
    async fn update(&self, partial: &str) -> Result<()> {
        self.send_packet(partial, false).await
    }

    async fn finish(&self, final_text: &str) -> Result<()> {
        self.send_packet(final_text, true).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_returns_text_from_text_msg() {
        let body = serde_json::json!({
            "msgtype": "text",
            "text": { "content": "hello world" }
        });
        assert_eq!(WecomSender::extract_text_from_body(&body), "hello world");
    }

    #[test]
    fn extract_text_falls_back_for_image() {
        let body = serde_json::json!({ "msgtype": "image" });
        assert_eq!(WecomSender::extract_text_from_body(&body), "(image)");
    }

    #[test]
    fn req_id_ttl_check_recognizes_expired() {
        let expires_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(WecomSender::is_req_id_expired(expires_at));
    }

    #[test]
    fn req_id_ttl_check_recognizes_valid() {
        let expires_at = Utc::now() + chrono::Duration::seconds(60);
        assert!(!WecomSender::is_req_id_expired(expires_at));
    }

    #[test]
    fn wecom_sender_new_accepts_status_tx() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<ChannelRuntimeStatus>();
        let _sender = WecomSender::new(
            "inst-test",
            &serde_json::json!({}),
            &serde_json::json!({"bot_id": "b1", "secret": "s1"}),
            tx,
        );
        // Constructor accepted status_tx without panicking.
    }
}
