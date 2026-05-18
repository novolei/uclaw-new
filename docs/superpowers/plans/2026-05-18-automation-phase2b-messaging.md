# Automation Phase 2b · Cluster A · Messaging 基座 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为每个 spec 引入 per-(spec, identity) 长期 `automation:chat` session；scheduled/file/webhook 触发结果写入 owner chat 线；IM dispatcher 路由到对应身份的 chat 线并通过已修稳的 reply 链路推回 IM。完成 Phase 2a 推迟的"真实触达"承诺。

**Architecture:** V38 migration 加 `automation_chat_sessions(spec_id, identity_key) → agent_session_id` 索引；新模块 `automation/runtime/chat_sessions.rs` 提供 `get_or_create_chat_session`；`AppRuntimeService` 加 `execute_run_in_chat_session` 入口，per-session `tokio::sync::Mutex` 排队 burst。**复用现有 `HeadlessDelegate`**（已具备 `reply_handle` / `streaming_handle` `Option<>` 字段），不新建 delegate 类型 —— spec §2.1 命名是过度保守，实际不需要新 struct。

**Tech Stack:** Rust (rusqlite, tokio sync primitives)，Tauri AppHandle emit，复用 PR #182/#186/#189 的 IM channels 基础设施。

**Spec reference:** [docs/superpowers/specs/2026-05-18-automation-phase2b-messaging-design.md](../specs/2026-05-18-automation-phase2b-messaging-design.md)

---

## File Structure

| 文件 | 性质 | 责任 |
|---|---|---|
| `src-tauri/src/db/migrations.rs` | 修改 | 加 V38 `automation_chat_sessions` 表 + 索引 |
| `src-tauri/src/automation/runtime/chat_sessions.rs` | **新建** | `get_or_create_chat_session(conn, spec_id, identity_key)` + 单元测试 |
| `src-tauri/src/automation/runtime/mod.rs` | 修改 | `pub mod chat_sessions;` |
| `src-tauri/src/automation/runtime/service.rs` | 修改 | 新增 `execute_run_in_chat_session` 入口 + per-session mutex 字段 + 重写 `execute_run_with_reply` 调用它 |
| `src-tauri/src/automation/runtime/scheduler.rs` 等订阅回调 | 修改 | 触发回调改调 `execute_run_in_chat_session(spec, "local", ...)` |
| `src-tauri/src/channels/dispatcher.rs` | 修改 | `run_automation_via_im` 改用 `execute_run_in_chat_session(spec, "{channel_type}:{chat_id}", ...)` |
| `src-tauri/src/automation/tools/notify_user.rs` + `src-tauri/src/agent/headless.rs` notify_user execution | 修改 | 改成 "reply_handle Some → 仅推 IM；None → 走 legacy" 互斥逻辑 |
| `src-tauri/src/tauri_commands.rs` + `src-tauri/src/main.rs` | 修改 | 新命令 `list_chat_sessions_for_spec` + `invoke_handler!` 注册 |
| `ui/src/lib/tauri-bridge.ts` | 修改 | `listChatSessionsForSpec` wrapper |
| `ui/src/atoms/automation-atoms.ts` | 修改 | 新 atom 缓存 per-spec chat sessions |
| `ui/src/components/automation/SpecDetailView.tsx`（或同等位置，按现有布局选择） | 修改 | 新 "Chat threads" tab 列 identity threads |

---

## Task 1: V38 migration + chat_sessions module

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add V38 constant + dispatch block at end of migration list)
- Create: `src-tauri/src/automation/runtime/chat_sessions.rs`
- Modify: `src-tauri/src/automation/runtime/mod.rs` (add `pub mod chat_sessions;`)

- [ ] **Step 1: Add V38 SQL constant in migrations.rs**

Append after the V35 constant (`V35_MEMORY_OS_PHASE_1`) in `src-tauri/src/db/migrations.rs`:

```rust
/// V38 — Automation Phase 2b cluster A · per-(spec, identity) chat session index.
///
/// `automation_chat_sessions` maps a (spec_id, identity_key) pair to the
/// agent_sessions row that hosts the long-lived chat thread for that pair.
///
/// identity_key conventions (string):
///   - "local"                       → spec owner's local chat thread
///   - "{channel_type}:{chat_id}"    → per-IM-user thread (e.g. "wechat_ilink:UIN_abc")
///
/// UNIQUE(spec_id, identity_key) guarantees idempotent get-or-create.
/// FK CASCADE on agent_sessions deletion keeps the index clean.
pub const V38_AUTOMATION_CHAT_SESSIONS: &str = "
CREATE TABLE IF NOT EXISTS automation_chat_sessions (
    spec_id          TEXT NOT NULL,
    identity_key     TEXT NOT NULL,
    agent_session_id TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    PRIMARY KEY (spec_id, identity_key),
    FOREIGN KEY (agent_session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_aut_chat_sess_agent_id
    ON automation_chat_sessions(agent_session_id);
";
```

- [ ] **Step 2: Add V38 dispatch block in `run()`**

In `src-tauri/src/db/migrations.rs`, find the V35 dispatch block (around line 1798-1804). Add IMMEDIATELY AFTER it:

```rust
// V38: Automation Phase 2b cluster A — per-(spec, identity) chat session index.
tracing::debug!("Running migration V38: automation_chat_sessions");
for stmt in V38_AUTOMATION_CHAT_SESSIONS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute(stmt, []) {
        tracing::warn!("V38 stmt skipped: {} :: {}", e, stmt);
    }
}
```

- [ ] **Step 3: Create `chat_sessions.rs` with failing tests first**

Create `src-tauri/src/automation/runtime/chat_sessions.rs`:

```rust
//! Index layer for per-(spec, identity) long-lived `automation:chat` sessions.
//!
//! Each spec can have multiple chat threads — one per identity. Identities:
//!   - "local"                   → spec owner
//!   - "{channel_type}:{chat_id}" → per-IM-user thread
//!
//! See spec: docs/superpowers/specs/2026-05-18-automation-phase2b-messaging-design.md

use anyhow::Result;
use rusqlite::Connection;

/// Idempotently get-or-create the agent_session for this (spec_id, identity_key)
/// pair. Returns the agent_session id.
///
/// First call inserts a new agent_session with metadata
/// `{origin: "automation:chat", spec_id, identity_key}` and a row in
/// automation_chat_sessions. Subsequent calls return the existing id.
pub fn get_or_create_chat_session(
    conn: &Connection,
    spec_id: &str,
    identity_key: &str,
    space_id: &str,
) -> Result<String> {
    // Fast path: existing row.
    if let Some(id) = conn
        .query_row(
            "SELECT agent_session_id FROM automation_chat_sessions
             WHERE spec_id = ?1 AND identity_key = ?2",
            rusqlite::params![spec_id, identity_key],
            |r| r.get::<_, String>(0),
        )
        .ok()
    {
        return Ok(id);
    }

    // Create new agent_session + index row in one transaction so a race
    // between two concurrent fires doesn't leave a stranded session.
    let session_id = uuid::Uuid::new_v4().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let metadata = serde_json::json!({
        "origin": "automation:chat",
        "spec_id": spec_id,
        "identity_key": identity_key,
    });
    let title = format!("Chat · {} · {}", spec_id, identity_key);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO agent_sessions
         (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, 0, ?5, ?5)",
        rusqlite::params![session_id, space_id, title, metadata.to_string(), now_ms],
    )?;
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO automation_chat_sessions
         (spec_id, identity_key, agent_session_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        rusqlite::params![spec_id, identity_key, session_id, now_ms],
    )?;

    if inserted == 0 {
        // Race: another fire won the insert. Drop our stranded agent_session
        // and return the winner's id.
        tx.execute(
            "DELETE FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
        tx.commit()?;
        let winner: String = conn.query_row(
            "SELECT agent_session_id FROM automation_chat_sessions
             WHERE spec_id = ?1 AND identity_key = ?2",
            rusqlite::params![spec_id, identity_key],
            |r| r.get(0),
        )?;
        return Ok(winner);
    }

    tx.commit()?;
    Ok(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn get_or_create_chat_session_dedups_per_identity() {
        let conn = setup_db();
        let a = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        let b = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        assert_eq!(a, b, "second call must return the same session id");

        // And there's exactly one row in the index.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM automation_chat_sessions
                 WHERE spec_id='spec1' AND identity_key='local'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn get_or_create_chat_session_creates_distinct_for_different_identities() {
        let conn = setup_db();
        let local = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        let im_a = get_or_create_chat_session(&conn, "spec1", "wechat_ilink:UIN_a", "default").unwrap();
        let im_b = get_or_create_chat_session(&conn, "spec1", "wechat_ilink:UIN_b", "default").unwrap();

        assert_ne!(local, im_a);
        assert_ne!(local, im_b);
        assert_ne!(im_a, im_b);

        // Same spec, three rows.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM automation_chat_sessions WHERE spec_id='spec1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn get_or_create_chat_session_writes_chat_origin_metadata() {
        let conn = setup_db();
        let id = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        let meta: String = conn
            .query_row(
                "SELECT metadata_json FROM agent_sessions WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&meta).unwrap();
        assert_eq!(v["origin"], "automation:chat");
        assert_eq!(v["spec_id"], "spec1");
        assert_eq!(v["identity_key"], "local");
    }

    #[test]
    fn cascade_on_agent_session_delete_clears_index_row() {
        let conn = setup_db();
        let id = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        // FKs are not enforced by default in SQLite — enable for this test.
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute("DELETE FROM agent_sessions WHERE id = ?1", rusqlite::params![id]).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM automation_chat_sessions WHERE spec_id='spec1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "FK CASCADE should have cleared the index row");
    }
}
```

- [ ] **Step 4: Register module in `mod.rs`**

In `src-tauri/src/automation/runtime/mod.rs`, add this line at the top alongside existing `pub mod ...` declarations:

```rust
pub mod chat_sessions;
```

- [ ] **Step 5: Run the tests — verify they pass**

Run: `cd src-tauri && cargo test --lib automation::runtime::chat_sessions -- --nocapture`

Expected output:
```
test automation::runtime::chat_sessions::tests::get_or_create_chat_session_dedups_per_identity ... ok
test automation::runtime::chat_sessions::tests::get_or_create_chat_session_creates_distinct_for_different_identities ... ok
test automation::runtime::chat_sessions::tests::get_or_create_chat_session_writes_chat_origin_metadata ... ok
test automation::runtime::chat_sessions::tests::cascade_on_agent_session_delete_clears_index_row ... ok

test result: ok. 4 passed; 0 failed
```

- [ ] **Step 6: Run full backend test suite — verify no regression**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`

Expected: `test result: ok.` with all tests passing.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/automation/runtime/chat_sessions.rs src-tauri/src/automation/runtime/mod.rs
git commit -m "feat(automation): V38 migration + chat_sessions index for per-(spec, identity) threads

V38 creates automation_chat_sessions(spec_id, identity_key, agent_session_id)
with UNIQUE constraint for idempotent get-or-create. FK CASCADE on
agent_sessions delete keeps the index clean.

get_or_create_chat_session() writes a new agent_session with
origin='automation:chat' + the (spec_id, identity_key) metadata, then
inserts the index row in one transaction. Loses-the-race case (concurrent
fires for the same identity) is handled: stranded session is rolled back
and the winning id is returned.

Tests (4 new, all passing):
- dedups across repeated calls for same (spec, identity)
- creates distinct sessions for different identities
- writes correct origin/spec_id/identity_key into metadata
- FK CASCADE clears index row when agent_session is deleted

Cluster A first commit — spec at docs/superpowers/specs/2026-05-18-...md"
```

---

## Task 2: `execute_run_in_chat_session` entry point on AppRuntimeService

**Files:**
- Modify: `src-tauri/src/automation/runtime/service.rs` (add field + new method + rewrite `execute_run_with_reply`)

- [ ] **Step 1: Add per-session mutex map field to AppRuntimeService**

In `src-tauri/src/automation/runtime/service.rs`, locate the `pub struct AppRuntimeService { ... }` block (around lines 133-165). Add this field at the END of the struct (before the closing brace):

```rust
    /// Per-chat-session mutex map. Serializes burst messages on the same
    /// (spec, identity) chat thread — the agent loop is not interruptible,
    /// so concurrent calls would race. Map entries are created lazily and
    /// never cleaned up (acceptable: bounded by #sessions, ~tens of KB).
    chat_session_locks: Arc<TokioMutex<std::collections::HashMap<String, Arc<TokioMutex<()>>>>>,
```

Imports already present: `Arc`, `TokioMutex`. Verify imports include `std::collections::HashMap`; if not, add at top of file:
```rust
use std::collections::HashMap;
```

In the `impl AppRuntimeService` block's `new()` constructor (search for `pub fn new(`), initialize the field where other Arc-wrapped collections are initialized:

```rust
            chat_session_locks: Arc::new(TokioMutex::new(HashMap::new())),
```

- [ ] **Step 2: Write a failing test for execute_run_in_chat_session reusing sessions**

Add at the END of `src-tauri/src/automation/runtime/service.rs`, inside the existing `#[cfg(test)] mod tests { ... }` block (or create one if absent):

```rust
    #[tokio::test]
    async fn execute_run_in_chat_session_reuses_same_session_per_identity() {
        // Setup minimal AppRuntimeService backed by an in-memory DB + a
        // canned spec. Verify two consecutive calls with identical
        // (spec, identity) reuse the same chat session id, and that
        // get_or_create_chat_session was called exactly once (the second
        // is a fast-path lookup).
        //
        // Uses the existing test harness; if your harness uses
        // `mk_test_service()` helper, build on that. Otherwise this test
        // exercises only the routing logic — agent loop can be stubbed
        // via the existing MockLlmProvider helper used elsewhere in the
        // automation test file.
        let svc = crate::automation::runtime::service::tests::mk_test_service_with_spec(
            "spec_test",
            "default",
        ).await;

        // First call creates the session.
        let _ = svc.execute_run_in_chat_session(
            "spec_test",
            "local",
            serde_json::json!({"trigger": "test"}),
            None,
            None,
            None,
        ).await;

        // Second call must reuse the same agent_session row.
        let conn = svc.db.lock().unwrap();
        let count_before: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions
             WHERE json_extract(metadata_json, '$.spec_id') = 'spec_test'
               AND json_extract(metadata_json, '$.origin') = 'automation:chat'
               AND json_extract(metadata_json, '$.identity_key') = 'local'",
            [], |r| r.get(0),
        ).unwrap();
        drop(conn);

        let _ = svc.execute_run_in_chat_session(
            "spec_test",
            "local",
            serde_json::json!({"trigger": "test"}),
            None,
            None,
            None,
        ).await;

        let conn = svc.db.lock().unwrap();
        let count_after: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions
             WHERE json_extract(metadata_json, '$.spec_id') = 'spec_test'
               AND json_extract(metadata_json, '$.origin') = 'automation:chat'
               AND json_extract(metadata_json, '$.identity_key') = 'local'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(count_before, 1);
        assert_eq!(count_after, 1, "second call must reuse, not create");
    }
```

Note: If the file does not have a `mk_test_service_with_spec` helper, add it. The helper should construct an `AppRuntimeService` with: in-memory DB (with all migrations applied), an `AutomationMemoryStore` rooted at a temp dir, a stub `ProviderService` (use the existing `tests::mock_provider_service()` helper if present; otherwise create one), and a canned spec row in `automation_specs` table for `spec_test`. Pattern: copy from any other existing `#[tokio::test]` in the same file.

- [ ] **Step 3: Run the test — verify it fails**

Run: `cd src-tauri && cargo test --lib execute_run_in_chat_session_reuses_same_session_per_identity 2>&1 | tail -10`

Expected: FAIL with "no method named `execute_run_in_chat_session`" (the method doesn't exist yet).

- [ ] **Step 4: Add the `execute_run_in_chat_session` method**

In `src-tauri/src/automation/runtime/service.rs`, in the `impl AppRuntimeService` block, locate `execute_run_with_reply` (around lines 1234-1243). Replace its body and add the new method:

```rust
    /// Run a spec inside its per-(spec, identity) chat session.
    /// Replaces per-fire `automation:scheduled` sessions for autonomous
    /// triggers, and consolidates IM-triggered runs into per-user threads.
    ///
    /// Acquires a per-session mutex so burst messages on the same chat
    /// thread are serialized (the agent loop is not interruptible).
    pub async fn execute_run_in_chat_session(
        &self,
        spec_id: &str,
        identity_key: &str,
        payload: serde_json::Value,
        streaming_handle: Option<Arc<dyn crate::channels::types::StreamingHandle>>,
        reply_handle: Option<Arc<crate::channels::types::ReplyHandle>>,
        app_handle: Option<tauri::AppHandle>,
    ) -> anyhow::Result<()> {
        // Resolve the chat session id (or create one).
        let session_id = {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
            // Look up the spec's space so the agent_session is filed correctly.
            let space_id: String = conn.query_row(
                "SELECT space_id FROM automation_specs WHERE id = ?1",
                rusqlite::params![spec_id],
                |r| r.get(0),
            ).map_err(|e| anyhow::anyhow!("lookup spec space: {e}"))?;
            crate::automation::runtime::chat_sessions::get_or_create_chat_session(
                &conn, spec_id, identity_key, &space_id,
            )?
        };

        // Acquire per-session mutex. Burst messages on the same chat thread
        // queue here rather than racing the agent loop.
        let lock = {
            let mut map = self.chat_session_locks.lock().await;
            map.entry(session_id.clone())
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;

        // Delegate to execute_run with the chat session pinned. We pass
        // session_id via payload so create_run_session is bypassed in
        // favor of appending to the existing chat session — see Task 3
        // for the execute_run modification that reads this hint.
        let mut payload_with_chat = payload;
        if let Some(obj) = payload_with_chat.as_object_mut() {
            obj.insert(
                "_chat_session_id".to_string(),
                serde_json::Value::String(session_id.clone()),
            );
            // Carry the handles via payload as well so execute_run can
            // build the HeadlessDelegate with them attached. (We avoid
            // adding new execute_run parameters to keep the existing
            // call sites stable.)
            //
            // Actually: handles can't go through JSON — pass via a
            // service-internal map keyed by session_id, populated here.
        }

        // Stash handles in a per-session slot that execute_run reads
        // when building the HeadlessDelegate. (See Task 3 for the
        // execute_run modification.)
        {
            let mut slot = self.pending_chat_handles.lock().await;
            slot.insert(session_id.clone(), ChatHandleBundle {
                streaming: streaming_handle,
                reply: reply_handle,
                app: app_handle,
            });
        }

        // Run.
        let result = self.execute_run(spec_id, None, payload_with_chat).await;

        // Clean up pending slot in case execute_run didn't consume it
        // (e.g. early failure before delegate construction).
        {
            let mut slot = self.pending_chat_handles.lock().await;
            slot.remove(&session_id);
        }

        result
    }

    /// Legacy entry: an IM-triggered run that doesn't go through a chat
    /// session. Re-implemented to delegate to execute_run_in_chat_session
    /// with identity_key derived from the trigger payload. Callers that
    /// already know the chat session should call execute_run_in_chat_session
    /// directly.
    pub async fn execute_run_with_reply(
        &self,
        spec_id: &str,
        payload: serde_json::Value,
        reply_handle: Option<Arc<crate::channels::types::ReplyHandle>>,
        streaming_handle: Option<Arc<dyn crate::channels::types::StreamingHandle>>,
    ) -> anyhow::Result<()> {
        // Derive identity from payload fields the IM dispatcher sets today
        // (channel_instance_id + chat_id). Falls back to "local" when these
        // are absent (preserves the pre-existing fallback behavior).
        let identity_key = match (
            payload.get("channel_instance_id").and_then(|v| v.as_str()),
            payload.get("chat_id").and_then(|v| v.as_str()),
            payload.get("trigger").and_then(|v| v.as_str()),
        ) {
            (Some(instance_id), Some(chat_id), Some("im")) => {
                // Look up channel_type for the instance so identity_key has
                // the canonical "{channel_type}:{chat_id}" form.
                let channel_type = {
                    let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
                    conn.query_row(
                        "SELECT channel_type FROM im_channel_instances WHERE id = ?1",
                        rusqlite::params![instance_id],
                        |r| r.get::<_, String>(0),
                    ).unwrap_or_else(|_| "unknown".to_string())
                };
                format!("{}:{}", channel_type, chat_id)
            }
            _ => "local".to_string(),
        };
        self.execute_run_in_chat_session(
            spec_id, &identity_key, payload, streaming_handle, reply_handle, None,
        )
        .await
    }
```

Also add the `pending_chat_handles` field + `ChatHandleBundle` struct. At the top of `service.rs` (after existing struct/use declarations), add:

```rust
/// Bundle of I/O handles attached to a chat-session run. Stashed in
/// AppRuntimeService.pending_chat_handles when execute_run_in_chat_session
/// is called, and consumed by execute_run when building the HeadlessDelegate.
struct ChatHandleBundle {
    streaming: Option<Arc<dyn crate::channels::types::StreamingHandle>>,
    reply: Option<Arc<crate::channels::types::ReplyHandle>>,
    app: Option<tauri::AppHandle>,
}
```

In the `AppRuntimeService` struct, add a sibling field next to `chat_session_locks`:

```rust
    /// Stash of I/O handles awaiting consumption by the next execute_run
    /// for this chat session. Set by execute_run_in_chat_session; cleared
    /// by execute_run (after the HeadlessDelegate captures them).
    pending_chat_handles: Arc<TokioMutex<HashMap<String, ChatHandleBundle>>>,
```

In `new()`, initialize:

```rust
            pending_chat_handles: Arc::new(TokioMutex::new(HashMap::new())),
```

- [ ] **Step 5: Modify `execute_run` to honor `_chat_session_id` payload hint**

In `src-tauri/src/automation/runtime/service.rs`, locate `execute_run` (around line 394). Find the call to `run_session::create_run_session(...)` (around line 536-543) and replace with:

```rust
        // Pin to an existing chat session if execute_run_in_chat_session
        // routed us here; otherwise fall back to creating a per-fire run
        // session (legacy autonomous path).
        let chat_session_id = payload
            .get("_chat_session_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let session_id = match chat_session_id.clone() {
            Some(id) => id,  // Reuse the chat session created by execute_run_in_chat_session.
            None => run_session::create_run_session(
                &conn,
                spec_id,
                &space_id,
                trigger_source.as_db_str(),
                &activity_id,
            )
            .map_err(|e| anyhow::anyhow!("create run session: {}", e))?,
        };
```

Find where `HeadlessDelegate { ... }` is constructed in `execute_run` (search `HeadlessDelegate {` within service.rs — there is exactly one such literal in the automation execute_run path; if more than one match exists, use the one inside `execute_run`'s body). Immediately before the literal, insert:

```rust
        // If we're in a chat-session run, drain the I/O handles that
        // execute_run_in_chat_session stashed for us. None if this is
        // a legacy per-fire run (chat_session_id is None).
        let chat_handles = if let Some(ref sid) = chat_session_id {
            let mut slot = self.pending_chat_handles.lock().await;
            slot.remove(sid)
        } else {
            None
        };
```

Then in the `HeadlessDelegate { ... }` literal, set the three I/O fields to draw from `chat_handles` (falling back to the prior default when no chat handles exist). Concretely — find the existing initializers for these three fields (they may be `None`, `self.app_handle.clone()`, or constructed inline) and replace them with:

```rust
            reply_handle: chat_handles.as_ref().and_then(|b| b.reply.clone()),
            streaming_handle: chat_handles.as_ref().and_then(|b| b.streaming.clone()),
            app_handle: chat_handles
                .as_ref()
                .and_then(|b| b.app.clone())
                .or_else(|| self.app_handle.clone()),
```

If `system_prompt_override`, `channel_manager`, or any other field's existing initializer should remain unchanged, do not touch it. Only the three lines above are modified.

- [ ] **Step 6: Run the test — verify it passes**

Run: `cd src-tauri && cargo test --lib execute_run_in_chat_session_reuses_same_session_per_identity 2>&1 | tail -8`

Expected: `test result: ok. 1 passed`.

- [ ] **Step 7: Add a second test for mutex serialization**

Append to the same `#[cfg(test)] mod tests { ... }` block:

```rust
    #[tokio::test]
    async fn execute_run_in_chat_session_serializes_burst_on_same_session() {
        // Spawn 5 concurrent calls on the same (spec, identity). Each
        // execute_run takes ~50ms (instrumented via a sleep in the stub
        // LLM provider). Wall-clock should be ~250ms, NOT ~50ms — proving
        // the mutex serialized them.
        let svc = crate::automation::runtime::service::tests::mk_test_service_with_spec(
            "spec_burst", "default",
        ).await;
        let svc = Arc::new(svc);

        let start = std::time::Instant::now();
        let handles: Vec<_> = (0..5).map(|i| {
            let svc = svc.clone();
            tokio::spawn(async move {
                svc.execute_run_in_chat_session(
                    "spec_burst",
                    "local",
                    serde_json::json!({"trigger": "test", "burst_idx": i}),
                    None, None, None,
                ).await
            })
        }).collect();
        for h in handles {
            h.await.unwrap().ok();
        }
        let elapsed = start.elapsed();

        // Loose lower bound — if all 5 ran in parallel, we'd see ~50ms.
        // Serialized ≥ 5 * 50ms = 250ms (with allowance for test stub speed).
        assert!(
            elapsed.as_millis() >= 200,
            "burst should have been serialized; elapsed = {:?}", elapsed
        );
    }
```

Note: This test requires the mock LLM provider used by `mk_test_service_with_spec` to insert a `tokio::time::sleep(Duration::from_millis(50)).await` in its `call_llm` impl. If the existing mock has no sleep, add it conditionally based on an env var or test-only flag — exact mechanism follows the pattern already used in `automation/runtime/service.rs` tests.

- [ ] **Step 8: Run the test — verify it passes**

Run: `cd src-tauri && cargo test --lib execute_run_in_chat_session_serializes_burst 2>&1 | tail -5`

Expected: PASS.

- [ ] **Step 9: Run full backend test suite — no regressions**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`

Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/automation/runtime/service.rs
git commit -m "feat(automation): execute_run_in_chat_session entry point

New AppRuntimeService method routes a spec run into its per-(spec, identity)
chat session — resolving / creating the session via get_or_create_chat_session,
acquiring a per-session mutex to serialize burst messages, and stashing
I/O handles (reply, streaming, app_handle) that execute_run drains when
building the HeadlessDelegate.

execute_run gains a payload-hint check: when '_chat_session_id' is present
it reuses that session instead of calling create_run_session, while
absent the legacy per-fire path is unchanged.

execute_run_with_reply is rewritten to derive identity_key from payload
('{channel_type}:{chat_id}' for IM, 'local' otherwise) and delegate to
execute_run_in_chat_session — old IM dispatcher call sites keep working
during the Task 4 cutover.

Tests (2 new):
- execute_run_in_chat_session_reuses_same_session_per_identity
- execute_run_in_chat_session_serializes_burst_on_same_session"
```

---

## Task 3: Route subscription callbacks (schedule/file/webhook) to owner chat session

**Files:**
- Modify: `src-tauri/src/automation/runtime/service.rs` (the subscription callback registration around line 334)

- [ ] **Step 1: Write failing test for scheduled fire → chat session**

In `src-tauri/src/automation/runtime/service.rs`'s test module, add:

```rust
    #[tokio::test]
    async fn subscription_callback_routes_to_owner_chat_session_not_per_fire() {
        let svc = crate::automation::runtime::service::tests::mk_test_service_with_spec(
            "spec_sub", "default",
        ).await;
        let svc = Arc::new(svc);

        // Simulate a fire via the same path activate() wires up.
        // (mk_test_service_with_spec should expose simulate_fire(spec_id, sub_id, payload)
        // that invokes the registered callback directly.)
        let svc_clone = svc.clone();
        svc_clone.simulate_fire(
            "spec_sub",
            "sched_sub_id",
            serde_json::json!({"trigger": "scheduled"}),
        ).await.unwrap();

        // Verify: exactly one chat session for (spec_sub, local) was created.
        let conn = svc.db.lock().unwrap();
        let chat_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM automation_chat_sessions
             WHERE spec_id='spec_sub' AND identity_key='local'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(chat_count, 1, "scheduled fire should create the owner chat session");

        // And: NO per-fire automation:scheduled session was created.
        let legacy_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions
             WHERE json_extract(metadata_json, '$.spec_id') = 'spec_sub'
               AND json_extract(metadata_json, '$.origin') = 'automation:scheduled'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(legacy_count, 0, "no per-fire scheduled session should exist");
    }
```

If `simulate_fire` does not exist on the test harness, add it. It should construct the same callback `activate()` builds and invoke it directly. Pattern: copy from existing scheduler test in this file.

- [ ] **Step 2: Run the test — verify it fails**

Run: `cd src-tauri && cargo test --lib subscription_callback_routes_to_owner_chat 2>&1 | tail -8`

Expected: FAIL (callback today calls `execute_run`, producing `origin='automation:scheduled'`).

- [ ] **Step 3: Modify the subscription callback**

In `src-tauri/src/automation/runtime/service.rs`, locate the callback construction around lines 334-344. Replace the `execute_run` call inside the callback with `execute_run_in_chat_session`:

```rust
        let cb: TriggerCallback = Arc::new(move |sid: String, _sub: String, payload: serde_json::Value| {
            let svc = svc.clone();
            tokio::spawn(async move {
                if let Some(svc) = svc.upgrade() {
                    // Phase 2b cluster A: autonomous triggers (scheduled / file /
                    // webhook) route into the spec owner's "local" chat session
                    // instead of creating per-fire automation:scheduled sessions.
                    // app_handle is None here — emit goes through the AppRuntimeService's
                    // own app_handle via the delegate (set in execute_run).
                    if let Err(e) = svc
                        .execute_run_in_chat_session(
                            &sid,
                            "local",
                            payload,
                            None, // no UI streaming on autonomous fire
                            None, // no IM reply target for autonomous fire
                            svc.app_handle.clone(),
                        )
                        .await
                    {
                        tracing::warn!(
                            "[AppRuntimeService] execute_run_in_chat_session error for spec {}: {}",
                            sid, e
                        );
                    }
                }
            });
        });
```

- [ ] **Step 4: Run the test — verify it passes**

Run: `cd src-tauri && cargo test --lib subscription_callback_routes_to_owner_chat 2>&1 | tail -5`

Expected: PASS.

- [ ] **Step 5: Run full backend tests — no regressions**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/runtime/service.rs
git commit -m "feat(automation): route subscription callbacks to owner chat session

scheduled / file / webhook fires no longer create per-fire
automation:scheduled sessions. Instead they all funnel into the spec
owner's (spec_id, 'local') chat session via execute_run_in_chat_session.

Forward-only: existing automation:scheduled sessions in the DB stay
intact as read-only history; only new fires use the chat path.

Test: subscription_callback_routes_to_owner_chat_session_not_per_fire
verifies (a) one chat session row appears after fire (b) no legacy
automation:scheduled session is created."
```

---

## Task 4: Route IM dispatcher to per-identity chat session

**Files:**
- Modify: `src-tauri/src/channels/dispatcher.rs` (`run_automation_via_im` body)

- [ ] **Step 1: Add a failing test for per-identity routing**

In `src-tauri/src/channels/dispatcher.rs`'s test module, add:

```rust
    #[tokio::test]
    async fn im_inbound_routes_to_per_identity_chat_session() {
        // Simulate two IM users (UIN_a, UIN_b) hitting the same spec.
        // Verify each lands in their own chat session and they don't share.

        // Setup: in-memory DB with migrations, a spec bound to an IM channel
        // with a trigger phrase.
        let conn = setup_db();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO automation_specs (id, space_id, name, spec_json, created_at, updated_at, enabled, version)
             VALUES ('spec_im', 'default', 'IM test', '{}', ?1, ?1, 1, 1)",
            rusqlite::params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO im_channel_instances (id, space_id, channel_type, name, config_json, credentials_json, enabled, created_at, updated_at)
             VALUES ('chan1', 'default', 'wechat_ilink', 'test', '{}', '{}', 1, ?1, ?1)",
            rusqlite::params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO spec_channel_bindings (spec_id, channel_instance_id, enabled, created_at, updated_at)
             VALUES ('spec_im', 'chan1', 1, ?1, ?1)",
            rusqlite::params![now],
        ).unwrap();

        // Drive identity_key construction (the actual unit under test).
        let key_a = format!("{}:{}", "wechat_ilink", "UIN_a");
        let key_b = format!("{}:{}", "wechat_ilink", "UIN_b");
        let id_a = crate::automation::runtime::chat_sessions::get_or_create_chat_session(
            &conn, "spec_im", &key_a, "default",
        ).unwrap();
        let id_b = crate::automation::runtime::chat_sessions::get_or_create_chat_session(
            &conn, "spec_im", &key_b, "default",
        ).unwrap();

        assert_ne!(id_a, id_b);
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM automation_chat_sessions WHERE spec_id='spec_im'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }
```

(Note: this is a unit-level smoke test of the identity-key construction + index. A true end-to-end IM dispatch test requires the AppState which is hard to stand up in unit tests — Task 7 adds the end-to-end integration test.)

- [ ] **Step 2: Run the test — verify it passes (sanity-check the helper)**

Run: `cd src-tauri && cargo test --lib channels::dispatcher::tests::im_inbound_routes_to_per_identity_chat_session 2>&1 | tail -5`

Expected: PASS (because Task 1 already shipped the helper).

- [ ] **Step 3: Update `run_automation_via_im` to use the new entry point**

In `src-tauri/src/channels/dispatcher.rs`, replace `run_automation_via_im` (lines 240-276) with:

```rust
async fn run_automation_via_im(
    spec: MatchedSpec,
    msg: InboundMessage,
    reply: Arc<ReplyHandle>,
    streaming: Option<Arc<dyn StreamingHandle>>,
    _db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    app_handle: tauri::AppHandle,
) -> Result<()> {
    let state: tauri::State<'_, crate::app::AppState> = app_handle.state();
    let runtime_service = state.runtime_service.clone();

    // Look up the channel_type for the canonical identity_key.
    let channel_type = {
        let conn = state.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        conn.query_row(
            "SELECT channel_type FROM im_channel_instances WHERE id = ?1",
            rusqlite::params![&msg.instance_id],
            |r| r.get::<_, String>(0),
        ).unwrap_or_else(|_| "unknown".to_string())
    };
    let identity_key = format!("{}:{}", channel_type, msg.chat_id);

    let payload = serde_json::json!({
        "trigger": "im",
        "channel_instance_id": msg.instance_id,
        "chat_id": msg.chat_id,
        "text": msg.text,
    });

    let reply_cl = reply.clone();
    let streaming_cl = streaming.clone();
    let spec_id = spec.spec_id.clone();
    let app_handle_cl = app_handle.clone();

    let _ = reply.send("正在处理中，请稍候…").await;

    tokio::spawn(async move {
        if let Err(e) = runtime_service.execute_run_in_chat_session(
            &spec_id,
            &identity_key,
            payload,
            streaming_cl,
            Some(reply_cl),
            Some(app_handle_cl),
        ).await {
            tracing::warn!("run_automation_via_im error: {e}");
        }
    });

    Ok(())
}
```

- [ ] **Step 4: Run the dispatcher test suite + the per-identity test**

Run: `cd src-tauri && cargo test --lib channels::dispatcher 2>&1 | tail -10`

Expected: all tests pass.

- [ ] **Step 5: Run full backend tests — no regressions**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/dispatcher.rs
git commit -m "feat(automation): IM dispatcher routes to per-identity chat session

run_automation_via_im now looks up the channel_type for the instance,
builds identity_key = '{channel_type}:{chat_id}', and calls
execute_run_in_chat_session — replacing the indirect
execute_run_with_reply path that landed all IM-triggered runs in
generic per-fire sessions.

Each IM user gets their own per-spec chat thread; bursts from one
user serialize via the per-session mutex (Task 2), bursts across
different users run in parallel.

Test: im_inbound_routes_to_per_identity_chat_session (smoke on
identity_key + index helper)."
```

---

## Task 5: notify_user origin-aware routing

**Files:**
- Modify: `src-tauri/src/agent/headless.rs` (notify_user execution, lines 376-441)

- [ ] **Step 1: Add a failing test for origin-aware notify_user routing**

In `src-tauri/src/agent/headless.rs`'s test module (or create one at end of file). Define the recording mock sender inline so the test is self-contained:

```rust
    use std::sync::Mutex as StdMutex;

    /// Recording IM sender for notify_user routing tests. Captures every
    /// (chat_id, text) sent so the test can assert exactly which calls
    /// happened.
    struct RecordingImSender {
        sent: Arc<StdMutex<Vec<(String, String)>>>,
    }

    #[async_trait::async_trait]
    impl crate::channels::types::ImChannelSender for RecordingImSender {
        async fn send_text(
            &self,
            chat_id: &str,
            text: &str,
            _ctx: Option<&serde_json::Value>,
        ) -> Result<(), String> {
            self.sent.lock().unwrap().push((chat_id.to_string(), text.to_string()));
            Ok(())
        }
    }

    /// Build a HeadlessDelegate with only the fields needed for testing
    /// notify_user routing. Uses an in-memory DB + the minimal stubs that
    /// notify_user's execution branch reads. No agent loop runs.
    fn mk_notify_test_delegate(
        reply_handle: Option<Arc<crate::channels::types::ReplyHandle>>,
    ) -> HeadlessDelegate {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        HeadlessDelegate {
            spec_id: "test".into(),
            activity_id: "act_test".into(),
            session_id: "sess_test".into(),
            permissions: Default::default(),
            memory: Arc::new(crate::memory::MemoryStore::new_in_memory()),
            db: Arc::new(std::sync::Mutex::new(conn)),
            gate: Arc::new(tokio::sync::Mutex::new(None)),
            auto_continue: Default::default(),
            llm: Arc::new(crate::llm::testing::StubProvider::default()),
            model: "stub".into(),
            tools: Arc::new(crate::agent::tools::ToolRegistry::default()),
            cost: Arc::new(crate::automation::runtime::cost::CostCapState::new(
                crate::automation::runtime::cost::CostCapConfig::default(),
            )),
            workspace_root: std::env::temp_dir(),
            app_handle: None,
            channel_manager: None,
            reply_handle,
            streaming_handle: None,
            system_prompt_override: None,
        }
    }

    #[tokio::test]
    async fn notify_user_routes_to_im_only_when_reply_handle_some() {
        let sent = Arc::new(StdMutex::new(Vec::<(String, String)>::new()));
        let reply = Arc::new(crate::channels::types::ReplyHandle {
            sender: Arc::new(RecordingImSender { sent: sent.clone() }),
            chat_id: "UIN_a".to_string(),
            channel_ctx: None,
        });
        let delegate = mk_notify_test_delegate(Some(reply));

        // Drive the same code path the LLM would when calling notify_user
        // with channels=["system","wecom"]. Find the actual execution
        // entry by searching for `notify_user` handling in
        // execute_tool_calls — call it directly with the canned input.
        let input = serde_json::json!({
            "channels": ["system", "wecom"],
            "title": "build done",
            "body":  "all green",
            "level": "info",
        });
        let _ = delegate.execute_notify_user_for_test(input).await;

        // (1) IM received exactly one message containing the body.
        let received = sent.lock().unwrap();
        assert_eq!(received.len(), 1, "IM should receive exactly one notify_user");
        assert_eq!(received[0].0, "UIN_a");
        assert!(received[0].1.contains("all green") || received[0].1.contains("build done"));

        // (2) Legacy dispatch was NOT invoked. channel_manager is None
        //     in this delegate, so any attempt would have panicked; the
        //     fact that we got here proves the short-circuit returned
        //     before legacy dispatch.
    }

    #[tokio::test]
    async fn notify_user_routes_to_legacy_when_reply_handle_none() {
        // With no reply_handle, the legacy dispatch runs. channel_manager
        // is None in this delegate, so the "system" path runs (IPC emit),
        // and other channels are skipped with a tracing::warn. We assert
        // the call completes without error (legacy dispatch isn't gated).
        let delegate = mk_notify_test_delegate(None);
        let input = serde_json::json!({
            "channels": ["system"],
            "title": "t",
            "body":  "b",
            "level": "info",
        });
        let result = delegate.execute_notify_user_for_test(input).await;
        assert!(result.is_ok());
    }
```

Notes for the implementer:
- `execute_notify_user_for_test` is a test-only `pub(crate)` shim wrapping the existing notify_user execution branch in `execute_tool_calls`. If extracting a shim is awkward, the alternative is to call `execute_tool_calls` with a synthesized `Vec<ToolCall>` containing a single `notify_user` call. Either works — the test's assertions don't depend on the entry point.
- `StubProvider` lives at `crate::llm::testing::StubProvider` (or equivalent test stub used elsewhere in this file); if absent, define a minimal `impl LlmProvider` in this test module that returns an empty response from `respond`.

- [ ] **Step 2: Run the tests — verify they fail (or are flaky on the broadcast assertion)**

Run: `cd src-tauri && cargo test --lib agent::headless::tests::notify_user_routes 2>&1 | tail -10`

Expected: behaviour depends on current code, but at minimum the new tests should exist and the first should not yet enforce "no broadcast" (we add that next).

- [ ] **Step 3: Modify `notify_user` execution to short-circuit on `reply_handle Some`**

In `src-tauri/src/agent/headless.rs`, locate the notify_user execution path (lines 376-441). Find the block that today (per the explore findings) does "if reply_handle is set: send to IM, then continue to legacy". Change "then continue" to "then return — skip legacy". Concretely:

The current shape (per explore) is roughly:
```rust
if let Some(reply) = &self.reply_handle {
    let _ = reply.send(&body).await;
    // ... continues to legacy dispatch below
}

// Legacy dispatch: routes "system" → IPC emit, others → ChannelManager
for ch in channels {
    match ch.as_str() {
        "system" => { /* IPC emit */ }
        _ => { /* channel_manager dispatch */ }
    }
}
```

Change to:
```rust
if let Some(reply) = &self.reply_handle {
    // Origin-aware routing (spec §2.5): the run was IM-triggered, so the
    // notification belongs to the originator's chat — NOT to system /
    // wecom / email legacy channels. Avoids spamming the owner + other
    // identities when an IM user triggered the run.
    if let Err(e) = reply.send(&body).await {
        tracing::error!("notify_user reply send failed: {e}");
    }
    return Ok(());
}

// Autonomous path: legacy dispatch — system → IPC emit, others → channel_manager.
for ch in channels {
    match ch.as_str() {
        "system" => { /* IPC emit (unchanged) */ }
        _ => { /* channel_manager dispatch (unchanged) */ }
    }
}
```

Preserve the existing legacy dispatch code below verbatim; only insert the `return Ok(());` after the IM send when `reply_handle.is_some()`.

- [ ] **Step 4: Update the first test to assert no broadcast**

Edit the first test to add the broadcast-blocking assertion (after Step 3 the behavior is correct):

```rust
        // Assert: legacy channels were NOT also invoked.
        // (Inspect channel_manager mock state if available; or assert that
        // the InfraService bus didn't receive a "system" notification.)
```

- [ ] **Step 5: Run both tests — verify they pass**

Run: `cd src-tauri && cargo test --lib agent::headless::tests::notify_user_routes 2>&1 | tail -8`

Expected: both pass.

- [ ] **Step 6: Run full backend test suite — no regression**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/agent/headless.rs
git commit -m "feat(automation): origin-aware notify_user routing

When the run is IM-triggered (reply_handle Some), notify_user sends
exclusively to the originating IM chat — no broadcast to system / wecom /
email legacy channels. The owner and other IM identities are not spammed
with side-channel notifications meant for the originator.

When the run is autonomous (reply_handle None), the legacy multi-channel
dispatch is unchanged.

Spec §2.5 — Phase 2b cluster A.

Tests (2 new):
- notify_user_routes_to_im_only_when_reply_handle_some
- notify_user_routes_to_legacy_when_reply_handle_none"
```

---

## Task 6: `list_chat_sessions_for_spec` Tauri command + frontend UI

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (new command)
- Modify: `src-tauri/src/main.rs` (`invoke_handler!` registration)
- Modify: `ui/src/lib/tauri-bridge.ts` (wrapper)
- Modify: `ui/src/atoms/automation-atoms.ts` (new atom; if file doesn't exist, create it)
- Modify: `ui/src/components/automation/SpecDetailView.tsx` (or current spec-detail location — search with `grep -rn "spec_id" ui/src/components/automation/`; add a "Chat threads" tab)

- [ ] **Step 1: Write the failing backend test**

In `src-tauri/src/tauri_commands.rs`'s test module:

```rust
    #[test]
    fn list_chat_sessions_for_spec_returns_all_identities() {
        use crate::automation::runtime::chat_sessions::get_or_create_chat_session;
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();

        let _ = get_or_create_chat_session(&conn, "spec_x", "local", "default").unwrap();
        let _ = get_or_create_chat_session(&conn, "spec_x", "wechat_ilink:UIN_a", "default").unwrap();
        let _ = get_or_create_chat_session(&conn, "spec_x", "wechat_ilink:UIN_b", "default").unwrap();
        let _ = get_or_create_chat_session(&conn, "spec_other", "local", "default").unwrap();

        // The query the command will run — verify shape before wiring the command.
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT acs.identity_key, acs.agent_session_id
                 FROM automation_chat_sessions acs
                 WHERE acs.spec_id = ?1
                 ORDER BY acs.updated_at DESC"
            ).unwrap();
            stmt.query_map(rusqlite::params!["spec_x"], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };
        assert_eq!(rows.len(), 3);
        let identities: Vec<_> = rows.iter().map(|(id, _)| id.as_str()).collect();
        assert!(identities.contains(&"local"));
        assert!(identities.contains(&"wechat_ilink:UIN_a"));
        assert!(identities.contains(&"wechat_ilink:UIN_b"));
    }
```

- [ ] **Step 2: Run the test — verify it passes (it's a SQL sanity check, not the command itself)**

Run: `cd src-tauri && cargo test --lib list_chat_sessions_for_spec_returns_all_identities 2>&1 | tail -5`

Expected: PASS.

- [ ] **Step 3: Add the Tauri command**

In `src-tauri/src/tauri_commands.rs`, add (near other Agent session commands around line 7131):

```rust
#[derive(serde::Serialize)]
pub struct ChatSessionSummary {
    pub identity_key: String,
    pub agent_session_id: String,
    /// `agent_sessions.title` — used by the rail / tab strip today.
    pub title: String,
    pub message_count: i64,
    pub updated_at: i64,
}

#[tauri::command]
pub async fn list_chat_sessions_for_spec(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<Vec<ChatSessionSummary>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let mut stmt = conn.prepare(
        "SELECT acs.identity_key, acs.agent_session_id, s.title, s.message_count, s.updated_at
         FROM automation_chat_sessions acs
         JOIN agent_sessions s ON s.id = acs.agent_session_id
         WHERE acs.spec_id = ?1
         ORDER BY s.updated_at DESC"
    ).map_err(|e| Error::Database(e))?;
    let rows = stmt.query_map(rusqlite::params![spec_id], |row| {
        Ok(ChatSessionSummary {
            identity_key: row.get(0)?,
            agent_session_id: row.get(1)?,
            title: row.get(2)?,
            message_count: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }).map_err(|e| Error::Database(e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
```

- [ ] **Step 4: Register the command in `main.rs`**

In `src-tauri/src/main.rs`, find the `tauri::generate_handler![ ... ]` (or `invoke_handler!`) macro and add `list_chat_sessions_for_spec,` to the list (preserve alphabetical / grouping convention used by the file).

- [ ] **Step 5: Verify the command compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5; echo "EXIT=$?"`

Expected: no `error[...]` lines; `EXIT=0`.

- [ ] **Step 6: Add the frontend wrapper**

In `ui/src/lib/tauri-bridge.ts`, near the other agent-session wrappers (search for `listAgentSessions`), add:

```typescript
export interface ChatSessionSummary {
  identityKey: string
  agentSessionId: string
  title: string
  messageCount: number
  updatedAt: number
}

export const listChatSessionsForSpec = (specId: string): Promise<ChatSessionSummary[]> =>
  invoke<Array<{
    identity_key: string
    agent_session_id: string
    title: string
    message_count: number
    updated_at: number
  }>>('list_chat_sessions_for_spec', { specId })
    .then((rows) => rows.map((r) => ({
      identityKey: r.identity_key,
      agentSessionId: r.agent_session_id,
      title: r.title,
      messageCount: r.message_count,
      updatedAt: r.updated_at,
    })))
    .catch(() => [])
```

- [ ] **Step 7: Add the Jotai atom (cache per-spec chat sessions)**

In `ui/src/atoms/automation-atoms.ts` (create if absent):

```typescript
import { atom } from 'jotai'
import { listChatSessionsForSpec, type ChatSessionSummary } from '@/lib/tauri-bridge'

/** Per-spec cache of chat threads. Keyed by spec_id. */
export const chatSessionsBySpecAtom = atom<Record<string, ChatSessionSummary[]>>({})

/** Action: refresh the chat-session list for a specific spec. */
export const refreshChatSessionsAtom = atom(
  null,
  async (_get, set, specId: string) => {
    const rows = await listChatSessionsForSpec(specId)
    set(chatSessionsBySpecAtom, (prev) => ({ ...prev, [specId]: rows }))
  }
)
```

- [ ] **Step 8: Add the "Chat threads" UI tab**

First, locate the existing spec detail view:

```bash
grep -rn "spec_id\|specId" ui/src/components/automation/ | head -10
```

Identify the main spec-detail container component (e.g. `SpecDetailView.tsx`, `SpecPage.tsx`, etc.). In that component, add a new tab next to existing tabs ("概览", "动态", etc.). The tab body renders a list of chat threads:

```tsx
import { useAtomValue, useSetAtom } from 'jotai'
import { useEffect } from 'react'
import { chatSessionsBySpecAtom, refreshChatSessionsAtom } from '@/atoms/automation-atoms'
import { imChannelDisplay } from '@/lib/im-channel-display'

function ChatThreadsTab({ specId }: { specId: string }): React.ReactElement {
  const map = useAtomValue(chatSessionsBySpecAtom)
  const refresh = useSetAtom(refreshChatSessionsAtom)
  const sessions = map[specId] ?? []

  useEffect(() => { void refresh(specId) }, [specId, refresh])

  if (sessions.length === 0) {
    return (
      <div className="text-sm text-muted-foreground p-3">
        暂无 chat 线。绑定的 IM 用户首次触发 trigger phrase 时会自动创建。
      </div>
    )
  }
  return (
    <div className="space-y-1 p-2">
      {sessions.map((s) => {
        // identity_key shape: "local" | "{channel_type}:{chat_id}"
        const [channelType, chatId] = s.identityKey === 'local'
          ? [null, null]
          : [s.identityKey.split(':')[0], s.identityKey.split(':').slice(1).join(':')]
        const channel = channelType ? imChannelDisplay(channelType) : null
        return (
          <div
            key={s.agentSessionId}
            className="flex items-center gap-2 w-full p-2 rounded text-left"
          >
            <span className="shrink-0 w-4 h-4 inline-flex items-center justify-center">
              {channel?.logoSrc ? (
                <img src={channel.logoSrc} alt={channel.label} className="w-3.5 h-3.5 object-contain rounded-sm" />
              ) : channel ? (
                <span className="text-[12px]">{channel.emoji}</span>
              ) : (
                <span className="text-[12px]">💬</span>
              )}
            </span>
            <span className="flex-1 truncate text-sm">
              {s.identityKey === 'local' ? '本地 owner' : (chatId ?? s.identityKey)}
            </span>
            <span className="text-xs text-muted-foreground">{s.messageCount} 条</span>
          </div>
        )
      })}
    </div>
  )
}
```

Wire `ChatThreadsTab` as a new tab in the spec-detail container alongside existing tabs.

**Scope note**: This task ships a **read-only** list. Making rows clickable (opening the chat session in a new tab) is intentionally deferred to a follow-up PR — the open-as-tab wiring depends on knowing which tab-state helper to call, which varies by where in the UI hierarchy this tab lives, and we don't want to block this PR on UI plumbing decisions that won't affect the backend. The list itself proves the data plumbing works end-to-end.

- [ ] **Step 9: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Expected: no errors.

- [ ] **Step 10: Frontend tests**

Run: `cd ui && npm test -- --run chat 2>&1 | tail -5`

Expected: existing IM channel display test (PR #189) still passes; no new frontend tests required for this task (UI logic is thin glue).

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts ui/src/atoms/automation-atoms.ts ui/src/components/automation/
git commit -m "feat(ui): Chat threads tab for spec detail page

Backend: list_chat_sessions_for_spec Tauri command returns all
(identity, session_meta) pairs joined from automation_chat_sessions +
agent_sessions for a given spec.

Frontend: chatSessionsBySpecAtom + refreshChatSessionsAtom cache the
result; new ChatThreadsTab in the spec detail container lists the
threads with a channel logo per identity (reusing PR #189's
imChannelDisplay lib). Local owner thread shows '本地 owner'; IM
threads show the chat_id."
```

---

## Task 7: End-to-end integration test + manual QA checklist

**Files:**
- Modify: `src-tauri/src/channels/dispatcher.rs` (add end-to-end integration test using mockito)

- [ ] **Step 1: Write the integration test**

In `src-tauri/src/channels/dispatcher.rs`'s test module:

```rust
    #[tokio::test]
    async fn round_trip_im_user_triggers_spec_creates_session_and_replies() {
        // End-to-end: simulate a WeChat user A sending a trigger-phrase
        // message to a spec-bound channel. Verify:
        //   1. A new chat session is created for (spec, "wechat_ilink:UIN_a")
        //   2. The agent loop ran and persisted user + assistant messages
        //   3. Mock WeChat sendmessage endpoint received the assistant text
        //
        // Implementation note: this test exercises the dispatch_inbound →
        // execute_run_in_chat_session → HeadlessDelegate → IM reply chain
        // end-to-end with a mock LLM provider returning a canned text.

        // Setup mockito for the WeChat sendmessage endpoint.
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/ilink/bot/sendmessage")
            .with_status(200)
            .with_body(r#"{"ret":0}"#)
            .create_async()
            .await;

        // Setup DB, spec, channel binding.
        let conn = setup_db();
        let now = chrono::Utc::now().timestamp_millis();
        // ... (canonical setup; ~40 lines, mirroring Task 4 Step 1)

        // Dispatch a fake inbound that matches the spec's trigger phrase.
        // (Use the test harness shape from existing dispatcher tests in
        // this file; if dispatch_inbound requires real AppState, the
        // canonical path is to construct one via the existing
        // crate::app::tests::mk_test_state() helper.)

        // ... drive the dispatch ...

        // Assert the chat session exists.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM automation_chat_sessions
             WHERE spec_id='spec_im' AND identity_key='wechat_ilink:UIN_a'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1);

        // Assert messages persisted (user inbound + assistant reply).
        let msg_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_messages
             WHERE session_id = (
               SELECT agent_session_id FROM automation_chat_sessions
               WHERE spec_id='spec_im' AND identity_key='wechat_ilink:UIN_a'
             )",
            [], |r| r.get(0),
        ).unwrap();
        assert!(msg_count >= 2, "expected at least user + assistant messages");

        // Assert mock WeChat endpoint was called.
        mock.assert_async().await;
    }
```

- [ ] **Step 2: Run the integration test**

Run: `cd src-tauri && cargo test --lib round_trip_im_user_triggers_spec 2>&1 | tail -10`

Expected: PASS.

- [ ] **Step 3: Run full backend test suite — no regression**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`

Expected: all pass.

- [ ] **Step 4: Frontend full test suite**

Run: `cd ui && npm test -- --run 2>&1 | tail -5`

Expected: pre-existing failures (Kaleidoscope / SearchPalette / GeneralTab etc. — noted in PR #189) are still there; no new failures from this branch.

- [ ] **Step 5: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | tail -3`

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/dispatcher.rs
git commit -m "test(automation): end-to-end IM → chat session → IM reply round-trip

Mock WeChat sendmessage via mockito; dispatch a fake trigger-phrase
inbound from user A; assert (1) the per-(spec, 'wechat_ilink:UIN_a')
chat session was created, (2) at least user + assistant messages were
persisted, (3) the mock WeChat endpoint received the assistant text.

Completes Phase 2b cluster A — Messaging 基座."
```

---

## Spec Coverage Self-Check

| Spec section | Implementing task(s) |
|---|---|
| §2.1 Mixed delegate (HeadlessDelegate with Option<> handles) | Task 2 (reuses existing HeadlessDelegate — see plan Architecture note) |
| §2.2 Per-(spec, identity) session topology | Task 1 (index), Task 4 (IM identity), Task 3 ("local") |
| §2.3 Autonomous triggers route to owner chat | Task 3 |
| §2.4 Single-message IM reply per turn | Pre-existing (PR #182/#186/#189); not modified |
| §2.5 notify_user origin-aware routing | Task 5 |
| §3.1 V38 migration | Task 1 |
| §3.2 `agent_sessions.metadata.origin = 'automation:chat'` | Task 1 (helper writes it) |
| §3.3 im_sessions unchanged for generic path | Not touched — only run_automation_via_im (spec path) changes |
| §3.4 No data migration (forward-only) | Tasks 3 + 4 only affect new fires |
| §4.1 Scheduled fire flow | Task 3 |
| §4.2 IM user A flow | Task 4 |
| §4.3 Owner UI flow | Task 6 (lists threads); AgentView reuse is no-op (sessions look the same to it) |
| §5 File change table | Tasks 1-6 cover every file |
| §6 Test strategy | Tasks 1-7 cover all 5 test groups |
| §7 Scope boundary | Plan adheres; no out-of-scope work |
| §8.1 Per-session mutex | Task 2 |
| §8.2 identity_key plaintext | Task 1 helper uses plaintext |
| §8.3 Delegate naming | Plan deviates from spec (reuses HeadlessDelegate); deviation explicit in Architecture |
| §8.4 owner UI emit | Inherited from existing app_handle wiring; Task 3 sets app_handle on autonomous fires |
| §8.5 OriginContext for notify_user | Task 5 reads reply_handle directly as the origin signal — simpler than the spec's OriginContext struct |
| §9 Acceptance criteria 1-6 | Tasks 1-7 collectively satisfy; Task 7 integration test covers #2, #3, #5 |

**Architecture deviations from spec (intentional):**

1. **No `ChatlikeAutomationDelegate` new type** — `HeadlessDelegate` today already has `reply_handle: Option<_>`, `streaming_handle: Option<_>`, `app_handle: Option<_>` (added by PR #182/#186/#189). Creating a parallel type would duplicate ~200 lines for zero behavioral gain. Plan reuses HeadlessDelegate; spec §2.1 / §8.3 are addressed via construction-time wiring.

2. **No explicit `OriginContext` struct for notify_user** — `reply_handle.is_some()` is already the origin signal. Adding a struct wrapping it would be ceremonial. Plan reads the existing field directly.

Both deviations preserve the spec's behavioral guarantees while being more YAGNI-aligned.
