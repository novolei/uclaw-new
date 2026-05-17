# WeChat iLink QR 绑定渠道重设计 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 uclaw 的 `wechat_ilink` 渠道从错误的 app_id/api_key 表单改造为 QR 码扫码绑定流程，绑定后自动启动 HTTP long-poll 收发消息，会话 -14 失效时自动切回绑定界面。

**Architecture:** 后端新增 `ilink_binding.rs` 代理 iLink QR API；`IlinkSender` 接入 `status_tx` 基础设施，会话 -14 时发 `NeedsRebind` 状态；前端新增 `WechatIlinkBindingPanel` 状态机组件替换现有静态表单。`account_id` 存入 `config_json`（list 接口可返回），`bot_token` 存入 `credentials_json`（仅后端可见），彻底解决 update_im_channel 清空凭据的问题。

**Tech Stack:** Rust (reqwest, mockito 0.32 dev-dep), React 18 + TypeScript, `qrcode` npm package, Jotai, Tauri invoke IPC

---

## 重要背景（必读）

### 现有 `ilink.rs` 已经正确的部分

- `bot_token` 来自 `credentials["bot_token"]`，`base_url` 来自 `config["base_url"]`（默认 `https://ilinkai.weixin.qq.com`）
- Long-poll `POST /ilink/bot/getupdates`，`-14` 检测已有，但只是 `break` 没有发 status 通知
- Auth headers 正确：`Authorization: Bearer {bot_token}`、`AuthorizationType: ilink_bot_token`、`X-WECHAT-UIN: random-uint32-base64`
- 此次 **不修改 long-poll 逻辑**，只加 status_tx

### Manager 模式

WecomBot 走 early-return 路径（`if config.channel_type == ImChannelType::WecomBot { ... return Ok(()); }`），包含 status_relay task。WechatIlink 目前走 tuple 路径，`_status_relay_task: None`。此次将 WechatIlink 也改为 early-return 路径。

### 凭据存储约定（此次引入）

| 字段 | 存储位置 | 原因 |
|---|---|---|
| `bot_token` | `credentials_json` | 敏感，list 接口不返回 |
| `account_id` | `config_json` | 非敏感，list 接口返回，解决 update 清空问题 |
| `base_url` | `config_json` | 可选覆盖，非敏感 |

### 文件路径总览

```
src-tauri/src/channels/types.rs             ← 改：+NeedsRebind
src-tauri/src/channels/im/ilink.rs          ← 改：+status_tx
src-tauri/src/channels/im/ilink_binding.rs  ← 新建
src-tauri/src/channels/im/mod.rs            ← 改：+pub mod ilink_binding
src-tauri/src/channels/manager.rs           ← 改：WechatIlink early-return
src-tauri/src/tauri_commands.rs             ← 改：+4 个命令
src-tauri/src/main.rs                       ← 改：invoke_handler! 注册
src-tauri/Cargo.toml                        ← 改：+mockito dev-dep
ui/src/atoms/im-channel-atoms.ts            ← 改：needs_rebind 状态
ui/src/components/settings/WechatIlinkBindingPanel.tsx   ← 新建
ui/src/components/settings/WechatIlinkBindingPanel.test.tsx ← 新建
ui/src/components/settings/ImChannelAccordionRow.tsx     ← 改
ui/src/components/settings/ImChannelAccordionRow.test.tsx ← 改：+1 测试
```

---

## Task 1: `ChannelState::NeedsRebind` + `IlinkSender` status_tx + Manager wiring

**Files:**
- Modify: `src-tauri/src/channels/types.rs`
- Modify: `src-tauri/src/channels/im/ilink.rs`
- Modify: `src-tauri/src/channels/manager.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add mockito to dev-dependencies in `src-tauri/Cargo.toml`**

In `src-tauri/Cargo.toml`, find the `[dev-dependencies]` section (currently at line 165) and add mockito:

```toml
[dev-dependencies]
tauri = { version = "2.11", features = ["test"] }
tempfile = "3"
mockito = "0.32"
```

- [ ] **Step 2: Write the failing test for NeedsRebind emission**

At the bottom of `src-tauri/src/channels/im/ilink.rs`, add inside `#[cfg(test)] mod tests`:

```rust
#[tokio::test]
async fn status_tx_receives_needs_rebind_on_session_expired() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/ilink/bot/getupdates")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ret":-14}"#)
        .create_async()
        .await;

    let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel();
    let (inbound_tx, _inbound_rx) = tokio::sync::mpsc::unbounded_channel();

    let sender = Arc::new(IlinkSender::new(
        "test-inst",
        &serde_json::json!({ "base_url": server.url() }),
        &serde_json::json!({ "bot_token": "tok123" }),
        status_tx,
    ));
    let abort = sender.clone().start(inbound_tx);

    let status = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        status_rx.recv(),
    )
    .await
    .expect("timeout waiting for NeedsRebind status")
    .expect("status channel closed unexpectedly");

    abort.abort();
    assert_eq!(status.state, crate::channels::types::ChannelState::NeedsRebind);
    assert_eq!(status.instance_id, "test-inst");
}
```

- [ ] **Step 3: Run test to confirm it fails (IlinkSender::new doesn't accept status_tx yet)**

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::status_tx_receives_needs_rebind_on_session_expired 2>&1 | grep -E "^error|FAILED|passed"
```

Expected: compile error because `IlinkSender::new` doesn't accept 4 args yet.

- [ ] **Step 4: Add `NeedsRebind` to `ChannelState` in `src-tauri/src/channels/types.rs`**

Find the `ChannelState` enum (around line 44) and add the new variant:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelState {
    Online,
    Error,
    Offline,
    NeedsRebind,
}
```

- [ ] **Step 5: Add `status_tx` to `IlinkSender` and update `new()` in `src-tauri/src/channels/im/ilink.rs`**

Replace the `IlinkSender` struct definition and `new()` method (lines 25–48):

```rust
use crate::channels::types::{ChannelRuntimeStatus, ChannelState, ImChannelSender, InboundMessage, ReplyHandle};

pub struct IlinkSender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    context_tokens: Arc<RwLock<HashMap<String, String>>>,
    client: reqwest::Client,
    status_tx: tokio::sync::mpsc::UnboundedSender<ChannelRuntimeStatus>,
}

impl IlinkSender {
    pub fn new(
        instance_id: &str,
        config: &serde_json::Value,
        credentials: &serde_json::Value,
        status_tx: tokio::sync::mpsc::UnboundedSender<ChannelRuntimeStatus>,
    ) -> Self {
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
            status_tx,
        }
    }
```

- [ ] **Step 6: Update `poll_loop` in `ilink.rs` to emit status events**

Replace the `poll_loop` method body (lines 61–109):

```rust
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
    let mut connected = false;

    loop {
        match self.single_poll(&mut updates_buf, &inbound_tx).await {
            Ok(true) => {
                if !connected {
                    connected = true;
                    let _ = self.status_tx.send(ChannelRuntimeStatus {
                        instance_id: self.instance_id.clone(),
                        state: ChannelState::Online,
                        last_error: None,
                        connected_since_ms: Some(chrono::Utc::now().timestamp_millis()),
                        message_count_today: 0,
                    });
                }
                attempt = 0;
            }
            Ok(false) => {
                tracing::error!(
                    "[IlinkBot:{}] Session expired (code -14) — re-auth required",
                    self.instance_id
                );
                let _ = self.status_tx.send(ChannelRuntimeStatus {
                    instance_id: self.instance_id.clone(),
                    state: ChannelState::NeedsRebind,
                    last_error: Some("iLink 会话已失效（-14），请重新扫码绑定".to_string()),
                    connected_since_ms: None,
                    message_count_today: 0,
                });
                break;
            }
            Err(e) => {
                let _ = self.status_tx.send(ChannelRuntimeStatus {
                    instance_id: self.instance_id.clone(),
                    state: ChannelState::Error,
                    last_error: Some(e.to_string()),
                    connected_since_ms: None,
                    message_count_today: 0,
                });
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
```

- [ ] **Step 7: Convert WechatIlink branch in `manager.rs` to early-return with status relay**

In `src-tauri/src/channels/manager.rs`, find the `if config.channel_type == ImChannelType::WecomBot` block (lines ~101–142) and add a parallel block immediately after it (before `// All other channel types use the tuple pattern.`):

```rust
// WechatIlink has its own early-return path with status relay (same pattern as WecomBot).
if config.channel_type == ImChannelType::WechatIlink {
    let (inbound_tx, inbound_rx) =
        tokio::sync::mpsc::unbounded_channel::<(
            crate::channels::types::InboundMessage,
            Arc<crate::channels::types::ReplyHandle>,
        )>();
    let (status_tx, status_rx) =
        tokio::sync::mpsc::unbounded_channel::<crate::channels::types::ChannelRuntimeStatus>();
    let ilink = Arc::new(crate::channels::im::IlinkSender::new(
        &config.id,
        &config.config,
        &config.credentials,
        status_tx,
    ));
    let abort = if config.enabled {
        Some(ilink.clone().start(inbound_tx))
    } else {
        None
    };
    let fanout_abort = if config.enabled {
        Some(self.spawn_fanout_loop(config.id.clone(), inbound_rx))
    } else {
        drop(inbound_rx);
        None
    };
    let relay_abort = if config.enabled {
        Some(self.spawn_status_relay(status_rx))
    } else {
        None
    };
    let running = RunningInstance {
        config: config.clone(),
        sender: Arc::new(IlinkNoopSender) as Arc<dyn ImChannelSender>,
        _inbound_task: abort,
        _fanout_task: fanout_abort,
        _status_relay_task: relay_abort,
    };
    self.instances.write().await.insert(config.id.clone(), running);
    tracing::info!("ImChannelManager: started instance {} ({})", config.id, config.channel_type);
    return Ok(());
}
```

Then remove the `ImChannelType::WechatIlink => { ... }` arm from the `match config.channel_type` tuple block below (since it's now handled above). The match arm to remove starts at line ~147 and ends at `(Arc::new(IlinkNoopSender) as Arc<dyn ImChannelSender>, abort, fanout_abort)`.

- [ ] **Step 8: Run the test to confirm it passes**

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::status_tx_receives_needs_rebind_on_session_expired 2>&1 | tail -5
```

Expected: `test channels::im::ilink::tests::status_tx_receives_needs_rebind_on_session_expired ... ok`

- [ ] **Step 9: Verify full compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output (clean build).

- [ ] **Step 10: Run all lib tests to check no regressions**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: all existing tests pass.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/channels/types.rs src-tauri/src/channels/im/ilink.rs src-tauri/src/channels/manager.rs
git commit -m "feat(channels): NeedsRebind state + IlinkSender status_tx + manager relay"
```

---

## Task 2: `ilink_binding.rs` — QR API functions

**Files:**
- Create: `src-tauri/src/channels/im/ilink_binding.rs`
- Modify: `src-tauri/src/channels/im/mod.rs`

- [ ] **Step 1: Write the failing tests first**

Create `src-tauri/src/channels/im/ilink_binding.rs` with the tests only (functions not yet implemented):

```rust
//! iLink QR code binding — fetch QR, poll status.
//! These are standalone async functions, not tied to any running instance.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub const ILINK_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QrStatusKind {
    Wait,
    Scaned,
    Confirmed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrStatus {
    pub status: QrStatusKind,
    pub bot_token: Option<String>,
    /// account_id extracted from ilink_bot_id field in QR status response.
    pub account_id: Option<String>,
}

pub async fn fetch_qr(base_url: &str) -> Result<String> {
    todo!()
}

pub async fn poll_qr_status(base_url: &str, qrcode: &str) -> Result<QrStatus> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_qr_returns_qrcode_string() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/ilink/bot/get_bot_qrcode?bot_type=3")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"qrcode":"test_qr_abc"}"#)
            .create_async()
            .await;

        let result = fetch_qr(&server.url()).await.unwrap();
        assert_eq!(result, "test_qr_abc");
    }

    #[tokio::test]
    async fn poll_qr_status_confirmed_extracts_token_and_account_id() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"confirmed","bot_token":"tok999","ilink_bot_id":"acc123"}"#)
            .create_async()
            .await;

        let result = poll_qr_status(&server.url(), "qr1").await.unwrap();
        assert_eq!(result.status, QrStatusKind::Confirmed);
        assert_eq!(result.bot_token, Some("tok999".to_string()));
        assert_eq!(result.account_id, Some("acc123".to_string()));
    }

    #[tokio::test]
    async fn poll_qr_status_expired_has_no_token() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"expired"}"#)
            .create_async()
            .await;

        let result = poll_qr_status(&server.url(), "qr1").await.unwrap();
        assert_eq!(result.status, QrStatusKind::Expired);
        assert!(result.bot_token.is_none());
        assert!(result.account_id.is_none());
    }
}
```

- [ ] **Step 2: Export `ilink_binding` from `src-tauri/src/channels/im/mod.rs`**

```rust
pub mod ilink;
pub mod ilink_binding;
pub mod wecom;

pub use ilink::IlinkSender;
pub use wecom::{WecomSender, WecomStreamingHandle};
```

- [ ] **Step 3: Run tests to confirm they fail**

```bash
cd src-tauri && cargo test --lib channels::im::ilink_binding::tests 2>&1 | tail -5
```

Expected: all 3 tests fail with `not yet implemented`.

- [ ] **Step 4: Implement `fetch_qr` and `poll_qr_status` in `ilink_binding.rs`**

Replace the `todo!()` bodies:

```rust
pub async fn fetch_qr(base_url: &str) -> Result<String> {
    let url = format!("{base_url}/ilink/bot/get_bot_qrcode?bot_type=3");
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header("iLink-App-ClientVersion", "1")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;
    resp["qrcode"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| anyhow!("iLink QR response missing 'qrcode' field"))
}

pub async fn poll_qr_status(base_url: &str, qrcode: &str) -> Result<QrStatus> {
    let url = format!("{base_url}/ilink/bot/get_qrcode_status?qrcode={qrcode}");
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header("iLink-App-ClientVersion", "1")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;
    let kind = match resp["status"].as_str().unwrap_or("wait") {
        "wait"      => QrStatusKind::Wait,
        "scaned"    => QrStatusKind::Scaned,
        "confirmed" => QrStatusKind::Confirmed,
        "expired"   => QrStatusKind::Expired,
        other       => return Err(anyhow!("Unknown iLink QR status: {other}")),
    };
    Ok(QrStatus {
        status: kind,
        bot_token: resp["bot_token"].as_str().map(String::from),
        account_id: resp["ilink_bot_id"].as_str().map(String::from),
    })
}
```

- [ ] **Step 5: Run tests to confirm all 3 pass**

```bash
cd src-tauri && cargo test --lib channels::im::ilink_binding::tests 2>&1 | tail -5
```

Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/im/ilink_binding.rs src-tauri/src/channels/im/mod.rs
git commit -m "feat(channels): ilink_binding QR fetch/poll functions with tests"
```

---

## Task 3: 4 Tauri Commands + main.rs registration

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add 4 new Tauri commands to `tauri_commands.rs`**

Find the end of the `toggle_im_channel` command (around line 3072) and add the 4 new commands after it, before the `// ─── Spec-Channel Bindings ───` comment:

```rust
#[tauri::command]
pub async fn request_wechat_ilink_qrcode(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<serde_json::Value, Error> {
    let base_url = {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        let config_json: String = conn
            .query_row(
                "SELECT config_json FROM im_channel_instances WHERE id = ?1",
                [&instance_id],
                |r| r.get(0),
            )
            .map_err(|_| Error::NotFound(format!("Channel {instance_id} not found")))?;
        let config: serde_json::Value =
            serde_json::from_str(&config_json).unwrap_or_default();
        config["base_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(crate::channels::im::ilink_binding::ILINK_BASE_URL)
            .to_string()
    };
    let qrcode = crate::channels::im::ilink_binding::fetch_qr(&base_url)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(serde_json::json!({ "qrcode": qrcode }))
}

#[tauri::command]
pub async fn poll_wechat_ilink_qrcode_status(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    qrcode: String,
) -> Result<serde_json::Value, Error> {
    let base_url = {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        let config_json: String = conn
            .query_row(
                "SELECT config_json FROM im_channel_instances WHERE id = ?1",
                [&instance_id],
                |r| r.get(0),
            )
            .map_err(|_| Error::NotFound(format!("Channel {instance_id} not found")))?;
        let config: serde_json::Value =
            serde_json::from_str(&config_json).unwrap_or_default();
        config["base_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(crate::channels::im::ilink_binding::ILINK_BASE_URL)
            .to_string()
    };
    let status = crate::channels::im::ilink_binding::poll_qr_status(&base_url, &qrcode)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(serde_json::to_value(&status).unwrap_or_default())
}

/// Save bot_token to credentials_json and account_id to config_json, then restart instance.
#[tauri::command]
pub async fn save_wechat_ilink_token(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    bot_token: String,
    account_id: String,
) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let creds_json = serde_json::json!({ "bot_token": bot_token }).to_string();
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        // Merge account_id into existing config (preserves base_url etc.)
        let existing_config: String = conn
            .query_row(
                "SELECT config_json FROM im_channel_instances WHERE id = ?1",
                [&instance_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "{}".to_string());
        let mut config: serde_json::Value =
            serde_json::from_str(&existing_config).unwrap_or_default();
        config["account_id"] = serde_json::Value::String(account_id);
        let config_json = config.to_string();
        conn.execute(
            "UPDATE im_channel_instances \
             SET credentials_json = ?1, config_json = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![creds_json, config_json, now, instance_id],
        )?;
    }
    let _ = state.im_channel_manager.restart_instance_by_id(&instance_id).await;
    Ok(())
}

/// Clear bot_token from credentials and account_id from config, then restart instance.
#[tauri::command]
pub async fn disconnect_wechat_ilink(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), Error> {
    let now = chrono::Utc::now().timestamp_millis();
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        let existing_config: String = conn
            .query_row(
                "SELECT config_json FROM im_channel_instances WHERE id = ?1",
                [&instance_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "{}".to_string());
        let mut config: serde_json::Value =
            serde_json::from_str(&existing_config).unwrap_or_default();
        if let Some(obj) = config.as_object_mut() {
            obj.remove("account_id");
        }
        let config_json = config.to_string();
        conn.execute(
            "UPDATE im_channel_instances \
             SET credentials_json = '{}', config_json = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![config_json, now, instance_id],
        )?;
    }
    let _ = state.im_channel_manager.restart_instance_by_id(&instance_id).await;
    Ok(())
}
```

- [ ] **Step 2: Register the 4 new commands in `src-tauri/src/main.rs`**

Find the `invoke_handler!` IM channel block (lines ~365–374) and add the 4 new commands after `toggle_im_channel`:

```rust
// IM Channel Instance CRUD
uclaw_core::tauri_commands::list_im_channels,
uclaw_core::tauri_commands::get_im_channel_statuses,
uclaw_core::tauri_commands::create_im_channel,
uclaw_core::tauri_commands::update_im_channel,
uclaw_core::tauri_commands::delete_im_channel,
uclaw_core::tauri_commands::toggle_im_channel,
uclaw_core::tauri_commands::request_wechat_ilink_qrcode,
uclaw_core::tauri_commands::poll_wechat_ilink_qrcode_status,
uclaw_core::tauri_commands::save_wechat_ilink_token,
uclaw_core::tauri_commands::disconnect_wechat_ilink,
```

- [ ] **Step 3: Verify compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output (clean build).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(ipc): request_wechat_ilink_qrcode, poll status, save token, disconnect"
```

---

## Task 4: Frontend — `WechatIlinkBindingPanel.tsx` + atoms update

**Files:**
- Modify: `ui/src/atoms/im-channel-atoms.ts`
- Create: `ui/src/components/settings/WechatIlinkBindingPanel.tsx`
- Create: `ui/src/components/settings/WechatIlinkBindingPanel.test.tsx`

- [ ] **Step 1: Install `qrcode` and `@types/qrcode`**

```bash
cd ui && npm install qrcode @types/qrcode
```

Expected: package.json and package-lock.json updated, no errors.

- [ ] **Step 2: Update `ImChannelStatus` type in `ui/src/atoms/im-channel-atoms.ts`**

Find the `ImChannelStatus` interface and add `'needs_rebind'` to the `state` union:

```typescript
export interface ImChannelStatus {
  instanceId: string
  state: 'online' | 'error' | 'offline' | 'needs_rebind'
  lastError?: string
  connectedSinceMs?: number
  messageCountToday?: number
}
```

- [ ] **Step 3: Write the failing tests in `WechatIlinkBindingPanel.test.tsx`**

Create `ui/src/components/settings/WechatIlinkBindingPanel.test.tsx`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'
import { WechatIlinkBindingPanel } from './WechatIlinkBindingPanel'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))
vi.mock('qrcode', () => ({
  default: { toCanvas: vi.fn().mockResolvedValue(undefined) },
}))

const PROPS = {
  instanceId: 'inst-1',
  onSaved: vi.fn(),
  onDisconnect: vi.fn(),
}

beforeEach(() => {
  invokeMock.mockReset()
  PROPS.onSaved = vi.fn()
  PROPS.onDisconnect = vi.fn()
})

describe('WechatIlinkBindingPanel', () => {
  it('idle: shows get-qr button, no canvas', () => {
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    expect(screen.getByText('获取二维码')).not.toBeNull()
    expect(screen.queryByRole('img')).toBeNull()
  })

  it('qr-shown: fetching QR invokes request command and shows canvas', async () => {
    invokeMock.mockResolvedValueOnce({ qrcode: 'mock_qr_data' })
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    const btn = screen.getByText('获取二维码')
    await act(async () => { btn.click() })
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('request_wechat_ilink_qrcode', { instanceId: 'inst-1' })
    )
    expect(screen.getByText('用微信扫码绑定账号')).not.toBeNull()
  })

  it('scanning: poll returning scaned shows "已扫码" text', async () => {
    vi.useFakeTimers()
    invokeMock
      .mockResolvedValueOnce({ qrcode: 'qr123' })               // request_wechat_ilink_qrcode
      .mockResolvedValueOnce({ status: 'wait' })                 // first poll
      .mockResolvedValueOnce({ status: 'scaned' })               // second poll → scanning
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    await act(async () => { screen.getByText('获取二维码').click() })
    await act(async () => { vi.advanceTimersByTime(2100) })
    await act(async () => { vi.advanceTimersByTime(2100) })
    await waitFor(() =>
      expect(screen.getByText('已扫码，等待确认…')).not.toBeNull()
    )
    vi.useRealTimers()
  })

  it('confirmed: poll returning confirmed calls save_wechat_ilink_token and onSaved', async () => {
    vi.useFakeTimers()
    invokeMock
      .mockResolvedValueOnce({ qrcode: 'qr123' })
      .mockResolvedValueOnce({ status: 'confirmed', bot_token: 'tok999', account_id: 'acc456' })
      .mockResolvedValueOnce(undefined)                          // save_wechat_ilink_token
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    await act(async () => { screen.getByText('获取二维码').click() })
    await act(async () => { vi.advanceTimersByTime(2100) })
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        'save_wechat_ilink_token',
        expect.objectContaining({ instanceId: 'inst-1', botToken: 'tok999', accountId: 'acc456' })
      )
    )
    expect(PROPS.onSaved).toHaveBeenCalledOnce()
    vi.useRealTimers()
  })
})
```

- [ ] **Step 4: Run tests to confirm they fail**

```bash
cd ui && npm test -- --run src/components/settings/WechatIlinkBindingPanel.test.tsx 2>&1 | tail -10
```

Expected: all 4 tests fail (component doesn't exist yet).

- [ ] **Step 5: Implement `WechatIlinkBindingPanel.tsx`**

Create `ui/src/components/settings/WechatIlinkBindingPanel.tsx`:

```typescript
import { useState, useEffect, useRef, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import QRCode from 'qrcode'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'

type BindState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'qr-shown'; qrcode: string }
  | { kind: 'scanning'; qrcode: string }
  | { kind: 'confirmed' }
  | { kind: 'qr-expired' }
  | { kind: 'error'; message: string }

interface Props {
  instanceId: string
  accountId?: string
  status: ImChannelStatus | undefined
  onSaved: () => void
  onDisconnect: () => void
}

export function WechatIlinkBindingPanel({
  instanceId, accountId, status, onSaved, onDisconnect,
}: Props) {
  const [bindState, setBindState] = useState<BindState>(
    accountId ? { kind: 'confirmed' } : { kind: 'idle' }
  )
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const pollStartRef = useRef<number>(0)

  const stopPolling = useCallback(() => {
    if (pollRef.current !== null) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  useEffect(() => () => { stopPolling() }, [stopPolling])

  // Render QR canvas whenever qr-shown or scanning state is entered
  useEffect(() => {
    if (
      (bindState.kind === 'qr-shown' || bindState.kind === 'scanning') &&
      canvasRef.current
    ) {
      QRCode.toCanvas(canvasRef.current, bindState.qrcode, { width: 128 }).catch(() => {})
    }
  }, [bindState])

  // Auto-trigger QR fetch on iLink session expiry (-14)
  useEffect(() => {
    if (status?.state === 'needs_rebind') {
      fetchQr()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status?.state])

  async function fetchQr() {
    stopPolling()
    setBindState({ kind: 'loading' })
    try {
      const result = await invoke<{ qrcode: string }>(
        'request_wechat_ilink_qrcode',
        { instanceId }
      )
      setBindState({ kind: 'qr-shown', qrcode: result.qrcode })
      startPolling(result.qrcode)
    } catch (e) {
      setBindState({ kind: 'error', message: String(e) })
    }
  }

  function startPolling(qrcode: string) {
    stopPolling()
    pollStartRef.current = Date.now()
    pollRef.current = setInterval(async () => {
      if (Date.now() - pollStartRef.current > 120_000) {
        stopPolling()
        setBindState({ kind: 'qr-expired' })
        return
      }
      try {
        const result = await invoke<{
          status: string
          bot_token?: string
          account_id?: string
        }>('poll_wechat_ilink_qrcode_status', { instanceId, qrcode })

        if (result.status === 'scaned') {
          setBindState({ kind: 'scanning', qrcode })
        } else if (result.status === 'confirmed' && result.bot_token && result.account_id) {
          stopPolling()
          await saveToken(result.bot_token, result.account_id, qrcode)
        } else if (result.status === 'expired') {
          stopPolling()
          setBindState({ kind: 'qr-expired' })
        }
      } catch {
        // Network error during poll — keep retrying
      }
    }, 2000)
  }

  async function saveToken(botToken: string, accId: string, qrcode: string) {
    try {
      await invoke('save_wechat_ilink_token', {
        instanceId,
        botToken,
        accountId: accId,
      })
      setBindState({ kind: 'confirmed' })
      onSaved()
    } catch (e) {
      toast.error('保存绑定信息失败：' + String(e))
      setBindState({ kind: 'qr-shown', qrcode })
      startPolling(qrcode)
    }
  }

  async function handleDisconnect() {
    stopPolling()
    try {
      await invoke('disconnect_wechat_ilink', { instanceId })
      setBindState({ kind: 'idle' })
      onDisconnect()
    } catch (e) {
      toast.error('断开失败：' + String(e))
    }
  }

  if (bindState.kind === 'idle') {
    return (
      <div className="flex flex-col items-center gap-3 py-4">
        <p className="text-xs text-muted-foreground text-center">
          扫描二维码将此渠道与您的微信账号绑定，即可收发消息
        </p>
        <button
          type="button"
          onClick={fetchQr}
          className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
        >
          获取二维码
        </button>
      </div>
    )
  }

  if (bindState.kind === 'loading') {
    return (
      <div className="flex items-center justify-center py-8">
        <span className="text-sm text-muted-foreground">正在获取二维码…</span>
      </div>
    )
  }

  if (bindState.kind === 'qr-shown' || bindState.kind === 'scanning') {
    return (
      <div className="flex flex-col items-center gap-2 py-3">
        <canvas ref={canvasRef} width={128} height={128} className="rounded border border-border" />
        <p className="text-xs text-muted-foreground">
          {bindState.kind === 'scanning' ? '已扫码，等待确认…' : '用微信扫码绑定账号'}
        </p>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={fetchQr}
            className="text-xs text-muted-foreground hover:underline"
          >
            刷新二维码
          </button>
          <span className="text-xs text-muted-foreground">·</span>
          <button
            type="button"
            onClick={() => { stopPolling(); setBindState({ kind: 'idle' }) }}
            className="text-xs text-muted-foreground hover:underline"
          >
            取消
          </button>
        </div>
      </div>
    )
  }

  if (bindState.kind === 'qr-expired') {
    return (
      <div className="flex flex-col items-center gap-2 py-4">
        <p className="text-sm text-amber-500">二维码已过期</p>
        <button
          type="button"
          onClick={fetchQr}
          className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
        >
          重新获取
        </button>
      </div>
    )
  }

  if (bindState.kind === 'error') {
    return (
      <div className="flex flex-col items-center gap-2 py-4">
        <p className="text-sm text-destructive text-center">{bindState.message}</p>
        <button
          type="button"
          onClick={() => setBindState({ kind: 'idle' })}
          className="text-xs text-muted-foreground hover:underline"
        >
          重试
        </button>
      </div>
    )
  }

  // confirmed
  return (
    <div className="rounded border border-success/30 bg-success/5 p-3 space-y-2">
      <div className="flex items-center gap-2">
        <span className="w-2 h-2 rounded-full bg-success flex-shrink-0" />
        <span className="text-xs font-medium text-success">已绑定</span>
      </div>
      {accountId && (
        <p className="text-xs text-muted-foreground">账号: {accountId}</p>
      )}
      <div className="flex items-center gap-2 pt-1">
        <button
          type="button"
          onClick={fetchQr}
          className="text-xs text-muted-foreground hover:underline"
        >
          重新绑定
        </button>
        <span className="text-xs text-muted-foreground">·</span>
        <button
          type="button"
          onClick={handleDisconnect}
          className="text-xs text-destructive hover:underline"
        >
          断开连接
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 6: Run tests to confirm all 4 pass**

```bash
cd ui && npm test -- --run src/components/settings/WechatIlinkBindingPanel.test.tsx 2>&1 | tail -10
```

Expected: `4 passed`.

- [ ] **Step 7: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add ui/package.json ui/package-lock.json ui/src/atoms/im-channel-atoms.ts ui/src/components/settings/WechatIlinkBindingPanel.tsx ui/src/components/settings/WechatIlinkBindingPanel.test.tsx
git commit -m "feat(ui): WechatIlinkBindingPanel QR binding state machine + atoms needs_rebind"
```

---

## Task 5: `ImChannelAccordionRow.tsx` — replace wechat_ilink form with BindingPanel

**Files:**
- Modify: `ui/src/components/settings/ImChannelAccordionRow.tsx`
- Modify: `ui/src/components/settings/ImChannelAccordionRow.test.tsx`

- [ ] **Step 1: Write the failing test**

In `ui/src/components/settings/ImChannelAccordionRow.test.tsx`, add a new test inside `describe('ImChannelAccordionRow', ...)`:

```typescript
it('wechat_ilink expanded shows binding panel, not app_id field', () => {
  renderRow({
    channel: {
      ...BASE_CHANNEL,
      channelType: 'wechat_ilink',
      config: { account_id: 'wx_user_abc' },
    },
    open: true,
  })
  // Should show binding panel content
  expect(screen.getByText('重新绑定')).not.toBeNull()
  // Must NOT show the old app_id or api_key form fields
  expect(screen.queryByText('App ID')).toBeNull()
  expect(screen.queryByText('API Key')).toBeNull()
})
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd ui && npm test -- --run src/components/settings/ImChannelAccordionRow.test.tsx 2>&1 | tail -10
```

Expected: the new test fails (wechat_ilink still shows App ID field).

- [ ] **Step 3: Add import for WechatIlinkBindingPanel at the top of `ImChannelAccordionRow.tsx`**

After the existing imports (line 4):

```typescript
import { WechatIlinkBindingPanel } from './WechatIlinkBindingPanel'
```

- [ ] **Step 4: Update `getMetaLine` for `wechat_ilink` in `ImChannelAccordionRow.tsx`**

Replace the wechat_ilink branch in `getMetaLine` (lines 35–38):

```typescript
if (ct === 'wechat_ilink') {
  const accountId = (channel.config.account_id as string | undefined) ?? ''
  if (status?.state === 'needs_rebind') return `账号: ${accountId.slice(0, 16) || '未知'} · 需要重新绑定`
  if (accountId) return `账号: ${accountId.slice(0, 16)}`
  return '未绑定'
}
```

- [ ] **Step 5: Add `needs_rebind` to the status dot color in the closed row**

Find the inline ternary for the status dot (lines ~298–305):

```tsx
<span
  className={`w-2 h-2 rounded-full flex-shrink-0 ${
    status?.state === 'online'
      ? 'bg-success animate-pulse'
      : status?.state === 'needs_rebind'
      ? 'bg-amber-400'
      : status?.state === 'error'
      ? 'bg-destructive'
      : 'bg-muted-foreground'
  }`}
/>
```

- [ ] **Step 6: Replace the `wechat_ilink` section inside the expanded grid with BindingPanel**

In the expanded content's channel-specific grid section, replace the wechat_ilink block (lines 436–463):

```tsx
{channelType === 'wechat_ilink' && (
  <>
    <div className="col-span-2">
      <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
      <select
        value={spaceId}
        onChange={e => { setSpaceId(e.target.value); markDirty() }}
        className={inputCls()}
      >
        {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
      </select>
    </div>
    {!isNew && (
      <div className="col-span-2">
        <WechatIlinkBindingPanel
          instanceId={channel!.id}
          accountId={channel!.config.account_id as string | undefined}
          status={status}
          onSaved={onSaved}
          onDisconnect={onSaved}
        />
      </div>
    )}
  </>
)}
```

- [ ] **Step 7: Remove `appId`, `apiKey` state and their `useEffect` sync entries**

In the state declarations (lines 81–82), remove:
```typescript
const [appId, setAppId] = useState((channel?.config.app_id as string | undefined) ?? '')
const [apiKey, setApiKey] = useState('')
```

In the `useEffect([channel])` sync block (lines 117–118), remove:
```typescript
setAppId((channel.config.app_id as string | undefined) ?? '')
```

In `handleCancel()` (lines 143–144), remove:
```typescript
setAppId((channel?.config.app_id as string | undefined) ?? '')
setApiKey('')
```

In `buildInput()` wechat_ilink case (lines 169–172), replace:
```typescript
case 'wechat_ilink':
  config = {}
  credentials = {}
  break
```

- [ ] **Step 8: Run the failing test to confirm it now passes**

```bash
cd ui && npm test -- --run src/components/settings/ImChannelAccordionRow.test.tsx 2>&1 | tail -10
```

Expected: all tests pass including the new `wechat_ilink` test.

- [ ] **Step 9: Run full Vitest suite**

```bash
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: all previously passing tests still pass (pre-existing failures in GeneralTab, IntelligenceTab, etc. are unrelated to this PR).

- [ ] **Step 10: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 11: Commit**

```bash
git add ui/src/components/settings/ImChannelAccordionRow.tsx ui/src/components/settings/ImChannelAccordionRow.test.tsx
git commit -m "feat(ui): replace wechat_ilink form with WechatIlinkBindingPanel in accordion"
```

---

## Final verification

- [ ] **Full backend test suite**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Full frontend test suite**

```bash
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: no new failures vs. branch baseline.

- [ ] **Full backend compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output.
