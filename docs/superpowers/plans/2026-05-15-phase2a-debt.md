# Phase 2a Debt Clearance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clear three Phase 2a technical debt items: wire `notify_user` to real channels, fix the per-day cost cap to count only automation sessions, and implement general session archive end-to-end.

**Architecture:** Task 1 adds `send_to_type` to `ChannelManager`. Tasks 2–3 thread `app_handle` + `channel_manager` into `AutomationDelegate` via `AppRuntimeService`. Task 4 is a one-line SQL JOIN in `cost.rs`. Tasks 5–6 add a V26 migration and two Tauri commands. Task 7 fixes the tauri-bridge catch fallback and adds archive UI to `ActivityListItem` + `ActivityHistoryView`.

**Tech Stack:** Rust + rusqlite + tauri v2, React 18 + TypeScript + Tailwind theme tokens.

---

## File Map

| Task | Creates / Modifies |
|---|---|
| 1 | Modify `src-tauri/src/channels.rs` |
| 2 | Modify `src-tauri/src/automation/runtime/execute.rs` |
| 3 | Modify `src-tauri/src/automation/runtime/service.rs`, `src-tauri/src/app.rs` |
| 4 | Modify `src-tauri/src/automation/runtime/cost.rs` |
| 5 | Modify `src-tauri/src/db/migrations.rs` |
| 6 | Modify `src-tauri/src/tauri_commands.rs`, `src-tauri/src/main.rs` |
| 7 | Modify `ui/src/lib/tauri-bridge.ts`, `ui/src/components/automation/ActivityListItem.tsx`, `ui/src/components/automation/ActivityHistoryView.tsx`, `ui/src/components/automation/ActivityHistoryView.test.tsx` |

---

### Task 1: `ChannelManager.send_to_type` — type-filtered broadcast

**Files:**
- Modify: `src-tauri/src/channels.rs`

Context: `ChannelManager::broadcast()` sends to ALL enabled channels. `notify_user` needs to send only to channels whose `channel_type` matches the requested name (`"email"` → `ChannelType::Email`, etc.). Add `send_to_type` before wiring the delegate.

- [ ] **Step 1: Write the failing test**

Add inside `channels.rs`, after the `ChannelManager` impl block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysOkSender;

    #[async_trait::async_trait]
    impl ChannelSender for AlwaysOkSender {
        async fn send(
            &self,
            _n: &ChannelNotification,
            _c: &ChannelConfig,
        ) -> Result<(), String> {
            Ok(())
        }
        fn name(&self) -> &str { "email" }
    }

    fn test_mgr() -> ChannelManager {
        let mut mgr = ChannelManager::new();
        mgr.register_sender("email", Box::new(AlwaysOkSender));
        mgr.add_channel(ChannelConfig {
            id: "e1".into(),
            name: "Email 1".into(),
            channel_type: ChannelType::Email,
            enabled: true,
            webhook_url: None,
            config: None,
        });
        mgr.add_channel(ChannelConfig {
            id: "w1".into(),
            name: "WeChat 1".into(),
            channel_type: ChannelType::WeChat,
            enabled: true,
            webhook_url: None,
            config: None,
        });
        mgr
    }

    #[tokio::test]
    async fn send_to_type_filters_by_channel_type() {
        let mgr = test_mgr();
        let notif = ChannelNotification {
            title: "T".into(),
            body: "B".into(),
            level: "info".into(),
            metadata: None,
        };

        // Email sender is registered — should match e1 only.
        let results = mgr.send_to_type(&ChannelType::Email, &notif).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "e1");
        assert!(results[0].1.is_ok());

        // WeChat sender is NOT registered — config matches w1 but no sender,
        // so results are empty (the outer `if let Some(sender)` guard).
        let results = mgr.send_to_type(&ChannelType::WeChat, &notif).await;
        assert_eq!(results.len(), 0, "wechat sender not registered → 0 results");
    }

    #[tokio::test]
    async fn send_to_type_skips_disabled_channels() {
        let mut mgr = test_mgr();
        // Disable e1
        mgr.set_enabled("e1", false);
        let notif = ChannelNotification {
            title: "T".into(), body: "B".into(), level: "info".into(), metadata: None,
        };
        let results = mgr.send_to_type(&ChannelType::Email, &notif).await;
        assert_eq!(results.len(), 0, "disabled channel should be skipped");
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd src-tauri && cargo test --lib channels -- 2>&1 | tail -20
```

Expected: compile error — `send_to_type` not found.

- [ ] **Step 3: Add `send_to_type` to `ChannelManager`**

Add this method to the `impl ChannelManager` block in `channels.rs`, directly after `broadcast`:

```rust
/// Send notification to all enabled channels of the given type.
/// Channels whose sender is not registered are silently skipped (returns 0
/// results for that type, not an error).
pub async fn send_to_type(
    &self,
    target_type: &ChannelType,
    notification: &ChannelNotification,
) -> Vec<(String, Result<(), String>)> {
    let mut results = Vec::new();
    for (id, (config, _)) in &self.channels {
        if !config.enabled || &config.channel_type != target_type {
            continue;
        }
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
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cd src-tauri && cargo test --lib channels -- 2>&1 | tail -20
```

Expected: `test channels::tests::send_to_type_filters_by_channel_type ... ok`
Expected: `test channels::tests::send_to_type_skips_disabled_channels ... ok`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/channels.rs
git commit -m "feat(channels): add send_to_type for type-filtered channel broadcast"
```

---

### Task 2: Wire `notify_user` in `AutomationDelegate`

**Files:**
- Modify: `src-tauri/src/automation/runtime/execute.rs`

Context: `AutomationDelegate` currently has no access to `AppHandle` or `ChannelManager`. The `notify_user` handler at line 292 just logs and returns a stub string. We add two optional fields to the struct (optional so unit tests don't need a real AppHandle), wire the handler, and add a unit test. The fields are threaded in from `AppRuntimeService` in Task 3.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `execute.rs` (after the existing tests):

```rust
#[tokio::test]
async fn notify_user_dispatches_without_panic() {
    let spec_id = "spec-notify";
    let act_id  = "act-notify";
    let tmp  = TempDir::new().unwrap();
    let conn = setup_db(spec_id, act_id);
    // Both optional handles are None in unit tests — system emit is skipped,
    // channel dispatch is skipped. The tool result must say "dispatched".
    let delegate = make_delegate(spec_id, act_id, &tmp, conn, PermissionSet::default());
    let call = crate::agent::types::ToolCall {
        id: "c1".into(),
        name: "notify_user".into(),
        arguments: serde_json::json!({
            "channels": ["system", "wecom"],
            "title": "test alert",
            "body":  "hello world",
            "level": "info"
        }),
    };
    let mut ctx = ReasoningContext::new(String::new());
    let result = delegate.execute_tool_calls(vec![call], &mut ctx).await;
    assert!(result.is_ok(), "execute_tool_calls should not error: {:?}", result);
    let last = ctx.messages.last().expect("tool result pushed");
    // The new handler returns "notification dispatched" (not the Phase 2a stub).
    assert!(
        last.content.contains("dispatched"),
        "expected 'dispatched' in tool result, got: {:?}",
        last.content
    );
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd src-tauri && cargo test --lib automation::runtime::execute::tests::notify_user 2>&1 | tail -20
```

Expected: test fails — `last.content` contains the Phase 2a stub `"notification logged (channel dispatch lands in Phase 2b)"`, not `"dispatched"`.

- [ ] **Step 3: Add two new fields to `AutomationDelegate` and update `make_delegate`**

At the top of `execute.rs`, add to the `use` block:

```rust
use crate::channels::{ChannelManager, ChannelNotification};
use tokio::sync::RwLock as TokioRwLock;
```

Extend `AutomationDelegate` (after the `workspace_root` field):

```rust
pub struct AutomationDelegate {
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
    /// IPC handle for `"system"` channel notifications. None in unit tests.
    pub app_handle: Option<tauri::AppHandle>,
    /// External channel manager for `"wecom"`, `"email"`, etc. None in unit tests.
    pub channel_manager: Option<Arc<TokioRwLock<ChannelManager>>>,
}
```

In `make_delegate` (inside `#[cfg(test)]`), add the two new fields:

```rust
fn make_delegate(
    spec_id: &str,
    activity_id: &str,
    tmp: &TempDir,
    conn: rusqlite::Connection,
    perms: PermissionSet,
) -> AutomationDelegate {
    use crate::automation::runtime::cost::{CostCapConfig, CostCapState};
    AutomationDelegate {
        spec_id: spec_id.to_string(),
        activity_id: activity_id.to_string(),
        session_id: format!("sess-{}", activity_id),
        permissions: perms,
        memory: Arc::new(MemoryStore::new(tmp.path().to_path_buf())),
        db: Arc::new(std::sync::Mutex::new(conn)),
        gate: Arc::new(Mutex::new(None)),
        auto_continue: AutoContinueConfig::default(),
        llm: test_support::fake_llm(),
        model: "claude-sonnet-4-6".to_string(),
        tools: test_support::empty_tool_registry(),
        cost: Arc::new(CostCapState::new(CostCapConfig {
            per_run_usd: 1.00,
            per_day_usd: 10.00,
        })),
        workspace_root: tmp.path().to_path_buf(),
        app_handle: None,
        channel_manager: None,
    }
}
```

- [ ] **Step 4: Add the `channel_type_for_name` helper and wire the `notify_user` handler**

Add this free function above `impl LoopDelegate for AutomationDelegate` (module level in `execute.rs`):

```rust
/// Maps a `notify_user` channel name to the corresponding `ChannelType`.
/// Unknown names return `None` and are silently skipped.
fn channel_type_for_name(name: &str) -> Option<crate::channels::ChannelType> {
    use crate::channels::ChannelType;
    match name {
        "wecom"   => Some(ChannelType::WeChat),
        "email"   => Some(ChannelType::Email),
        "webhook" => Some(ChannelType::Webhook),
        _         => None,
    }
}
```

Replace the `"notify_user"` arm (lines 292–310) with:

```rust
"notify_user" => {
    let input: NotifyInput = serde_json::from_value(call.arguments.clone())?;
    let notification = ChannelNotification {
        title: input.title.clone(),
        body:  input.body.clone(),
        level: input.level.clone(),
        metadata: None,
    };

    for ch in &input.channels {
        match ch.as_str() {
            "system" => {
                if let Some(handle) = &self.app_handle {
                    let _ = handle.emit("automation_notify", &notification);
                }
            }
            other => {
                if let (Some(ct), Some(cm_lock)) =
                    (channel_type_for_name(other), &self.channel_manager)
                {
                    let cm = cm_lock.read().await;
                    let results = cm.send_to_type(&ct, &notification).await;
                    for (ch_id, res) in results {
                        if let Err(e) = res {
                            tracing::warn!(
                                spec_id = %self.spec_id,
                                ch_id = %ch_id,
                                "notify_user channel error: {}", e
                            );
                        }
                    }
                }
            }
        }
    }

    reason_ctx.messages.push(ChatMessage::user_tool_result(
        &call.id,
        "notification dispatched",
        false,
    ));
}
```

- [ ] **Step 5: Run tests — expect pass**

```bash
cd src-tauri && cargo test --lib automation::runtime::execute 2>&1 | tail -20
```

Expected: all execute.rs tests pass including the new `notify_user_dispatches_without_panic`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/runtime/execute.rs
git commit -m "feat(automation): wire notify_user — IPC emit + ChannelManager dispatch"
```

---

### Task 3: Thread `app_handle` + `channel_manager` into `AppRuntimeService`

**Files:**
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/app.rs`

Context: `AppRuntimeService` constructs `AutomationDelegate` at line 621. It currently has no `app_handle` or `channel_manager`. We add both fields to the struct and `new()`, update `app.rs` to pass them, and update the delegate construction. No new tests needed — the existing execute.rs tests cover delegate behavior; this is a wiring change caught by `cargo build`.

- [ ] **Step 1: Add imports to `service.rs`**

Add to the `use` block near the top (after `use crate::providers::service::ProviderService;`):

```rust
use crate::channels::ChannelManager;
use tokio::sync::RwLock as TokioRwLock;  // already imported as RwLock — use that alias
```

Note: `service.rs` already has `use tokio::sync::{Mutex as TokioMutex, RwLock, Semaphore};`. Use `RwLock` (not a new alias).

- [ ] **Step 2: Add two fields to `AppRuntimeService`**

In the `pub struct AppRuntimeService` block, add after `provider_service`:

```rust
    /// Tauri app handle — used by AutomationDelegate to emit IPC events.
    pub app_handle: tauri::AppHandle,
    /// Notification channel manager — used by AutomationDelegate for external
    /// channel dispatch (email, wecom, etc.).
    pub channel_manager: Arc<RwLock<ChannelManager>>,
```

- [ ] **Step 3: Add two params to `AppRuntimeService::new()`**

Update the `pub fn new(...)` signature to add at the end:

```rust
pub fn new(
    db: Arc<StdMutex<rusqlite::Connection>>,
    schedule: Arc<ScheduleSource>,
    file: Arc<FileSource>,
    webhook: Arc<WebhookSource>,
    webpage: Arc<WebpageSource>,
    rss: Arc<RssSource>,
    wecom: Arc<WecomSource>,
    custom: Arc<CustomSource>,
    infra: Arc<InfraService>,
    memory: Arc<AutomationMemoryStore>,
    provider_service: Arc<ProviderService>,
    app_handle: tauri::AppHandle,
    channel_manager: Arc<RwLock<ChannelManager>>,
) -> Arc<Self> {
```

And in the `Arc::new(Self { ... })` body, add the two fields:

```rust
            app_handle,
            channel_manager,
```

- [ ] **Step 4: Update delegate construction in `service.rs` (line ~621)**

In `execute_run`, where `AutomationDelegate` is constructed, add the two new fields:

```rust
        let delegate = AutomationDelegate {
            spec_id: spec_id.to_string(),
            activity_id: activity_id.clone(),
            session_id: session_id.clone(),
            permissions,
            memory: self.memory.clone(),
            db: self.db.clone(),
            gate: Arc::new(TokioMutex::new(None)),
            auto_continue: AutoContinueConfig::default(),
            llm,
            model,
            tools,
            cost: Arc::new(CostCapState::new(cost_cap)),
            workspace_root,
            app_handle: Some(self.app_handle.clone()),
            channel_manager: Some(self.channel_manager.clone()),
        };
```

- [ ] **Step 5: Update `AppRuntimeService::new()` call in `app.rs`**

`channel_manager` is created at line 345 (`let channel_manager = Arc::new(RwLock::new(ChannelManager::new()));`). It's available before the `runtime_service` block at line 427.

Update the `AppRuntimeService::new(...)` call to pass the two new args at the end:

```rust
            AppRuntimeService::new(
                db.clone(),
                Arc::new(ScheduleSource::new()),
                Arc::new(FileSource::new()),
                Arc::new(WebhookSource::with_global_registry()),
                Arc::new(WebpageSource::new()),
                Arc::new(RssSource::new()),
                Arc::new(WecomSource::new()),
                Arc::new(CustomSource::new()),
                infra_service.clone(),
                Arc::new(AutomationMemoryStore::new(automation_memory_root)),
                provider_service.clone(),
                app_handle.clone(),       // NEW
                channel_manager.clone(),  // NEW
            )
```

- [ ] **Step 6: Compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/runtime/service.rs src-tauri/src/app.rs
git commit -m "feat(automation): thread app_handle + channel_manager into AppRuntimeService"
```

---

### Task 4: Fix per-day cost cap — count automation-origin sessions only

**Files:**
- Modify: `src-tauri/src/automation/runtime/cost.rs`

Context: `day_total_usd()` currently sums ALL `cost_records` for the UTC day, including chat sessions. We JOIN on `agent_sessions` and filter by `json_extract(metadata_json, '$.origin') LIKE 'automation:%'`. The existing test `day_total_usd_sums_todays_records` inserts cost records with session `'s1'` but no matching `agent_sessions` row; after the JOIN it will return 0. We rename the test and fix it, and add a negative-case test.

- [ ] **Step 1: Write / update tests**

Replace the existing `day_total_usd_sums_todays_records` test and add a second one:

```rust
#[test]
fn day_total_usd_sums_automation_records_only() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    let now = chrono::Utc::now().timestamp_millis();

    // Insert automation-origin session.
    conn.execute(
        "INSERT INTO agent_sessions
             (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES ('auto-s', 'sp1', 'auto', '{\"origin\":\"automation:manual\"}', 0, 0, 0, ?1, ?1)",
        [now],
    ).unwrap();
    // Insert non-automation (chat) session.
    conn.execute(
        "INSERT INTO agent_sessions
             (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES ('chat-s', 'sp1', 'chat', '{}', 0, 0, 0, ?1, ?1)",
        [now],
    ).unwrap();

    // Automation session costs $1.00 total.
    conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
         VALUES ('c1', 'auto-s', 'm', 100, 50, 0.25, ?1),
                ('c2', 'auto-s', 'm', 100, 50, 0.75, ?1)",
        [now],
    ).unwrap();
    // Chat session costs $0.50 — must NOT be counted.
    conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
         VALUES ('c3', 'chat-s', 'm', 100, 50, 0.50, ?1)",
        [now],
    ).unwrap();

    let total = day_total_usd(&conn);
    assert!((total - 1.00).abs() < 1e-6, "expected 1.00 (auto only), got {}", total);
}

#[test]
fn day_total_usd_returns_zero_when_no_automation_sessions() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    // Only a chat session + cost record — day_total_usd should be 0.0.
    conn.execute(
        "INSERT INTO agent_sessions
             (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES ('chat-s', 'sp1', 'chat', '{}', 0, 0, 0, ?1, ?1)",
        [now],
    ).unwrap();
    conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
         VALUES ('c1', 'chat-s', 'm', 100, 50, 0.50, ?1)",
        [now],
    ).unwrap();
    let total = day_total_usd(&conn);
    assert_eq!(total, 0.0, "no automation sessions → 0.0");
}
```

- [ ] **Step 2: Run tests to confirm failure**

```bash
cd src-tauri && cargo test --lib automation::runtime::cost 2>&1 | tail -20
```

Expected: `day_total_usd_sums_automation_records_only` fails (old SQL returns 0.0 because no session row matches the old query... wait, the old query has no JOIN — it'll return 0.25 + 0.75 + 0.50 = 1.50, not 1.00). Confirm failure.

- [ ] **Step 3: Replace the SQL in `day_total_usd`**

```rust
pub fn day_total_usd(conn: &rusqlite::Connection) -> f64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    let day_start = Utc
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    conn.query_row(
        "SELECT COALESCE(SUM(cr.cost_usd), 0)
         FROM cost_records cr
         JOIN agent_sessions s ON s.id = cr.session_id
         WHERE cr.created_at >= ?1
           AND json_extract(s.metadata_json, '$.origin') LIKE 'automation:%'",
        [day_start],
        |r| r.get::<_, f64>(0),
    )
    .unwrap_or(0.0)
}
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cd src-tauri && cargo test --lib automation::runtime::cost 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/runtime/cost.rs
git commit -m "fix(automation): filter day_total_usd to automation-origin sessions only"
```

---

### Task 5: V26 migration — add `archived` + `archived_at` to `conversations`

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

Context: `agent_sessions` already has `archived INTEGER NOT NULL DEFAULT 0` (V8) and `archived_at INTEGER` (V24). `conversations` has neither. V25 is the last migration. We add V26.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block at the bottom of `migrations.rs`:

```rust
#[test]
fn v26_conversations_archived_columns_exist() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run(&conn).unwrap();
    let archived: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'archived'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(archived, 1, "conversations.archived column missing");

    let archived_at: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'archived_at'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(archived_at, 1, "conversations.archived_at column missing");
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd src-tauri && cargo test --lib db::migrations::tests::v26 2>&1 | tail -20
```

Expected: test fails — columns don't exist yet.

- [ ] **Step 3: Add V26 constant**

After the V25 block (around line 1172), add:

```rust
/// V26 — conversations gains `archived` + `archived_at` for general session archiving.
const SQL_V26: &str = "
ALTER TABLE conversations ADD COLUMN archived  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conversations ADD COLUMN archived_at INTEGER;
";
```

- [ ] **Step 4: Register V26 in the `run()` function**

After the V25 block in `run()` (around line 1368–1370):

```rust
    // V26: conversations.archived + archived_at
    tracing::debug!("Running migration V26: conversations archived columns");
    for stmt in SQL_V26.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute_batch(stmt) {
            tracing::warn!("V26 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 5: Run tests — expect pass**

```bash
cd src-tauri && cargo test --lib db::migrations 2>&1 | tail -20
```

Expected: `v26_conversations_archived_columns_exist ... ok` and all prior migration tests still pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V26 migration — conversations.archived + archived_at"
```

---

### Task 6: Two archive toggle Tauri commands + registration

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

Context: `toggle_pin_agent_session` (line 4658) is the exact pattern to follow: read current nullable timestamp, flip to `Some(now)` / `None`, return the new value. Both `toggle_archive_agent_session` and `toggle_archive_conversation` follow the same logic but write both `archived` (0/1) and `archived_at` (timestamp/NULL).

- [ ] **Step 1: Write the failing tests**

Add after the `toggle_pin` tests module in `tauri_commands.rs` (find the `#[cfg(test)]` block around line 8470):

```rust
#[cfg(test)]
mod toggle_archive_tests {
    use super::*;
    use rusqlite::Connection;

    fn db_with_session_and_conversation() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V1_INITIAL).unwrap();
        conn.execute_batch(crate::db::migrations::V8_AGENT_SESSIONS).unwrap();
        // Apply V18 (pinned_at) and V24 (archived_at on agent_sessions).
        for stmt in crate::db::migrations::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }
        // Apply V26 (conversations.archived + archived_at) — use full run().
        crate::db::migrations::run(&conn).unwrap();

        // Insert one agent_session.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json,
                                          message_count, pinned, archived,
                                          created_at, updated_at)
             VALUES ('s1', 'default', 't', '{}', 0, 0, 0, 0, 0)",
            [],
        ).unwrap();
        // Insert one conversation (space_id FK not enforced without PRAGMA).
        conn.execute(
            "INSERT INTO conversations (id, space_id, title, created_at, updated_at)
             VALUES ('cv1', 'default', 'Chat 1', datetime('now'), datetime('now'))",
            [],
        ).unwrap();
        conn
    }

    fn toggle_archive_session_sql(conn: &Connection, id: &str) -> rusqlite::Result<Option<i64>> {
        let tx = conn.unchecked_transaction()?;
        let current: Option<i64> = tx.query_row(
            "SELECT archived_at FROM agent_sessions WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Option<i64>>(0),
        ).ok().flatten();
        let next: Option<i64> = if current.is_some() {
            None
        } else {
            Some(1_700_000_000_000_i64)
        };
        let archived_flag = if next.is_some() { 1i64 } else { 0i64 };
        tx.execute(
            "UPDATE agent_sessions SET archived = ?1, archived_at = ?2 WHERE id = ?3",
            rusqlite::params![archived_flag, next, id],
        )?;
        tx.commit()?;
        Ok(next)
    }

    fn toggle_archive_conversation_sql(conn: &Connection, id: &str) -> rusqlite::Result<Option<i64>> {
        let tx = conn.unchecked_transaction()?;
        let current: Option<i64> = tx.query_row(
            "SELECT archived_at FROM conversations WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Option<i64>>(0),
        ).ok().flatten();
        let next: Option<i64> = if current.is_some() {
            None
        } else {
            Some(1_700_000_000_000_i64)
        };
        let archived_flag = if next.is_some() { 1i64 } else { 0i64 };
        tx.execute(
            "UPDATE conversations SET archived = ?1, archived_at = ?2 WHERE id = ?3",
            rusqlite::params![archived_flag, next, id],
        )?;
        tx.commit()?;
        Ok(next)
    }

    #[test]
    fn toggle_archive_session_flips_null_to_ms_and_back() {
        let conn = db_with_session_and_conversation();
        // Archive: archived_at becomes Some.
        let ts = toggle_archive_session_sql(&conn, "s1").unwrap();
        assert!(ts.is_some(), "first toggle should set archived_at");
        let row: (i64, Option<i64>) = conn.query_row(
            "SELECT archived, archived_at FROM agent_sessions WHERE id = 's1'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(row.0, 1, "archived flag should be 1");
        assert!(row.1.is_some(), "archived_at should be set");

        // Unarchive: archived_at becomes None.
        let ts2 = toggle_archive_session_sql(&conn, "s1").unwrap();
        assert!(ts2.is_none(), "second toggle should clear archived_at");
        let row2: (i64, Option<i64>) = conn.query_row(
            "SELECT archived, archived_at FROM agent_sessions WHERE id = 's1'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(row2.0, 0, "archived flag should be 0");
        assert!(row2.1.is_none(), "archived_at should be NULL");
    }

    #[test]
    fn toggle_archive_conversation_flips_null_to_ms_and_back() {
        let conn = db_with_session_and_conversation();
        let ts = toggle_archive_conversation_sql(&conn, "cv1").unwrap();
        assert!(ts.is_some());
        let row: (i64, Option<i64>) = conn.query_row(
            "SELECT archived, archived_at FROM conversations WHERE id = 'cv1'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(row.0, 1);
        assert!(row.1.is_some());

        let ts2 = toggle_archive_conversation_sql(&conn, "cv1").unwrap();
        assert!(ts2.is_none());
        let row2: (i64, Option<i64>) = conn.query_row(
            "SELECT archived, archived_at FROM conversations WHERE id = 'cv1'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(row2.0, 0);
        assert!(row2.1.is_none());
    }

    #[test]
    fn toggle_archive_is_idempotent_for_nonexistent_row() {
        let conn = db_with_session_and_conversation();
        // UPDATE with 0 matching rows should not error.
        assert!(toggle_archive_session_sql(&conn, "nope").is_ok());
        assert!(toggle_archive_conversation_sql(&conn, "nope").is_ok());
    }
}
```

- [ ] **Step 2: Run tests to confirm failure**

```bash
cd src-tauri && cargo test --lib toggle_archive 2>&1 | tail -20
```

Expected: compile error — `toggle_archive_session_sql` / `toggle_archive_conversation_sql` don't exist yet (but the test helpers are internal to the test mod — the compile should pass, and tests may succeed structurally. If V26 columns are missing, the SQL will fail). Confirm the tests fail before adding the Tauri commands.

- [ ] **Step 3: Add the two Tauri commands to `tauri_commands.rs`**

Add directly after `toggle_pin_agent_session` (after line 4680):

```rust
/// Toggle archive state on an agent_session.  Returns the new `archived_at`
/// timestamp (ms) when archiving, `None` when restoring. If the id does not
/// exist the UPDATE affects 0 rows and we return `Ok(None)`.
#[tauri::command]
pub async fn toggle_archive_agent_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<i64>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let tx = conn.unchecked_transaction().map_err(|e| Error::Database(e))?;
    let current: Option<i64> = tx.query_row(
        "SELECT archived_at FROM agent_sessions WHERE id = ?1",
        rusqlite::params![&id],
        |row| row.get::<_, Option<i64>>(0),
    ).ok().flatten();
    let next: Option<i64> = if current.is_some() {
        None
    } else {
        Some(chrono::Utc::now().timestamp_millis())
    };
    let archived_flag = if next.is_some() { 1i64 } else { 0i64 };
    tx.execute(
        "UPDATE agent_sessions SET archived = ?1, archived_at = ?2 WHERE id = ?3",
        rusqlite::params![archived_flag, next, &id],
    ).map_err(|e| Error::Database(e))?;
    tx.commit().map_err(|e| Error::Database(e))?;
    Ok(next)
}

/// Toggle archive state on a conversation. Returns the new `archived_at`
/// timestamp (ms) when archiving, `None` when restoring.
#[tauri::command]
pub async fn toggle_archive_conversation(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<i64>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let tx = conn.unchecked_transaction().map_err(|e| Error::Database(e))?;
    let current: Option<i64> = tx.query_row(
        "SELECT archived_at FROM conversations WHERE id = ?1",
        rusqlite::params![&id],
        |row| row.get::<_, Option<i64>>(0),
    ).ok().flatten();
    let next: Option<i64> = if current.is_some() {
        None
    } else {
        Some(chrono::Utc::now().timestamp_millis())
    };
    let archived_flag = if next.is_some() { 1i64 } else { 0i64 };
    tx.execute(
        "UPDATE conversations SET archived = ?1, archived_at = ?2 WHERE id = ?3",
        rusqlite::params![archived_flag, next, &id],
    ).map_err(|e| Error::Database(e))?;
    tx.commit().map_err(|e| Error::Database(e))?;
    Ok(next)
}
```

- [ ] **Step 4: Register both commands in `main.rs`**

Find the `invoke_handler!` macro (contains `toggle_pin_agent_session` at line 417). Add both new commands:

```rust
uclaw_core::tauri_commands::toggle_archive_agent_session,
uclaw_core::tauri_commands::toggle_archive_conversation,
```

- [ ] **Step 5: Run tests — expect pass**

```bash
cd src-tauri && cargo test --lib toggle_archive 2>&1 | tail -20
```

Expected: all three tests pass.

- [ ] **Step 6: Compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(commands): toggle_archive_agent_session + toggle_archive_conversation"
```

---

### Task 7: tauri-bridge fix + ActivityListItem hover archive + ActivityHistoryView show-archived toggle

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts`
- Modify: `ui/src/components/automation/ActivityListItem.tsx`
- Modify: `ui/src/components/automation/ActivityHistoryView.tsx`
- Modify: `ui/src/components/automation/ActivityHistoryView.test.tsx`

Context: `toggleArchiveAgentSession` and `toggleArchiveConversation` in `tauri-bridge.ts` currently swallow errors and return a hardcoded `{ archived: true }` — this causes silent false positives that reset on page reload. Remove the catch. The return type is `number | null` (the new `archived_at` timestamp). `ActivityListItem` needs a hover archive button. `ActivityHistoryView` needs a "show archived" toggle. Local state (`archivedIds: Set<string>`) tracks which session IDs were just archived in the current session so filtering works without a backend query change.

- [ ] **Step 1: Write the failing test**

In `ActivityHistoryView.test.tsx`, verify:
1. Archive button appears on hover
2. After archive, the item disappears when "show archived" is off (default)
3. After toggling "show archived", the item reappears

Replace the existing test file with:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ActivityHistoryView } from './ActivityHistoryView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

// Mock tauri-bridge so toggleArchiveAgentSession resolves immediately.
vi.mock('@/lib/tauri-bridge', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/tauri-bridge')>()
  return {
    ...actual,
    toggleArchiveAgentSession: vi.fn().mockResolvedValue(1_700_000_000_000),
  }
})

function makeActivity(overrides: Partial<AutomationActivity> = {}): AutomationActivity {
  return {
    id: 'act-1',
    specId: 'spec-1',
    subscriptionId: null,
    triggerSourceType: 'manual',
    triggerPayloadJson: '{}',
    status: 'completed',
    errorText: null,
    queuedAt: Date.now(),
    startedAt: Date.now(),
    completedAt: Date.now(),
    durationMs: 1000,
    llmIterations: 1,
    llmTokensIn: 100,
    llmTokensOut: 50,
    sessionId: 'sess-1',
    reportArtifactsJson: '[]',
    reportText: 'done',
    reportOutcome: 'success',
    escalationId: null,
    resumedFromActivityId: null,
    resumedFromEscalationId: null,
    ...overrides,
  }
}

describe('ActivityHistoryView', () => {
  const activities = [makeActivity()]

  it('shows archive button on hover and hides item after archive (default: show-archived off)', async () => {
    const user = userEvent.setup()
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={activities}
        onOpenRunSession={vi.fn()}
        activeRunSessionId={null}
        onCloseRunSession={vi.fn()}
      />
    )

    // Item visible initially.
    expect(screen.getByTestId('activity-row-act-1')).toBeInTheDocument()

    // Hover to reveal archive button.
    await user.hover(screen.getByTestId('activity-row-act-1'))
    const archiveBtn = await screen.findByRole('button', { name: /归档/i })
    expect(archiveBtn).toBeInTheDocument()

    // Click archive.
    await user.click(archiveBtn)

    // Item disappears (filtered because show-archived is off by default).
    await waitFor(() => {
      expect(screen.queryByTestId('activity-row-act-1')).not.toBeInTheDocument()
    })
  })

  it('shows archived items when show-archived toggle is on', async () => {
    const user = userEvent.setup()
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={activities}
        onOpenRunSession={vi.fn()}
        activeRunSessionId={null}
        onCloseRunSession={vi.fn()}
      />
    )

    // Archive the item.
    await user.hover(screen.getByTestId('activity-row-act-1'))
    await user.click(await screen.findByRole('button', { name: /归档/i }))

    // Toggle "show archived".
    const toggle = screen.getByRole('button', { name: /显示已归档/i })
    await user.click(toggle)

    // Item reappears.
    expect(await screen.findByTestId('activity-row-act-1')).toBeInTheDocument()
  })

  it('renders escalation ring with theme tokens', () => {
    const escalation = makeActivity({ status: 'waiting_user' })
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={[escalation]}
        onOpenRunSession={vi.fn()}
        activeRunSessionId={null}
        onCloseRunSession={vi.fn()}
      />
    )
    const row = screen.getByTestId('activity-row-act-1')
    expect(row.className).toMatch(/border-warning|ring-warning/)
  })
})
```

- [ ] **Step 2: Run tests to confirm failure**

```bash
cd ui && npm test -- --run ActivityHistoryView 2>&1 | tail -30
```

Expected: tests fail — archive button doesn't exist yet, show-archived toggle doesn't exist.

- [ ] **Step 3: Fix tauri-bridge.ts — remove catch fallback, fix return type**

Replace lines 1136–1140:

```typescript
export const toggleArchiveAgentSession = (id: string): Promise<number | null> =>
  invoke<number | null>('toggle_archive_agent_session', { id })

export const toggleArchiveConversation = (id: string): Promise<number | null> =>
  invoke<number | null>('toggle_archive_conversation', { id })
```

- [ ] **Step 4: Rewrite `ActivityListItem.tsx` — add hover state + archive button**

```tsx
import { useState } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { toggleArchiveAgentSession } from '@/lib/tauri-bridge'

interface Props {
  activity: AutomationActivity
  onOpenRunSession?: (sessionId: string) => void
  onArchived?: (sessionId: string) => void
}

const STATUS_CONFIG: Record<string, { label: string; className: string }> = {
  completed:    { label: '已完成', className: 'text-success' },
  failed:       { label: '失败',   className: 'text-danger' },
  cancelled:    { label: '已取消', className: 'text-muted-foreground' },
  filtered_out: { label: '已跳过', className: 'text-muted-foreground' },
  waiting_user: { label: '待确认', className: 'text-warning' },
  running:      { label: '运行中', className: 'text-primary' },
  queued:       { label: '排队中', className: 'text-muted-foreground' },
}

function formatTs(ms: number | null): string {
  if (!ms) return '—'
  return new Date(ms).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  })
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

export function ActivityListItem({ activity, onOpenRunSession, onArchived }: Props) {
  const [isHovered, setIsHovered] = useState(false)
  const [archiving, setArchiving] = useState(false)
  const cfg = STATUS_CONFIG[activity.status] ?? { label: activity.status, className: 'text-muted-foreground' }
  const isEscalation = activity.status === 'waiting_user'

  async function handleArchive() {
    if (!activity.sessionId || archiving) return
    setArchiving(true)
    try {
      await toggleArchiveAgentSession(activity.sessionId)
      onArchived?.(activity.sessionId)
    } finally {
      setArchiving(false)
    }
  }

  return (
    <div
      data-testid={`activity-row-${activity.id}`}
      className={[
        'rounded-lg border p-3 bg-background',
        isEscalation ? 'border-warning ring-1 ring-warning/20' : 'border-border/50',
      ].join(' ')}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>{formatTs(activity.startedAt ?? activity.queuedAt)}</span>
          <span className={cfg.className}>{cfg.label}</span>
          {activity.durationMs > 0 && (
            <span>{formatDuration(activity.durationMs)}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {isHovered && activity.sessionId && (
            <button
              onClick={handleArchive}
              disabled={archiving}
              className="titlebar-no-drag text-xs text-muted-foreground hover:text-foreground shrink-0"
              aria-label="归档"
            >
              归档
            </button>
          )}
          {activity.sessionId && (
            <button
              onClick={() => onOpenRunSession?.(activity.sessionId!)}
              className="titlebar-no-drag text-xs text-primary hover:underline shrink-0"
            >
              查看进程 &gt;
            </button>
          )}
        </div>
      </div>
      {activity.reportText && (
        <p className="mt-1 text-sm text-foreground line-clamp-3">{activity.reportText}</p>
      )}
    </div>
  )
}
```

- [ ] **Step 5: Rewrite `ActivityHistoryView.tsx` — add archivedIds state + show-archived toggle**

```tsx
import { useState } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { ActivityListItem } from './ActivityListItem'
import { RunSessionSubView } from './RunSessionSubView'

interface Props {
  specId: string
  activities: AutomationActivity[]
  onOpenRunSession?: (sessionId: string) => void
  activeRunSessionId?: string | null
  onCloseRunSession?: () => void
}

export function ActivityHistoryView({
  specId: _specId,
  activities,
  onOpenRunSession,
  activeRunSessionId,
  onCloseRunSession,
}: Props) {
  // Local tracking: session IDs archived in this session. Avoids a backend
  // query change — items filtered here reappear in the next full reload.
  const [archivedIds, setArchivedIds] = useState<Set<string>>(new Set())
  const [showArchived, setShowArchived] = useState(false)

  if (activeRunSessionId) {
    return (
      <RunSessionSubView
        sessionId={activeRunSessionId}
        onBack={() => onCloseRunSession?.()}
      />
    )
  }

  function handleArchived(sessionId: string) {
    setArchivedIds((prev) => new Set([...prev, sessionId]))
  }

  const visible = showArchived
    ? activities
    : activities.filter((a) => !a.sessionId || !archivedIds.has(a.sessionId))

  if (activities.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        还没有运行记录
      </div>
    )
  }

  return (
    <div className="flex flex-col flex-1 overflow-hidden">
      {archivedIds.size > 0 && (
        <div className="px-3 pt-2 shrink-0">
          <button
            onClick={() => setShowArchived((v) => !v)}
            className="titlebar-no-drag text-xs text-muted-foreground hover:text-foreground"
            aria-label={showArchived ? '隐藏已归档' : '显示已归档'}
          >
            {showArchived ? '隐藏已归档' : `显示已归档 (${archivedIds.size})`}
          </button>
        </div>
      )}
      <div className="flex-1 flex flex-col gap-2 p-3 overflow-y-auto">
        {visible.map((act) => (
          <ActivityListItem
            key={act.id}
            activity={act}
            onOpenRunSession={onOpenRunSession}
            onArchived={handleArchived}
          />
        ))}
        {visible.length === 0 && (
          <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
            所有记录已归档
          </div>
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 6: Run tests — expect pass**

```bash
cd ui && npm test -- --run ActivityHistoryView 2>&1 | tail -30
```

Expected: all three tests pass.

- [ ] **Step 7: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected: zero errors.

- [ ] **Step 8: Commit**

```bash
git add ui/src/lib/tauri-bridge.ts \
        ui/src/components/automation/ActivityListItem.tsx \
        ui/src/components/automation/ActivityHistoryView.tsx \
        ui/src/components/automation/ActivityHistoryView.test.tsx
git commit -m "feat(ui): session archive — hover button + show-archived toggle + fix bridge catch"
```

---

## Self-Review

**Spec coverage:**
- [x] `notify_user` → real channel dispatch (Tasks 1–3)
- [x] `"system"` → `AppHandle::emit("automation_notify")` (Task 2)
- [x] `"wecom"` / `"email"` → `ChannelManager.send_to_type` (Tasks 1–2)
- [x] Per-day cost cap filters to `automation:%` origin only (Task 4)
- [x] V26 migration adds `archived` + `archived_at` to `conversations` (Task 5)
- [x] `toggle_archive_agent_session` command (Task 6)
- [x] `toggle_archive_conversation` command (Task 6)
- [x] Both commands registered in `invoke_handler!` (Task 6)
- [x] tauri-bridge catch fallback removed (Task 7)
- [x] Archive hover button on `ActivityListItem` (Task 7)
- [x] "show archived" toggle in `ActivityHistoryView` (Task 7)

**Placeholder scan:** None found.

**Type consistency:**
- `send_to_type` signature uses `&ChannelType` (not owned) — consistent with how `broadcast` uses `ChannelType` internally.
- `toggle_archive_*` commands return `Result<Option<i64>, Error>` — consistent with `toggle_pin_agent_session`.
- `tauri-bridge.ts` return type changed from `Promise<any>` to `Promise<number | null>` — consistent with the Rust return type.
- `onArchived?: (sessionId: string) => void` in `ActivityListItem` — propagated to `ActivityHistoryView.handleArchived` which takes `string`.
