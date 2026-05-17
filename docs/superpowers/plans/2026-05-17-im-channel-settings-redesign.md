# IM 渠道设置 UI 重设计 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the full-page-swap IM channel settings UI with a tab-per-type + accordion-per-instance design that shows live connection status without leaving the list.

**Architecture:** Add `ChannelRuntimeStatus` to the backend types, have WecomBot emit status via an unbounded MPSC channel relayed by `ImChannelManager` to the frontend via `im_channel_status_changed` Tauri event. On the frontend, rewrite `ImChannelsSettings` into a tab container and introduce `ImChannelAccordionRow` for per-instance inline editing with a connection-aware status block.

**Tech Stack:** Rust/Tokio (channels, async tasks), Tauri v2 `app_handle.emit()`, React 18 + Jotai atoms, Tailwind CSS tokens, `sonner` toast, Vitest + React Testing Library.

---

## File Structure

| File | Change | Why |
|------|--------|-----|
| `src-tauri/src/channels/types.rs` | Add `ChannelRuntimeStatus` + `ChannelState` | Shared between manager and wecom, avoid circular deps |
| `src-tauri/src/channels/im/wecom.rs` | Add `status_tx` field + emit on connect/error | WecomBot is the only bidirectional channel with live status now |
| `src-tauri/src/channels/manager.rs` | Add `statuses` map, `spawn_status_relay()`, update `RunningInstance` | Manager owns lifetime of relay task |
| `src-tauri/src/tauri_commands.rs` | Add `get_im_channel_statuses` command | Frontend polls once on mount |
| `src-tauri/src/main.rs` | Register new command in `invoke_handler!` | Required for all Tauri commands |
| `ui/src/atoms/im-channel-atoms.ts` | Add `ImChannelStatus` type + two new atoms | Status stored separately from row data |
| `ui/src/components/settings/ImChannelsSettings.tsx` | Full rewrite → tab container | Orchestrates tabs, IPC subscription, accordion state |
| `ui/src/components/settings/ImChannelAccordionRow.tsx` | New file | Single instance row: closed + expanded states |
| `ui/src/components/settings/ImChannelsSettings.test.tsx` | New file | Tab rendering, badge counts, toggle optimistic |
| `ui/src/components/settings/ImChannelAccordionRow.test.tsx` | New file | Status block, dirty tracking, save button label |

---

## Task 1: Backend Types — ChannelRuntimeStatus + ChannelState

**Files:**
- Modify: `src-tauri/src/channels/types.rs` (append before `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test in types.rs**

Open `src-tauri/src/channels/types.rs`. Inside the existing `#[cfg(test)]` block, add this test **before** the closing `}`:

```rust
    #[test]
    fn channel_runtime_status_serializes_correctly() {
        let s = ChannelRuntimeStatus {
            instance_id: "inst-1".into(),
            state: ChannelState::Online,
            last_error: None,
            connected_since_ms: Some(1_700_000_000_000),
            message_count_today: 42,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"state\":\"online\""));
        assert!(json.contains("\"instance_id\":\"inst-1\""));
        assert!(json.contains("\"message_count_today\":42"));

        let err_s = ChannelRuntimeStatus {
            instance_id: "inst-2".into(),
            state: ChannelState::Error,
            last_error: Some("认证失败".into()),
            connected_since_ms: None,
            message_count_today: 0,
        };
        let json2 = serde_json::to_string(&err_s).unwrap();
        assert!(json2.contains("\"state\":\"error\""));
        assert!(json2.contains("认证失败"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::types::tests::channel_runtime_status 2>&1 | tail -5
```

Expected: FAIL — `ChannelRuntimeStatus` not defined yet.

- [ ] **Step 3: Add the types to types.rs**

In `src-tauri/src/channels/types.rs`, insert after the closing `}` of `impl std::fmt::Display for ImChannelType` (around line 39, before `/// Per-user permission policy`):

```rust
/// Runtime connection state for a bidirectional channel instance.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelState {
    Online,
    Error,
    Offline,
}

/// Live status snapshot — emitted as `im_channel_status_changed` Tauri event
/// and returned by `get_im_channel_statuses`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelRuntimeStatus {
    pub instance_id: String,
    pub state: ChannelState,
    pub last_error: Option<String>,
    /// Epoch ms when last connected (Some only when state == Online).
    pub connected_since_ms: Option<i64>,
    /// Messages received today (resets on restart; 0 for notify-only channels).
    pub message_count_today: u32,
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd src-tauri && cargo test --lib channels::types::tests::channel_runtime_status 2>&1 | tail -5
```

Expected: `test channels::types::tests::channel_runtime_status_serializes_correctly ... ok`

- [ ] **Step 5: Compile check**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no lines.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/types.rs
git commit -m "feat(channels): add ChannelRuntimeStatus + ChannelState types"
```

---

## Task 2: ImChannelManager — statuses map + relay infrastructure

**Files:**
- Modify: `src-tauri/src/channels/manager.rs`

Background: `ImChannelManager` needs a shared `statuses` map that WecomBot can write to (via a relay task) and `get_im_channel_statuses` can read from. We use an MPSC channel: WecomBot sends `ChannelRuntimeStatus` to an unbounded sender; `spawn_status_relay()` consumes from the receiver, writes to the map, and emits the Tauri IPC event.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/channels/manager.rs`, inside the existing `mod tests { ... }` block (around line 368), add:

```rust
    #[tokio::test]
    async fn statuses_map_starts_empty() {
        // We can't call start_all() without a real DB in unit tests,
        // but we can verify the map is empty on construction.
        // Use the existing test-helper that builds a manager with an in-memory DB.
        let manager = build_test_manager();
        let statuses = manager.statuses.read().await;
        assert!(statuses.is_empty());
    }

    #[tokio::test]
    async fn get_all_statuses_returns_inserted_entry() {
        let manager = build_test_manager();
        let status = crate::channels::types::ChannelRuntimeStatus {
            instance_id: "x1".into(),
            state: crate::channels::types::ChannelState::Online,
            last_error: None,
            connected_since_ms: Some(1_700_000_000_000),
            message_count_today: 5,
        };
        manager.statuses.write().await.insert("x1".into(), status);
        let all = manager.get_all_statuses().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].instance_id, "x1");
    }
```

Note: `build_test_manager()` is already defined later in the tests block. If it's not yet accessible at this location, place these tests after the `build_test_manager` helper.

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test --lib channels::manager::tests::statuses_map 2>&1 | tail -5
cd src-tauri && cargo test --lib channels::manager::tests::get_all_statuses 2>&1 | tail -5
```

Expected: FAIL — `statuses` field and `get_all_statuses` method not defined.

- [ ] **Step 3: Add statuses field and relay infrastructure to manager.rs**

At the top of `src-tauri/src/channels/manager.rs`, add to the use statements:

```rust
use crate::channels::types::{ChannelRuntimeStatus, ImChannelInstanceConfig, ImChannelSender, ImChannelType};
```

(Replace the existing `use crate::channels::types::{ImChannelInstanceConfig, ImChannelSender, ImChannelType};` line.)

Update `RunningInstance` to add the relay task field:

```rust
struct RunningInstance {
    config: ImChannelInstanceConfig,
    sender: Arc<dyn ImChannelSender>,
    /// Present for bidirectional channels (WeCom/iLink); None for notify-only.
    _inbound_task: Option<AbortHandle>,
    /// Present when config.enabled=true for bidirectional channels; None otherwise.
    _fanout_task: Option<AbortHandle>,
    /// Consumes ChannelRuntimeStatus from WecomBot and updates the shared statuses map.
    _status_relay_task: Option<AbortHandle>,
}
```

Add `statuses` field to `ImChannelManager`:

```rust
pub struct ImChannelManager {
    instances: Arc<RwLock<HashMap<String, RunningInstance>>>,
    pub statuses: Arc<RwLock<HashMap<String, ChannelRuntimeStatus>>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    session_registry: Arc<crate::channels::session_registry::ImSessionRegistry>,
    app_handle: tauri::AppHandle,
}
```

Update `ImChannelManager::new()` to initialize `statuses`:

```rust
pub fn new(
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    session_registry: Arc<crate::channels::session_registry::ImSessionRegistry>,
    app_handle: tauri::AppHandle,
) -> Self {
    Self {
        instances: Arc::new(RwLock::new(HashMap::new())),
        statuses: Arc::new(RwLock::new(HashMap::new())),
        db,
        session_registry,
        app_handle,
    }
}
```

Add `get_all_statuses()` and `spawn_status_relay()` methods after `list_instances()`:

```rust
/// Return a snapshot of all known channel runtime statuses.
pub async fn get_all_statuses(&self) -> Vec<ChannelRuntimeStatus> {
    self.statuses.read().await.values().cloned().collect()
}

/// Spawn a task that drains `status_rx`, updates the shared statuses map,
/// and emits `im_channel_status_changed` to the Tauri frontend.
/// Returns an AbortHandle so stop_instance() can cancel it.
fn spawn_status_relay(
    &self,
    mut status_rx: tokio::sync::mpsc::UnboundedReceiver<ChannelRuntimeStatus>,
) -> AbortHandle {
    use tauri::Emitter;
    let statuses = self.statuses.clone();
    let app_handle = self.app_handle.clone();
    let handle = tokio::spawn(async move {
        while let Some(status) = status_rx.recv().await {
            statuses.write().await.insert(status.instance_id.clone(), status.clone());
            let _ = app_handle.emit("im_channel_status_changed", &status);
        }
    });
    handle.abort_handle()
}
```

- [ ] **Step 4: Update start_instance() for WecomBot to create a status channel**

In `start_instance()`, find the `ImChannelType::WecomBot` arm and replace it. The current code (around line 96–123) creates a `WecomSender` without status_tx. Replace that arm with:

```rust
ImChannelType::WecomBot => {
    let (inbound_tx, inbound_rx) =
        tokio::sync::mpsc::unbounded_channel::<(
            crate::channels::types::InboundMessage,
            Arc<crate::channels::types::ReplyHandle>,
        )>();
    let (status_tx, status_rx) =
        tokio::sync::mpsc::unbounded_channel::<ChannelRuntimeStatus>();
    let wecom = Arc::new(crate::channels::im::WecomSender::new(
        &config.id,
        &config.config,
        &config.credentials,
        status_tx,
    ));
    let abort = if config.enabled {
        Some(wecom.clone().start(inbound_tx))
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
    // Store the running instance with all three task handles
    let running = RunningInstance {
        config: config.clone(),
        sender: Arc::new(WecomNoopSender) as Arc<dyn ImChannelSender>,
        _inbound_task: abort,
        _fanout_task: fanout_abort,
        _status_relay_task: relay_abort,
    };
    self.instances.write().await.insert(config.id.clone(), running);
    tracing::info!("ImChannelManager: started instance {} ({})", config.id, config.channel_type);
    return Ok(());
}
```

For the `WechatIlink` arm, keep no status_tx (iLink status not in scope); add `_status_relay_task: None` to its `RunningInstance`. Find the `WechatIlink` arm's `RunningInstance` construction and add the field:

```rust
let running = RunningInstance {
    config: config.clone(),
    sender: Arc::new(IlinkNoopSender) as Arc<dyn ImChannelSender>,
    _inbound_task: abort,
    _fanout_task: fanout_abort,
    _status_relay_task: None,
};
self.instances.write().await.insert(config.id.clone(), running);
tracing::info!("ImChannelManager: started instance {} ({})", config.id, config.channel_type);
return Ok(());
```

For the `_ =>` (notify-only) arm, after building `running`:

```rust
let running = RunningInstance {
    config: config.clone(),
    sender,
    _inbound_task: None,
    _fanout_task: None,
    _status_relay_task: None,
};
self.instances.write().await.insert(config.id.clone(), running);
```

Remove the original `RunningInstance` and `self.instances.write().await.insert(...)` lines that appear after the `match` block (they were the common insertion point; now each arm inserts itself and returns early).

Note: Because WecomBot and iLink now `return Ok(())` from their arms, the common insertion block at the end of `start_instance()` only applies to the `_ =>` arm. Move those lines into `_ =>` and remove the trailing common block.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test --lib channels::manager::tests::statuses_map 2>&1 | tail -5
cd src-tauri && cargo test --lib channels::manager::tests::get_all_statuses 2>&1 | tail -5
```

Expected: both pass.

- [ ] **Step 6: Compile check**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no lines. (If WecomSender::new() arity errors appear, that's expected until Task 3 — fix them in Task 3.)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/channels/manager.rs
git commit -m "feat(channels): add statuses map + status relay infrastructure to ImChannelManager"
```

---

## Task 3: WecomBot — status emission + manager wiring

**Files:**
- Modify: `src-tauri/src/channels/im/wecom.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/channels/im/wecom.rs`, find the end of the file and add before the final `}`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::types::{ChannelState, ChannelRuntimeStatus};

    #[test]
    fn wecom_sender_new_accepts_status_tx() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<ChannelRuntimeStatus>();
        let _sender = WecomSender::new(
            "inst-test",
            &serde_json::json!({}),
            &serde_json::json!({"bot_id": "b1", "secret": "s1"}),
            tx,
        );
        // If this compiles and runs, the constructor accepts status_tx correctly.
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib channels::im::wecom::tests 2>&1 | tail -5
```

Expected: FAIL — `WecomSender::new()` doesn't accept a 4th parameter yet.

- [ ] **Step 3: Update WecomSender to accept and use status_tx**

In `src-tauri/src/channels/im/wecom.rs`, update the `use` imports at the top to add:

```rust
use crate::channels::types::{ChannelRuntimeStatus, ChannelState, ImChannelSender, InboundMessage, ReplyHandle, StreamingHandle};
```

(Replace the existing `use crate::channels::types::{ImChannelSender, InboundMessage, ReplyHandle, StreamingHandle};`)

Update the `WecomSender` struct to add the `status_tx` field:

```rust
pub struct WecomSender {
    instance_id: String,
    bot_id: String,
    secret: String,
    ws_url: String,
    req_ids: Arc<RwLock<std::collections::HashMap<String, ReqIdEntry>>>,
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
    status_tx: mpsc::UnboundedSender<ChannelRuntimeStatus>,
}
```

Update `WecomSender::new()` to accept `status_tx`:

```rust
pub fn new(
    instance_id: &str,
    config: &serde_json::Value,
    credentials: &serde_json::Value,
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
```

- [ ] **Step 4: Emit status in connection_loop and connect_and_run**

In `connection_loop`, update it to emit `Error` status when `connect_and_run` fails, and `Offline` when max retries is reached:

```rust
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
```

In `connect_and_run`, emit `Online` after the WebSocket connection is established and subscribe message is sent. Add this block right after the `write.send(sub_msg.to_string()).await?;` line:

```rust
// Emit Online status — subscribe has been sent; treat connected as online.
let _ = self.status_tx.send(ChannelRuntimeStatus {
    instance_id: self.instance_id.clone(),
    state: ChannelState::Online,
    last_error: None,
    connected_since_ms: Some(chrono::Utc::now().timestamp_millis()),
    message_count_today: 0,
});
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
cd src-tauri && cargo test --lib channels::im::wecom::tests 2>&1 | tail -5
```

Expected: `test channels::im::wecom::tests::wecom_sender_new_accepts_status_tx ... ok`

- [ ] **Step 6: Full compile check**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: no lines.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/channels/im/wecom.rs src-tauri/src/channels/manager.rs
git commit -m "feat(channels): WecomBot emits ChannelRuntimeStatus on connect/error/timeout"
```

---

## Task 4: Tauri Command — get_im_channel_statuses

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add the command to tauri_commands.rs**

In `src-tauri/src/tauri_commands.rs`, add this function after `list_im_channels` (around line 2874). Add before the blank line that precedes `fn validate_im_channel_url`:

```rust
#[tauri::command]
pub async fn get_im_channel_statuses(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::channels::types::ChannelRuntimeStatus>, Error> {
    Ok(state.im_channel_manager.get_all_statuses().await)
}
```

- [ ] **Step 2: Register the command in main.rs**

In `src-tauri/src/main.rs`, find the `invoke_handler!` block and add the new command next to the other `im_channel` commands (around line 366–370):

```rust
uclaw_core::tauri_commands::get_im_channel_statuses,
```

Place it on the line immediately after `uclaw_core::tauri_commands::list_im_channels,`.

- [ ] **Step 3: Compile check**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no lines.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(ipc): add get_im_channel_statuses Tauri command"
```

---

## Task 5: Frontend Atoms — ImChannelStatus types + atoms

**Files:**
- Modify: `ui/src/atoms/im-channel-atoms.ts`

- [ ] **Step 1: Add the new types and atoms**

Replace the entire contents of `ui/src/atoms/im-channel-atoms.ts` with:

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

/** Runtime connection status for one channel instance. */
export interface ImChannelStatus {
  instanceId: string
  state: 'online' | 'error' | 'offline'
  lastError?: string
  /** Epoch ms when WebSocket connected. Defined only when state === 'online'. */
  connectedSinceMs?: number
  /** Messages received today (resets on restart). */
  messageCountToday?: number
}

export const imChannelsAtom = atom<ImChannelRow[]>([])

export const fetchImChannelsAtom = atom(null, async (_get, set) => {
  const rows = await invoke<ImChannelRow[]>('list_im_channels')
  set(imChannelsAtom, rows)
})

/** Map of instanceId → ImChannelStatus. Updated by IPC events and initial fetch. */
export const imChannelStatusesAtom = atom<Record<string, ImChannelStatus>>({})

export const fetchImChannelStatusesAtom = atom(null, async (_get, set) => {
  const statuses = await invoke<ImChannelStatus[]>('get_im_channel_statuses')
  const map: Record<string, ImChannelStatus> = {}
  for (const s of statuses) map[s.instanceId] = s
  set(imChannelStatusesAtom, map)
})
```

- [ ] **Step 2: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors mentioning `im-channel-atoms.ts`.

- [ ] **Step 3: Commit**

```bash
git add ui/src/atoms/im-channel-atoms.ts
git commit -m "feat(atoms): add ImChannelStatus type + imChannelStatusesAtom"
```

---

## Task 6: Frontend — ImChannelsSettings.tsx Rewrite

**Files:**
- Modify: `ui/src/components/settings/ImChannelsSettings.tsx`

This component becomes a pure orchestrator: Tab bar, IPC event subscription, accordion row list, "add new instance" dashed button.

- [ ] **Step 1: Replace ImChannelsSettings.tsx**

Replace the entire file with:

```tsx
import { useAtom, useSetAtom } from 'jotai'
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import {
  imChannelsAtom,
  fetchImChannelsAtom,
  imChannelStatusesAtom,
  fetchImChannelStatusesAtom,
} from '@/atoms/im-channel-atoms'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'
import { ImChannelAccordionRow } from './ImChannelAccordionRow'
import type { SpaceSummary } from '@/lib/types'

const CHANNEL_TYPES_ORDER = [
  'wecom_bot', 'wechat_ilink', 'email', 'dingtalk', 'feishu', 'webhook',
]

const CHANNEL_TYPE_LABELS: Record<string, string> = {
  wecom_bot:    '企业微信',
  wechat_ilink: '微信 iLink',
  email:        '邮件',
  dingtalk:     '钉钉',
  feishu:       '飞书',
  webhook:      'Webhook',
}

const CHANNEL_DESCRIPTIONS: Record<string, string> = {
  wecom_bot:    '企业微信 Bot 通过 WebSocket 长连接收发消息，每个实例对应一个独立的 Corp App。',
  wechat_ilink: '微信 iLink 通过 HTTP 长轮询桥接个人微信账号，需配合 iLink 桥接服务运行。',
  email:        '通过 SMTP 发送邮件通知，适用于低频告警场景。',
  dingtalk:     '钉钉 Webhook 通知，不支持双向对话。',
  feishu:       '飞书 Webhook 通知，不支持双向对话。',
  webhook:      '通用 HTTP Webhook，POST JSON 到目标 URL。',
}

export function ImChannelsSettings() {
  const [channels, setChannels] = useAtom(imChannelsAtom)
  const fetchChannels = useSetAtom(fetchImChannelsAtom)
  const [statuses, setStatuses] = useAtom(imChannelStatusesAtom)
  const fetchStatuses = useSetAtom(fetchImChannelStatusesAtom)
  const [spaces, setSpaces] = useState<{ id: string; name: string }[]>([])
  const [activeTab, setActiveTab] = useState<string | null>(null)
  const [openRowId, setOpenRowId] = useState<string | null>(null)
  const [addingToType, setAddingToType] = useState<string | null>(null)

  useEffect(() => {
    fetchChannels()
    fetchStatuses()
    invoke<SpaceSummary[]>('list_spaces')
      .then(rows => setSpaces(rows.map(s => ({ id: s.id, name: s.name }))))
      .catch(() => {})
  }, [fetchChannels, fetchStatuses])

  // Realtime status updates from backend
  useEffect(() => {
    const unlisten = listen<ImChannelStatus>('im_channel_status_changed', ({ payload }) => {
      setStatuses(prev => ({ ...prev, [payload.instanceId]: payload }))
    })
    return () => { unlisten.then(fn => fn()) }
  }, [setStatuses])

  // Group channels by type
  const channelsByType: Record<string, typeof channels> = {}
  for (const ch of channels) {
    if (!channelsByType[ch.channelType]) channelsByType[ch.channelType] = []
    channelsByType[ch.channelType].push(ch)
  }

  const tabs = CHANNEL_TYPES_ORDER.filter(t => (channelsByType[t]?.length ?? 0) > 0)
  const currentTab = (activeTab && tabs.includes(activeTab)) ? activeTab : (tabs[0] ?? null)

  async function handleToggle(id: string, enabled: boolean) {
    setChannels(prev => prev.map(ch => ch.id === id ? { ...ch, enabled } : ch))
    try {
      await invoke('toggle_im_channel', { id, enabled })
    } catch (e) {
      fetchChannels()
      toast.error('切换失败：' + String(e))
    }
  }

  function handleToggleRow(id: string) {
    setOpenRowId(prev => (prev === id ? null : id))
    setAddingToType(null)
  }

  function handleSaved() {
    setOpenRowId(null)
    setAddingToType(null)
    fetchChannels()
    fetchStatuses()
  }

  async function handleDelete(id: string) {
    if (!confirm('确定删除此渠道实例？')) return
    try {
      await invoke('delete_im_channel', { id })
      fetchChannels()
    } catch (e) {
      toast.error('删除失败：' + String(e))
    }
  }

  const tabChannels = currentTab ? (channelsByType[currentTab] ?? []) : []

  return (
    <div className="space-y-0">
      {/* Tab bar */}
      <div className="flex items-end gap-0 border-b border-border overflow-x-auto">
        {tabs.map(type => {
          const count = channelsByType[type]?.length ?? 0
          const hasError = (channelsByType[type] ?? []).some(
            ch => statuses[ch.id]?.state === 'error'
          )
          return (
            <button
              key={type}
              onClick={() => { setActiveTab(type); setOpenRowId(null); setAddingToType(null) }}
              className={[
                'flex items-center gap-1.5 whitespace-nowrap px-3 py-2 text-sm border-b-2 transition-colors',
                currentTab === type
                  ? 'border-primary font-medium text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              ].join(' ')}
            >
              {CHANNEL_TYPE_LABELS[type] ?? type}
              {count > 0 && (
                <span className={[
                  'rounded-full px-1.5 py-0.5 text-xs font-medium leading-none',
                  hasError
                    ? 'bg-destructive text-destructive-foreground'
                    : 'bg-muted text-muted-foreground',
                ].join(' ')}>
                  {count}
                </span>
              )}
            </button>
          )
        })}
        <span className="ml-auto cursor-not-allowed px-3 py-2 text-sm text-muted-foreground opacity-40">
          + 新增渠道类型
        </span>
      </div>

      {currentTab ? (
        <div className="pt-3 space-y-1.5">
          {CHANNEL_DESCRIPTIONS[currentTab] && (
            <p className="text-xs text-muted-foreground px-1 pb-2">
              {CHANNEL_DESCRIPTIONS[currentTab]}
            </p>
          )}

          {tabChannels.map(ch => (
            <ImChannelAccordionRow
              key={ch.id}
              channel={ch}
              status={statuses[ch.id]}
              spaces={spaces}
              open={openRowId === ch.id}
              onToggleOpen={() => handleToggleRow(ch.id)}
              onToggleEnabled={(enabled) => handleToggle(ch.id, enabled)}
              onSaved={handleSaved}
              onDeleted={() => handleDelete(ch.id)}
            />
          ))}

          {/* New instance row */}
          {addingToType === currentTab ? (
            <ImChannelAccordionRow
              key="__new__"
              channel={undefined}
              newChannelType={currentTab}
              status={undefined}
              spaces={spaces}
              open={true}
              onToggleOpen={() => setAddingToType(null)}
              onToggleEnabled={() => {}}
              onSaved={handleSaved}
              onDeleted={() => setAddingToType(null)}
            />
          ) : (
            <button
              onClick={() => { setAddingToType(currentTab); setOpenRowId(null) }}
              className="flex w-full items-center gap-2 rounded border border-dashed border-border px-3 py-2 text-sm text-primary opacity-70 hover:opacity-100 transition-opacity"
            >
              <span className="text-base leading-none">+</span>
              新增{CHANNEL_TYPE_LABELS[currentTab] ?? currentTab}实例
            </button>
          )}
        </div>
      ) : (
        <div className="py-10 text-center text-sm text-muted-foreground">
          还没有配置任何渠道。
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep "ImChannelsSettings\|ImChannelAccordionRow" | head -10
```

Expected: errors about `ImChannelAccordionRow` not found (that's Task 7). No other errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/settings/ImChannelsSettings.tsx
git commit -m "feat(settings): rewrite ImChannelsSettings with tab nav + accordion orchestration"
```

---

## Task 7: Frontend — ImChannelAccordionRow (new component)

**Files:**
- Create: `ui/src/components/settings/ImChannelAccordionRow.tsx`

This is the core UI component. Handles both "edit existing" and "create new" modes. Shows: closed row (status dot, name, error badge, space badge, inline toggle, chevron, meta line) and open row (status block, 2-col credential fields, options row, bottom bar).

- [ ] **Step 1: Create ImChannelAccordionRow.tsx**

Create `ui/src/components/settings/ImChannelAccordionRow.tsx` with this complete implementation:

```tsx
import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import type { ImChannelRow, ImChannelInput, ImChannelStatus } from '@/atoms/im-channel-atoms'

// ──────────────── helpers ────────────────

function formatDuration(fromMs: number): string {
  const secs = Math.floor((Date.now() - fromMs) / 1000)
  if (secs < 60) return `${secs}s`
  const mins = Math.floor(secs / 60) % 60
  const hours = Math.floor(secs / 3600)
  if (hours > 0) return `${hours}h ${mins}m`
  return `${mins}m`
}

function getMetaLine(channel: ImChannelRow, status?: ImChannelStatus): string {
  const ct = channel.channelType
  if (ct === 'wecom_bot') {
    const corpId = (channel.config.corp_id as string | undefined) ?? ''
    const prefix = corpId.length > 10 ? corpId.slice(0, 10) + '…' : corpId
    if (status?.state === 'online') {
      const since = status.connectedSinceMs
        ? `在线 ${formatDuration(status.connectedSinceMs)}`
        : '在线'
      const count = status.messageCountToday ? ` · 今日 ${status.messageCountToday} 条` : ''
      return `corp_id: ${prefix} · ${since}${count}`
    }
    if (status?.state === 'error') {
      const snippet = status.lastError?.slice(0, 50) ?? '连接错误'
      return `corp_id: ${prefix} · ${snippet}`
    }
    return `corp_id: ${prefix} · 已停用`
  }
  if (ct === 'wechat_ilink') {
    const appId = (channel.config.app_id as string | undefined) ?? ''
    return `app_id: ${appId.slice(0, 12) || '未设置'}`
  }
  const url =
    (channel.config.url as string | undefined) ??
    (channel.config.webhook_url as string | undefined) ?? ''
  return url ? `url: ${url.slice(0, 50)}${url.length > 50 ? '…' : ''}` : ''
}

// ──────────────── props ────────────────

interface Props {
  channel?: ImChannelRow       // undefined = new-instance mode
  newChannelType?: string      // required when channel is undefined
  status?: ImChannelStatus
  spaces: { id: string; name: string }[]
  open: boolean
  onToggleOpen: () => void
  onToggleEnabled: (enabled: boolean) => void
  onSaved: () => void
  onDeleted: () => void
}

// ──────────────── component ────────────────

export function ImChannelAccordionRow({
  channel, newChannelType, status, spaces, open,
  onToggleOpen, onToggleEnabled, onSaved, onDeleted,
}: Props) {
  const isNew = channel === undefined
  const channelType = channel?.channelType ?? newChannelType ?? 'webhook'

  // ── field state (initialized from channel or empty) ──
  const [name, setName] = useState(channel?.name ?? '')
  const [spaceId, setSpaceId] = useState(channel?.spaceId ?? spaces[0]?.id ?? '')
  const [streaming, setStreaming] = useState(channel?.streaming ?? false)
  const [permissionEnabled, setPermissionEnabled] = useState(channel?.permissionEnabled ?? false)
  const [owners, setOwners] = useState(channel?.owners.join(', ') ?? '')
  const [mcpEnabled, setMcpEnabled] = useState(channel?.guestPolicy.mcp_enabled ?? false)

  // channel-type-specific
  const [corpId] = useState((channel?.config.corp_id as string | undefined) ?? '')
  const [agentId] = useState((channel?.config.agent_id as string | undefined) ?? '')
  const [corpSecret, setCorpSecret] = useState('')
  const [wecomWsUrl, setWecomWsUrl] = useState((channel?.config.ws_url as string | undefined) ?? '')
  const [appId, setAppId] = useState((channel?.config.app_id as string | undefined) ?? '')
  const [apiKey, setApiKey] = useState('')
  const [webhookUrl, setWebhookUrl] = useState(
    (channel?.config.url as string | undefined) ??
    (channel?.config.webhook_url as string | undefined) ?? ''
  )
  const [signingSecret, setSigningSecret] = useState('')
  const [smtpHost, setSmtpHost] = useState((channel?.config.smtp_host as string | undefined) ?? '')
  const [smtpPort, setSmtpPort] = useState(String(channel?.config.smtp_port ?? '587'))
  const [smtpUser, setSmtpUser] = useState((channel?.config.username as string | undefined) ?? '')
  const [smtpPass, setSmtpPass] = useState('')
  const [toAddresses, setToAddresses] = useState(
    (channel?.config.to_addresses as string[] | undefined)?.join(', ') ?? ''
  )

  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Keep spaceId in sync if spaces loads after mount
  useEffect(() => {
    if (!channel && spaces.length > 0 && !spaceId) setSpaceId(spaces[0].id)
  }, [spaces, channel, spaceId])

  function markDirty() { setDirty(true) }

  function handleCancel() {
    // Reset all fields to initial values from channel
    setName(channel?.name ?? '')
    setSpaceId(channel?.spaceId ?? spaces[0]?.id ?? '')
    setStreaming(channel?.streaming ?? false)
    setPermissionEnabled(channel?.permissionEnabled ?? false)
    setOwners(channel?.owners.join(', ') ?? '')
    setMcpEnabled(channel?.guestPolicy.mcp_enabled ?? false)
    setCorpSecret('')
    setWecomWsUrl((channel?.config.ws_url as string | undefined) ?? '')
    setAppId((channel?.config.app_id as string | undefined) ?? '')
    setApiKey('')
    setWebhookUrl(
      (channel?.config.url as string | undefined) ??
      (channel?.config.webhook_url as string | undefined) ?? ''
    )
    setSigningSecret('')
    setSmtpHost((channel?.config.smtp_host as string | undefined) ?? '')
    setSmtpPort(String(channel?.config.smtp_port ?? '587'))
    setSmtpUser((channel?.config.username as string | undefined) ?? '')
    setSmtpPass('')
    setToAddresses((channel?.config.to_addresses as string[] | undefined)?.join(', ') ?? '')
    setDirty(false)
    setError(null)
    if (isNew) onDeleted()
    else onToggleOpen()
  }

  function buildInput(): ImChannelInput {
    let config: Record<string, unknown> = {}
    let credentials: Record<string, unknown> = {}
    switch (channelType) {
      case 'wecom_bot':
        config = { corp_id: corpId, agent_id: agentId, ...(wecomWsUrl ? { ws_url: wecomWsUrl } : {}) }
        credentials = corpSecret ? { corp_secret: corpSecret } : {}
        break
      case 'wechat_ilink':
        config = { app_id: appId }
        credentials = apiKey ? { api_key: apiKey } : {}
        break
      case 'dingtalk':
      case 'feishu':
        config = { webhook_url: webhookUrl }
        credentials = signingSecret ? { signing_secret: signingSecret } : {}
        break
      case 'email':
        config = {
          smtp_host: smtpHost,
          smtp_port: Number(smtpPort),
          username: smtpUser,
          to_addresses: toAddresses.split(',').map(s => s.trim()).filter(Boolean),
        }
        credentials = smtpPass ? { password: smtpPass } : {}
        break
      default: // webhook
        config = { url: webhookUrl }
        credentials = {}
    }
    return {
      spaceId,
      channelType,
      name,
      config,
      credentials,
      enabled: channel?.enabled ?? true,
      streaming,
      replyScope: 'all',
      permissionEnabled,
      owners: owners.split(',').map(s => s.trim()).filter(Boolean),
      guestPolicy: { tool_allowlist: [], mcp_enabled: mcpEnabled },
    }
  }

  async function handleSave() {
    if (channelType === 'email') {
      const port = Number(smtpPort)
      if (!Number.isInteger(port) || port < 1 || port > 65535) {
        setError('端口号必须是 1–65535 之间的整数')
        return
      }
    }
    setSaving(true)
    setError(null)
    try {
      const input = buildInput()
      if (isNew) {
        await invoke('create_im_channel', { input })
      } else {
        await invoke('update_im_channel', { id: channel!.id, input })
      }
      setDirty(false)
      onSaved()
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  async function handleStatusAction() {
    if (!channel) return
    const state = status?.state
    try {
      if (state === 'online') {
        await invoke('toggle_im_channel', { id: channel.id, enabled: false })
        onSaved()
      } else if (state === 'error') {
        // Reconnect: re-save with current (unmodified) input to trigger restart
        await invoke('update_im_channel', { id: channel.id, input: buildInput() })
        onSaved()
      } else {
        await invoke('toggle_im_channel', { id: channel.id, enabled: true })
        onSaved()
      }
    } catch (e) {
      toast.error(String(e))
    }
  }

  // ── save button label ──
  const saveLabel = (() => {
    if (!dirty) return '保存'
    if (status?.state === 'online') return '保存并重连'
    return '保存'
  })()

  // ── status block ──
  const stateColor = {
    online: 'bg-success/10 border-success/30',
    error:  'bg-destructive/10 border-destructive/30',
    offline: 'bg-muted border-border',
  }[status?.state ?? 'offline']

  const stateDotCls = {
    online:  'bg-success',
    error:   'bg-destructive',
    offline: 'bg-muted-foreground',
  }[status?.state ?? 'offline']

  const stateTitle = status?.state === 'online'
    ? `WebSocket 已连接${status.connectedSinceMs ? ` · 在线 ${formatDuration(status.connectedSinceMs)}` : ''}`
    : status?.state === 'error'
    ? `连接错误`
    : '未连接'

  const stateDetail = status?.state === 'online'
    ? status.messageCountToday ? `今日 ${status.messageCountToday} 条消息` : ''
    : status?.state === 'error'
    ? (status.lastError ?? '')
    : ''

  const stateActionLabel = status?.state === 'online' ? '停用' : status?.state === 'error' ? '重连' : '启用'
  const stateActionCls = status?.state === 'error'
    ? 'border-destructive/50 text-destructive'
    : 'border-border text-muted-foreground'

  // error-highlight credential fields
  const credHighlight = status?.state === 'error'

  // ── input CSS helpers ──
  const inputCls = (highlight = false) =>
    `w-full rounded border bg-background px-2 py-1.5 text-sm ${highlight ? 'border-destructive' : 'border-border'}`

  // ──────────────── closed row ────────────────
  const closedRow = (
    <div
      className="flex items-center justify-between px-3 py-2 cursor-pointer select-none"
      onClick={onToggleOpen}
    >
      <div className="flex items-center gap-2 min-w-0">
        {!isNew && (
          <span
            className={`w-2 h-2 rounded-full flex-shrink-0 ${
              status?.state === 'online'
                ? 'bg-success animate-pulse'
                : status?.state === 'error'
                ? 'bg-destructive'
                : 'bg-muted-foreground'
            }`}
          />
        )}
        <span className="text-sm font-medium truncate">
          {isNew ? `新${channelType === 'wecom_bot' ? '企业微信' : ''}实例` : channel!.name}
        </span>
        {!isNew && status?.state === 'error' && (
          <span className="rounded px-1.5 py-0.5 text-xs bg-destructive/10 border border-destructive/30 text-destructive whitespace-nowrap">
            {status.lastError?.slice(0, 10) ?? '连接错误'}
          </span>
        )}
        {!isNew && channel!.spaceId && (
          <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground whitespace-nowrap">
            {spaces.find(s => s.id === channel!.spaceId)?.name ?? channel!.spaceId}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 flex-shrink-0" onClick={e => e.stopPropagation()}>
        {!isNew && (
          <button
            type="button"
            aria-label={channel!.enabled ? '停用' : '启用'}
            onClick={() => onToggleEnabled(!channel!.enabled)}
            className={[
              'relative inline-flex h-4 w-8 cursor-pointer rounded-full border-2 border-transparent transition-colors',
              channel!.enabled ? 'bg-success' : 'bg-muted',
            ].join(' ')}
          >
            <span
              className={[
                'pointer-events-none inline-block h-3 w-3 rounded-full bg-white shadow transform transition-transform',
                channel!.enabled ? 'translate-x-4' : 'translate-x-0',
              ].join(' ')}
            />
          </button>
        )}
        <span
          className={`text-muted-foreground text-sm transition-transform ${open ? 'rotate-90' : ''}`}
        >
          ›
        </span>
      </div>
    </div>
  )

  // ── meta line (only when closed and not new) ──
  const metaLine = !isNew && !open && (
    <div className="px-3 pb-2 text-xs text-muted-foreground" onClick={onToggleOpen} style={{cursor:'pointer'}}>
      {getMetaLine(channel!, status)}
    </div>
  )

  // ──────────────── expanded content ────────────────
  const expandedContent = open && (
    <div className="border-t border-border px-3 py-3 space-y-3">

      {/* Status block (not shown for new instances) */}
      {!isNew && (
        <div className={`flex items-start justify-between gap-3 rounded border p-2.5 ${stateColor}`}>
          <div className="flex items-start gap-2">
            <span className={`mt-0.5 w-2 h-2 rounded-full flex-shrink-0 ${stateDotCls}`} />
            <div>
              <div className={`text-xs font-medium ${status?.state === 'error' ? 'text-destructive' : status?.state === 'online' ? 'text-success' : 'text-muted-foreground'}`}>
                {stateTitle}
              </div>
              {stateDetail && (
                <div className="text-xs text-muted-foreground mt-0.5">{stateDetail}</div>
              )}
            </div>
          </div>
          <button
            type="button"
            onClick={handleStatusAction}
            className={`flex-shrink-0 rounded border px-2 py-1 text-xs whitespace-nowrap ${stateActionCls}`}
          >
            {stateActionLabel}
          </button>
        </div>
      )}

      {/* Name field (always shown) */}
      <div>
        <label className="block text-xs text-muted-foreground mb-1">名称</label>
        <input
          value={name}
          onChange={e => { setName(e.target.value); markDirty() }}
          className={inputCls()}
          placeholder="我的企微机器人"
        />
      </div>

      {/* Channel-type-specific fields in 2-column grid */}
      <div className="grid grid-cols-2 gap-x-3 gap-y-2">

        {channelType === 'wecom_bot' && <>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">Corp ID</label>
            <input value={corpId} readOnly className={`${inputCls()} font-mono opacity-70`} />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">Agent ID</label>
            <input value={agentId} readOnly className={`${inputCls()} font-mono opacity-70`} />
          </div>
          <div className="col-span-2">
            <label className={`block text-xs mb-1 ${credHighlight ? 'text-destructive font-medium' : 'text-muted-foreground'}`}>
              Corp Secret{credHighlight && <span className="ml-0.5 text-destructive">*</span>}
            </label>
            <input
              type="password"
              value={corpSecret}
              onChange={e => { setCorpSecret(e.target.value); markDirty() }}
              className={inputCls(credHighlight)}
              placeholder="留空则不修改"
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select
              value={spaceId}
              onChange={e => { setSpaceId(e.target.value); markDirty() }}
              className={inputCls()}
            >
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">WebSocket URL（可选）</label>
            <input
              value={wecomWsUrl}
              onChange={e => { setWecomWsUrl(e.target.value); markDirty() }}
              className={`${inputCls()} font-mono`}
              placeholder="wss://openws.work.weixin.qq.com"
            />
          </div>
        </>}

        {channelType === 'wechat_ilink' && <>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">App ID</label>
            <input
              value={appId}
              onChange={e => { setAppId(e.target.value); markDirty() }}
              className={inputCls()}
            />
          </div>
          <div>
            <label className={`block text-xs mb-1 ${credHighlight ? 'text-destructive font-medium' : 'text-muted-foreground'}`}>
              API Key{credHighlight && <span className="ml-0.5 text-destructive">*</span>}
            </label>
            <input
              type="password"
              value={apiKey}
              onChange={e => { setApiKey(e.target.value); markDirty() }}
              className={inputCls(credHighlight)}
              placeholder="留空则不修改"
            />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select value={spaceId} onChange={e => { setSpaceId(e.target.value); markDirty() }} className={inputCls()}>
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
        </>}

        {(channelType === 'dingtalk' || channelType === 'feishu') && <>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">Webhook URL</label>
            <input
              value={webhookUrl}
              onChange={e => { setWebhookUrl(e.target.value); markDirty() }}
              className={inputCls()}
            />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">签名密钥（可选）</label>
            <input
              type="password"
              value={signingSecret}
              onChange={e => { setSigningSecret(e.target.value); markDirty() }}
              className={inputCls()}
            />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select value={spaceId} onChange={e => { setSpaceId(e.target.value); markDirty() }} className={inputCls()}>
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
        </>}

        {channelType === 'email' && <>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">SMTP Host</label>
            <input value={smtpHost} onChange={e => { setSmtpHost(e.target.value); markDirty() }} className={inputCls()} placeholder="smtp.gmail.com" />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">端口</label>
            <input value={smtpPort} onChange={e => { setSmtpPort(e.target.value); markDirty() }} className={inputCls()} placeholder="587" />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">用户名</label>
            <input value={smtpUser} onChange={e => { setSmtpUser(e.target.value); markDirty() }} className={inputCls()} />
          </div>
          <div>
            <label className={`block text-xs mb-1 ${credHighlight ? 'text-destructive font-medium' : 'text-muted-foreground'}`}>
              密码{credHighlight && <span className="ml-0.5 text-destructive">*</span>}
            </label>
            <input type="password" value={smtpPass} onChange={e => { setSmtpPass(e.target.value); markDirty() }} className={inputCls(credHighlight)} placeholder="留空则不修改" />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">收件人（逗号分隔）</label>
            <input value={toAddresses} onChange={e => { setToAddresses(e.target.value); markDirty() }} className={inputCls()} placeholder="a@example.com, b@example.com" />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select value={spaceId} onChange={e => { setSpaceId(e.target.value); markDirty() }} className={inputCls()}>
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
        </>}

        {channelType === 'webhook' && <>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">Webhook URL</label>
            <input value={webhookUrl} onChange={e => { setWebhookUrl(e.target.value); markDirty() }} className={inputCls()} placeholder="https://example.com/hook" />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select value={spaceId} onChange={e => { setSpaceId(e.target.value); markDirty() }} className={inputCls()}>
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
        </>}
      </div>

      {/* Options row */}
      <div className="flex gap-4 text-sm">
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={streaming} onChange={e => { setStreaming(e.target.checked); markDirty() }} />
          流式回复
        </label>
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={permissionEnabled} onChange={e => { setPermissionEnabled(e.target.checked); markDirty() }} />
          开启权限控制
        </label>
      </div>

      {permissionEnabled && (
        <div className="rounded border border-border p-2.5 space-y-2">
          <div>
            <label className="block text-xs text-muted-foreground mb-1">Owners（chat_id，逗号分隔）</label>
            <input value={owners} onChange={e => { setOwners(e.target.value); markDirty() }} className={inputCls()} placeholder="openid_1, openid_2" />
          </div>
          <label className="flex items-center gap-1.5 text-sm">
            <input type="checkbox" checked={mcpEnabled} onChange={e => { setMcpEnabled(e.target.checked); markDirty() }} />
            Guest 允许 MCP 工具
          </label>
        </div>
      )}

      {/* Error message */}
      {error && <p className="text-sm text-destructive">{error}</p>}

      {/* Bottom bar */}
      <div className="flex items-center justify-between pt-2 border-t border-border">
        {!isNew ? (
          <button
            type="button"
            onClick={onDeleted}
            className="text-xs text-destructive hover:underline"
          >
            删除实例
          </button>
        ) : <span />}
        <div className="flex gap-2">
          <button
            type="button"
            onClick={handleCancel}
            className="rounded border border-border bg-background px-3 py-1.5 text-sm hover:bg-muted"
          >
            取消
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={saving || !dirty || !name || !spaceId}
            className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
          >
            {saving ? '保存中…' : saveLabel}
          </button>
        </div>
      </div>
    </div>
  )

  // ──────────────── render ────────────────
  return (
    <div className={`rounded border transition-colors ${open ? 'border-primary' : 'border-border'}`}>
      {closedRow}
      {metaLine}
      <div
        className="overflow-hidden transition-[max-height] duration-200 ease-out"
        style={{ maxHeight: open ? '1000px' : '0px' }}
      >
        {expandedContent}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | grep "ImChannelAccordionRow\|im-channel-atoms" | head -10
```

Expected: no errors.

- [ ] **Step 3: Full TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/settings/ImChannelAccordionRow.tsx
git commit -m "feat(settings): add ImChannelAccordionRow — accordion row with connection-aware form"
```

---

## Task 8: Vitest Tests

**Files:**
- Create: `ui/src/components/settings/ImChannelsSettings.test.tsx`
- Create: `ui/src/components/settings/ImChannelAccordionRow.test.tsx`

- [ ] **Step 1: Create ImChannelsSettings.test.tsx**

```tsx
// ui/src/components/settings/ImChannelsSettings.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { imChannelsAtom, imChannelStatusesAtom } from '@/atoms/im-channel-atoms'
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'
import { ImChannelsSettings } from './ImChannelsSettings'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

const makeChannel = (overrides: Partial<ImChannelRow> = {}): ImChannelRow => ({
  id: 'ch-1', spaceId: 'sp-1', channelType: 'wecom_bot', name: '产品组机器人',
  config: { corp_id: 'wx12abc', agent_id: '1000042' }, enabled: true,
  streaming: false, replyScope: 'all', permissionEnabled: false,
  owners: [], guestPolicy: { tool_allowlist: [], mcp_enabled: false },
  createdAt: 1_700_000_000_000, updatedAt: 1_700_000_000_000,
  ...overrides,
})

beforeEach(() => {
  invokeMock.mockReset()
  invokeMock.mockResolvedValue([])
})

describe('ImChannelsSettings', () => {
  it('renders tab with instance count badge', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel()])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText('企业微信')).not.toBeNull()
    expect(screen.getByText('1')).not.toBeNull()
  })

  it('shows error badge on tab when any instance has error status', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ id: 'ch-err' })])
    store.set(imChannelStatusesAtom, {
      'ch-err': { instanceId: 'ch-err', state: 'error', lastError: '认证失败' } as ImChannelStatus,
    })
    renderWithProviders(<ImChannelsSettings />, { store })
    // The badge "1" should exist and be in destructive style
    const badge = screen.getByText('1')
    expect(badge.className).toMatch(/destructive/)
  })

  it('renders instance name in the list', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ name: '测试机器人' })])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText('测试机器人')).not.toBeNull()
  })

  it('renders add-new dashed button for current tab', () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel()])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText(/新增企业微信实例/)).not.toBeNull()
  })

  it('calls toggle_im_channel and optimistically updates enabled state', async () => {
    invokeMock.mockResolvedValue(undefined)
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ enabled: true })])
    renderWithProviders(<ImChannelsSettings />, { store })
    const toggleBtn = screen.getByRole('button', { name: '停用' })
    fireEvent.click(toggleBtn)
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('toggle_im_channel', { id: 'ch-1', enabled: false })
    })
  })

  it('reverts optimistic toggle on invoke failure', async () => {
    invokeMock.mockRejectedValue(new Error('network error'))
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ enabled: true })])
    renderWithProviders(<ImChannelsSettings />, { store })
    const toggleBtn = screen.getByRole('button', { name: '停用' })
    fireEvent.click(toggleBtn)
    // fetchChannels() is called — invokeMock called with list_im_channels to revert
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('list_im_channels')
    })
  })
})
```

- [ ] **Step 2: Create ImChannelAccordionRow.test.tsx**

```tsx
// ui/src/components/settings/ImChannelAccordionRow.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'
import { ImChannelAccordionRow } from './ImChannelAccordionRow'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

const BASE_CHANNEL: ImChannelRow = {
  id: 'ch-1', spaceId: 'sp-1', channelType: 'wecom_bot', name: '客服机器人',
  config: { corp_id: 'wx99def', agent_id: '1000099' }, enabled: true,
  streaming: false, replyScope: 'all', permissionEnabled: false,
  owners: [], guestPolicy: { tool_allowlist: [], mcp_enabled: false },
  createdAt: 1_700_000_000_000, updatedAt: 1_700_000_000_000,
}
const SPACES = [{ id: 'sp-1', name: '工作区' }]

function renderRow(overrides: {
  channel?: ImChannelRow
  status?: ImChannelStatus
  open?: boolean
  newChannelType?: string
} = {}) {
  const onToggleOpen = vi.fn()
  const onToggleEnabled = vi.fn()
  const onSaved = vi.fn()
  const onDeleted = vi.fn()
  renderWithProviders(
    <ImChannelAccordionRow
      channel={overrides.channel ?? BASE_CHANNEL}
      newChannelType={overrides.newChannelType}
      status={overrides.status}
      spaces={SPACES}
      open={overrides.open ?? false}
      onToggleOpen={onToggleOpen}
      onToggleEnabled={onToggleEnabled}
      onSaved={onSaved}
      onDeleted={onDeleted}
    />
  )
  return { onToggleOpen, onToggleEnabled, onSaved, onDeleted }
}

beforeEach(() => { invokeMock.mockReset() })

describe('ImChannelAccordionRow', () => {
  it('renders channel name in closed state', () => {
    renderRow()
    expect(screen.getByText('客服机器人')).not.toBeNull()
  })

  it('shows error badge in closed state when status is error', () => {
    renderRow({
      status: { instanceId: 'ch-1', state: 'error', lastError: '认证失败 xyz' },
    })
    expect(screen.getByText(/认证失/)).not.toBeNull()
  })

  it('renders status block in open state for online channel', () => {
    renderRow({
      open: true,
      status: { instanceId: 'ch-1', state: 'online', connectedSinceMs: Date.now() - 60000 },
    })
    expect(screen.getByText(/WebSocket 已连接/)).not.toBeNull()
  })

  it('renders status block with error message in open state', () => {
    renderRow({
      open: true,
      status: { instanceId: 'ch-1', state: 'error', lastError: 'corp_secret 过期' },
    })
    expect(screen.getByText(/连接错误/)).not.toBeNull()
    expect(screen.getByText('corp_secret 过期')).not.toBeNull()
  })

  it('save button is disabled when not dirty', () => {
    renderRow({ open: true })
    const saveBtn = screen.getByRole('button', { name: '保存' })
    expect(saveBtn.hasAttribute('disabled')).toBe(true)
  })

  it('save button enables after changing name', async () => {
    renderRow({ open: true })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '新名称' } })
    await waitFor(() => {
      const saveBtn = screen.getByRole('button', { name: '保存' })
      expect(saveBtn.hasAttribute('disabled')).toBe(false)
    })
  })

  it('shows 保存并重连 when dirty and channel is online', async () => {
    renderRow({
      open: true,
      status: { instanceId: 'ch-1', state: 'online' },
    })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '改名' } })
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '保存并重连' })).not.toBeNull()
    })
  })

  it('calls update_im_channel on save', async () => {
    invokeMock.mockResolvedValue(undefined)
    renderRow({ open: true })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '新名称' } })
    await waitFor(() => {
      fireEvent.click(screen.getByRole('button', { name: '保存' }))
    })
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'update_im_channel',
        expect.objectContaining({ id: 'ch-1' })
      )
    })
  })

  it('calls create_im_channel in new-instance mode', async () => {
    invokeMock.mockResolvedValue(undefined)
    renderRow({ channel: undefined, newChannelType: 'wecom_bot', open: true })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '新机器人' } })
    await waitFor(() => {
      fireEvent.click(screen.getByRole('button', { name: '保存' }))
    })
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'create_im_channel',
        expect.objectContaining({})
      )
    })
  })
})
```

- [ ] **Step 3: Run all tests**

```bash
cd ui && npm test -- --run 2>&1 | tail -20
```

Expected: All tests pass. Look for `ImChannelsSettings` and `ImChannelAccordionRow` suites with all tests green.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/settings/ImChannelsSettings.test.tsx ui/src/components/settings/ImChannelAccordionRow.test.tsx
git commit -m "test(settings): Vitest tests for ImChannelsSettings and ImChannelAccordionRow"
```

---

## Final verification

- [ ] **Backend build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no lines.

- [ ] **Frontend TS clean**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: no lines.

- [ ] **All Vitest pass**

```bash
cd ui && npm test -- --run 2>&1 | grep -E "FAIL|PASS|Tests" | tail -5
```

Expected: no FAIL lines.

- [ ] **Commit summary**

Branch `feat+im-framework` will have these new commits on top of PR #178:
1. `feat(channels): add ChannelRuntimeStatus + ChannelState types`
2. `feat(channels): add statuses map + status relay infrastructure to ImChannelManager`
3. `feat(channels): WecomBot emits ChannelRuntimeStatus on connect/error/timeout`
4. `feat(ipc): add get_im_channel_statuses Tauri command`
5. `feat(atoms): add ImChannelStatus type + imChannelStatusesAtom`
6. `feat(settings): rewrite ImChannelsSettings with tab nav + accordion orchestration`
7. `feat(settings): add ImChannelAccordionRow — accordion row with connection-aware form`
8. `test(settings): Vitest tests for ImChannelsSettings and ImChannelAccordionRow`
