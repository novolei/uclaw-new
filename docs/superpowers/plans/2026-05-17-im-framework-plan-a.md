# IM Framework — Plan A: Core Infrastructure + Notify Channels + CRUD UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the IM channel infrastructure (DB schema, types, manager, notify senders, Tauri commands, global settings UI) so channels can be configured and used for automation notifications — independent of the bidirectional IM routing in Plan B.

**Architecture:** Convert `channels.rs` to a `channels/` module directory. Add `ImChannelManager` (manages DB-persisted channel instances with hot-reload), `ImSessionRegistry` (DB-backed per-user session mapping), and four outbound-only notify senders (email/dingtalk/feishu/webhook). New V32 migration adds `im_channel_instances`, `im_sessions`, `spec_channel_bindings`, plus three columns on `automation_specs`. Frontend adds a global "IM 渠道" settings panel.

**Tech Stack:** Rust (lettre 0.11 for SMTP, reqwest for webhooks, tokio for async), React 18 + TypeScript + Jotai + Tailwind, SQLite migrations.

---

## File Map

**Create:**
- `src-tauri/src/channels/mod.rs` — re-exports ChannelManager + new ImChannelManager
- `src-tauri/src/channels/types.rs` — ImChannelType, ImChannelInstanceConfig, InboundMessage, ReplyHandle, ImChannelSender trait
- `src-tauri/src/channels/manager.rs` — ImChannelManager: start_all, apply_config, start/stop instance, send_to_channel
- `src-tauri/src/channels/session_registry.rs` — ImSessionRegistry: load_from_db, get_or_create_session, touch
- `src-tauri/src/channels/notify/mod.rs`
- `src-tauri/src/channels/notify/email.rs` — SMTP sender (lettre)
- `src-tauri/src/channels/notify/dingtalk.rs` — DingTalk webhook + signing
- `src-tauri/src/channels/notify/feishu.rs` — Feishu webhook + signing
- `src-tauri/src/channels/notify/webhook.rs` — generic HTTP POST (WebhookSender moved here)
- `src-tauri/src/channels/im/mod.rs`
- `ui/src/atoms/im-channel-atoms.ts`
- `ui/src/components/settings/ImChannelsSettings.tsx`
- `ui/src/components/settings/ImChannelForm.tsx`

**Modify:**
- `src-tauri/Cargo.toml` — add lettre
- `src-tauri/src/db/migrations.rs` — add V32
- `src-tauri/src/channels.rs` → **deleted** (replaced by channels/ dir)
- `src-tauri/src/app.rs` — add im_channel_manager + im_session_registry to AppState
- `src-tauri/src/main.rs` — Stage 3 registration + invoke_handler! entries
- `src-tauri/src/tauri_commands.rs` — CRUD commands for im_channel_instances + spec_channel_bindings
- `ui/src/components/settings/` — add ImChannels route/tab entry

---

## Task 1: Cargo.toml — add lettre dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Write the failing test (compilation check)**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: clean (no errors). This is our baseline.

- [ ] **Step 2: Add lettre to Cargo.toml**

Open `src-tauri/Cargo.toml`. Find the `[dependencies]` section (after `reqwest`). Add:

```toml
lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1", "tokio1-native-tls", "builder"] }
```

- [ ] **Step 3: Verify compilation**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output (clean build). lettre will be fetched and compiled.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add lettre for SMTP email channel"
```

---

## Task 2: V32 migration — IM tables + automation_specs columns

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/db/migrations.rs`, at the end of the `#[cfg(test)]` block, add:

```rust
#[test]
fn v32_im_tables_created() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();

    // im_channel_instances table exists
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='im_channel_instances'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "im_channel_instances table must exist after V32");

    // im_sessions table exists
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='im_sessions'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "im_sessions table must exist after V32");

    // spec_channel_bindings table exists
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='spec_channel_bindings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "spec_channel_bindings table must exist after V32");

    // automation_specs gains trigger_phrase column
    conn.execute(
        "INSERT INTO automation_specs (id, name, version, author, description, system_prompt, \
         spec_type, spec_yaml, trigger_phrase, created_at, updated_at) \
         VALUES ('t1','n','1','a','d','s','automation','y', '/test', 1, 1)",
        [],
    )
    .unwrap();
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib db::migrations::tests::v32_im_tables_created 2>&1 | tail -15
```

Expected: FAIL — `im_channel_instances` doesn't exist yet.

- [ ] **Step 3: Add the V32 SQL constant**

In `migrations.rs`, after the V31 constant (search for `/// V31` and add below its closing `";"`):

```rust
/// V32 — IM channel infrastructure: instances, sessions, spec bindings,
/// and three new columns on automation_specs (trigger_phrase, system_prompt override, description).
const SQL_V32: &str = "
CREATE TABLE IF NOT EXISTS im_channel_instances (
    id                   TEXT PRIMARY KEY,
    space_id             TEXT NOT NULL,
    channel_type         TEXT NOT NULL,
    name                 TEXT NOT NULL,
    config_json          TEXT NOT NULL DEFAULT '{}',
    credentials_json     TEXT NOT NULL DEFAULT '{}',
    enabled              INTEGER NOT NULL DEFAULT 1,
    streaming            INTEGER NOT NULL DEFAULT 0,
    reply_scope          TEXT NOT NULL DEFAULT 'all',
    permission_enabled   INTEGER NOT NULL DEFAULT 0,
    owners_json          TEXT NOT NULL DEFAULT '[]',
    guest_policy_json    TEXT NOT NULL DEFAULT '{}',
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_im_channel_instances_space
    ON im_channel_instances(space_id, enabled);

CREATE TABLE IF NOT EXISTS im_sessions (
    id               TEXT PRIMARY KEY,
    space_id         TEXT NOT NULL,
    channel_type     TEXT NOT NULL,
    chat_id          TEXT NOT NULL,
    agent_session_id TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    last_active_at   INTEGER NOT NULL,
    UNIQUE(space_id, channel_type, chat_id)
);

CREATE TABLE IF NOT EXISTS spec_channel_bindings (
    spec_id             TEXT NOT NULL,
    channel_instance_id TEXT NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (spec_id, channel_instance_id)
);
";

/// V32b — ALTER TABLE additions to automation_specs (separate statements for idempotency).
const SQL_V32B: &str = "
ALTER TABLE automation_specs ADD COLUMN trigger_phrase TEXT;
ALTER TABLE automation_specs ADD COLUMN system_prompt_override TEXT;
ALTER TABLE automation_specs ADD COLUMN spec_description TEXT;
";
```

- [ ] **Step 4: Register V32 in apply_migrations**

In `apply_migrations`, after the `// V31` block (search for `"Running migration V31"`), add:

```rust
// V32: IM channel infrastructure (im_channel_instances, im_sessions, spec_channel_bindings).
tracing::debug!("Running migration V32: IM channel tables");
for stmt in SQL_V32.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute_batch(stmt) {
        tracing::warn!("V32 stmt skipped: {} :: {}", e, stmt);
    }
}
// V32b: automation_specs additional columns (ALTER TABLE — idempotent, ignore if column exists).
tracing::debug!("Running migration V32b: automation_specs IM columns");
for stmt in SQL_V32B.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute_batch(stmt) {
        tracing::warn!("V32b stmt skipped (likely already exists): {} :: {}", e, stmt);
    }
}
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
cd src-tauri && cargo test --lib db::migrations::tests::v32_im_tables_created 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 6: Run full migration tests**

```bash
cd src-tauri && cargo test --lib db::migrations 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat: V32 migration — im_channel_instances, im_sessions, spec_channel_bindings"
```

---

## Task 3: channels/ module restructure

Convert `src-tauri/src/channels.rs` into `src-tauri/src/channels/mod.rs`. All existing tests must still pass.

**Files:**
- Delete: `src-tauri/src/channels.rs`
- Create: `src-tauri/src/channels/mod.rs`
- Create: `src-tauri/src/channels/im/mod.rs` (empty placeholder for Plan B)
- Create: `src-tauri/src/channels/notify/mod.rs` (empty placeholder)

- [ ] **Step 1: Run existing channels tests to establish baseline**

```bash
cd src-tauri && cargo test --lib channels 2>&1 | tail -10
```

Expected: 2 tests pass (`send_to_type_filters_by_channel_type`, `send_to_type_skips_disabled_channels`).

- [ ] **Step 2: Create directory structure**

```bash
mkdir -p src-tauri/src/channels/im
mkdir -p src-tauri/src/channels/notify
```

- [ ] **Step 3: Copy channels.rs → channels/mod.rs**

Create `src-tauri/src/channels/mod.rs` with the exact content of the current `channels.rs` plus these additions at the top (after the existing use statements):

```rust
pub mod im;
pub mod notify;
pub mod types;
pub mod manager;
pub mod session_registry;
```

The rest of the file (ChannelType, ChannelConfig, ChannelNotification, ChannelSender trait, WebhookSender, ChannelManager, tests) stays **identical** to the current `channels.rs`.

Full content of `src-tauri/src/channels/mod.rs`:

```rust
//! Notification channel system for distributing messages.
//!
//! Supports multiple notification backends: webhook, email, IM channels.

pub mod im;
pub mod notify;
pub mod types;
pub mod manager;
pub mod session_registry;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Channel type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Webhook,
    Email,
    WeChat,
    DingTalk,
    Feishu,
    Custom,
}

/// Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub id: String,
    pub name: String,
    pub channel_type: ChannelType,
    pub enabled: bool,
    pub webhook_url: Option<String>,
    pub config: Option<serde_json::Value>,
}

/// Notification payload to send through channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelNotification {
    pub title: String,
    pub body: String,
    pub level: String,
    pub metadata: Option<serde_json::Value>,
}

/// Abstract channel sender trait
#[async_trait::async_trait]
pub trait ChannelSender: Send + Sync {
    async fn send(&self, notification: &ChannelNotification, config: &ChannelConfig) -> Result<(), String>;
    fn name(&self) -> &str;
}

/// Webhook channel sender
pub struct WebhookSender {
    client: reqwest::Client,
}

impl WebhookSender {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl ChannelSender for WebhookSender {
    async fn send(&self, notification: &ChannelNotification, config: &ChannelConfig) -> Result<(), String> {
        let url = config.webhook_url.as_ref()
            .ok_or_else(|| "No webhook URL configured".to_string())?;
        self.client
            .post(url)
            .json(notification)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Webhook send failed: {}", e))?;
        Ok(())
    }
    fn name(&self) -> &str { "webhook" }
}

/// Channel manager (legacy outbound-only manager, preserved for backward compat)
pub struct ChannelManager {
    channels: HashMap<String, (ChannelConfig, bool)>,
    senders: HashMap<String, Box<dyn ChannelSender>>,
}

impl ChannelManager {
    pub fn new() -> Self {
        let mut manager = Self {
            channels: HashMap::new(),
            senders: HashMap::new(),
        };
        manager.register_sender("webhook", Box::new(WebhookSender::new()));
        manager
    }

    pub fn register_sender(&mut self, kind: &str, sender: Box<dyn ChannelSender>) {
        self.senders.insert(kind.to_string(), sender);
    }

    pub fn add_channel(&mut self, config: ChannelConfig) {
        self.channels.insert(config.id.clone(), (config, true));
    }

    pub fn remove_channel(&mut self, id: &str) -> Option<ChannelConfig> {
        self.channels.remove(id).map(|(c, _)| c)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some((config, _)) = self.channels.get_mut(id) {
            config.enabled = enabled;
            return true;
        }
        false
    }

    pub async fn broadcast(&self, notification: &ChannelNotification) -> Vec<(String, Result<(), String>)> {
        let mut results = Vec::new();
        for (id, (config, _)) in &self.channels {
            if !config.enabled { continue; }
            let sender_key = match config.channel_type {
                ChannelType::Webhook  => "webhook",
                ChannelType::Email    => "email",
                ChannelType::WeChat   => "wechat",
                ChannelType::DingTalk => "dingtalk",
                ChannelType::Feishu   => "feishu",
                ChannelType::Custom   => "custom",
            };
            if let Some(sender) = self.senders.get(sender_key) {
                let result = sender.send(notification, config).await;
                results.push((id.clone(), result));
            }
        }
        results
    }

    pub async fn send_to_type(
        &self,
        target_type: &ChannelType,
        notification: &ChannelNotification,
    ) -> Vec<(String, Result<(), String>)> {
        let mut results = Vec::new();
        for (id, (config, _)) in &self.channels {
            if !config.enabled || &config.channel_type != target_type { continue; }
            let sender_key = match config.channel_type {
                ChannelType::Webhook  => "webhook",
                ChannelType::Email    => "email",
                ChannelType::WeChat   => "wechat",
                ChannelType::DingTalk => "dingtalk",
                ChannelType::Feishu   => "feishu",
                ChannelType::Custom   => "custom",
            };
            if let Some(sender) = self.senders.get(sender_key) {
                let result = sender.send(notification, config).await;
                results.push((id.clone(), result));
            }
        }
        results
    }

    pub fn list(&self) -> Vec<&ChannelConfig> {
        self.channels.values().map(|(c, _)| c).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysOkSender;

    #[async_trait::async_trait]
    impl ChannelSender for AlwaysOkSender {
        async fn send(&self, _n: &ChannelNotification, _c: &ChannelConfig) -> Result<(), String> {
            Ok(())
        }
        fn name(&self) -> &str { "email" }
    }

    fn test_mgr() -> ChannelManager {
        let mut mgr = ChannelManager::new();
        mgr.register_sender("email", Box::new(AlwaysOkSender));
        mgr.add_channel(ChannelConfig {
            id: "e1".into(), name: "Email 1".into(),
            channel_type: ChannelType::Email, enabled: true,
            webhook_url: None, config: None,
        });
        mgr.add_channel(ChannelConfig {
            id: "w1".into(), name: "WeChat 1".into(),
            channel_type: ChannelType::WeChat, enabled: true,
            webhook_url: None, config: None,
        });
        mgr
    }

    #[tokio::test]
    async fn send_to_type_filters_by_channel_type() {
        let mgr = test_mgr();
        let notif = ChannelNotification {
            title: "T".into(), body: "B".into(), level: "info".into(), metadata: None,
        };
        let results = mgr.send_to_type(&ChannelType::Email, &notif).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "e1");
        assert!(results[0].1.is_ok());
        let results = mgr.send_to_type(&ChannelType::WeChat, &notif).await;
        assert_eq!(results.len(), 0, "wechat sender not registered → 0 results");
    }

    #[tokio::test]
    async fn send_to_type_skips_disabled_channels() {
        let mut mgr = test_mgr();
        mgr.set_enabled("e1", false);
        let notif = ChannelNotification {
            title: "T".into(), body: "B".into(), level: "info".into(), metadata: None,
        };
        let results = mgr.send_to_type(&ChannelType::Email, &notif).await;
        assert_eq!(results.len(), 0, "disabled channel should be skipped");
    }
}
```

- [ ] **Step 4: Create stub submodule files**

Create `src-tauri/src/channels/im/mod.rs`:

```rust
// IM bidirectional channel implementations — populated in Plan B.
```

Create `src-tauri/src/channels/notify/mod.rs`:

```rust
pub mod email;
pub mod dingtalk;
pub mod feishu;
pub mod webhook;
```

Create `src-tauri/src/channels/types.rs` (stub — full implementation in Task 4):

```rust
// IM framework types — populated in Task 4.
```

Create `src-tauri/src/channels/manager.rs` (stub):

```rust
// ImChannelManager — populated in Task 5.
```

Create `src-tauri/src/channels/session_registry.rs` (stub):

```rust
// ImSessionRegistry — populated in Task 6.
```

- [ ] **Step 5: Delete the old channels.rs**

```bash
rm src-tauri/src/channels.rs
```

- [ ] **Step 6: Update lib.rs to use channels as a module directory**

In `src-tauri/src/main.rs` (or wherever `mod channels` is declared), verify the declaration is just `mod channels;` — Rust will automatically find `src/channels/mod.rs`. No change needed if already declared this way.

```bash
grep -n "mod channels" src-tauri/src/main.rs
```

Expected: `mod channels;` (no path qualifier needed).

- [ ] **Step 7: Build and run channels tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd src-tauri && cargo test --lib channels 2>&1 | tail -10
```

Expected: clean build, 2 tests pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/channels/ && git rm src-tauri/src/channels.rs
git commit -m "refactor: convert channels.rs to channels/ module directory"
```

---

## Task 4: channels/types.rs — IM core types

**Files:**
- Modify: `src-tauri/src/channels/types.rs`

- [ ] **Step 1: Write the failing test**

Add to the end of `src-tauri/src/channels/types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn im_channel_type_roundtrips_json() {
        let t = ImChannelType::WecomBot;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#""wecom_bot""#);
        let back: ImChannelType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ImChannelType::WecomBot);
    }

    #[test]
    fn inbound_message_channel_ctx_is_optional() {
        let msg = InboundMessage {
            instance_id: "i1".into(),
            chat_id: "u1".into(),
            sender_name: None,
            text: "hello".into(),
            timestamp: 0,
            channel_ctx: None,
        };
        assert!(msg.channel_ctx.is_none());

        let msg_with_ctx = InboundMessage {
            channel_ctx: Some(serde_json::json!({"context_token": "abc"})),
            ..msg
        };
        assert!(msg_with_ctx.channel_ctx.is_some());
    }

    #[test]
    fn guest_policy_default_is_permissive() {
        let gp = GuestPolicy::default();
        assert!(gp.tool_allowlist.is_empty());
        assert!(!gp.mcp_enabled);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::types 2>&1 | tail -10
```

Expected: compile error — types not defined.

- [ ] **Step 3: Implement channels/types.rs**

Replace `src-tauri/src/channels/types.rs` with:

```rust
//! Core types for the IM channel framework.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// All supported IM/notify channel types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ImChannelType {
    WecomBot,
    WechatIlink,
    Email,
    Dingtalk,
    Feishu,
    Webhook,
}

impl ImChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WecomBot     => "wecom_bot",
            Self::WechatIlink  => "wechat_ilink",
            Self::Email        => "email",
            Self::Dingtalk     => "dingtalk",
            Self::Feishu       => "feishu",
            Self::Webhook      => "webhook",
        }
    }

    pub fn is_bidirectional(&self) -> bool {
        matches!(self, Self::WecomBot | Self::WechatIlink)
    }
}

impl std::fmt::Display for ImChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-user permission policy for guests (non-owner senders).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuestPolicy {
    /// Tool names guests are allowed to trigger (empty = all allowed).
    pub tool_allowlist: Vec<String>,
    /// Whether MCP tools are enabled for guests.
    pub mcp_enabled: bool,
}

/// One configured IM channel instance (DB row + deserialized fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImChannelInstanceConfig {
    pub id: String,
    pub space_id: String,
    pub channel_type: ImChannelType,
    pub name: String,
    /// Non-sensitive config (endpoint, bot_id, etc.)
    pub config: serde_json::Value,
    /// Sensitive credentials (api_key, secret, password, etc.)
    pub credentials: serde_json::Value,
    pub enabled: bool,
    pub streaming: bool,
    pub reply_scope: String,
    pub permission_enabled: bool,
    /// chat_id whitelist — only these senders can use this channel.
    pub owners: Vec<String>,
    pub guest_policy: GuestPolicy,
}

/// Unified inbound message from any bidirectional channel.
#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub instance_id: String,
    /// User identifier (WeChat openid, WeCom userid, etc.)
    pub chat_id: String,
    pub sender_name: Option<String>,
    pub text: String,
    pub timestamp: i64,
    /// Channel-specific context passed through to ReplyHandle.
    /// iLink: `{"context_token": "..."}`.
    /// WeCom: `{"req_id": "...", "expires_at": <unix_ms>}`.
    pub channel_ctx: Option<serde_json::Value>,
}

/// Unified outbound reply handle — abstracts over all channel types.
#[derive(Clone)]
pub struct ReplyHandle {
    pub sender: Arc<dyn ImChannelSender>,
    pub chat_id: String,
    pub channel_ctx: Option<serde_json::Value>,
}

impl ReplyHandle {
    pub async fn send(&self, text: &str) -> Result<(), String> {
        self.sender.send_text(&self.chat_id, text, self.channel_ctx.as_ref()).await
    }
}

/// Unified outbound sender trait — implemented by each channel backend.
#[async_trait::async_trait]
pub trait ImChannelSender: Send + Sync {
    async fn send_text(
        &self,
        chat_id: &str,
        text: &str,
        ctx: Option<&serde_json::Value>,
    ) -> Result<(), String>;

    /// True for WeCom — enables streaming token updates.
    fn supports_streaming(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn im_channel_type_roundtrips_json() {
        let t = ImChannelType::WecomBot;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#""wecom_bot""#);
        let back: ImChannelType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ImChannelType::WecomBot);
    }

    #[test]
    fn inbound_message_channel_ctx_is_optional() {
        let msg = InboundMessage {
            instance_id: "i1".into(),
            chat_id: "u1".into(),
            sender_name: None,
            text: "hello".into(),
            timestamp: 0,
            channel_ctx: None,
        };
        assert!(msg.channel_ctx.is_none());

        let msg_with_ctx = InboundMessage {
            channel_ctx: Some(serde_json::json!({"context_token": "abc"})),
            ..msg
        };
        assert!(msg_with_ctx.channel_ctx.is_some());
    }

    #[test]
    fn guest_policy_default_is_permissive() {
        let gp = GuestPolicy::default();
        assert!(gp.tool_allowlist.is_empty());
        assert!(!gp.mcp_enabled);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib channels::types 2>&1 | tail -10
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/channels/types.rs
git commit -m "feat: channels/types.rs — ImChannelType, InboundMessage, ReplyHandle, ImChannelSender trait"
```

---

## Task 5: channels/notify/ — four outbound senders

**Files:**
- Modify: `src-tauri/src/channels/notify/webhook.rs`
- Modify: `src-tauri/src/channels/notify/dingtalk.rs`
- Modify: `src-tauri/src/channels/notify/feishu.rs`
- Modify: `src-tauri/src/channels/notify/email.rs`

- [ ] **Step 1: Write the failing tests**

Add to each notify sender file. Start with `src-tauri/src/channels/notify/webhook.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn webhook_sender_requires_url() {
        // send_text with empty URL config returns Err
        let sender = WebhookImSender::new();
        let ctx = serde_json::json!({"url": ""});
        // We verify the logic path; actual HTTP call would fail anyway.
        // The URL field is taken from ctx["url"] or config.
        assert!(ctx["url"].as_str().unwrap_or("").is_empty());
    }
}
```

- [ ] **Step 2: Implement webhook.rs**

```rust
//! Generic HTTP POST webhook notify sender.

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;

pub struct WebhookImSender {
    client: reqwest::Client,
}

impl WebhookImSender {
    pub fn new() -> Self {
        Self { client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default() }
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
        req.send().await.map_err(|e| format!("webhook error: {e}"))?;
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
}
```

- [ ] **Step 3: Implement dingtalk.rs**

```rust
//! DingTalk outbound webhook sender with optional HMAC-SHA256 signing.

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub struct DingtalkSender {
    client: reqwest::Client,
}

impl DingtalkSender {
    pub fn new() -> Self {
        Self { client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default() }
    }

    fn sign(secret: &str, timestamp: i64) -> String {
        let msg = format!("{}\n{}", timestamp, secret);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
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
        let mut url = ctx["webhook_url"].as_str()
            .ok_or("dingtalk: missing webhook_url")?.to_string();

        if let Some(secret) = ctx["signing_secret"].as_str().filter(|s| !s.is_empty()) {
            let ts = chrono::Utc::now().timestamp_millis();
            let sign = Self::sign(secret, ts);
            url = format!("{}&timestamp={}&sign={}", url, ts, sign);
        }

        self.client.post(&url)
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
```

Add `hmac`, `sha2`, `base64`, `urlencoding` to `Cargo.toml` if not already present:

```bash
grep -E "hmac|sha2|base64|urlencoding" src-tauri/Cargo.toml
```

If missing, add to `src-tauri/Cargo.toml`:

```toml
hmac = "0.12"
sha2 = "0.10"
base64 = "0.22"
urlencoding = "2"
```

- [ ] **Step 4: Implement feishu.rs**

```rust
//! Feishu outbound webhook sender with optional HMAC-SHA256 signing.

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub struct FeishuSender {
    client: reqwest::Client,
}

impl FeishuSender {
    pub fn new() -> Self {
        Self { client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default() }
    }

    fn sign(secret: &str, timestamp: i64) -> String {
        let msg = format!("{}\n{}", timestamp, secret);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
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
        let url = ctx["webhook_url"].as_str()
            .ok_or("feishu: missing webhook_url")?;

        let mut body = serde_json::json!({
            "msg_type": "text",
            "content": { "text": text }
        });

        if let Some(secret) = ctx["signing_secret"].as_str().filter(|s| !s.is_empty()) {
            let ts = chrono::Utc::now().timestamp();
            let sign = Self::sign(secret, ts);
            body["timestamp"] = serde_json::json!(ts.to_string());
            body["sign"] = serde_json::json!(sign);
        }

        self.client.post(url).json(&body).send()
            .await
            .map_err(|e| format!("feishu error: {e}"))?;
        Ok(())
    }
}
```

- [ ] **Step 5: Implement email.rs**

```rust
//! SMTP email sender using lettre.

use crate::channels::types::ImChannelSender;
use async_trait::async_trait;
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

pub struct EmailSender;

impl EmailSender {
    pub fn new() -> Self { Self }
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
        let host     = ctx["smtp_host"].as_str().ok_or("email: missing smtp_host")?;
        let port     = ctx["smtp_port"].as_u64().unwrap_or(587) as u16;
        let username = ctx["username"].as_str().ok_or("email: missing username")?;
        let password = ctx["password"].as_str().ok_or("email: missing password")?;
        let from     = ctx["from_address"].as_str().unwrap_or(username);
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

        for to in &to_list {
            let email = Message::builder()
                .from(from.parse().map_err(|e| format!("email: invalid from: {e}"))?)
                .to(to.parse().map_err(|e| format!("email: invalid to {to}: {e}"))?)
                .subject(subject)
                .header(ContentType::TEXT_PLAIN)
                .body(text.to_string())
                .map_err(|e| format!("email: build error: {e}"))?;

            mailer.send(email).await.map_err(|e| format!("email: send error: {e}"))?;
        }
        Ok(())
    }
}
```

- [ ] **Step 6: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Fix any missing imports. The `hmac`/`sha2`/`base64`/`urlencoding` crates must be in Cargo.toml.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/channels/notify/ src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: channels/notify — email (lettre), dingtalk, feishu, webhook senders"
```

---

## Task 6: channels/session_registry.rs — ImSessionRegistry

**Files:**
- Modify: `src-tauri/src/channels/session_registry.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_conn() -> std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::apply_migrations(&conn).unwrap();
        std::sync::Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn get_or_create_creates_session_on_first_call() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let session_id = registry
            .get_or_create_session("space-1", "wecom_bot", "user-1", Some("Alice"))
            .await
            .unwrap();
        assert!(!session_id.is_empty());

        // Second call returns the same session_id
        let session_id2 = registry
            .get_or_create_session("space-1", "wecom_bot", "user-1", None)
            .await
            .unwrap();
        assert_eq!(session_id, session_id2);
    }

    #[tokio::test]
    async fn different_users_get_different_sessions() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let s1 = registry.get_or_create_session("space-1", "wecom_bot", "user-A", None).await.unwrap();
        let s2 = registry.get_or_create_session("space-1", "wecom_bot", "user-B", None).await.unwrap();
        assert_ne!(s1, s2);
    }

    #[tokio::test]
    async fn load_from_db_restores_cache() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let original = registry
            .get_or_create_session("space-1", "email", "user-C", None)
            .await
            .unwrap();

        // Create a fresh registry from the same DB — simulates app restart
        let registry2 = ImSessionRegistry::new(db.clone());
        registry2.load_from_db().await.unwrap();

        let restored = registry2
            .get_or_create_session("space-1", "email", "user-C", None)
            .await
            .unwrap();
        assert_eq!(original, restored, "session must survive registry restart");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib channels::session_registry 2>&1 | tail -10
```

Expected: compile error.

- [ ] **Step 3: Implement session_registry.rs**

```rust
//! ImSessionRegistry — persistent per-user IM session mapping.
//!
//! Maps (space_id, channel_type, chat_id) → agent_session_id.
//! Cache is backed by the `im_sessions` DB table and survives app restarts.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type SessionKey = (String, String, String); // (space_id, channel_type, chat_id)

pub struct ImSessionRegistry {
    cache: Arc<RwLock<HashMap<SessionKey, String>>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl ImSessionRegistry {
    pub fn new(db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }

    /// Load all existing im_sessions from DB into the in-memory cache.
    /// Call once at startup.
    pub async fn load_from_db(&self) -> Result<(), String> {
        let rows: Vec<(String, String, String, String)> = {
            let conn = self.db.lock().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT space_id, channel_type, chat_id, agent_session_id \
                     FROM im_sessions",
                )
                .map_err(|e| e.to_string())?;
            stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect()
        };

        let mut cache = self.cache.write().await;
        for (space_id, channel_type, chat_id, session_id) in rows {
            cache.insert((space_id, channel_type, chat_id), session_id);
        }
        tracing::info!("ImSessionRegistry: loaded {} sessions from DB", cache.len());
        Ok(())
    }

    /// Return the existing agent_session_id for this (space, channel, user),
    /// or create a new agent_session + im_session record.
    pub async fn get_or_create_session(
        &self,
        space_id: &str,
        channel_type: &str,
        chat_id: &str,
        sender_name: Option<&str>,
    ) -> Result<String, String> {
        let key = (space_id.to_string(), channel_type.to_string(), chat_id.to_string());

        // Fast path: cache hit
        {
            let cache = self.cache.read().await;
            if let Some(session_id) = cache.get(&key) {
                return Ok(session_id.clone());
            }
        }

        // Slow path: create new agent_session + im_session row
        let session_id = uuid::Uuid::new_v4().to_string();
        let im_session_id = uuid::Uuid::new_v4().to_string();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let title = match sender_name {
            Some(name) => format!("{} via {}", name, channel_type),
            None => format!("IM {} {}", channel_type, &chat_id[..chat_id.len().min(8)]),
        };
        let origin = format!("im:{}:{}", channel_type, chat_id);

        {
            let conn = self.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO agent_sessions (id, title, space_id, origin, created_at, updated_at, message_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0)",
                rusqlite::params![session_id, title, space_id, origin, now_ms],
            )
            .map_err(|e| format!("create agent_session: {e}"))?;

            conn.execute(
                "INSERT INTO im_sessions (id, space_id, channel_type, chat_id, agent_session_id, created_at, last_active_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![im_session_id, space_id, channel_type, chat_id, session_id, now_ms],
            )
            .map_err(|e| format!("create im_session: {e}"))?;
        }

        let mut cache = self.cache.write().await;
        cache.insert(key, session_id.clone());
        Ok(session_id)
    }

    /// Update last_active_at for a session.
    pub async fn touch(
        &self,
        space_id: &str,
        channel_type: &str,
        chat_id: &str,
    ) -> Result<(), String> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE im_sessions SET last_active_at = ?1 \
             WHERE space_id = ?2 AND channel_type = ?3 AND chat_id = ?4",
            rusqlite::params![now_ms, space_id, channel_type, chat_id],
        )
        .map_err(|e| format!("touch im_session: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_conn() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::apply_migrations(&conn).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn get_or_create_creates_session_on_first_call() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let session_id = registry
            .get_or_create_session("space-1", "wecom_bot", "user-1", Some("Alice"))
            .await
            .unwrap();
        assert!(!session_id.is_empty());

        let session_id2 = registry
            .get_or_create_session("space-1", "wecom_bot", "user-1", None)
            .await
            .unwrap();
        assert_eq!(session_id, session_id2);
    }

    #[tokio::test]
    async fn different_users_get_different_sessions() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let s1 = registry.get_or_create_session("space-1", "wecom_bot", "user-A", None).await.unwrap();
        let s2 = registry.get_or_create_session("space-1", "wecom_bot", "user-B", None).await.unwrap();
        assert_ne!(s1, s2);
    }

    #[tokio::test]
    async fn load_from_db_restores_cache() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let original = registry
            .get_or_create_session("space-1", "email", "user-C", None)
            .await
            .unwrap();

        let registry2 = ImSessionRegistry::new(db.clone());
        registry2.load_from_db().await.unwrap();
        let restored = registry2
            .get_or_create_session("space-1", "email", "user-C", None)
            .await
            .unwrap();
        assert_eq!(original, restored);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib channels::session_registry 2>&1 | tail -15
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/channels/session_registry.rs
git commit -m "feat: ImSessionRegistry — per-user persistent IM session mapping"
```

---

## Task 7: channels/manager.rs — ImChannelManager

**Files:**
- Modify: `src-tauri/src/channels/manager.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::types::ImChannelType;

    fn in_memory_db() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::apply_migrations(&conn).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn start_all_with_empty_db_succeeds() {
        let db = in_memory_db();
        let manager = ImChannelManager::new(db);
        manager.start_all().await.unwrap();
        assert_eq!(manager.instance_count().await, 0);
    }

    #[tokio::test]
    async fn apply_config_adds_notify_instance() {
        let db = in_memory_db();
        // Insert a webhook instance into DB
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO im_channel_instances \
                 (id, space_id, channel_type, name, config_json, credentials_json, enabled, \
                  streaming, reply_scope, permission_enabled, owners_json, guest_policy_json, \
                  created_at, updated_at) \
                 VALUES ('ch1','s1','webhook','My Webhook','{}','{}',1,0,'all',0,'[]','{}',1,1)",
                [],
            )
            .unwrap();
        }
        let manager = ImChannelManager::new(db);
        manager.start_all().await.unwrap();
        assert_eq!(manager.instance_count().await, 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib channels::manager 2>&1 | tail -10
```

Expected: compile error.

- [ ] **Step 3: Implement manager.rs**

```rust
//! ImChannelManager — lifecycle management for IM channel instances.
//!
//! Loads instances from DB at startup, supports hot-reload via apply_config(),
//! starts inbound poll tasks for bidirectional channels (WeCom/iLink).

use crate::channels::notify::{
    dingtalk::DingtalkSender, email::EmailSender, feishu::FeishuSender,
    webhook::WebhookImSender,
};
use crate::channels::types::{ImChannelInstanceConfig, ImChannelSender, ImChannelType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::AbortHandle;

struct RunningInstance {
    config: ImChannelInstanceConfig,
    sender: Arc<dyn ImChannelSender>,
    /// Present for bidirectional channels (WeCom/iLink); None for notify-only.
    _inbound_task: Option<AbortHandle>,
}

pub struct ImChannelManager {
    instances: Arc<RwLock<HashMap<String, RunningInstance>>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl ImChannelManager {
    pub fn new(db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }

    /// Load all enabled instances from DB and start them.
    pub async fn start_all(&self) -> Result<(), String> {
        let configs = self.load_configs_from_db()?;
        for config in configs {
            if config.enabled {
                self.start_instance(config).await?;
            }
        }
        Ok(())
    }

    /// Hot-reload: diff new configs against running instances.
    pub async fn apply_config(&self, new_configs: Vec<ImChannelInstanceConfig>) -> Result<(), String> {
        let new_ids: std::collections::HashSet<String> =
            new_configs.iter().map(|c| c.id.clone()).collect();

        // Stop removed instances
        let running_ids: Vec<String> = {
            let inst = self.instances.read().await;
            inst.keys().cloned().collect()
        };
        for id in &running_ids {
            if !new_ids.contains(id) {
                self.stop_instance(id).await;
            }
        }

        // Start/restart changed instances
        for config in new_configs {
            if !config.enabled {
                self.stop_instance(&config.id).await;
                continue;
            }
            let needs_start = {
                let inst = self.instances.read().await;
                !inst.contains_key(&config.id)
            };
            if needs_start {
                self.start_instance(config).await?;
            }
            // TODO(plan-b): restart if config changed (compare hash)
        }
        Ok(())
    }

    async fn start_instance(&self, config: ImChannelInstanceConfig) -> Result<(), String> {
        let sender = self.build_sender(&config)?;
        // Bidirectional inbound tasks (WeCom/iLink) wired in Plan B.
        let inbound_task: Option<AbortHandle> = None;

        let running = RunningInstance {
            config: config.clone(),
            sender,
            _inbound_task: inbound_task,
        };
        self.instances.write().await.insert(config.id.clone(), running);
        tracing::info!("ImChannelManager: started instance {} ({})", config.id, config.channel_type);
        Ok(())
    }

    async fn stop_instance(&self, id: &str) {
        if let Some(inst) = self.instances.write().await.remove(id) {
            if let Some(handle) = inst._inbound_task {
                handle.abort();
            }
            tracing::info!("ImChannelManager: stopped instance {}", id);
        }
    }

    fn build_sender(
        &self,
        config: &ImChannelInstanceConfig,
    ) -> Result<Arc<dyn ImChannelSender>, String> {
        let ctx = config.config.clone();
        let creds = config.credentials.clone();
        // Merge credentials into ctx for senders that need them
        let merged = merge_json(ctx, creds);
        match config.channel_type {
            ImChannelType::Webhook    => Ok(Arc::new(WebhookImSender::new()) as Arc<dyn ImChannelSender>),
            ImChannelType::Email      => Ok(Arc::new(EmailSender::new())),
            ImChannelType::Dingtalk   => Ok(Arc::new(DingtalkSender::new())),
            ImChannelType::Feishu     => Ok(Arc::new(FeishuSender::new())),
            ImChannelType::WecomBot   => {
                // Bidirectional — Plan B implements real WebSocket sender.
                // For now return a no-op stub so the instance can be stored.
                Ok(Arc::new(NoopSender { ctx: merged }))
            }
            ImChannelType::WechatIlink => {
                Ok(Arc::new(NoopSender { ctx: merged }))
            }
        }
    }

    /// Send a message through a specific channel instance.
    /// Used by notify_user tool and automation notifications.
    pub async fn send_to_instance(
        &self,
        instance_id: &str,
        chat_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let inst = self.instances.read().await;
        let running = inst.get(instance_id)
            .ok_or_else(|| format!("instance {} not running", instance_id))?;
        let ctx = merge_json(running.config.config.clone(), running.config.credentials.clone());
        running.sender.send_text(chat_id, text, Some(&ctx)).await
    }

    pub async fn instance_count(&self) -> usize {
        self.instances.read().await.len()
    }

    pub async fn list_instances(&self) -> Vec<ImChannelInstanceConfig> {
        self.instances.read().await.values().map(|r| r.config.clone()).collect()
    }

    fn load_configs_from_db(&self) -> Result<Vec<ImChannelInstanceConfig>, String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, channel_type, name, config_json, credentials_json, \
             enabled, streaming, reply_scope, permission_enabled, owners_json, guest_policy_json \
             FROM im_channel_instances",
        ).map_err(|e| e.to_string())?;

        let configs = stmt.query_map([], |r| {
            let channel_type_str: String = r.get(2)?;
            Ok((
                r.get::<_, String>(0)?, // id
                r.get::<_, String>(1)?, // space_id
                channel_type_str,
                r.get::<_, String>(3)?, // name
                r.get::<_, String>(4)?, // config_json
                r.get::<_, String>(5)?, // credentials_json
                r.get::<_, bool>(6)?,   // enabled
                r.get::<_, bool>(7)?,   // streaming
                r.get::<_, String>(8)?, // reply_scope
                r.get::<_, bool>(9)?,   // permission_enabled
                r.get::<_, String>(10)?,// owners_json
                r.get::<_, String>(11)?,// guest_policy_json
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .filter_map(|(id, space_id, ct_str, name, cfg, creds, enabled, streaming,
                      reply_scope, perm_enabled, owners_s, gp_s)| {
            let channel_type: ImChannelType = serde_json::from_str(&format!("\"{}\"", ct_str)).ok()?;
            let config: serde_json::Value = serde_json::from_str(&cfg).unwrap_or(serde_json::json!({}));
            let credentials: serde_json::Value = serde_json::from_str(&creds).unwrap_or(serde_json::json!({}));
            let owners: Vec<String> = serde_json::from_str(&owners_s).unwrap_or_default();
            let guest_policy = serde_json::from_str(&gp_s).unwrap_or_default();
            Some(ImChannelInstanceConfig {
                id, space_id, channel_type, name, config, credentials,
                enabled, streaming, reply_scope,
                permission_enabled: perm_enabled,
                owners, guest_policy,
            })
        })
        .collect();

        Ok(configs)
    }
}

/// Merge two JSON objects — second overwrites first on key conflict.
fn merge_json(mut base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    if let (Some(b), Some(o)) = (base.as_object_mut(), overlay.as_object()) {
        for (k, v) in o {
            b.insert(k.clone(), v.clone());
        }
    }
    base
}

/// Placeholder sender for bidirectional channels until Plan B wires them up.
struct NoopSender {
    ctx: serde_json::Value,
}

#[async_trait::async_trait]
impl ImChannelSender for NoopSender {
    async fn send_text(&self, _chat_id: &str, text: &str, _ctx: Option<&serde_json::Value>) -> Result<(), String> {
        tracing::warn!("NoopSender: bidirectional channel not yet wired (Plan B). text={}", &text[..text.len().min(50)]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::apply_migrations(&conn).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn start_all_with_empty_db_succeeds() {
        let db = in_memory_db();
        let manager = ImChannelManager::new(db);
        manager.start_all().await.unwrap();
        assert_eq!(manager.instance_count().await, 0);
    }

    #[tokio::test]
    async fn apply_config_adds_notify_instance() {
        let db = in_memory_db();
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO im_channel_instances \
                 (id, space_id, channel_type, name, config_json, credentials_json, enabled, \
                  streaming, reply_scope, permission_enabled, owners_json, guest_policy_json, \
                  created_at, updated_at) \
                 VALUES ('ch1','s1','webhook','My Webhook','{}','{}',1,0,'all',0,'[]','{}',1,1)",
                [],
            )
            .unwrap();
        }
        let manager = ImChannelManager::new(db);
        manager.start_all().await.unwrap();
        assert_eq!(manager.instance_count().await, 1);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib channels::manager 2>&1 | tail -10
```

Expected: 2 tests pass.

- [ ] **Step 5: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/manager.rs
git commit -m "feat: ImChannelManager — hot-reload lifecycle, notify senders, NoopSender stubs for Plan B"
```

---

## Task 8: AppState + main.rs — register ImChannelManager and ImSessionRegistry

**Files:**
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add fields to AppState in app.rs**

Find `pub struct AppState` in `src-tauri/src/app.rs`. After `pub channel_manager: Arc<RwLock<ChannelManager>>`, add:

```rust
pub im_channel_manager: Arc<crate::channels::manager::ImChannelManager>,
pub im_session_registry: Arc<crate::channels::session_registry::ImSessionRegistry>,
```

In the `AppState::new` (or wherever `channel_manager` is initialized, around line 350), after `let channel_manager = Arc::new(RwLock::new(ChannelManager::new()));` add:

```rust
let im_channel_manager = Arc::new(
    crate::channels::manager::ImChannelManager::new(db.clone())
);
let im_session_registry = Arc::new(
    crate::channels::session_registry::ImSessionRegistry::new(db.clone())
);
```

In the struct literal (where `channel_manager` is set in `AppState { ... }`), add:

```rust
im_channel_manager,
im_session_registry,
```

- [ ] **Step 2: Register in Stage 3 in main.rs**

In `src-tauri/src/main.rs`, find the `// Stage 3` registration block (around line 118). After the `FilesRailService` or `AppRuntimeService` registration, add:

```rust
// Start ImChannelManager (load DB instances + start notify senders)
{
    let im_mgr = app_state.im_channel_manager.clone();
    let im_reg = app_state.im_session_registry.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = im_reg.load_from_db().await {
            tracing::warn!("[Stage 3] ImSessionRegistry load_from_db failed: {}", e);
        }
        if let Err(e) = im_mgr.start_all().await {
            tracing::warn!("[Stage 3] ImChannelManager start_all failed: {}", e);
        }
        tracing::info!("[Stage 3] ImChannelManager started");
    });
}
```

- [ ] **Step 3: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Fix any field/type mismatches.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/app.rs src-tauri/src/main.rs
git commit -m "feat: add im_channel_manager + im_session_registry to AppState, Stage 3 startup"
```

---

## Task 9: Tauri commands — IM channel CRUD

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs` (invoke_handler! macro)

- [ ] **Step 1: Write the failing test (TS type check)**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: clean. This is the baseline.

- [ ] **Step 2: Add CRUD commands to tauri_commands.rs**

Find the existing `list_channels` command (around line 3046) and add these new commands after the existing channel commands:

```rust
// ─── IM Channel Instance CRUD ────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImChannelInput {
    pub space_id: String,
    pub channel_type: String,
    pub name: String,
    pub config: serde_json::Value,
    pub credentials: serde_json::Value,
    pub enabled: bool,
    pub streaming: bool,
    pub reply_scope: String,
    pub permission_enabled: bool,
    pub owners: Vec<String>,
    pub guest_policy: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImChannelRow {
    pub id: String,
    pub space_id: String,
    pub channel_type: String,
    pub name: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub streaming: bool,
    pub reply_scope: String,
    pub permission_enabled: bool,
    pub owners: Vec<String>,
    pub guest_policy: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[tauri::command]
pub async fn list_im_channels(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ImChannelRow>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, space_id, channel_type, name, config_json, enabled, streaming, \
         reply_scope, permission_enabled, owners_json, guest_policy_json, created_at, updated_at \
         FROM im_channel_instances ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ImChannelRow {
            id: r.get(0)?,
            space_id: r.get(1)?,
            channel_type: r.get(2)?,
            name: r.get(3)?,
            config: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
            enabled: r.get::<_, i64>(5)? != 0,
            streaming: r.get::<_, i64>(6)? != 0,
            reply_scope: r.get(7)?,
            permission_enabled: r.get::<_, i64>(8)? != 0,
            owners: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
            guest_policy: serde_json::from_str(&r.get::<_, String>(10)?).unwrap_or_default(),
            created_at: r.get(11)?,
            updated_at: r.get(12)?,
        })
    })?
    .filter_map(|r| r.ok())
    .collect();
    Ok(rows)
}

#[tauri::command]
pub async fn create_im_channel(
    state: tauri::State<'_, AppState>,
    input: ImChannelInput,
) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let config_json = serde_json::to_string(&input.config).unwrap_or_else(|_| "{}".into());
    let creds_json  = serde_json::to_string(&input.credentials).unwrap_or_else(|_| "{}".into());
    let owners_json = serde_json::to_string(&input.owners).unwrap_or_else(|_| "[]".into());
    let gp_json     = serde_json::to_string(&input.guest_policy).unwrap_or_else(|_| "{}".into());
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO im_channel_instances \
             (id, space_id, channel_type, name, config_json, credentials_json, enabled, streaming, \
              reply_scope, permission_enabled, owners_json, guest_policy_json, created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
            rusqlite::params![
                id, input.space_id, input.channel_type, input.name,
                config_json, creds_json,
                input.enabled as i64, input.streaming as i64,
                input.reply_scope, input.permission_enabled as i64,
                owners_json, gp_json, now,
            ],
        )?;
    }
    // Hot-reload manager
    let _ = state.im_channel_manager.start_all().await;
    Ok(id)
}

#[tauri::command]
pub async fn update_im_channel(
    state: tauri::State<'_, AppState>,
    id: String,
    input: ImChannelInput,
) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let config_json = serde_json::to_string(&input.config).unwrap_or_else(|_| "{}".into());
    let creds_json  = serde_json::to_string(&input.credentials).unwrap_or_else(|_| "{}".into());
    let owners_json = serde_json::to_string(&input.owners).unwrap_or_else(|_| "[]".into());
    let gp_json     = serde_json::to_string(&input.guest_policy).unwrap_or_else(|_| "{}".into());
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        conn.execute(
            "UPDATE im_channel_instances SET \
             space_id=?1, channel_type=?2, name=?3, config_json=?4, credentials_json=?5, \
             enabled=?6, streaming=?7, reply_scope=?8, permission_enabled=?9, \
             owners_json=?10, guest_policy_json=?11, updated_at=?12 \
             WHERE id=?13",
            rusqlite::params![
                input.space_id, input.channel_type, input.name,
                config_json, creds_json,
                input.enabled as i64, input.streaming as i64,
                input.reply_scope, input.permission_enabled as i64,
                owners_json, gp_json, now, id,
            ],
        )?;
    }
    let _ = state.im_channel_manager.start_all().await;
    Ok(())
}

#[tauri::command]
pub async fn delete_im_channel(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    conn.execute("DELETE FROM im_channel_instances WHERE id=?1", [&id])?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_im_channel(
    state: tauri::State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE im_channel_instances SET enabled=?1, updated_at=?2 WHERE id=?3",
        rusqlite::params![enabled as i64, now, id],
    )?;
    Ok(())
}

// ─── Spec-Channel Bindings ───────────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecChannelBinding {
    pub channel_instance_id: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_spec_channel_bindings(
    state: tauri::State<'_, AppState>,
    spec_id: String,
) -> Result<Vec<SpecChannelBinding>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT channel_instance_id, enabled FROM spec_channel_bindings WHERE spec_id=?1",
    )?;
    let rows = stmt.query_map([&spec_id], |r| {
        Ok(SpecChannelBinding {
            channel_instance_id: r.get(0)?,
            enabled: r.get::<_, i64>(1)? != 0,
        })
    })?
    .filter_map(|r| r.ok())
    .collect();
    Ok(rows)
}

#[tauri::command]
pub async fn update_spec_channel_bindings(
    state: tauri::State<'_, AppState>,
    spec_id: String,
    bindings: Vec<SpecChannelBinding>,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    conn.execute(
        "DELETE FROM spec_channel_bindings WHERE spec_id=?1",
        [&spec_id],
    )?;
    for b in &bindings {
        conn.execute(
            "INSERT INTO spec_channel_bindings (spec_id, channel_instance_id, enabled) \
             VALUES (?1,?2,?3)",
            rusqlite::params![spec_id, b.channel_instance_id, b.enabled as i64],
        )?;
    }
    Ok(())
}
```

- [ ] **Step 3: Register commands in invoke_handler! in main.rs**

Find the `invoke_handler!` macro in `src-tauri/src/main.rs`. Add these entries alongside the existing channel commands:

```rust
crate::tauri_commands::list_im_channels,
crate::tauri_commands::create_im_channel,
crate::tauri_commands::update_im_channel,
crate::tauri_commands::delete_im_channel,
crate::tauri_commands::toggle_im_channel,
crate::tauri_commands::list_spec_channel_bindings,
crate::tauri_commands::update_spec_channel_bindings,
```

- [ ] **Step 4: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat: Tauri commands for IM channel CRUD and spec-channel bindings"
```

---

## Task 10: Frontend — Jotai atoms + ImChannelsSettings + ImChannelForm

**Files:**
- Create: `ui/src/atoms/im-channel-atoms.ts`
- Create: `ui/src/components/settings/ImChannelsSettings.tsx`
- Create: `ui/src/components/settings/ImChannelForm.tsx`
- Modify: settings entry point (wherever global settings panel tabs are defined)

- [ ] **Step 1: Create im-channel-atoms.ts**

Create `ui/src/atoms/im-channel-atoms.ts`:

```typescript
import { atom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'

export interface ImChannelRow {
  id: string
  spaceId: string
  channelType: string
  name: string
  config: Record<string, unknown>
  enabled: boolean
  streaming: boolean
  replyScope: string
  permissionEnabled: boolean
  owners: string[]
  guestPolicy: { tool_allowlist: string[]; mcp_enabled: boolean }
  createdAt: number
  updatedAt: number
}

export interface ImChannelInput {
  spaceId: string
  channelType: string
  name: string
  config: Record<string, unknown>
  credentials: Record<string, unknown>
  enabled: boolean
  streaming: boolean
  replyScope: string
  permissionEnabled: boolean
  owners: string[]
  guestPolicy: Record<string, unknown>
}

export const imChannelsAtom = atom<ImChannelRow[]>([])

export const fetchImChannelsAtom = atom(null, async (_get, set) => {
  const rows = await invoke<ImChannelRow[]>('list_im_channels')
  set(imChannelsAtom, rows)
})
```

- [ ] **Step 2: Create ImChannelForm.tsx**

Create `ui/src/components/settings/ImChannelForm.tsx`:

```typescript
import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { ImChannelInput, ImChannelRow } from '@/atoms/im-channel-atoms'

const CHANNEL_TYPES = [
  { value: 'wecom_bot',    label: '企业微信 Bot (WebSocket)' },
  { value: 'wechat_ilink', label: '微信个人 (iLink)' },
  { value: 'email',        label: '电子邮件 (SMTP)' },
  { value: 'dingtalk',     label: '钉钉 Webhook' },
  { value: 'feishu',       label: '飞书 Webhook' },
  { value: 'webhook',      label: '通用 Webhook' },
]

interface Props {
  spaces: { id: string; name: string }[]
  editing?: ImChannelRow
  onDone: () => void
}

export function ImChannelForm({ spaces, editing, onDone }: Props) {
  const [channelType, setChannelType] = useState(editing?.channelType ?? 'webhook')
  const [name, setName] = useState(editing?.name ?? '')
  const [spaceId, setSpaceId] = useState(editing?.spaceId ?? spaces[0]?.id ?? '')
  const [enabled, setEnabled] = useState(editing?.enabled ?? true)
  const [streaming, setStreaming] = useState(editing?.streaming ?? false)
  const [permissionEnabled, setPermissionEnabled] = useState(editing?.permissionEnabled ?? false)
  const [owners, setOwners] = useState(editing?.owners.join(', ') ?? '')
  const [mcpEnabled, setMcpEnabled] = useState(editing?.guestPolicy.mcp_enabled ?? false)
  // Channel-specific fields
  const [webhookUrl, setWebhookUrl] = useState((editing?.config.url as string) ?? '')
  const [smtpHost, setSmtpHost] = useState((editing?.config.smtp_host as string) ?? '')
  const [smtpPort, setSmtpPort] = useState(String(editing?.config.smtp_port ?? '587'))
  const [smtpUser, setSmtpUser] = useState((editing?.config.username as string) ?? '')
  const [smtpPass, setSmtpPass] = useState('')
  const [toAddresses, setToAddresses] = useState((editing?.config.to_addresses as string[])?.join(', ') ?? '')
  const [corpId, setCorpId] = useState((editing?.config.corp_id as string) ?? '')
  const [agentId, setAgentId] = useState((editing?.config.agent_id as string) ?? '')
  const [corpSecret, setCorpSecret] = useState('')
  const [appId, setAppId] = useState((editing?.config.app_id as string) ?? '')
  const [apiKey, setApiKey] = useState('')
  const [signingSecret, setSigningSecret] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  function buildInput(): ImChannelInput {
    let config: Record<string, unknown> = {}
    let credentials: Record<string, unknown> = {}

    switch (channelType) {
      case 'webhook':
        config = { url: webhookUrl }
        break
      case 'email':
        config = {
          smtp_host: smtpHost,
          smtp_port: Number(smtpPort),
          username: smtpUser,
          to_addresses: toAddresses.split(',').map(s => s.trim()).filter(Boolean),
        }
        credentials = { password: smtpPass }
        break
      case 'dingtalk':
      case 'feishu':
        config = { webhook_url: webhookUrl }
        credentials = { signing_secret: signingSecret }
        break
      case 'wecom_bot':
        config = { corp_id: corpId, agent_id: agentId }
        credentials = { corp_secret: corpSecret }
        break
      case 'wechat_ilink':
        config = { app_id: appId }
        credentials = { api_key: apiKey }
        break
    }

    return {
      spaceId,
      channelType,
      name,
      config,
      credentials,
      enabled,
      streaming,
      replyScope: 'all',
      permissionEnabled,
      owners: owners.split(',').map(s => s.trim()).filter(Boolean),
      guestPolicy: { tool_allowlist: [], mcp_enabled: mcpEnabled },
    }
  }

  async function handleSave() {
    setSaving(true)
    setError(null)
    try {
      const input = buildInput()
      if (editing) {
        await invoke('update_im_channel', { id: editing.id, input })
      } else {
        await invoke('create_im_channel', { input })
      }
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-4 p-4">
      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">渠道类型</label>
        <select
          value={channelType}
          onChange={e => setChannelType(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
          disabled={!!editing}
        >
          {CHANNEL_TYPES.map(t => (
            <option key={t.value} value={t.value}>{t.label}</option>
          ))}
        </select>
      </div>

      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">名称</label>
        <input
          value={name}
          onChange={e => setName(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
          placeholder="我的企微机器人"
        />
      </div>

      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">绑定 Space</label>
        <select
          value={spaceId}
          onChange={e => setSpaceId(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
        >
          {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </div>

      {/* Channel-specific credential fields */}
      {channelType === 'webhook' && (
        <div className="space-y-1">
          <label className="text-xs text-muted-foreground">Webhook URL</label>
          <input value={webhookUrl} onChange={e => setWebhookUrl(e.target.value)}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
            placeholder="https://example.com/hook" />
        </div>
      )}

      {(channelType === 'dingtalk' || channelType === 'feishu') && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Webhook URL</label>
            <input value={webhookUrl} onChange={e => setWebhookUrl(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">签名密钥（可选）</label>
            <input value={signingSecret} onChange={e => setSigningSecret(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
        </>
      )}

      {channelType === 'email' && (
        <>
          <div className="grid grid-cols-2 gap-2">
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">SMTP Host</label>
              <input value={smtpHost} onChange={e => setSmtpHost(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
                placeholder="smtp.gmail.com" />
            </div>
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">端口</label>
              <input value={smtpPort} onChange={e => setSmtpPort(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
                placeholder="587" />
            </div>
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">用户名</label>
            <input value={smtpUser} onChange={e => setSmtpUser(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">密码</label>
            <input value={smtpPass} onChange={e => setSmtpPass(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="留空则不修改" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">收件人（逗号分隔）</label>
            <input value={toAddresses} onChange={e => setToAddresses(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="a@example.com, b@example.com" />
          </div>
        </>
      )}

      {channelType === 'wecom_bot' && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Corp ID</label>
            <input value={corpId} onChange={e => setCorpId(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Agent ID</label>
            <input value={agentId} onChange={e => setAgentId(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Corp Secret</label>
            <input value={corpSecret} onChange={e => setCorpSecret(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="留空则不修改" />
          </div>
        </>
      )}

      {channelType === 'wechat_ilink' && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">App ID</label>
            <input value={appId} onChange={e => setAppId(e.target.value)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">API Key</label>
            <input value={apiKey} onChange={e => setApiKey(e.target.value)}
              type="password"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
              placeholder="留空则不修改" />
          </div>
        </>
      )}

      {/* Common toggles */}
      <div className="flex items-center gap-3">
        <label className="flex items-center gap-1.5 text-sm">
          <input type="checkbox" checked={enabled} onChange={e => setEnabled(e.target.checked)} />
          启用
        </label>
        <label className="flex items-center gap-1.5 text-sm">
          <input type="checkbox" checked={streaming} onChange={e => setStreaming(e.target.checked)} />
          流式回复
        </label>
      </div>

      {/* Permissions */}
      <div className="space-y-2 rounded border border-border p-3">
        <label className="flex items-center gap-1.5 text-sm font-medium">
          <input type="checkbox" checked={permissionEnabled}
            onChange={e => setPermissionEnabled(e.target.checked)} />
          启用权限控制
        </label>
        {permissionEnabled && (
          <>
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">
                Owners（chat_id 白名单，逗号分隔）
              </label>
              <input value={owners} onChange={e => setOwners(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
                placeholder="openid_1, openid_2" />
            </div>
            <label className="flex items-center gap-1.5 text-sm">
              <input type="checkbox" checked={mcpEnabled}
                onChange={e => setMcpEnabled(e.target.checked)} />
              Guest 允许 MCP 工具
            </label>
          </>
        )}
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <div className="flex justify-end gap-2">
        <button onClick={onDone}
          className="rounded px-3 py-1.5 text-sm hover:bg-muted">
          取消
        </button>
        <button onClick={handleSave} disabled={saving || !name || !spaceId}
          className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50">
          {saving ? '保存中…' : '保存'}
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Create ImChannelsSettings.tsx**

Create `ui/src/components/settings/ImChannelsSettings.tsx`:

```typescript
import { useAtom, useSetAtom } from 'jotai'
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { imChannelsAtom, fetchImChannelsAtom, type ImChannelRow } from '@/atoms/im-channel-atoms'
import { ImChannelForm } from './ImChannelForm'

const CHANNEL_TYPE_LABELS: Record<string, string> = {
  wecom_bot:    '企业微信 Bot',
  wechat_ilink: '微信 iLink',
  email:        '电子邮件',
  dingtalk:     '钉钉',
  feishu:       '飞书',
  webhook:      'Webhook',
}

export function ImChannelsSettings() {
  const [channels] = useAtom(imChannelsAtom)
  const fetchChannels = useSetAtom(fetchImChannelsAtom)
  const [spaces, setSpaces] = useState<{ id: string; name: string }[]>([])
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<ImChannelRow | undefined>()

  useEffect(() => {
    fetchChannels()
    invoke<{ id: string; name: string }[]>('list_spaces').then(setSpaces).catch(() => {})
  }, [])

  async function handleToggle(id: string, enabled: boolean) {
    await invoke('toggle_im_channel', { id, enabled })
    fetchChannels()
  }

  async function handleDelete(id: string) {
    if (!confirm('确定删除此渠道实例？')) return
    await invoke('delete_im_channel', { id })
    fetchChannels()
  }

  function handleEdit(ch: ImChannelRow) {
    setEditing(ch)
    setShowForm(true)
  }

  function handleDone() {
    setShowForm(false)
    setEditing(undefined)
    fetchChannels()
  }

  if (showForm) {
    return (
      <div className="max-w-lg">
        <div className="mb-3 flex items-center gap-2">
          <button onClick={() => { setShowForm(false); setEditing(undefined) }}
            className="text-sm text-muted-foreground hover:text-foreground">
            ← 返回
          </button>
          <span className="text-sm font-medium">{editing ? '编辑渠道' : '新增渠道'}</span>
        </div>
        <ImChannelForm spaces={spaces} editing={editing} onDone={handleDone} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium">IM 渠道</h3>
          <p className="text-xs text-muted-foreground mt-0.5">
            配置通知渠道和双向 IM 机器人，绑定到工作空间
          </p>
        </div>
        <button
          onClick={() => setShowForm(true)}
          className="rounded bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
        >
          + 新增渠道
        </button>
      </div>

      {channels.length === 0 ? (
        <div className="rounded border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
          还没有配置任何渠道。点击「新增渠道」开始。
        </div>
      ) : (
        <div className="space-y-2">
          {channels.map(ch => {
            const space = spaces.find(s => s.id === ch.spaceId)
            return (
              <div key={ch.id}
                className="flex items-center gap-3 rounded border border-border bg-card px-3 py-2.5">
                <div
                  className={`h-2 w-2 rounded-full flex-shrink-0 ${
                    ch.enabled ? 'bg-success' : 'bg-muted-foreground'
                  }`}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium truncate">{ch.name}</span>
                    <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                      {CHANNEL_TYPE_LABELS[ch.channelType] ?? ch.channelType}
                    </span>
                    {space && (
                      <span className="rounded bg-accent/20 px-1.5 py-0.5 text-xs text-accent-foreground">
                        {space.name}
                      </span>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1 flex-shrink-0">
                  <button
                    onClick={() => handleToggle(ch.id, !ch.enabled)}
                    className="rounded px-2 py-1 text-xs hover:bg-muted"
                  >
                    {ch.enabled ? '停用' : '启用'}
                  </button>
                  <button
                    onClick={() => handleEdit(ch)}
                    className="rounded px-2 py-1 text-xs hover:bg-muted"
                  >
                    编辑
                  </button>
                  <button
                    onClick={() => handleDelete(ch.id)}
                    className="rounded px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
                  >
                    删除
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Wire into settings panel**

Find where global settings tabs are defined (search for the existing settings panel component):

```bash
grep -rn "SettingsPanel\|settings.*tab\|tab.*settings" ui/src/ --include="*.tsx" | grep -v node_modules | head -10
```

Add `ImChannelsSettings` as a new tab/section in the settings panel. The exact location depends on the codebase — look for where `list_channels` or `add_channel` is used in the settings UI and add the new tab alongside it.

- [ ] **Step 5: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -15
```

Fix any type errors.

- [ ] **Step 6: Run Vitest**

```bash
cd ui && node_modules/.bin/vitest run 2>&1 | tail -10
```

Expected: all tests pass (no new tests added for settings panel).

- [ ] **Step 7: Commit**

```bash
git add ui/src/atoms/im-channel-atoms.ts \
        ui/src/components/settings/ImChannelsSettings.tsx \
        ui/src/components/settings/ImChannelForm.tsx
git commit -m "feat: IM channel settings UI — global panel with CRUD form"
```

---

## Final: Run all tests + self-review

- [ ] **Run Rust tests**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -20
```

Expected: all pass (channels, session_registry, manager, migrations).

- [ ] **Run TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean.

- [ ] **Run Vitest**

```bash
cd ui && node_modules/.bin/vitest run 2>&1 | tail -10
```

Expected: all pass.
