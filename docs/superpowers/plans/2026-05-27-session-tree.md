# session_tree(功能 fork/回溯 + 树谱系)实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 接活 stub 的 `fork_agent_session`/`rewind_session` 并引入 Pi 形态 `session_tree` 谱系 DB 层(V55)+ 存取 API,LLM 读/写主路径不动。

**Architecture:** V55 建 `session_tree`+`session_leaves`;`agent/session_tree.rs` 纯函数存取 API(lazy-materialize 从 `agent_messages` 构树);fork=复制到点+新会话+fork 边,rewind=截断到点+重建树+移 leaf;命令薄包装 + 并发 guard。

**Tech Stack:** Rust + rusqlite,Tauri,serde_json。

**Spec:** `docs/superpowers/specs/2026-05-27-session-tree-design.md`
**Branch/worktree:** `codex/sprint3-session-tree`(base = main `8203f909`)

---

## ⚠️ 规划期事实(实现以此为准)

- `migrations.rs`:`pub fn run(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error>`(:2306);每 V 是 `pub const Vxx: &str`,`run()` 内 `for stmt in Vxx.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) { if let Err(e)=conn.execute(stmt,[]) { tracing::warn!("Vxx stmt skipped: {} :: {}", e, stmt); } }`;末尾 `tracing::info!("Database migrations complete"); Ok(())`(V55 loop 插其前,:2728)。最新 V54。迁移测试范式(:2774)`v54_persona_events_table_is_created_and_idempotent`:`Connection::open_in_memory()` → `super::run` 跑两次 → query `sqlite_master` 断言表存在。
- `agent_messages` 全列(V8+V9+V15+V29):`id TEXT PK(app uuid), session_id, role, content, created_at INTEGER, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd REAL, compacted INTEGER NOT NULL DEFAULT 0`。
- `agent_sessions` 全列:`id PK, space_id NOT NULL DEFAULT 'default', title NOT NULL DEFAULT 'New session', metadata_json NOT NULL DEFAULT '{}', message_count NOT NULL DEFAULT 0, pinned, archived, created_at, updated_at, attached_dirs NOT NULL DEFAULT '[]', pinned_at NULL, archived_at`。建会话 INSERT 范式(`tauri_commands.rs:9783`):`INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) VALUES (?1,?2,?3,?4,0,0,0,?5,?5)`;`created_at/updated_at = chrono::Utc::now().timestamp_millis()`。
- stub(`tauri_commands.rs:11895-11909`):`pub async fn fork_agent_session(_state: State<'_, AppState>, _input: serde_json::Value) -> Result<serde_json::Value, Error>` + 同形 `rewind_session`。`Error`(`error.rs:36-81`):`Database(#[from] rusqlite::Error)`、`NotFound(String)`、`InvalidInput(String)`、`Internal(String)`。`AppState.db: Arc<std::sync::Mutex<rusqlite::Connection>>`(app.rs:168);锁:`let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;`。`state.is_session_running(&id).await`(app.rs:1528,async)。
- 输入是 `serde_json::Value`,**camelCase**(前端发 `{sessionId, upToMessageUuid}` / `{sessionId, assistantMessageUuid}`,Tauri 对 raw Value **不转 snake_case**)。读 `input.get("sessionId").and_then(|v| v.as_str())`。
- 前端(`AgentView.tsx:1490-1556`):`handleFork` 读 `meta.id`+`meta.title`,把整个 `meta` push 进 `agentSessionsAtom`(应含 `id,title,workspaceId,messageCount,createdAt,updatedAt,pinned,archived`);`handleRewindConfirm` 读 `result.fileRewind?.canRewind` / `.error` / `.filesChanged`。`tauri-bridge.ts:1657-1661` 输入形状同上。
- 单测范式:`fn setup_db() -> Connection { let conn = Connection::open_in_memory().expect(..); crate::db::migrations::run(&conn).expect(..); conn }`(`tauri_commands.rs:16085` / `persona/store.rs:660`)。
- `agent/mod.rs`:`pub mod ...;` 列表(:1-67);在 `pub mod session;` 后加 `pub mod session_tree;`。

---

## 验证命令
- 编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)
- 单测:同目录 `cargo test --lib <filter> 2>&1 | tail -15`
- 已知预存失败 ~7(daemon_approval / truncate_for_error / browser provider_execution×3 / runtime_pack_ipc / gbrain_eval_harness);stale tauri 产物报旧路径则清 `target/debug/build/tauri-*`+`uclaw-*`。

---

## File Structure
| 文件 | 责任 |
|---|---|
| `src-tauri/src/db/migrations.rs` | `V55_SESSION_TREE` const + `run()` 接入 + 迁移测试 |
| `CONTEXT.md` | Active migration registry 加 V55 行(同 PR)|
| `src-tauri/src/agent/session_tree.rs` | **新建** 类型 + 存取 API + 单测;`agent/mod.rs` 加 `pub mod session_tree;` |
| `src-tauri/src/tauri_commands.rs` | `fork_agent_session`/`rewind_session` 接通 |

---

## Task 1: V55 迁移 + CONTEXT 登记

**Files:** Modify `src-tauri/src/db/migrations.rs`, `CONTEXT.md`

- [ ] **Step 1: const** — `migrations.rs`(near `V54_PERSONA_EVENTS`,~:2090 后)加:

```rust
pub const V55_SESSION_TREE: &str = "
CREATE TABLE IF NOT EXISTS session_tree (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    parent_id   TEXT,
    entry_type  TEXT NOT NULL,
    data_json   TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_session_tree_session ON session_tree(session_id);
CREATE INDEX IF NOT EXISTS idx_session_tree_parent ON session_tree(parent_id);

CREATE TABLE IF NOT EXISTS session_leaves (
    session_id  TEXT PRIMARY KEY,
    leaf_id     TEXT,
    updated_at  INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
";
```

- [ ] **Step 2: run() 接入** — 在 `tracing::info!("Database migrations complete");`(:2728)**之前**加(镜像 V54 loop):

```rust
    // V55: session_tree + session_leaves — fork/rewind lineage (Sprint 3 ③).
    tracing::debug!("Running migration V55: session tree");
    for stmt in V55_SESSION_TREE.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V55 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 3: 迁移测试** — `migrations.rs` 测试模块(near v54 test)加:

```rust
#[test]
fn v55_session_tree_tables_created_and_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    super::run(&conn).expect("first run");
    super::run(&conn).expect("second run must not error");
    let tables: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('session_tree','session_leaves')",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(tables, 2);
    let idx: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('idx_session_tree_session','idx_session_tree_parent')",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(idx, 2);
}
```

- [ ] **Step 4: CONTEXT.md 登记** — 在 Active migration registry 表(V54 行后)加:
```
| V55 | session_tree + session_leaves — fork/rewind 谱系 | (this PR) |
```

- [ ] **Step 5: 测试 + 提交** — `cargo test --lib v55_session_tree 2>&1 | tail -6`(1 passed);`cargo build 2>&1 | grep -E "^error"`(空)。

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree add src-tauri/src/db/migrations.rs CONTEXT.md
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree commit -m "feat(db): V55 session_tree + session_leaves migration

New tables for fork/rewind lineage (Pi session_tree shape). Idempotent
(IF NOT EXISTS). Registered V55 in CONTEXT.md Active migration registry.

Verification: cargo test --lib v55_session_tree -> 1 passed; build clean"
```

---

## Task 2: session_tree.rs — 类型 + materialize/get_path_to_root/leaf + 单测

**Files:** Create `src-tauri/src/agent/session_tree.rs`; Modify `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: 模块 + 类型 + 基础 API** — 新建 `agent/session_tree.rs`:

```rust
//! session_tree —— fork/rewind 谱系存取(Sprint 3 ③)。
//! lazy-materialize:agent_messages 是 source of truth;树按需从它构建。
//! 纯函数 over &rusqlite::Connection。读/写主路径不接管(getPathToRoot 备而不用)。
use crate::error::Error;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TreeNode {
    pub id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub entry_type: String,
    pub data_json: String,
    pub created_at: i64,
}

fn now_ms() -> i64 { chrono::Utc::now().timestamp_millis() }

/// 追加一个节点,返回其 id。created_at 显式传入(materialize 用消息 created_at,以便按时间剪枝/排序)。
pub fn append_node(
    conn: &Connection,
    session_id: &str,
    parent_id: Option<&str>,
    entry_type: &str,
    data_json: &str,
    created_at: i64,
) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO session_tree (id, session_id, parent_id, entry_type, data_json, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![id, session_id, parent_id, entry_type, data_json, created_at],
    )?;
    Ok(id)
}

pub fn get_leaf(conn: &Connection, session_id: &str) -> Result<Option<String>, Error> {
    let leaf: Option<String> = conn
        .query_row("SELECT leaf_id FROM session_leaves WHERE session_id = ?1", params![session_id], |r| r.get(0))
        .ok()
        .flatten();
    Ok(leaf)
}

pub fn set_leaf(conn: &Connection, session_id: &str, node_id: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO session_leaves (session_id, leaf_id, updated_at) VALUES (?1,?2,?3)
         ON CONFLICT(session_id) DO UPDATE SET leaf_id = excluded.leaf_id, updated_at = excluded.updated_at",
        params![session_id, node_id, now_ms()],
    )?;
    Ok(())
}

/// 该 session 是否已有树行。
fn has_tree(conn: &Connection, session_id: &str) -> Result<bool, Error> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_tree WHERE session_id = ?1", params![session_id], |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 幂等:若无树行,从 agent_messages 构线性 message-node 链(created_at 序,parent=前一节点),
/// 节点 created_at = 消息 created_at;set_leaf 到末节点。
pub fn materialize_session_tree(conn: &Connection, session_id: &str) -> Result<(), Error> {
    if has_tree(conn, session_id)? {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "SELECT id, role, created_at FROM agent_messages WHERE session_id = ?1 ORDER BY created_at ASC, rowid ASC",
    )?;
    let rows: Vec<(String, String, i64)> = stmt
        .query_map(params![session_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
        .collect::<Result<_, _>>()?;
    let mut parent: Option<String> = None;
    let mut last: Option<String> = None;
    for (msg_id, role, created_at) in rows {
        let data = serde_json::json!({ "message_id": msg_id, "role": role }).to_string();
        let node = append_node(conn, session_id, parent.as_deref(), "message", &data, created_at)?;
        parent = Some(node.clone());
        last = Some(node);
    }
    if let Some(leaf) = last {
        set_leaf(conn, session_id, &leaf)?;
    }
    Ok(())
}

/// 从 leaf 沿 parent_id 递归走到 root,返回 root→leaf(created_at 序)。
/// 本 slice 建好 + 单测;读路径暂不接管。
pub fn get_path_to_root(conn: &Connection, leaf_id: &str) -> Result<Vec<TreeNode>, Error> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE path(id, session_id, parent_id, entry_type, data_json, created_at) AS (
            SELECT id, session_id, parent_id, entry_type, data_json, created_at FROM session_tree WHERE id = ?1
            UNION ALL
            SELECT t.id, t.session_id, t.parent_id, t.entry_type, t.data_json, t.created_at
              FROM session_tree t JOIN path p ON t.id = p.parent_id
        )
        SELECT id, session_id, parent_id, entry_type, data_json, created_at FROM path ORDER BY created_at ASC, rowid ASC",
    )?;
    let nodes = stmt
        .query_map(params![leaf_id], |r| Ok(TreeNode {
            id: r.get(0)?, session_id: r.get(1)?, parent_id: r.get(2)?,
            entry_type: r.get(3)?, data_json: r.get(4)?, created_at: r.get(5)?,
        }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(nodes)
}

/// 找某 session 中对应 message_id 的节点 id。
pub(crate) fn node_for_message(conn: &Connection, session_id: &str, message_id: &str) -> Result<Option<String>, Error> {
    let id: Option<String> = conn.query_row(
        "SELECT id FROM session_tree WHERE session_id = ?1 AND entry_type = 'message' AND json_extract(data_json, '$.message_id') = ?2",
        params![session_id, message_id], |r| r.get(0),
    ).ok();
    Ok(id)
}
```

- [ ] **Step 2: 注册模块** — `agent/mod.rs` 在 `pub mod session;` 后加 `pub mod session_tree;`。

- [ ] **Step 3: 单测** — `session_tree.rs` 底部:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        crate::db::migrations::run(&conn).expect("run migrations");
        conn
    }

    /// 建一个 session + n 条 user/assistant 交替消息(created_at = 1000,1001,...)。返回 message ids(序)。
    fn seed_session(conn: &Connection, session_id: &str, n: usize) -> Vec<String> {
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) VALUES (?1,'default','S','{}',?2,0,0,1000,1000)",
            params![session_id, n as i64],
        ).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let mid = format!("m{i}");
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,?2,?3,?4,?5,0)",
                params![mid, session_id, role, format!("c{i}"), 1000 + i as i64],
            ).unwrap();
            ids.push(mid);
        }
        ids
    }

    #[test]
    fn materialize_builds_linear_chain_and_is_idempotent() {
        let conn = setup_db();
        seed_session(&conn, "s1", 3);
        materialize_session_tree(&conn, "s1").unwrap();
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM session_tree WHERE session_id='s1'", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 3);
        // 幂等:再调不增行
        materialize_session_tree(&conn, "s1").unwrap();
        let cnt2: i64 = conn.query_row("SELECT COUNT(*) FROM session_tree WHERE session_id='s1'", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt2, 3);
        // root 有一个(parent_id NULL)
        let roots: i64 = conn.query_row("SELECT COUNT(*) FROM session_tree WHERE session_id='s1' AND parent_id IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(roots, 1);
    }

    #[test]
    fn get_path_to_root_returns_root_to_leaf() {
        let conn = setup_db();
        seed_session(&conn, "s1", 3);
        materialize_session_tree(&conn, "s1").unwrap();
        let leaf = get_leaf(&conn, "s1").unwrap().unwrap();
        let path = get_path_to_root(&conn, &leaf).unwrap();
        assert_eq!(path.len(), 3);
        // root→leaf:第一个 parent 为 None
        assert!(path[0].parent_id.is_none());
        assert_eq!(path[2].id, leaf);
    }

    #[test]
    fn leaf_round_trips() {
        let conn = setup_db();
        seed_session(&conn, "s1", 1);
        materialize_session_tree(&conn, "s1").unwrap();
        let leaf = get_leaf(&conn, "s1").unwrap().unwrap();
        set_leaf(&conn, "s1", &leaf).unwrap();
        assert_eq!(get_leaf(&conn, "s1").unwrap().unwrap(), leaf);
    }
}
```

- [ ] **Step 4: 测试 + 提交** — `cargo test --lib session_tree:: 2>&1 | tail -8`(3 passed);`cargo build 2>&1 | grep -E "^error"`(空)。

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree add src-tauri/src/agent/session_tree.rs src-tauri/src/agent/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree commit -m "feat(agent): session_tree storage API (materialize/get_path_to_root/leaf)

New agent/session_tree.rs: TreeNode + append_node/materialize_session_tree/
get_path_to_root (recursive CTE)/get_leaf/set_leaf/node_for_message. Lazy
materialize from agent_messages; read path untouched. + unit tests.

Verification: cargo test --lib session_tree:: -> 3 passed; build clean"
```

---

## Task 3: fork_at + rewind_to + 单测

**Files:** Modify `src-tauri/src/agent/session_tree.rs`

- [ ] **Step 1: 结果类型 + fork_at + rewind_to** — 在 `session_tree.rs`(测试模块之前)加:

```rust
/// fork 返回:新会话 meta(前端 push 进 agentSessionsAtom)。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ForkResult {
    pub id: String,
    pub title: String,
    pub message_count: i64,
}

/// rewind 返回:文件态 rewind 本 slice 恒不可用。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RewindResult {
    pub deleted: i64,
}

/// fork:把 source 中 created_at <= up_to_message 的消息复制进新会话,记录 fork 边。
pub fn fork_at(conn: &Connection, source_session: &str, up_to_message_id: &str) -> Result<ForkResult, Error> {
    materialize_session_tree(conn, source_session)?;

    // fork 点消息的 created_at(校验存在)
    let fork_ts: i64 = conn.query_row(
        "SELECT created_at FROM agent_messages WHERE id = ?1 AND session_id = ?2",
        params![up_to_message_id, source_session],
        |r| r.get(0),
    ).map_err(|_| Error::NotFound(format!("message {up_to_message_id} not in session {source_session}")))?;

    // 新会话(复制 title/space/metadata/attached_dirs)
    let (space_id, src_title, metadata_json, attached_dirs): (String, String, String, String) = conn.query_row(
        "SELECT space_id, title, metadata_json, attached_dirs FROM agent_sessions WHERE id = ?1",
        params![source_session],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).map_err(|_| Error::NotFound(format!("session {source_session}")))?;
    let new_id = uuid::Uuid::new_v4().to_string();
    let new_title = format!("{src_title} (fork)");
    let now = now_ms();

    // 复制消息(created_at <= fork_ts),新 uuid
    let mut stmt = conn.prepare(
        "SELECT role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted \
         FROM agent_messages WHERE session_id = ?1 AND created_at <= ?2 ORDER BY created_at ASC, rowid ASC",
    )?;
    #[allow(clippy::type_complexity)]
    let copied: Vec<(String, String, i64, Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<i64>, Option<f64>, i64)> =
        stmt.query_map(params![source_session, fork_ts], |r| Ok((
            r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?,
        )))?.collect::<Result<_, _>>()?;

    conn.execute(
        "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at, attached_dirs) VALUES (?1,?2,?3,?4,?5,0,0,?6,?6,?7)",
        params![new_id, space_id, new_title, metadata_json, copied.len() as i64, now, attached_dirs],
    )?;
    for (role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted) in &copied {
        let nid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![nid, new_id, role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted],
        )?;
    }

    // 物化新会话树 + 记录 fork 边(新会话 root.parent_id → source 中 fork 点节点)
    materialize_session_tree(conn, &new_id)?;
    if let Some(src_node) = node_for_message(conn, source_session, up_to_message_id)? {
        conn.execute(
            "UPDATE session_tree SET parent_id = ?1 WHERE session_id = ?2 AND parent_id IS NULL",
            params![src_node, new_id],
        )?;
    }

    Ok(ForkResult { id: new_id, title: new_title, message_count: copied.len() as i64 })
}

/// rewind:删 target 之后的 agent_messages(保留含 target),重建本会话树,移 leaf。
pub fn rewind_to(conn: &Connection, session_id: &str, target_message_id: &str) -> Result<RewindResult, Error> {
    let target_ts: i64 = conn.query_row(
        "SELECT created_at FROM agent_messages WHERE id = ?1 AND session_id = ?2",
        params![target_message_id, session_id],
        |r| r.get(0),
    ).map_err(|_| Error::NotFound(format!("message {target_message_id} not in session {session_id}")))?;

    let deleted = conn.execute(
        "DELETE FROM agent_messages WHERE session_id = ?1 AND created_at > ?2",
        params![session_id, target_ts],
    )? as i64;

    let remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1", params![session_id], |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE agent_sessions SET message_count = ?1, updated_at = ?2 WHERE id = ?3",
        params![remaining, now_ms(), session_id],
    )?;

    // 重建树(删本会话树行 + 重新物化 → 反映截断后的 agent_messages),leaf 落到新末节点。
    conn.execute("DELETE FROM session_tree WHERE session_id = ?1", params![session_id])?;
    conn.execute("DELETE FROM session_leaves WHERE session_id = ?1", params![session_id])?;
    materialize_session_tree(conn, session_id)?;

    Ok(RewindResult { deleted })
}
```

> tie-break:`created_at <=`(fork 含等于)、`created_at >`(rewind 删严格之后,保留等于 target)。同 ms 多条以 `rowid` 兜底排序(materialize/查询已含 `rowid ASC`);极端同-ms 的精确边界以 `created_at` 为准并文档化。重建树会丢被 rewind 会话 root 上的 fork 边(若它本身是 fork 出来的)—— 本 slice 可接受(文档化;Pi 树作读源后再精修)。

- [ ] **Step 2: 单测** — 在 `tests` mod 加:

```rust
    #[test]
    fn fork_at_copies_messages_and_records_edge() {
        let conn = setup_db();
        let ids = seed_session(&conn, "s1", 4);   // m0..m3
        let res = fork_at(&conn, "s1", &ids[1]).unwrap();   // fork up to m1
        assert_eq!(res.message_count, 2);   // m0,m1
        assert!(res.title.ends_with("(fork)"));
        // 新会话有 2 条消息
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1", params![res.id], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        // 新会话 root 的 parent 指向 source 节点(fork 边)
        let root_parent: Option<String> = conn.query_row(
            "SELECT parent_id FROM session_tree WHERE session_id = ?1 AND parent_id IS NOT NULL ORDER BY created_at ASC LIMIT 1",
            params![res.id], |r| r.get(0)).optional().unwrap().flatten();
        assert!(root_parent.is_some());
        // source 仍 4 条(未动)
        let src_n: i64 = conn.query_row("SELECT COUNT(*) FROM agent_messages WHERE session_id='s1'", [], |r| r.get(0)).unwrap();
        assert_eq!(src_n, 4);
    }

    #[test]
    fn rewind_to_truncates_after_target() {
        let conn = setup_db();
        let ids = seed_session(&conn, "s1", 4);   // m0..m3
        let res = rewind_to(&conn, "s1", &ids[1]).unwrap();   // keep m0,m1; delete m2,m3
        assert_eq!(res.deleted, 2);
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM agent_messages WHERE session_id='s1'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        // message_count 同步
        let mc: i64 = conn.query_row("SELECT message_count FROM agent_sessions WHERE id='s1'", [], |r| r.get(0)).unwrap();
        assert_eq!(mc, 2);
        // 树重建为 2 节点,leaf = m1 对应节点
        let tn: i64 = conn.query_row("SELECT COUNT(*) FROM session_tree WHERE session_id='s1'", [], |r| r.get(0)).unwrap();
        assert_eq!(tn, 2);
    }

    #[test]
    fn fork_unknown_message_errors() {
        let conn = setup_db();
        seed_session(&conn, "s1", 2);
        assert!(matches!(fork_at(&conn, "s1", "nope"), Err(Error::NotFound(_))));
    }
```

> 需 `use rusqlite::OptionalExtension;`(`.optional()`)—— 在测试模块或文件顶 import。

- [ ] **Step 3: 测试 + 提交** — `cargo test --lib session_tree:: 2>&1 | tail -10`(6 passed);`cargo build 2>&1 | grep -E "^error"`(空)。

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree add src-tauri/src/agent/session_tree.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree commit -m "feat(agent): session_tree fork_at + rewind_to

fork_at copies agent_messages up to a point into a new session + records the
fork edge (new root parent -> source node). rewind_to truncates after a point +
rebuilds the tree + updates message_count. + unit tests.

Verification: cargo test --lib session_tree:: -> 6 passed; build clean"
```

---

## Task 4: 接通 fork_agent_session / rewind_session 命令

**Files:** Modify `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: 替换 stub** — `tauri_commands.rs:11895-11909` 两个 stub 替换为:

```rust
#[tauri::command]
pub async fn fork_agent_session(
    state: State<'_, AppState>,
    input: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let session_id = input.get("sessionId").and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidInput("sessionId required".into()))?.to_string();
    let up_to = input.get("upToMessageUuid").and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidInput("upToMessageUuid required".into()))?.to_string();

    if state.is_session_running(&session_id).await {
        return Err(Error::InvalidInput("先停止 agent 再 fork".into()));
    }

    let res = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        crate::agent::session_tree::fork_at(&conn, &session_id, &up_to)?
    };
    // 前端 push 进 agentSessionsAtom,需完整 meta(camelCase)
    Ok(serde_json::json!({
        "id": res.id,
        "title": res.title,
        "workspaceId": serde_json::Value::Null,
        "messageCount": res.message_count,
        "createdAt": chrono::Utc::now().timestamp_millis(),
        "updatedAt": chrono::Utc::now().timestamp_millis(),
        "pinned": false,
        "archived": false,
    }))
}

#[tauri::command]
pub async fn rewind_session(
    state: State<'_, AppState>,
    input: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let session_id = input.get("sessionId").and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidInput("sessionId required".into()))?.to_string();
    let target = input.get("assistantMessageUuid").and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidInput("assistantMessageUuid required".into()))?.to_string();

    if state.is_session_running(&session_id).await {
        return Err(Error::InvalidInput("先停止 agent 再回溯".into()));
    }

    let res = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        crate::agent::session_tree::rewind_to(&conn, &session_id, &target)?
    };
    // 文件态 rewind 范围外 → canRewind:false
    Ok(serde_json::json!({
        "deleted": res.deleted,
        "fileRewind": { "canRewind": false, "error": "file-state rewind not supported in this slice" },
    }))
}
```

> `workspaceId` 前端 AgentSessionMeta 可能映射自 space_id;本 slice 返回 Null(前端 sidebar 渲染不依赖它做关键逻辑;若发现需要,改取 source 的 space_id)。实现者读 `AgentSessionMeta` TS 类型确认必填字段,补齐(以编译/渲染不报错为准)。

- [ ] **Step 2: 编译** — `cargo build 2>&1 | grep -E "^error" | head`(空)。命令注册已在 main.rs(stub 时就有),无需改。

- [ ] **Step 3: 提交**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree add src-tauri/src/tauri_commands.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree commit -m "feat(api): wire fork_agent_session / rewind_session to session_tree

Both commands: parse camelCase input, reject if session is running, lock db,
call session_tree::fork_at/rewind_to, return frontend-expected shapes
(fork -> AgentSessionMeta; rewind -> { fileRewind: { canRewind: false } }).

Verification: cargo build -> no errors"
```

---

## Task 5: 验收收口

- [ ] **Step 1: 全量验证** — `cargo build`(空 error);`cargo test --lib "session_tree::" "v55_session_tree" 2>&1 | tail -12`(全过);全量 `cargo test --lib 2>&1 | tail -6`(仅 ~7 已知预存失败,零新增)。
- [ ] **Step 2: 手动 smoke 提示(写入 PR)** — `cargo tauri dev`:对某条消息触发 fork → sidebar 出现「… (fork)」会话、其消息为 fork 点及之前;对某条 assistant 消息 rewind → 其后消息消失、message_count 更新。
- [ ] **Step 3: Commit(若有收口改动)**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint3-session-tree commit -m "test(session-tree): acceptance verification

Verification: cargo test --lib -> no new failures (7 known pre-existing)"
```

---

## 最终验收
- [ ] `cargo build` → 空 error
- [ ] `v55_session_tree` 迁移测试 + `session_tree::`(6)全过
- [ ] 全量 `cargo test --lib` → 仅 ~7 已知预存失败
- [ ] CONTEXT.md 已登记 V55
- [ ] fork/rewind 命令返回前端期待形状;读/写 agent 主路径零改动
- [ ] 手动 smoke:fork 出分叉会话 / rewind 截断生效

---

## Self-Review
**Spec coverage:** §3 V55 schema + CONTEXT 登记 → Task1;§4 存取 API(materialize/get_path_to_root/leaf)→ Task2、fork_at/rewind_to → Task3、命令接通 + 返回形状 + 并发 guard → Task4;§5 错误/并发/行为保持 → Task3(NotFound)/Task4(is_session_running guard);§6 测试 → Task1(迁移)/Task2/Task3/Task5;§7 commit 序列 → Task1-5 顺序一致。lazy-materialize(读写主路径不动)贯穿 Task2-4。

**Placeholder scan:** Task4 的 `workspaceId` 备注是「读 AgentSessionMeta 确认必填、补齐」的明确指令 + 默认 Null,非 TBD。tie-break 备注是文档化的边界决策。无 TODO/TBD;每步含完整代码。

**Type consistency:** `TreeNode{id,session_id,parent_id,entry_type,data_json,created_at}`(Task2)→ get_path_to_root 返回一致;`append_node(conn,session_id,parent_id,entry_type,data_json,created_at)`(Task2)→ materialize 调用一致(6 参,created_at 显式);`materialize_session_tree`/`get_leaf`/`set_leaf`/`node_for_message`(Task2)→ Task3 fork_at/rewind_to 调用一致;`ForkResult{id,title,message_count}` / `RewindResult{deleted}`(Task3)→ Task4 序列化字段(id/title/messageCount;deleted/fileRewind)一致;命令名 `fork_agent_session`/`rewind_session`(Task4)↔ 现有注册一致;输入键 `sessionId`/`upToMessageUuid`/`assistantMessageUuid`(Task4)↔ tauri-bridge 一致。
