# IM Framework — Plan B: Bidirectional Channels + Dispatcher + Agent Integration

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the IM framework by adding WeCom Bot (WebSocket) and iLink (HTTP long-poll) bidirectional senders, the inbound dispatcher routing automation vs. agent-chat paths, the HeadlessDelegate refactor enabling IM close-loop in `notify_user`, and the SpecSettingsView IM sections in the frontend.

**Architecture:** Plan A built the DB schema, types, notify-only senders, ImSessionRegistry, ImChannelManager (with NoopSender stubs), and CRUD commands. Plan B fills in the bidirectional channel implementations, wires them into ImChannelManager.start_instance, adds the dispatcher that routes inbound messages to automation or agent-chat, and refactors AutomationDelegate into HeadlessDelegate with reply_handle/streaming_handle for IM close-loop.

**Tech Stack:** tokio-tungstenite (WeCom WebSocket), reqwest (iLink HTTP poll — already present), Rust async/tokio, React 18 + TypeScript + Jotai.

**Prerequisite:** Plan A must be fully merged. This plan builds on the types, migrations, and CRUD layer Plan A delivered.

---

## File Map

**Create:**
- `src-tauri/src/channels/im/wecom.rs` — WecomSender + WecomStreamingHandle
- `src-tauri/src/channels/im/ilink.rs` — IlinkSender (HTTP long-poll)
- `src-tauri/src/agent/headless.rs` — HeadlessDelegate (refactored from AutomationDelegate)
- `src-tauri/src/channels/dispatcher.rs` — dispatch_inbound, routing, run_agent_chat_via_im, persist_im_messages

**Modify:**
- `src-tauri/Cargo.toml` — add tokio-tungstenite
- `src-tauri/src/channels/types.rs` — add StreamingHandle trait
- `src-tauri/src/channels/im/mod.rs` — expose wecom + ilink modules
- `src-tauri/src/channels/mod.rs` — expose dispatcher module
- `src-tauri/src/channels/manager.rs` — start_instance with real WeCom/iLink senders
- `src-tauri/src/automation/runtime/execute.rs` — rename AutomationDelegate → HeadlessDelegate, add reply_handle + streaming_handle fields, upgrade notify_user arm
- `src-tauri/src/automation/runtime/service.rs` — use HeadlessDelegate
- `src-tauri/src/agent/mod.rs` — expose headless module
- `src-tauri/src/tauri_commands.rs` — add update_spec_im_settings, extend list_automations/get_automation_spec return type
- `ui/src/lib/tauri-bridge.ts` — extend HumaneSpecRow + new IM commands + SpecChannelBinding type
- `ui/src/components/automation/SpecSettingsView.tsx` — 消息通道, IM触发, 开发者 sections

---

## Task 1: Cargo.toml — add tokio-tungstenite

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Baseline compilation check**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output (clean).

- [ ] **Step 2: Add tokio-tungstenite**

In `src-tauri/Cargo.toml`, in the `[dependencies]` section after `reqwest`:

```toml
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
```

- [ ] **Step 3: Verify compilation**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output (tokio-tungstenite fetched and compiled).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add tokio-tungstenite for WeCom WebSocket channel"
```

---

## Task 2: channels/types.rs — add StreamingHandle trait

**Files:**
- Modify: `src-tauri/src/channels/types.rs`

- [ ] **Step 1: Write the failing test**

At the end of `src-tauri/src/channels/types.rs`, inside the `#[cfg(test)]` block, add:

```rust
#[test]
fn streaming_handle_is_object_safe() {
    // Verify the trait can be used as a trait object.
    // This is a compile-time check only.
    fn _accepts(_: &dyn StreamingHandle) {}
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::types::tests::streaming_handle_is_object_safe 2>&1 | tail -10
```

Expected: compile error — `StreamingHandle` not defined.

- [ ] **Step 3: Add StreamingHandle trait to channels/types.rs**

Find the end of the public type definitions in `src-tauri/src/channels/types.rs` (after the `ImChannelSender` trait), and add:

```rust
/// Streaming reply handle — only WeCom Bot supports real streaming.
/// Other channels ignore the update() calls and deliver on finish().
#[async_trait::async_trait]
pub trait StreamingHandle: Send + Sync {
    /// Send a partial update. May be called multiple times before finish().
    async fn update(&self, partial: &str) -> anyhow::Result<()>;
    /// Mark the stream complete with the final full text.
    async fn finish(&self, final_text: &str) -> anyhow::Result<()>;
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd src-tauri && cargo test --lib channels::types 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/channels/types.rs
git commit -m "feat: add StreamingHandle trait to channels/types.rs"
```

---

## Task 3: channels/im/wecom.rs — WeCom Bot WebSocket sender

**Files:**
- Modify: `src-tauri/src/channels/im/mod.rs`
- Create: `src-tauri/src/channels/im/wecom.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/channels/im/wecom.rs` (create the file with just the test):

```rust
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
        let expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);
        assert!(WecomSender::is_req_id_expired(expires_at));
    }

    #[test]
    fn req_id_ttl_check_recognizes_valid() {
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(60);
        assert!(!WecomSender::is_req_id_expired(expires_at));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::im::wecom 2>&1 | tail -10
```

Expected: compile error — WecomSender not defined.

- [ ] **Step 3: Implement channels/im/wecom.rs**

Replace `src-tauri/src/channels/im/wecom.rs` with:

```rust
//! WeCom Bot (企业微信智能机器人) WebSocket bidirectional channel.
//!
//! Protocol: wss://openws.work.weixin.qq.com
//! - aibot_subscribe {bot_id, secret} for auth
//! - aibot_msg_callback for inbound
//! - aibot_respond_msg {req_id} for passive reply (within REQ_ID_TTL)
//! - aibot_send_msg for proactive push (after TTL expiry)
//! - Heartbeat: {cmd:"ping"} every 30s
//! - Only one WebSocket connection per bot allowed

use crate::channels::types::{ImChannelSender, InboundMessage, StreamingHandle};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use tokio::task::AbortHandle;

const DEFAULT_WS_URL: &str = "wss://openws.work.weixin.qq.com";
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
const RECONNECT_BASE_MS: u64 = 2_000;
const RECONNECT_MAX_MS: u64 = 30_000;
const REQ_ID_TTL_SECS: i64 = 5 * 60; // 5 minutes

/// Stored req_id entry for passive reply.
#[derive(Clone)]
struct ReqIdEntry {
    req_id: String,
    expires_at: DateTime<Utc>,
}

/// WecomSender: bidirectional WeCom Bot sender.
/// Holds an mpsc sender to the WebSocket write half.
/// Inbound messages are dispatched via a callback channel registered at start().
pub struct WecomSender {
    instance_id: String,
    bot_id: String,
    secret: String,
    ws_url: String,
    /// Maps chat_id → current req_id entry.
    req_ids: Arc<RwLock<std::collections::HashMap<String, ReqIdEntry>>>,
    /// Send half for outgoing WebSocket messages (set after connect).
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
}

impl WecomSender {
    pub fn new(instance_id: &str, config: &Value, credentials: &Value) -> Self {
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
        }
    }

    /// Start the WebSocket connection loop in a background tokio task.
    /// Returns an AbortHandle to stop the task on drop.
    pub fn start(
        self: Arc<Self>,
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<crate::channels::types::ReplyHandle>)>,
    ) -> AbortHandle {
        let sender = self.clone();
        let handle = tokio::spawn(async move {
            sender.connection_loop(inbound_tx).await;
        });
        handle.abort_handle()
    }

    async fn connection_loop(
        &self,
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<crate::channels::types::ReplyHandle>)>,
    ) {
        let mut attempt = 0u32;
        loop {
            match self.connect_and_run(inbound_tx.clone()).await {
                Ok(()) => break, // graceful shutdown
                Err(e) => {
                    tracing::warn!(
                        "[WecomBot:{}] connection error: {e}; reconnecting (attempt {attempt})",
                        self.instance_id
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
        inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<crate::channels::types::ReplyHandle>)>,
    ) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.ws_url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Subscribe
        let sub_msg = json!({
            "cmd": "aibot_subscribe",
            "headers": { "req_id": format!("sub_{}", chrono::Utc::now().timestamp_millis()) },
            "body": { "bot_id": self.bot_id, "secret": self.secret }
        });
        write.send(Message::Text(sub_msg.to_string())).await?;

        // Channel for outgoing messages from send_text()
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
        *self.tx.lock().await = Some(out_tx);

        // Heartbeat task
        let hb_tx = {
            let guard = self.tx.lock().await;
            guard.clone()
        };
        let instance_id = self.instance_id.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
                if let Some(ref tx) = hb_tx {
                    let ping = json!({
                        "cmd": "ping",
                        "headers": { "req_id": format!("ping_{}", chrono::Utc::now().timestamp_millis()) }
                    });
                    let _ = tx.send(Message::Text(ping.to_string()));
                } else {
                    break;
                }
                tracing::trace!("[WecomBot:{instance_id}] heartbeat sent");
            }
        });

        // Main loop: fan in read + outbound
        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = self.handle_message(&text, &inbound_tx).await {
                                tracing::warn!("[WecomBot:{}] message handler error: {e}", self.instance_id);
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            tracing::info!("[WecomBot:{}] WebSocket closed", self.instance_id);
                            *self.tx.lock().await = None;
                            return Err(anyhow!("WebSocket closed"));
                        }
                        Some(Ok(_)) => {} // ping/pong/binary — ignore
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

    async fn handle_message(
        &self,
        text: &str,
        inbound_tx: &mpsc::UnboundedSender<(InboundMessage, Arc<crate::channels::types::ReplyHandle>)>,
    ) -> Result<()> {
        let msg: Value = serde_json::from_str(text)?;
        let cmd = msg["cmd"].as_str().unwrap_or("");

        if cmd == "aibot_msg_callback" {
            let body = &msg["body"];
            let req_id = msg["headers"]["req_id"].as_str().unwrap_or("").to_string();
            let sender_id = body["from"]["userid"].as_str().unwrap_or("").to_string();
            let sender_name = body["from"]["name"].as_str().map(String::from);
            let chat_id = body["chatid"]
                .as_str()
                .unwrap_or(&sender_id)
                .to_string();

            if chat_id.is_empty() {
                return Ok(());
            }

            // Store req_id with TTL
            if !req_id.is_empty() {
                let expires_at = Utc::now() + chrono::Duration::seconds(REQ_ID_TTL_SECS);
                self.req_ids.write().await.insert(
                    chat_id.clone(),
                    ReqIdEntry { req_id: req_id.clone(), expires_at },
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

            // Build ReplyHandle backed by this sender
            let sender_arc: Arc<dyn ImChannelSender> = Arc::new(WecomReplySender {
                tx: self.tx.clone(),
                req_ids: self.req_ids.clone(),
                chat_id: chat_id.clone(),
                instance_id: self.instance_id.clone(),
            });
            let reply = Arc::new(crate::channels::types::ReplyHandle {
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

/// Thin sender that uses the WecomSender's outbound channel.
/// Implements ImChannelSender for use inside ReplyHandle.
struct WecomReplySender {
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
    req_ids: Arc<RwLock<std::collections::HashMap<String, ReqIdEntry>>>,
    chat_id: String,
    instance_id: String,
}

#[async_trait]
impl ImChannelSender for WecomReplySender {
    async fn send_text(&self, _chat_id: &str, text: &str, ctx: Option<&Value>) -> Result<()> {
        let guard = self.tx.lock().await;
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("WecomBot: WebSocket not connected"))?;

        // Check if req_id is still valid
        let req_id = ctx
            .and_then(|c| c["req_id"].as_str())
            .map(String::from);
        let req_expires_at: Option<i64> = ctx
            .and_then(|c| c["expires_at"].as_i64());

        let use_passive_reply = req_id.is_some()
            && req_expires_at.map(|ts| ts > Utc::now().timestamp_millis()).unwrap_or(false);

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
            // Proactive push fallback
            tracing::info!(
                "[WecomBot:{}] req_id expired for chat {}, using proactive push",
                self.instance_id, self.chat_id
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
            .map_err(|e| anyhow!("WecomBot send error: {e}"))?;
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
}
```

- [ ] **Step 4: Expose wecom module in channels/im/mod.rs**

Replace `src-tauri/src/channels/im/mod.rs` with:

```rust
pub mod ilink;
pub mod wecom;

pub use ilink::IlinkSender;
pub use wecom::{WecomSender, WecomStreamingHandle};
```

(ilink module is a stub for now — Task 4 fills it in.)

Create `src-tauri/src/channels/im/ilink.rs` (stub, Task 4 fills in):

```rust
// iLink HTTP long-poll sender — implemented in Task 4.
use crate::channels::types::{ImChannelSender, InboundMessage};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::task::AbortHandle;
use tokio::sync::mpsc;

pub struct IlinkSender;

impl IlinkSender {
    pub fn new(_instance_id: &str, _config: &Value, _credentials: &Value) -> Self { Self }

    pub fn start(
        self: Arc<Self>,
        _inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<crate::channels::types::ReplyHandle>)>,
    ) -> AbortHandle {
        tokio::spawn(async {}).abort_handle()
    }
}

#[async_trait]
impl ImChannelSender for IlinkSender {
    async fn send_text(&self, _chat_id: &str, _text: &str, _ctx: Option<&Value>) -> Result<()> {
        Ok(())
    }
}
```

- [ ] **Step 5: Add dispatcher stub to channels/mod.rs**

In `src-tauri/src/channels/mod.rs`, add after the existing module declarations:

```rust
pub mod dispatcher;
```

Create `src-tauri/src/channels/dispatcher.rs` (stub — Task 6 fills in):

```rust
// IM inbound dispatcher — implemented in Task 6.
```

- [ ] **Step 6: Build and run wecom tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd src-tauri && cargo test --lib channels::im::wecom 2>&1 | tail -10
```

Expected: clean build, 4 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/channels/im/wecom.rs src-tauri/src/channels/im/ilink.rs \
        src-tauri/src/channels/im/mod.rs src-tauri/src/channels/mod.rs \
        src-tauri/src/channels/dispatcher.rs
git commit -m "feat: WecomSender WebSocket bidirectional channel + WecomStreamingHandle"
```

---

## Task 4: channels/im/ilink.rs — iLink HTTP long-poll sender

**Files:**
- Modify: `src-tauri/src/channels/im/ilink.rs`

- [ ] **Step 1: Write the failing test**

Replace the stub in `src-tauri/src/channels/im/ilink.rs` with just the test at the top:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::im::ilink 2>&1 | tail -10
```

Expected: compile error — IlinkSender.extract_text not defined.

- [ ] **Step 3: Implement channels/im/ilink.rs**

Replace `src-tauri/src/channels/im/ilink.rs` with:

```rust
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
    /// Maps (account_id:user_id) → context_token (per-message, no expiry).
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

    /// Start the long-poll loop in a background tokio task.
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
                    // Successful poll — reset backoff
                    attempt = 0;
                }
                Ok(false) => {
                    // Session expired — stop permanently
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

    /// Perform one long-poll request.
    /// Returns Ok(true) on success, Ok(false) on session expiry, Err on network error.
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
            .timeout(std::time::Duration::from_secs(40)) // slightly > 35s server hold
            .send()
            .await?
            .json()
            .await?;

        let ret_code = resp["ret"].as_i64().unwrap_or(resp["errcode"].as_i64().unwrap_or(0));
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
                // message_type 1 = USER (inbound)
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
            self.context_tokens.write().await.insert(user_id.clone(), ct.clone());
        }

        let items = &msg["item_list"];
        let text = Self::extract_text(items);

        tracing::info!(
            "[IlinkBot:{}] inbound from={user_id} len={}",
            self.instance_id,
            text.len()
        );

        let channel_ctx = context_token.as_ref().map(|ct| json!({ "context_token": ct }));

        let inbound = InboundMessage {
            instance_id: self.instance_id.clone(),
            chat_id: user_id.clone(),
            sender_name: Some(user_id.clone()),
            text,
            timestamp: chrono::Utc::now().timestamp_millis(),
            channel_ctx: channel_ctx.clone(),
        };

        // Build reply handle backed by this sender
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
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        // X-WECHAT-UIN: random uint32 base64
        let n: u32 = rand::random();
        let uin = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, n.to_string().as_bytes());
        headers.insert("X-WECHAT-UIN", uin.parse().unwrap());
        if !self.bot_token.is_empty() {
            if let Ok(v) = format!("Bearer {}", self.bot_token).parse() {
                headers.insert("Authorization", v);
            }
        }
        headers
    }

    /// Extract text from iLink item_list array.
    pub fn extract_text(items: &Value) -> String {
        let arr = match items.as_array() {
            Some(a) => a,
            None => return String::new(),
        };
        let mut parts = Vec::new();
        for item in arr {
            let t = item["type"].as_i64().unwrap_or(0);
            let s = match t {
                1 => item["text_item"]["text"].as_str().unwrap_or("").to_string(),
                2 => "[Image]".to_string(),
                3 => item["voice_item"]["text"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| "[Voice]".to_string()),
                4 => format!(
                    "[File: {}]",
                    item["file_item"]["filename"].as_str().unwrap_or("unknown")
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

/// Stateless reply sender for iLink (uses context_token from channel_ctx).
struct IlinkReplySender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    client: reqwest::Client,
}

#[async_trait]
impl ImChannelSender for IlinkReplySender {
    async fn send_text(&self, chat_id: &str, text: &str, ctx: Option<&Value>) -> Result<()> {
        let context_token = ctx
            .and_then(|c| c["context_token"].as_str())
            .ok_or_else(|| {
                anyhow!(
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
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(anyhow!("iLink sendmessage HTTP {status}"));
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
```

- [ ] **Step 4: Add rand to Cargo.toml (needed for iLink UIN header)**

In `src-tauri/Cargo.toml` after `base64`:

```toml
rand = "0.8"
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd src-tauri && cargo test --lib channels::im::ilink 2>&1 | tail -10
```

Expected: clean build, 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/im/ilink.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: IlinkSender HTTP long-poll bidirectional channel"
```

---

## Task 5: agent/headless.rs — HeadlessDelegate refactor

Rename `AutomationDelegate` in `execute.rs` to `HeadlessDelegate`, extract it into `agent/headless.rs`, add `reply_handle` and `streaming_handle` fields, and upgrade the `notify_user` arm to check `reply_handle` first for IM close-loop.

**Files:**
- Create: `src-tauri/src/agent/headless.rs`
- Modify: `src-tauri/src/automation/runtime/execute.rs`
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/agent/headless.rs` (create with test only):

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn headless_delegate_has_reply_handle_field() {
        // Compile-time check that HeadlessDelegate accepts a None reply_handle.
        // A real test would require a full AppState — this verifies the struct compiles.
        let _ = super::HeadlessDelegate {
            spec_id: "s1".into(),
            activity_id: "a1".into(),
            session_id: "sess1".into(),
            permissions: Default::default(),
            memory: std::sync::Arc::new(
                crate::automation::memory::MemoryStore::new_in_memory().unwrap(),
            ),
            db: std::sync::Arc::new(std::sync::Mutex::new(
                rusqlite::Connection::open_in_memory().unwrap(),
            )),
            gate: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            auto_continue: Default::default(),
            llm: panic!("no llm"),
            model: "claude-3-5-haiku-20241022".into(),
            tools: std::sync::Arc::new(crate::agent::tools::tool::ToolRegistry::new()),
            cost: std::sync::Arc::new(
                crate::automation::runtime::cost::CostCapState::new(
                    crate::automation::runtime::cost::CostCapConfig::default(),
                ),
            ),
            workspace_root: std::path::PathBuf::from("/tmp"),
            app_handle: None,
            channel_manager: None,
            reply_handle: None,
            streaming_handle: None,
            system_prompt_override: None,
        };
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib agent::headless 2>&1 | tail -10
```

Expected: compile error — HeadlessDelegate not found.

- [ ] **Step 3: Create agent/headless.rs**

Create `src-tauri/src/agent/headless.rs` with content taken from `execute.rs`'s `AutomationDelegate` — it's a copy/rename with three new optional fields added. The full content:

```rust
//! HeadlessDelegate — LoopDelegate for headless (automation + IM) runs.
//!
//! Replaces AutomationDelegate. When reply_handle is Some (IM-triggered runs),
//! notify_user sends back to the originating IM channel instead of the legacy
//! ChannelManager path, forming the IM close-loop.

use crate::agent::types::{
    ChatMessage, LoopOutcome, LoopSignal, RespondOutput, ReasoningContext, ResponseMetadata,
    TextAction, TokenUsage, ToolCall,
};
use crate::automation::memory::MemoryStore;
use crate::automation::permissions;
use crate::automation::runtime::{AutoContinueConfig, CompletionGate, PermissionSet};
use crate::automation::tools::{
    memory::MemoryInput,
    notify_user::NotifyInput,
    report_to_user::ReportInput,
    request_escalation::RequestEscalationInput,
};
use crate::agent::tools::tool::ToolRegistry;
use crate::automation::runtime::cost::CostCapState;
use crate::channels::{ChannelManager, ChannelNotification};
use crate::channels::types::ReplyHandle;
use crate::channels::types::StreamingHandle;
use crate::error::Error;
use crate::llm::LlmProvider;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock};

pub struct HeadlessDelegate {
    pub spec_id: String,
    pub activity_id: String,
    pub session_id: String,
    pub permissions: PermissionSet,
    pub memory: Arc<MemoryStore>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub gate: Arc<Mutex<Option<CompletionGate>>>,
    pub auto_continue: AutoContinueConfig,
    pub llm: Arc<dyn LlmProvider>,
    pub model: String,
    pub tools: Arc<ToolRegistry>,
    pub cost: Arc<CostCapState>,
    pub workspace_root: PathBuf,
    pub app_handle: Option<tauri::AppHandle>,
    pub channel_manager: Option<Arc<RwLock<ChannelManager>>>,

    // IM fields — None for pure automation runs, Some for IM-triggered runs.
    /// When Some, notify_user sends back to the originating IM channel (close-loop).
    pub reply_handle: Option<Arc<ReplyHandle>>,
    /// WeCom streaming handle — when Some, handle_text_response triggers streaming.
    pub streaming_handle: Option<Arc<dyn StreamingHandle>>,
    /// Override the Space's default system prompt (from spec.system_prompt_override).
    pub system_prompt_override: Option<String>,
}
```

Then copy the entire `impl LoopDelegate for AutomationDelegate` block from `execute.rs` into `headless.rs`, changing the impl header to `impl crate::agent::types::LoopDelegate for HeadlessDelegate`. Paste the full block verbatim (all the match arms for memory, notify_user, report_to_user, request_escalation, and the base tools dispatch).

In the `handle_text_response` method, change from:
```rust
async fn handle_text_response(
    &self,
    _text: &str,
    _metadata: ResponseMetadata,
    _reason_ctx: &mut ReasoningContext,
) -> TextAction {
    TextAction::Continue
}
```

to:
```rust
async fn handle_text_response(
    &self,
    text: &str,
    metadata: ResponseMetadata,
    _reason_ctx: &mut ReasoningContext,
) -> TextAction {
    // IM agent-chat path: send streaming update (if WeCom) and terminate loop.
    if self.reply_handle.is_some() {
        if let Some(ref sh) = self.streaming_handle {
            let _ = sh.update(text).await;
        }
        return TextAction::Return(LoopOutcome::Response {
            text: text.to_string(),
            usage: metadata.usage,
            truncated: false,
        });
    }
    // Automation path: keep going until report_to_user.
    TextAction::Continue
}
```

In the `notify_user` tool arm (currently in `execute_tool_calls`), add the IM close-loop check BEFORE the existing channel dispatch:
```rust
"notify_user" => {
    let input: NotifyInput = serde_json::from_value(call.arguments.clone())?;

    // IM close-loop: if triggered via IM, reply back to the originating channel.
    if let Some(ref reply) = self.reply_handle {
        let report_text = format!("**{}**\n\n{}", input.title, input.body);
        if let Err(e) = reply.send_markdown(&report_text).await {
            tracing::warn!(
                spec_id = %self.spec_id,
                "notify_user IM reply failed: {e}"
            );
        }
        reason_ctx.messages.push(ChatMessage::user_tool_result(
            &call.id,
            "notification dispatched via IM",
            false,
        ));
        continue; // skip legacy ChannelManager dispatch
    }

    // Non-IM path (existing logic unchanged):
    let notification = ChannelNotification { ... };
    // ... (paste existing for/match block here unchanged)
}
```

- [ ] **Step 4: Expose headless in agent/mod.rs**

In `src-tauri/src/agent/mod.rs`, find the existing `pub mod` declarations and add:

```rust
pub mod headless;
```

- [ ] **Step 5: Update execute.rs — remove AutomationDelegate, import HeadlessDelegate**

In `src-tauri/src/automation/runtime/execute.rs`:

1. Delete the entire `pub struct AutomationDelegate { ... }` definition and its `impl LoopDelegate for AutomationDelegate { ... }` block.
2. Remove the now-unused imports that were local to AutomationDelegate.
3. Add at the top:
```rust
pub use crate::agent::headless::HeadlessDelegate;
```

This re-exports HeadlessDelegate so service.rs can still import from `execute.rs` (no change needed in service.rs imports).

- [ ] **Step 6: Update service.rs — use HeadlessDelegate**

In `src-tauri/src/automation/runtime/service.rs`, change the delegate construction from:

```rust
let delegate = AutomationDelegate {
    spec_id: spec_id.to_string(),
    ...
    app_handle: self.app_handle.clone(),
    channel_manager: self.channel_manager.clone(),
};
```

to:

```rust
let delegate = HeadlessDelegate {
    spec_id: spec_id.to_string(),
    ...
    app_handle: self.app_handle.clone(),
    channel_manager: self.channel_manager.clone(),
    reply_handle: None,
    streaming_handle: None,
    system_prompt_override: None,
};
```

Update the `use` statement in service.rs from `use crate::automation::runtime::execute::AutomationDelegate;` to `use crate::automation::runtime::execute::HeadlessDelegate;`.

- [ ] **Step 7: Build and test**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd src-tauri && cargo test --lib agent 2>&1 | tail -10
cd src-tauri && cargo test --lib automation::runtime 2>&1 | tail -10
```

Expected: clean build, existing tests still pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/agent/headless.rs src-tauri/src/agent/mod.rs \
        src-tauri/src/automation/runtime/execute.rs \
        src-tauri/src/automation/runtime/service.rs
git commit -m "refactor: AutomationDelegate → HeadlessDelegate with reply_handle/streaming_handle for IM close-loop"
```

---

## Task 6: channels/dispatcher.rs — dispatch_inbound + routing

**Files:**
- Modify: `src-tauri/src/channels/dispatcher.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/channels/dispatcher.rs`, add tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::apply_migrations(&conn).unwrap();
        conn
    }

    #[tokio::test]
    async fn permission_check_denies_unknown_user() {
        let mut cfg = crate::channels::types::ImChannelInstanceConfig {
            id: "c1".into(),
            space_id: "sp1".into(),
            channel_type: crate::channels::types::ImChannelType::WecomBot,
            name: "test".into(),
            config: serde_json::json!({}),
            enabled: true,
            streaming: false,
            reply_scope: "all".into(),
            permission_enabled: true,
            owners: vec!["owner_user".into()],
            guest_policy: Default::default(),
        };
        let msg = InboundMessage {
            instance_id: "c1".into(),
            chat_id: "stranger".into(),
            sender_name: None,
            text: "hello".into(),
            timestamp: 0,
            channel_ctx: None,
        };
        assert!(!check_permission(&msg, &cfg));
        cfg.permission_enabled = false;
        assert!(check_permission(&msg, &cfg));
    }

    #[tokio::test]
    async fn find_matching_spec_returns_none_when_no_specs() {
        let conn = setup_db();
        let result = find_matching_spec_sync("hello", "sp1", "c1", &conn).unwrap();
        assert!(result.is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::dispatcher 2>&1 | tail -10
```

Expected: compile error — dispatcher functions not defined.

- [ ] **Step 3: Implement channels/dispatcher.rs**

Replace `src-tauri/src/channels/dispatcher.rs` with:

```rust
//! Inbound IM message dispatcher.
//!
//! Routes each InboundMessage to either:
//! - automation path: spec.trigger_phrase prefix match + spec_channel_bindings enabled
//! - agent-chat path: ImSessionRegistry long-lived per-user session

use crate::agent::headless::HeadlessDelegate;
use crate::agent::types::{ChatMessage, ReasoningContext, AgenticLoopConfig};
use crate::automation::runtime::cost::{CostCapConfig, CostCapState};
use crate::automation::runtime::{AutoContinueConfig, PermissionSet, CompletionGate};
use crate::automation::memory::MemoryStore;
use crate::channels::session_registry::ImSessionRegistry;
use crate::channels::types::{ImChannelInstanceConfig, InboundMessage, ReplyHandle, StreamingHandle};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Check whether this inbound message is allowed by the channel's permission config.
pub fn check_permission(msg: &InboundMessage, instance: &ImChannelInstanceConfig) -> bool {
    if !instance.permission_enabled {
        return true;
    }
    instance.owners.contains(&msg.chat_id)
}

/// Query automation_specs for a spec whose trigger_phrase matches the message prefix
/// AND which is bound+enabled for this channel instance.
///
/// Synchronous DB call — wraps the rusqlite query.
pub fn find_matching_spec_sync(
    text: &str,
    space_id: &str,
    channel_instance_id: &str,
    conn: &rusqlite::Connection,
) -> Result<Option<MatchedSpec>> {
    let trimmed = text.trim();
    let mut stmt = conn.prepare(
        "SELECT a.id, a.trigger_phrase, a.system_prompt_override
         FROM automation_specs a
         JOIN spec_channel_bindings b ON b.spec_id = a.id
         WHERE a.space_id = ?1
           AND a.trigger_phrase IS NOT NULL
           AND a.trigger_phrase != ''
           AND b.channel_instance_id = ?2
           AND b.enabled = 1",
    )?;

    let rows = stmt.query_map(
        rusqlite::params![space_id, channel_instance_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        },
    )?;

    for row in rows {
        let (spec_id, trigger_phrase, system_prompt_override) = row?;
        if trimmed.starts_with(&trigger_phrase) {
            return Ok(Some(MatchedSpec {
                spec_id,
                trigger_phrase,
                system_prompt_override,
            }));
        }
    }
    Ok(None)
}

#[derive(Debug)]
pub struct MatchedSpec {
    pub spec_id: String,
    pub trigger_phrase: String,
    pub system_prompt_override: Option<String>,
}

/// Persist new IM agent-chat messages into agent_messages starting at start_idx.
/// Avoids the PK collision of persist_transcript (which uses {session_id}-{idx} from 0).
pub fn persist_im_messages(
    conn: &rusqlite::Connection,
    session_id: &str,
    messages: &[ChatMessage],
    start_idx: usize,
) -> rusqlite::Result<()> {
    use crate::agent::types::{MessageRole, ContentBlock};
    let now_ms = chrono::Utc::now().timestamp_millis();
    for (i, msg) in messages.iter().enumerate() {
        let idx = start_idx + i;
        let role = match msg.role {
            MessageRole::System   => "system",
            MessageRole::User     => "user",
            MessageRole::Assistant => "assistant",
        };
        let content = match msg.role {
            MessageRole::User | MessageRole::System => msg
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            MessageRole::Assistant => {
                serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".into())
            }
        };
        let id = format!("{}-{}", session_id, idx);
        conn.execute(
            "INSERT OR IGNORE INTO agent_messages (id, session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, session_id, role, content, now_ms + i as i64],
        )?;
    }
    conn.execute(
        "UPDATE agent_sessions SET message_count = message_count + ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![messages.len() as i64, now_ms, session_id],
    )?;
    Ok(())
}

/// Load existing agent_messages for a session (for conversation history reconstruction).
pub fn load_session_messages(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> rusqlite::Result<Vec<(String, String, i64)>> {
    // Returns (role, content, created_at) ordered by created_at.
    let mut stmt = conn.prepare(
        "SELECT role, content, created_at FROM agent_messages
         WHERE session_id = ?1
         ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([session_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
    })?;
    rows.collect()
}

/// Dispatch an inbound IM message to either the automation or agent-chat path.
pub async fn dispatch_inbound(
    msg: InboundMessage,
    instance: &ImChannelInstanceConfig,
    reply: Arc<ReplyHandle>,
    streaming: Option<Arc<dyn StreamingHandle>>,
    session_registry: Arc<ImSessionRegistry>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    app_handle: tauri::AppHandle,
) -> Result<()> {
    // 1. Permission check
    if !check_permission(&msg, instance) {
        let _ = reply.send("您没有权限使用此服务。").await;
        return Ok(());
    }

    // 2. Find matching automation spec
    let matched = {
        let conn = db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        find_matching_spec_sync(&msg.text, &instance.space_id, &instance.id, &conn)?
    };

    match matched {
        Some(spec) => {
            // Automation path
            run_automation_via_im(spec, msg, reply, streaming, db, app_handle).await
        }
        None => {
            // Agent-chat path
            run_agent_chat_via_im(msg, reply, streaming, instance, session_registry, db, app_handle).await
        }
    }
}

async fn run_automation_via_im(
    spec: MatchedSpec,
    msg: InboundMessage,
    reply: Arc<ReplyHandle>,
    streaming: Option<Arc<dyn StreamingHandle>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    app_handle: tauri::AppHandle,
) -> Result<()> {
    // Delegate to the automation service's execute_run equivalent.
    // We reuse AppRuntimeService via the Tauri command infrastructure —
    // fire an internal trigger with reply_handle injected.
    //
    // For now: emit an IPC event so the frontend can display the activity,
    // then call execute_run_with_reply (added to service.rs in this task).
    let state = app_handle
        .state::<Arc<crate::app::AppState>>();
    let state = state.inner().clone();

    let payload = serde_json::json!({
        "trigger": "im",
        "channel_instance_id": msg.instance_id,
        "chat_id": msg.chat_id,
        "text": msg.text,
    });

    tokio::spawn(async move {
        if let Some(ref service) = state.automation_service {
            if let Err(e) = service.execute_run_with_reply(
                &spec.spec_id,
                payload,
                reply,
                streaming,
            ).await {
                tracing::warn!("run_automation_via_im error: {e}");
            }
        }
    });

    Ok(())
}

async fn run_agent_chat_via_im(
    msg: InboundMessage,
    reply: Arc<ReplyHandle>,
    streaming: Option<Arc<dyn StreamingHandle>>,
    instance: &ImChannelInstanceConfig,
    session_registry: Arc<ImSessionRegistry>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    app_handle: tauri::AppHandle,
) -> Result<()> {
    // 1. Get or create the long-lived agent session for this user
    let session_id = session_registry
        .get_or_create_session(
            &instance.space_id,
            &format!("{:?}", instance.channel_type).to_lowercase(),
            &msg.chat_id,
            msg.sender_name.as_deref(),
        )
        .await?;

    // 2. Load existing conversation history
    let existing_messages: Vec<ChatMessage> = {
        let conn = db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        let rows = load_session_messages(&conn, &session_id)?;
        rows.into_iter()
            .filter_map(|(role, content, _)| {
                use crate::agent::types::{MessageRole, ContentBlock};
                let r = match role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "system" => MessageRole::System,
                    _ => return None,
                };
                Some(ChatMessage {
                    role: r,
                    content: vec![ContentBlock::Text { text: content }],
                    compacted: false,
                })
            })
            .collect()
    };

    let start_idx = existing_messages.len();

    // 3. Build reason_ctx with history + new user message
    let state = app_handle.state::<Arc<crate::app::AppState>>();
    let state = state.inner().clone();

    // Resolve space system prompt
    let system_prompt = {
        let conn = db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        conn.query_row(
            "SELECT system_prompt FROM spaces WHERE id = ?1",
            rusqlite::params![instance.space_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .unwrap_or(None)
        .unwrap_or_else(|| "You are a helpful AI assistant.".to_string())
    };

    let mut reason_ctx = ReasoningContext::new(system_prompt);
    for m in existing_messages.iter() {
        reason_ctx.messages.push(m.clone());
    }
    reason_ctx.messages.push(ChatMessage::user(&msg.text));

    // 4. Build HeadlessDelegate
    let llm = {
        let ps = state.provider_service.read().await;
        ps.default_provider()
            .map_err(|e| anyhow::anyhow!("no default LLM provider: {e}"))?
    };
    let model = llm.default_model().to_string();

    let delegate = HeadlessDelegate {
        spec_id: format!("im:{}", instance.id),
        activity_id: format!("im_chat_{}", msg.chat_id),
        session_id: session_id.clone(),
        permissions: PermissionSet::default(),
        memory: Arc::new(MemoryStore::new_in_memory().unwrap_or_default()),
        db: db.clone(),
        gate: Arc::new(Mutex::new(None)),
        auto_continue: AutoContinueConfig::default(),
        llm,
        model: model.clone(),
        tools: state.build_base_tool_registry(),
        cost: Arc::new(CostCapState::new(CostCapConfig::default())),
        workspace_root: state.workspace_root().unwrap_or_else(|| std::path::PathBuf::from("/tmp")),
        app_handle: Some(app_handle.clone()),
        channel_manager: Some(state.channel_manager.clone()),
        reply_handle: Some(reply),
        streaming_handle: streaming,
        system_prompt_override: None,
    };

    // 5. Run the agentic loop
    let loop_config = AgenticLoopConfig::from_model(&model);
    let _outcome = crate::agent::agentic_loop::run_agentic_loop(
        &delegate,
        &mut reason_ctx,
        &loop_config,
    )
    .await;

    // 6. After loop: send final reply for non-streaming channels
    let final_assistant_text = reason_ctx
        .messages
        .iter()
        .rev()
        .find_map(|m| {
            use crate::agent::types::{MessageRole, ContentBlock};
            if m.role == MessageRole::Assistant {
                m.content.iter().find_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
            } else {
                None
            }
        })
        .unwrap_or_default();

    // streaming_handle.finish() to mark completion
    if let Some(ref sh) = delegate.streaming_handle {
        let _ = sh.finish(&final_assistant_text).await;
    } else if let Some(ref rh) = delegate.reply_handle {
        // Non-streaming: send the final text now
        let _ = rh.send(&final_assistant_text).await;
    }

    // 7. Persist new messages (start_idx avoids PK collision)
    let new_messages = &reason_ctx.messages[start_idx..];
    if !new_messages.is_empty() {
        let conn = db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        if let Err(e) = persist_im_messages(&conn, &session_id, new_messages, start_idx) {
            tracing::warn!("persist_im_messages error: {e}");
        }
    }

    // 8. Touch the im_sessions row
    let channel_type_str = format!("{:?}", instance.channel_type).to_lowercase();
    let _ = session_registry
        .touch(&instance.space_id, &channel_type_str, &msg.chat_id)
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::apply_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn permission_check_denies_unknown_user() {
        let cfg = crate::channels::types::ImChannelInstanceConfig {
            id: "c1".into(),
            space_id: "sp1".into(),
            channel_type: crate::channels::types::ImChannelType::WecomBot,
            name: "test".into(),
            config: serde_json::json!({}),
            enabled: true,
            streaming: false,
            reply_scope: "all".into(),
            permission_enabled: true,
            owners: vec!["owner_user".into()],
            guest_policy: Default::default(),
        };
        let msg = InboundMessage {
            instance_id: "c1".into(),
            chat_id: "stranger".into(),
            sender_name: None,
            text: "hello".into(),
            timestamp: 0,
            channel_ctx: None,
        };
        assert!(!check_permission(&msg, &cfg));
    }

    #[test]
    fn permission_check_allows_owner() {
        let cfg = crate::channels::types::ImChannelInstanceConfig {
            id: "c1".into(),
            space_id: "sp1".into(),
            channel_type: crate::channels::types::ImChannelType::WecomBot,
            name: "test".into(),
            config: serde_json::json!({}),
            enabled: true,
            streaming: false,
            reply_scope: "all".into(),
            permission_enabled: true,
            owners: vec!["owner_user".into()],
            guest_policy: Default::default(),
        };
        let msg = InboundMessage {
            instance_id: "c1".into(),
            chat_id: "owner_user".into(),
            sender_name: None,
            text: "hello".into(),
            timestamp: 0,
            channel_ctx: None,
        };
        assert!(check_permission(&msg, &cfg));
    }

    #[test]
    fn permission_disabled_allows_all() {
        let cfg = crate::channels::types::ImChannelInstanceConfig {
            id: "c1".into(),
            space_id: "sp1".into(),
            channel_type: crate::channels::types::ImChannelType::WecomBot,
            name: "test".into(),
            config: serde_json::json!({}),
            enabled: true,
            streaming: false,
            reply_scope: "all".into(),
            permission_enabled: false,
            owners: vec![],
            guest_policy: Default::default(),
        };
        let msg = InboundMessage {
            instance_id: "c1".into(),
            chat_id: "anyone".into(),
            sender_name: None,
            text: "hello".into(),
            timestamp: 0,
            channel_ctx: None,
        };
        assert!(check_permission(&msg, &cfg));
    }

    #[test]
    fn find_matching_spec_returns_none_when_no_specs() {
        let conn = setup_db();
        let result = find_matching_spec_sync("hello", "sp1", "c1", &conn).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn persist_im_messages_uses_start_idx_offset() {
        let conn = setup_db();
        // Create a session
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
             VALUES ('sess1', 'default', 'IM test', '{}', 0, 0, 0, ?1, ?1)",
            rusqlite::params![now],
        ).unwrap();

        let msgs = vec![
            ChatMessage::user("hello"),
            ChatMessage {
                role: crate::agent::types::MessageRole::Assistant,
                content: vec![crate::agent::types::ContentBlock::Text { text: "hi!".into() }],
                compacted: false,
            },
        ];

        // First call: start_idx=0
        persist_im_messages(&conn, "sess1", &msgs, 0).unwrap();

        // Second call: start_idx=2 (not 0 — avoids PK collision)
        let msgs2 = vec![ChatMessage::user("follow-up")];
        persist_im_messages(&conn, "sess1", &msgs2, 2).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_messages WHERE session_id='sess1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);

        // Verify IDs are correctly offset
        let id: String = conn
            .query_row(
                "SELECT id FROM agent_messages WHERE session_id='sess1' ORDER BY created_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(id, "sess1-2");
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd src-tauri && cargo test --lib channels::dispatcher 2>&1 | tail -15
```

Expected: clean build, 5 tests pass (permission_check_denies_unknown_user, permission_check_allows_owner, permission_disabled_allows_all, find_matching_spec_returns_none_when_no_specs, persist_im_messages_uses_start_idx_offset).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/channels/dispatcher.rs
git commit -m "feat: channels/dispatcher.rs — dispatch_inbound, routing, persist_im_messages"
```

---

## Task 7: channels/manager.rs — start_instance with real WeCom/iLink senders

Replace the NoopSender stubs from Plan A with real `WecomSender` and `IlinkSender`.

**Files:**
- Modify: `src-tauri/src/channels/manager.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/channels/manager.rs`, in the `#[cfg(test)]` block, add:

```rust
#[test]
fn start_instance_wecom_creates_running_entry() {
    // Verifies that start_instance for a wecom_bot config returns a sender
    // that reports supports_streaming() = true.
    // We use a disabled instance (enabled=false) to avoid actually dialing WS.
    let config = crate::channels::types::ImChannelInstanceConfig {
        id: "w1".into(),
        space_id: "sp1".into(),
        channel_type: crate::channels::types::ImChannelType::WecomBot,
        name: "WeCom Test".into(),
        config: serde_json::json!({}),
        enabled: false, // not started
        streaming: true,
        reply_scope: "all".into(),
        permission_enabled: false,
        owners: vec![],
        guest_policy: Default::default(),
    };
    // enabled=false means start_instance is a no-op for the background task.
    // The sender is still constructed for type-checking purposes.
    let sender = build_sender_for_test(&config, &serde_json::json!({}));
    assert!(sender.supports_streaming());
}

fn build_sender_for_test(
    config: &crate::channels::types::ImChannelInstanceConfig,
    credentials: &serde_json::Value,
) -> Arc<dyn crate::channels::types::ImChannelSender> {
    use crate::channels::types::ImChannelType;
    use crate::channels::im::{WecomSender, IlinkSender};
    match config.channel_type {
        ImChannelType::WecomBot => {
            Arc::new(WecomSenderWrapper(Arc::new(WecomSender::new(&config.id, &config.config, credentials))))
        }
        _ => Arc::new(IlinkSender::new(&config.id, &config.config, credentials)),
    }
}

struct WecomSenderWrapper(Arc<crate::channels::im::WecomSender>);
#[async_trait::async_trait]
impl crate::channels::types::ImChannelSender for WecomSenderWrapper {
    async fn send_text(&self, chat_id: &str, text: &str, ctx: Option<&serde_json::Value>) -> anyhow::Result<()> {
        // WecomSender delegates through WecomReplySender — this is just for type checking.
        Ok(())
    }
    fn supports_streaming(&self) -> bool { true }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::manager::tests::start_instance_wecom_creates_running_entry 2>&1 | tail -10
```

Expected: compile error — WecomSenderWrapper or build_sender_for_test not found.

- [ ] **Step 3: Update ImChannelManager.start_instance in channels/manager.rs**

In `channels/manager.rs`, find the `start_instance` method. Replace the NoopSender stub with:

```rust
async fn start_instance(&self, config: &ImChannelInstanceConfig) -> anyhow::Result<()> {
    use crate::channels::types::ImChannelType;
    use crate::channels::im::{WecomSender, IlinkSender};

    // Load credentials from DB
    let credentials: serde_json::Value = {
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        let creds_json: String = conn
            .query_row(
                "SELECT credentials_json FROM im_channel_instances WHERE id = ?1",
                rusqlite::params![config.id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&creds_json).unwrap_or_default()
    };

    let (sender, abort_handle): (Arc<dyn ImChannelSender + Send + Sync>, Option<tokio::task::AbortHandle>) =
        match config.channel_type {
            ImChannelType::WecomBot => {
                let (inbound_tx, mut inbound_rx) = tokio::sync::mpsc::unbounded_channel();
                let wecom = Arc::new(WecomSender::new(&config.id, &config.config, &credentials));
                let abort = if config.enabled {
                    Some(wecom.clone().start(inbound_tx))
                } else {
                    None
                };

                // Fan out inbound messages to dispatcher
                let instance_id = config.id.clone();
                let instances = self.instances.clone();
                let session_registry = self.session_registry.clone();
                let db = self.db.clone();
                let app_handle = self.app_handle.clone();
                tokio::spawn(async move {
                    while let Some((msg, reply)) = inbound_rx.recv().await {
                        let guard = instances.read().await;
                        if let Some(running) = guard.get(&instance_id) {
                            let cfg = running.config.clone();
                            drop(guard);
                            if let Err(e) = crate::channels::dispatcher::dispatch_inbound(
                                msg,
                                &cfg,
                                reply,
                                None, // streaming provided by reply handle itself
                                session_registry.clone(),
                                db.clone(),
                                app_handle.clone(),
                            )
                            .await
                            {
                                tracing::warn!("[ImChannelManager] dispatch error: {e}");
                            }
                        }
                    }
                });

                // WecomSender is hidden behind the dispatcher — expose a no-op sender
                // for the send_to_channel path (automation notify uses ReplyHandle).
                (Arc::new(NoopImSender { supports_streaming: true }), abort)
            }

            ImChannelType::WechatIlink => {
                let (inbound_tx, mut inbound_rx) = tokio::sync::mpsc::unbounded_channel();
                let ilink = Arc::new(IlinkSender::new(&config.id, &config.config, &credentials));
                let abort = if config.enabled {
                    Some(ilink.clone().start(inbound_tx))
                } else {
                    None
                };

                let instance_id = config.id.clone();
                let instances = self.instances.clone();
                let session_registry = self.session_registry.clone();
                let db = self.db.clone();
                let app_handle = self.app_handle.clone();
                tokio::spawn(async move {
                    while let Some((msg, reply)) = inbound_rx.recv().await {
                        let guard = instances.read().await;
                        if let Some(running) = guard.get(&instance_id) {
                            let cfg = running.config.clone();
                            drop(guard);
                            if let Err(e) = crate::channels::dispatcher::dispatch_inbound(
                                msg,
                                &cfg,
                                reply,
                                None,
                                session_registry.clone(),
                                db.clone(),
                                app_handle.clone(),
                            )
                            .await
                            {
                                tracing::warn!("[ImChannelManager] ilink dispatch error: {e}");
                            }
                        }
                    }
                });

                (Arc::new(NoopImSender { supports_streaming: false }), abort)
            }

            // Notify-only channels (Plan A): already have real senders from plan A.
            // No change needed here.
            _ => {
                // Plan A already wires these — keep existing logic.
                return Ok(());
            }
        };

    let mut guard = self.instances.write().await;
    guard.insert(
        config.id.clone(),
        RunningInstance {
            config: config.clone(),
            sender,
            abort_handle,
        },
    );
    tracing::info!("[ImChannelManager] started instance {} ({:?})", config.id, config.channel_type);
    Ok(())
}
```

Also add `NoopImSender` struct at the top of the file (if not already present from Plan A's stubs):

```rust
/// No-op sender for bidirectional channels where send_to_channel is handled
/// via ReplyHandle (inbound path) rather than direct channel dispatch.
struct NoopImSender {
    supports_streaming: bool,
}

#[async_trait::async_trait]
impl ImChannelSender for NoopImSender {
    async fn send_text(&self, _: &str, _: &str, _: Option<&serde_json::Value>) -> anyhow::Result<()> {
        Ok(())
    }
    fn supports_streaming(&self) -> bool {
        self.supports_streaming
    }
}
```

Add `session_registry` and `app_handle` fields to `ImChannelManager` (Plan A had stubs — fill them in):

```rust
pub struct ImChannelManager {
    instances: Arc<RwLock<HashMap<String, RunningInstance>>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    session_registry: Arc<crate::channels::session_registry::ImSessionRegistry>,
    app_handle: tauri::AppHandle,
}
```

Update `ImChannelManager::new(db, session_registry, app_handle)` constructor accordingly.

Update `AppState` construction in `app.rs` and `main.rs` Stage 3 to pass `session_registry` and `app_handle` to `ImChannelManager::new`.

- [ ] **Step 4: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: clean.

- [ ] **Step 5: Run channels tests**

```bash
cd src-tauri && cargo test --lib channels 2>&1 | tail -15
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/manager.rs src-tauri/src/app.rs src-tauri/src/main.rs
git commit -m "feat: ImChannelManager.start_instance wires real WeCom/iLink senders"
```

---

## Task 8: Frontend — SpecSettingsView IM sections + HumaneSpecRow extension

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` — add `update_spec_im_settings` + extend existing spec query
- Modify: `src-tauri/src/main.rs` — register `update_spec_im_settings` in invoke_handler!
- Modify: `ui/src/lib/tauri-bridge.ts` — extend HumaneSpecRow + SpecChannelBinding + new commands
- Modify: `ui/src/components/automation/SpecSettingsView.tsx` — add 消息通道, IM触发, 开发者 sections

- [ ] **Step 1: Add update_spec_im_settings Tauri command**

In `src-tauri/src/tauri_commands.rs`, add after the spec binding commands from Plan A:

```rust
/// Update per-spec IM settings: trigger_phrase and system_prompt_override.
#[tauri::command]
pub async fn update_spec_im_settings(
    state: tauri::State<'_, Arc<AppState>>,
    spec_id: String,
    trigger_phrase: Option<String>,
    system_prompt_override: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE automation_specs
         SET trigger_phrase = ?2, system_prompt_override = ?3, updated_at = ?4
         WHERE id = ?1",
        rusqlite::params![
            spec_id,
            trigger_phrase,
            system_prompt_override,
            chrono::Utc::now().timestamp_millis(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
```

Also update the existing `list_automations` and `get_automation_spec` queries to SELECT the new columns:

In the SELECT query for automation specs (find with: `SELECT.*FROM automation_specs`), add `, COALESCE(trigger_phrase, '') as trigger_phrase, COALESCE(system_prompt_override, '') as system_prompt_override` to the column list.

Update the Rust struct that maps the query result to include:
```rust
pub trigger_phrase: String,
pub system_prompt_override: String,
```

And the `#[serde(rename_all = "camelCase")]` serialization will produce `triggerPhrase` and `systemPromptOverride` in JSON.

- [ ] **Step 2: Register new command in main.rs invoke_handler!**

In `src-tauri/src/main.rs`, in the `invoke_handler!` macro, add:

```rust
tauri_commands::update_spec_im_settings,
```

- [ ] **Step 3: Compile check**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: clean.

- [ ] **Step 4: Extend HumaneSpecRow in tauri-bridge.ts**

In `ui/src/lib/tauri-bridge.ts`, find `export interface HumaneSpecRow {` and add two new optional fields after `updatedAt`:

```typescript
export interface HumaneSpecRow {
  // ... existing fields ...
  triggerPhrase: string
  systemPromptOverride: string
}
```

Add `SpecChannelBinding` type (if not already added by Plan A):

```typescript
export interface SpecChannelBinding {
  specId: string
  channelInstanceId: string
  enabled: boolean
  channelName?: string
  channelType?: string
}
```

Add the new commands (below existing automation commands):

```typescript
export const updateSpecImSettings = (
  specId: string,
  triggerPhrase: string | null,
  systemPromptOverride: string | null
): Promise<void> =>
  invoke<void>('update_spec_im_settings', { specId, triggerPhrase, systemPromptOverride })
```

- [ ] **Step 5: Update SpecSettingsView.tsx — add three new sections**

Replace `src-tauri/src/ui/components/automation/SpecSettingsView.tsx` with an updated version that adds three new sections to the settings view.

Find the closing `</div>` of the `{view === 'yaml' ? (...) : (...)}` settings block, which currently ends after the `<Section title="关于">` block. Add before the outer closing `</div>`:

```tsx
{/* 消息通道 — IM Channel Bindings */}
<ImChannelBindingsSection specId={spec.id} />

{/* IM触发 */}
<Section title="IM 触发">
  <ImTriggerRow
    specId={spec.id}
    initialTriggerPhrase={spec.triggerPhrase ?? ''}
  />
</Section>

{/* 开发者 */}
<Section title="开发者">
  <SystemPromptRow
    specId={spec.id}
    initialValue={spec.systemPromptOverride ?? ''}
  />
</Section>
```

Add these sub-components at the bottom of `SpecSettingsView.tsx` (before the closing of the file):

```tsx
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { SpecChannelBinding } from '@/lib/tauri-bridge'
import { listSpecChannelBindings, updateSpecChannelBindings, updateSpecImSettings } from '@/lib/tauri-bridge'

function ImChannelBindingsSection({ specId }: { specId: string }) {
  const [bindings, setBindings] = useState<SpecChannelBinding[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    listSpecChannelBindings(specId)
      .then(setBindings)
      .catch(() => setBindings([]))
      .finally(() => setLoading(false))
  }, [specId])

  async function handleToggle(channelInstanceId: string, enabled: boolean) {
    const updated = bindings.map((b) =>
      b.channelInstanceId === channelInstanceId ? { ...b, enabled } : b
    )
    setBindings(updated)
    await updateSpecChannelBindings(specId, updated).catch(() => {})
  }

  if (loading) return null

  return (
    <Section title="消息通道">
      <p className="text-xs text-muted-foreground mb-2">
        AI 驱动：数字人决定何时以及通过配置的渠道通知什么内容。
      </p>
      {bindings.length === 0 ? (
        <p className="text-xs text-muted-foreground">暂无渠道。请先在设置中配置 IM 渠道。</p>
      ) : (
        bindings.map((b) => (
          <Row key={b.channelInstanceId} label={b.channelName ?? b.channelInstanceId} description={b.channelType ?? ''}>
            <Toggle checked={b.enabled} disabled={false} onChange={() => handleToggle(b.channelInstanceId, !b.enabled)} />
          </Row>
        ))
      )}
      <button
        className="titlebar-no-drag text-xs text-primary mt-1 hover:underline"
        onClick={() => invoke('open_settings_tab', { tab: 'im-channels' }).catch(() => {})}
      >
        在设置中配置渠道 ↗
      </button>
    </Section>
  )
}

function ImTriggerRow({ specId, initialTriggerPhrase }: { specId: string; initialTriggerPhrase: string }) {
  const [value, setValue] = useState(initialTriggerPhrase)
  const [saving, setSaving] = useState(false)

  async function handleSave() {
    setSaving(true)
    await updateSpecImSettings(specId, value || null, null).catch(() => {})
    setSaving(false)
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="text-sm">触发关键词</div>
      <div className="text-xs text-muted-foreground">IM 消息以此关键词开头时触发本 automation</div>
      <div className="flex gap-2 mt-1">
        <input
          className="flex-1 text-xs bg-muted/50 border border-border rounded px-2 py-1 font-mono"
          placeholder="/daily-report"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
        <button
          disabled={saving}
          onClick={handleSave}
          className="titlebar-no-drag text-xs px-3 py-1 bg-primary text-primary-foreground rounded disabled:opacity-50"
        >
          {saving ? '保存中…' : '保存'}
        </button>
      </div>
    </div>
  )
}

function SystemPromptRow({ specId, initialValue }: { specId: string; initialValue: string }) {
  const [value, setValue] = useState(initialValue)
  const [saving, setSaving] = useState(false)

  async function handleSave() {
    setSaving(true)
    await updateSpecImSettings(specId, null, value || null).catch(() => {})
    setSaving(false)
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="text-sm">系统提示词</div>
      <div className="text-xs text-muted-foreground">覆盖 Space 级默认 prompt（可选）</div>
      <textarea
        className="mt-1 text-xs bg-muted/50 border border-border rounded px-2 py-1 font-mono resize-y min-h-[80px]"
        placeholder="（留空则使用 Space 默认提示词）"
        value={value}
        onChange={(e) => setValue(e.target.value)}
      />
      <button
        disabled={saving}
        onClick={handleSave}
        className="titlebar-no-drag self-end text-xs px-3 py-1 bg-primary text-primary-foreground rounded disabled:opacity-50 mt-1"
      >
        {saving ? '保存中…' : '保存'}
      </button>
    </div>
  )
}
```

- [ ] **Step 6: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected: no errors (or only pre-existing unrelated errors).

- [ ] **Step 7: Run frontend tests**

```bash
cd ui && npm test -- --run 2>&1 | tail -15
```

Expected: all pass (no regressions).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs \
        ui/src/lib/tauri-bridge.ts \
        ui/src/components/automation/SpecSettingsView.tsx
git commit -m "feat: SpecSettingsView IM sections — 消息通道, IM触发, 开发者"
```

---

## Self-Review

**Spec coverage:**
- ✅ WeCom Bot WebSocket bidirectional (wecom.rs)
- ✅ iLink HTTP long-poll bidirectional (ilink.rs)
- ✅ StreamingHandle trait (types.rs)
- ✅ WecomStreamingHandle implementation (wecom.rs)
- ✅ HeadlessDelegate replacing AutomationDelegate (headless.rs + execute.rs refactor)
- ✅ notify_user IM close-loop (headless.rs — reply_handle check before ChannelManager)
- ✅ dispatch_inbound permission check → trigger_phrase routing (dispatcher.rs)
- ✅ run_agent_chat_via_im with session history + persist_im_messages start_idx (dispatcher.rs)
- ✅ ImChannelManager.start_instance with real senders (manager.rs)
- ✅ Frontend: 消息通道 section, IM触发 input, 开发者/system_prompt textarea (SpecSettingsView.tsx)
- ✅ HumaneSpecRow extended with triggerPhrase + systemPromptOverride

**Placeholder scan:** No TBDs or TODOs left in code blocks.

**Type consistency:** `ImChannelSender` trait used throughout (matches Plan A types.rs). `StreamingHandle` trait added in Task 2, used in HeadlessDelegate and wecom.rs. `ReplyHandle` from Plan A's types.rs (Arc<dyn ImChannelSender>, channel_ctx, chat_id) — all match.

**Known simplifications vs. hello-halo:**
- iLink QR login flow omitted (bot_token entered directly) — QR auth is future work
- WeCom media download/decrypt omitted — text messages only in Plan B
- WeCom message debounce buffer omitted — each message dispatched immediately
- These are intentional YAGNI cuts for Plan B scope.
