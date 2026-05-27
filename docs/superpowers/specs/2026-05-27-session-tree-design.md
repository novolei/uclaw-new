# session_tree(功能 fork/回溯 + 树谱系)设计 (Sprint 3 ③)

**状态:** 设计已逐节批准,待 spec 评审 → writing-plans
**分支/worktree:** `codex/sprint3-session-tree`(base = main `8203f909`)
**前置:** Pi convergence。ADR §20(`docs/adr/2026-05-20-…north-star.md:1536` — Session Persistence:`session_tree` + `getPathToRoot` + `CompactionEntry`)、Pi 升级设计 §6(`…2026-05-26-agent-framework-pi-upgrade-design.md:1620-1744`)。

---

## 1. 目标与范围

**目标:** 把已接线但 stub 的 `fork_agent_session` / `rewind_session` 做成**真可用**,并引入 Pi 形态的 `session_tree` 谱系 DB 层 + 存取 API。**LLM 读路径仍走 `agent_messages`(不动,低风险)**;树作谱系/分支记录,按需从 `agent_messages` 物化(lazy-materialize,**零热路径触碰**)。

**范围内:**
- **V55 迁移**:`session_tree` + `session_leaves` 两表(+ CONTEXT.md Active migration registry 同 PR 登记 V55)。
- **`agent/session_tree.rs`** 存取 API:`materialize_session_tree`、`append_node`、`get_path_to_root`、`get_leaf`/`set_leaf`、`fork_at`、`rewind_to` + `TreeNode`/`SessionTreeEntry` 类型 + 单测(in-memory SQLite)。
- **接通 stub 命令**:`fork_agent_session`(复制 agent_messages 到点 + 新会话 + 记录 fork 边)、`rewind_session`(截断到点 + 移 leaf + 剪枝)—— 返回形状匹配已接线前端。
- **并发 guard**:run 中拒绝 fork/rewind。

**明确范围外(留后续):**
- 切 LLM 读路径到 `getPathToRoot`(`get_path_to_root` 本 slice 建好 + 单测,但不接管 send_agent_message 的读)。
- 消息持久化 dual-write 进树(本 slice 用 lazy-materialize)。
- compaction-as-node(`CompactionEntry`)— `agent_fold_baselines`(V52)/`compaction_markers`(V29)/summary placeholder 不动。
- 文件态 rewind(`fileRewind.canRewind` 本 slice 恒 false)。
- 分支导航 UI(切换 active leaf 的可视化)。

---

## 2. 现状锚点(实现以此为准)

- `agent_messages`(`migrations.rs:339`,扩展 V9/V15/V29):`id PK, session_id, role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted`。**无 parent_id/seq/branch**;顺序 = `created_at ASC`;index `idx_agent_messages_session(session_id)`。FK `session_id → agent_sessions(id) ON DELETE CASCADE`。
- `agent_sessions`(`migrations.rs:326`,V8 + V17/V18/V24):`id PK, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at, attached_dirs, pinned_at, archived_at`。**无 parent/forked_from**。
- `fork_agent_session` / `rewind_session`(`tauri_commands.rs:11895-11908`):均 `Err(InvalidInput("not yet implemented"))` stub,已注册 main.rs:1329-1330,已接前端。
- 前端(`AgentView.tsx:1490-1555`):`handleFork(upToMessageUuid)` 调 `forkAgentSession({sessionId, upToMessageUuid})`,期待返回含 `meta.id`+`meta.title`,加入 sidebar + 开新 tab + toast「已创建分叉会话」;`handleRewindConfirm()` 调 `rewindSession({sessionId, assistantMessageUuid})`,读 `result.fileRewind.canRewind` 决定 toast,刷新消息列表。`tauri-bridge.ts:1657-1661` 的 IPC 签名同上。
- LLM 读(不动):`SELECT role, content FROM agent_messages WHERE session_id=?1 AND compacted=0 ORDER BY created_at ASC`(`tauri_commands.rs:10558,9877`);UI 显示读(`:11534`,载全部含 compacted)。写 INSERT 5-7 处(`:10518` user、`:11425` assistant、`:10406` compaction placeholder、`dispatcher.rs:2885` steering 等)—— **本 slice 全不动**。
- 迁移机制(`migrations.rs:2306` `pub fn run(conn)`):无 version 表;每启动跑全部 V;幂等靠 `IF NOT EXISTS` + ALTER 吞错。每 V 是 `pub const Vxx_NAME: &str` SQL,`run()` 内 `execute_batch`/split-loop。**最新 = V54**(`V53_LIVING_PERSONA`、`V54_PERSONA_EVENTS`);**V55 free**(CONTEXT.md registry 与源码均无 V55+;V36 是死槽)。
- `is_session_running(session_id)`(Sprint 2,`AppState`,async)用于并发 guard。
- Pi 形态(spec §6.3):`session_tree(id, session_id, parent_id, entry_type, data, created_at)` + `session_leaves(session_id, leaf_id)`;`getPathToRoot` = 从 leaf 沿 parent_id 走到 root(递归 CTE);`CompactionEntry` = entry_type='compaction' 的节点(本 slice 不做)。

---

## 3. Schema(V55)+ 物化策略(已批准 Section 1)

```sql
-- V55_SESSION_TREE
CREATE TABLE IF NOT EXISTS session_tree (
    id          TEXT PRIMARY KEY,        -- node UUID
    session_id  TEXT NOT NULL,
    parent_id   TEXT,                    -- NULL=root;可跨会话(fork 边)
    entry_type  TEXT NOT NULL,           -- 'message' | 'leaf'(未来 compaction/branch_summary/label)
    data_json   TEXT NOT NULL,           -- per-type payload(message → {"message_id":..,"role":..})
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_session_tree_session ON session_tree(session_id);
CREATE INDEX IF NOT EXISTS idx_session_tree_parent  ON session_tree(parent_id);

CREATE TABLE IF NOT EXISTS session_leaves (
    session_id  TEXT PRIMARY KEY,
    leaf_id     TEXT,                    -- → session_tree.id(当前 active leaf)
    updated_at  INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
```
- `V55_SESSION_TREE` const 加入 `migrations.rs` `run()` 末尾(split-loop 模式,幂等);**同 PR** 在 `CONTEXT.md` Active migration registry 表加 `| V55 | session_tree + session_leaves | (this PR) |` 行。
- **lazy-materialize(非 dual-write)**:`agent_messages` 仍是 source of truth;树按需从它物化。不触碰任何热路径读/写。树是派生谱系/分支结构;后续整合 slice 再翻成 authoritative(dual-write + getPathToRoot 接管读)。

---

## 4. SessionStorage API + fork/rewind 语义(已批准 Section 2)

**新模块 `src-tauri/src/agent/session_tree.rs`** —— 纯函数 over `&rusqlite::Connection`(in-memory SQLite 可单测):

```rust
pub struct TreeNode { pub id: String, pub session_id: String, pub parent_id: Option<String>,
                      pub entry_type: String, pub data_json: String, pub created_at: i64 }

pub struct ForkResult { pub id: String, pub title: String }       // 前端期待 meta{id,title}
pub struct RewindResult { pub file_rewind_can_rewind: bool }      // 序列化为 {fileRewind:{canRewind}}

/// 若该 session 无树行,从 agent_messages 构线性 message-node 链(created_at 序,parent=前一节点),
/// set session_leaves 到末节点。幂等(已物化则 no-op)。
pub fn materialize_session_tree(conn: &Connection, session_id: &str) -> Result<(), Error>;
pub fn append_node(conn: &Connection, session_id: &str, parent_id: Option<&str>, entry_type: &str, data_json: &str) -> Result<String, Error>;
/// 从 leaf 沿 parent_id 递归 CTE 走到 root,返回 root→leaf 序(本 slice 建好+单测,读路径暂不接)。
pub fn get_path_to_root(conn: &Connection, leaf_id: &str) -> Result<Vec<TreeNode>, Error>;
pub fn get_leaf(conn: &Connection, session_id: &str) -> Result<Option<String>, Error>;
pub fn set_leaf(conn: &Connection, session_id: &str, node_id: &str) -> Result<(), Error>;
pub fn fork_at(conn: &Connection, session_id: &str, up_to_message_id: &str) -> Result<ForkResult, Error>;
pub fn rewind_to(conn: &Connection, session_id: &str, target_message_id: &str) -> Result<RewindResult, Error>;
```

**fork**(`fork_agent_session{sessionId, upToMessageUuid}` → `meta{id,title}`):
1. `materialize_session_tree(source)`;校验 `upToMessageUuid` 存在(否则 `InvalidInput`)。
2. 新建 `agent_session`(新 uuid;title `"{orig} (fork)"`;copy metadata_json/attached_dirs/space_id)。
3. 复制 `agent_messages`:source 中 `created_at <=` fork 点消息 created_at 的行(**含**该消息),写入新 session_id(新 row id,保留 role/content/created_at + 指标列);更新新 session 的 message_count。
4. `materialize_session_tree(new)`;记录 **fork 边**:新会话 root message-node 的 `parent_id` = source 中 `upToMessageUuid` 对应节点 id(跨会话谱系)。
5. 返回 `ForkResult{id, title}`。

**rewind**(`rewind_session{sessionId, assistantMessageUuid}` → `{fileRewind:{canRewind:false}}`):
1. `materialize_session_tree(session)`;定位 target message 节点(否则 `InvalidInput`)。
2. **截断**:`DELETE FROM agent_messages WHERE session_id=? AND created_at > <target.created_at>`(保留**含** target);更新 `agent_sessions.message_count`。
3. 剪枝 session_tree:删 target 之后的节点;`set_leaf` 到 target 节点。
4. 返回 `RewindResult{file_rewind_can_rewind:false}`(文件态 rewind 范围外)。

> 命令薄包装:`tauri_commands.rs` 的 `fork_agent_session`/`rewind_session` 取 `state.db` lock,调 `session_tree::fork_at`/`rewind_to`,序列化结果为前端期待形状(实现者读 `AgentView.tsx:1490-1555` 对齐 `meta{id,title}` / `{fileRewind:{canRewind}}`)。`created_at` 相同的 tie-break:截断/复制用 `created_at >`(rewind 保留等于)/ `<=`(fork 含等于);若同 ms 多条,以 rowid 兜底(实现者按需加 `OR (created_at = ? AND rowid <= ?)`,但优先简单 `created_at` 比较 + 文档化 tie 边界)。

---

## 5. 错误处理 + 并发 + 行为保持(已批准 Section 3)

- 未知 message uuid / 空会话 → `Err(Error::InvalidInput(...))`。
- `materialize_session_tree` 幂等(有树行则 no-op)。
- **并发 guard**:`fork`/`rewind` 前 `if state.is_session_running(&session_id).await { return Err(InvalidInput("先停止 agent 再 fork/回溯")) }` —— 避免在 live run 下截断/复制。
- **rewind 破坏性截断**:`DELETE` target 之后的 `agent_messages`(匹配前端「回溯后消息消失」)+ 剪 tree + 移 leaf,保持 `agent_messages`/`session_tree` 一致。(Pi 的「保留弃用分支于树」需树作读源 → 延后。)
- **行为保持**:读/写热路径零触碰 → 现有 agent 测试不受影响;fork/rewind 从 error-stub → functional 是纯增量(原本就返回错误),无回归面。

---

## 6. 测试(已批准 Section 3)

- `session_tree.rs` 单测(in-memory SQLite,建最小 agent_sessions/agent_messages fixture):
  - `materialize` 构正确 parent 链 + leaf;再调幂等(行数不变)。
  - `get_path_to_root` root→leaf 序正确。
  - `get_leaf`/`set_leaf` 往返。
  - `fork_at`:新 session 建、agent_messages 复制到点(含)、fork 边 parent_id 指向 source 节点、返回 `{id,title}`。
  - `rewind_to`:`agent_messages` target 之后删除、message_count 更新、tree 剪枝、leaf 移到 target、返回 `{canRewind:false}`。
  - 未知 uuid → Err。
- 迁移测试:`run()` 后 `session_tree`/`session_leaves` 存在 + 列正确(query `pragma table_info`)。
- 命令级:存取函数充分覆盖兜底;命令是薄包装(若 AppState 可构造则加 smoke)。
- 验收 gate:`cargo build` 干净;`cargo test --lib session_tree 2>&1`(全过);全量无新失败(~7 已知预存)。手动 smoke:`cargo tauri dev` → 对一条消息 fork → 出现分叉会话;rewind 到某点 → 其后消息消失。

---

## 7. 文件结构 + commit 序列(可二分)

| 文件 | 责任 |
|---|---|
| `src-tauri/src/db/migrations.rs` | `V55_SESSION_TREE` const + `run()` 接入 |
| `CONTEXT.md` | Active migration registry 加 V55 行(同 PR)|
| `src-tauri/src/agent/session_tree.rs` | **新建** 存取 API + 类型 + 单测;`agent/mod.rs` 加 `pub mod session_tree;` |
| `src-tauri/src/tauri_commands.rs` | `fork_agent_session`/`rewind_session` 接通(薄包装 + 并发 guard)|

commit 序列:`V55 迁移 + CONTEXT 登记(迁移测试)` → `session_tree.rs 存取 API + 类型 + 单测(materialize/get_path_to_root/leaf)` → `fork_at + rewind_to + 单测` → `接通 fork/rewind 命令(并发 guard + 返回形状)` → `验收收口`。
