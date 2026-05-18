# Automation Phase 2b · 集群 A · Messaging 基座设计

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在已稳定的 IM dispatcher 之上，让每个 spec 拥有 per-identity 的长期 chat session（`automation:chat`），并把 spec 的产出（自主触发结果、IM 回复、UI streaming）统一接到这条 chat 线上 —— 完成 Phase 2a 明确推迟的"真实触达"承诺。

**Architecture:** 引入混合 `ChatlikeAutomationDelegate` 取代 automation 路径中的 `HeadlessDelegate`，I/O handle 全部 `Option<>` 化按触发上下文装配；新增 `automation_chat_sessions(spec_id, identity_key) → agent_session_id` 索引表（V36 migration）；scheduled/file/webhook 路由改为追加 owner chat 线（不再创建 per-fire run session）；IM dispatcher 路径复用同一索引拿到 per-(spec, IM 身份) chat session。

**Tech Stack:** Rust + SQLite (rusqlite, MIGRATIONS array)，Tauri AppHandle emit，复用 PR #182/#186/#189 的 IM channels 基础设施。

---

## 1 · 背景与本 spec 范围

Phase 2a（PR #172）显式将下列工作推迟到 Phase 2b "真实触达"：

> "The full messaging system — the `automation:chat` long-lived thread, IM inbound close-loop, and the full unified-substrate implementation — is **Phase 2b** ("真实触达"). The delegate-identity question for an interactive chat thread (which delegate drives it) is a Phase 2b design item, recorded but not solved here."
> —— `2026-05-14-automation-phase2a-design.md` §0.10

Phase 2b 全集包含 5 个相关但可分离的子项目；本 spec 只覆盖 **集群 A · Messaging 基座**：

| 集群 | 包含项 | 本 spec 状态 |
|---|---|---|
| **A · Messaging 基座** | (1) `automation:chat` 长期对话线 + (2) IM inbound 闭环 | **本 spec 覆盖** |
| B · Memory 结构化 | (4) `memory_schema` Tier 1 + (3) Tier-2 promotion | 独立后续 spec |
| C · Fork affordance | (5) "Continue in a new session" | 依赖 A 完成后的独立 spec |

构建于 PR #182/#186/#189 修稳的 WeChat iLink dispatcher 之上。

---

## 2 · 关键架构抉择（已与用户对齐）

### 2.1 Delegate 身份 → 混合 delegate

**单一 `ChatlikeAutomationDelegate` 类型**，I/O handle 全部 `Option<>`。Session 始终绑一个 driver，行为按 handle 是否 `Some` 分支：

```
trait ChatlikeAutomationDelegate {
    fn streaming_handle(&self) -> Option<&dyn StreamingHandle>;  // Some when UI is attached
    fn reply_handle(&self) -> Option<&ReplyHandle>;              // Some when IM-originated
    fn app_handle(&self) -> Option<&AppHandle>;                  // Some when UI should receive emit
    // ... otherwise identical to HeadlessDelegate trait surface
}
```

替代 `HeadlessDelegate` **仅在 automation 路径**。通用 `run_agent_chat_via_im`（无 spec 的 generic agent chat）保留 `HeadlessDelegate`，避免影响已稳定路径。

### 2.2 Session 拓扑 → 按 (spec, identity) 切分

每个 (spec_id, identity_key) 拿一个独立的 `automation:chat` agent_session：

```
spec X
 ├── automation:chat session, identity_key = "local"                  (本地 owner)
 ├── automation:chat session, identity_key = "wechat_ilink:UIN_a"     (微信用户 A)
 ├── automation:chat session, identity_key = "wechat_ilink:UIN_b"     (微信用户 B)
 └── automation:chat session, identity_key = "wecom_bot:USER_x"       (企微用户 X)
```

`identity_key` 规范化规则：
- 本地 owner：固定字符串 `"local"`
- IM 来源：`"{channel_type}:{chat_id}"` —— 直接用 `chat_id` 明文，便于 debug 和与 `im_sessions` 表互相对照（`im_sessions` 已经存明文 chat_id）

UNIQUE 约束保证幂等创建。

### 2.3 自主触发的落点 → 写入 owner chat 线

scheduled / file / webhook 触发的 run 不再创建 per-fire `automation:scheduled` session，而是**追加消息到 owner 的 `(spec, "local")` automation:chat session**。

| 触发场景 | 目标 session |
|---|---|
| scheduled timer fires | `(spec_id, "local")` chat session |
| file watcher fires | `(spec_id, "local")` chat session |
| webhook 收到 | `(spec_id, "local")` chat session |
| 本地 owner 在 UI 里手动跑 | `(spec_id, "local")` chat session |
| IM 用户 A 触发 trigger phrase | `(spec_id, "wechat_ilink:UIN_a")` chat session |

效果：owner 在自己的 chat 线里看到一切（包括自主触发的产出），不需要去多个地方查 spec 状态。

### 2.4 IM 回复粒度 → 每 turn 1 条最终消息

保持 PR #182/#186/#189 设计：agent loop 结束后**单条** `send_text` 推回 IM，包含 `extract_final_assistant_text(&messages[start_idx..])` 的产物。不流式中间更新（避免微信连发垃圾消息）。Initial ack（"正在处理中…"）在 `dispatch_inbound` 已经存在，保留。

### 2.5 notify_user 路由 → 按触发者归属

spec 调用 `notify_user` 工具时按触发上下文路由：

| 触发来源 | notify_user 目标 |
|---|---|
| IM 用户 A 触发 | 推回 A 的 IM 渠道（不广播给 owner 或其他 IM 用户） |
| Scheduled / file / webhook | 推送给 owner（本地通知 toast + emit 到本地 chat 线） |
| Owner 在 UI 里手动跑 | 本地通知 toast |

不广播到所有绑定的 identity（避免 spam）。

---

## 3 · 数据模型

### 3.1 新 migration V36

`automation_chat_sessions` 索引表，键 `(spec_id, identity_key)` 映射到 agent_session：

```sql
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
```

### 3.2 `agent_sessions.metadata.origin` 扩展

`automation:chat` 加入可能值集合（与 `automation:scheduled` / `:file` / `:webhook` / `:manual` / `:im` 并列）。`metadata` JSON 同时包含 `spec_id` 字段（已有约定，复用）。

### 3.3 `im_sessions` 表关系

`im_sessions(space_id, channel_type, chat_id) → agent_session_id` 保持不变。

对于**绑定到 spec 的 IM 消息**，dispatcher 不再用 `im_sessions` 拿 generic agent session，而是：
1. 用 `(spec_id, "wechat_ilink:{chat_id}")` 查 `automation_chat_sessions`
2. 若无则创建一个 `automation:chat` agent_session 并写入索引

对于**未绑定 spec 的 IM 消息**（无 trigger phrase 匹配），仍走 `run_agent_chat_via_im`（generic）通过 `im_sessions` 查 session 不变。两种路径互不干扰。

### 3.4 旧数据兼容

历史的 `automation:scheduled` / `:file` / `:webhook` / `:manual` session 在 DB 中**保留不动**。UI 视图按 `origin` 字段区分新旧；前端可以选择只在 spec detail 页的 "Run history" 里展示历史 run session，而 "Chat threads" tab 只展示新的 `automation:chat` session。

无 migration 脚本、forward-only。

---

## 4 · 行为流程

### 4.1 Scheduled fire 路径

```
Scheduler timer fires at T
    ↓
runtime_service.scheduled_fire(spec_id, payload)
    ↓
session_id = get_or_create_chat_session(spec_id, "local")
    ↓
ChatlikeAutomationDelegate {
    streaming_handle: None,            // 无 UI 流式
    reply_handle: None,                // 无 IM 回复
    app_handle: Some(app),             // 仍 emit chat:stream-complete 让 UI 刷新
}
    ↓
run_agentic_loop → 完成
    ↓
persist 新消息到 agent_messages
    ↓
emit "chat:stream-complete" → owner 的 UI 若打开此 session 就刷新
```

### 4.2 IM 用户 A 触发 trigger phrase 路径

```
WeChat 消息 "/天气" 到达 → IM dispatcher
    ↓
match_spec_by_trigger_phrase → spec_id
    ↓
identity_key = format!("wechat_ilink:{}", msg.chat_id)
session_id = get_or_create_chat_session(spec_id, identity_key)
    ↓
ChatlikeAutomationDelegate {
    streaming_handle: None,                       // IM 不流式
    reply_handle: Some(WeChatReplyHandle for A),  // 用于发回 A
    app_handle: Some(app),                        // owner UI 看到此线刷新
}
    ↓
agent loop 跑
    ↓
final_text = extract_final_assistant_text(&messages[start_idx..])
    ↓
reply_handle.sender.send_text(&A.chat_id, &final_text, ctx) → A 的微信
emit "chat:stream-complete" → owner UI 若打开此 (spec, A) 线就刷新
```

### 4.3 本地 owner 在 UI 里打字路径

```
Owner 打开 spec detail 页 → 切到 "Chat threads" → 点 "local" 线
    ↓
现有 AgentView 组件呈现 (spec_id, "local") 对应的 agent_session
    ↓
Owner 打字 → invoke('send_agent_message', ...)
    ↓
session_id 已存在（或 get_or_create 幂等返回）
    ↓
ChatlikeAutomationDelegate {
    streaming_handle: Some(UiStreamingHandle),    // UI 流式更新
    reply_handle: None,                           // 无 IM
    app_handle: Some(app),
}
    ↓
agent loop 跑，streaming_handle 持续 emit token / tool activity
    ↓
完成后 persist
```

---

## 5 · 文件改动总表

| 文件 | 性质 | 改什么 |
|---|---|---|
| `src-tauri/src/db/migrations.rs` | 新 V36 | `automation_chat_sessions` 表 + 索引 |
| `src-tauri/src/automation/runtime/chat_sessions.rs` | **新文件** | `get_or_create_chat_session(spec_id, identity_key) → session_id` 实现 + 单元测试 |
| `src-tauri/src/automation/runtime/service.rs` | 重构 | `execute_run_with_reply` 不再用 `_` 忽略 handles；新增 `execute_run_in_chat_session(spec_id, identity_key, payload, handles...)` 入口 |
| `src-tauri/src/agent/chatlike_automation_delegate.rs` | **新文件** | `ChatlikeAutomationDelegate` 实现 `HeadlessDelegate` 当前的 trait surface，所有 I/O handle 字段改 `Option<>` |
| `src-tauri/src/agent/headless.rs` | 不动 | 通用 `run_agent_chat_via_im` 路径继续用，避免影响 #182/#186/#189 已稳定的链路 |
| `src-tauri/src/automation/runtime/scheduler.rs` | 改路由 | scheduled fire 走 `execute_run_in_chat_session(spec_id, "local", ...)`，不再 `create_session("automation:scheduled")` |
| `src-tauri/src/automation/runtime/file_watch.rs` | 改路由 | 同上 |
| `src-tauri/src/automation/runtime/webhook.rs`（如存在） | 改路由 | 同上 |
| `src-tauri/src/channels/dispatcher.rs` | 改路由 | `run_automation_via_im` 改为 `execute_run_in_chat_session(spec_id, "wechat_ilink:" + chat_id, ...)`，传入 reply_handle |
| `src-tauri/src/agent/tools/builtin/notify_user.rs` | 改实现 | 通过 delegate 上下文读出触发者归属，按 §2.5 路由 |
| `src-tauri/src/tauri_commands.rs` | 新命令 | `list_chat_sessions_for_spec(spec_id) → Vec<{ identity_key, session_meta }>` 供 UI 列 chat threads |
| `src-tauri/src/main.rs` | 注册 | 新命令加入 `invoke_handler!` 宏 |
| `ui/src/lib/tauri-bridge.ts` | 新调用 | `listChatSessionsForSpec(specId)` wrapper |
| `ui/src/components/automation/SpecDetailView.tsx`（或同等位置） | 新 tab | "Chat threads" 列出所有 identity threads，点击进入 AgentView 复用 |
| `ui/src/atoms/automation-atoms.ts`（或同等位置） | 新 atom | 缓存 per-spec chat session 列表 |
| `CLAUDE.md` | 更新 | 在 Active migration registry 增加 V36 行 |

**估算**：6-8 commits、约 1 周。

---

## 6 · 测试策略

每条按 TDD 顺序，对应一个独立的 `#[tokio::test]` 或 `#[test]`：

### 6.1 索引层（`chat_sessions.rs`）

- `get_or_create_chat_session_dedups_per_identity` — 同 (spec, identity) 第二次调用拿回同一 session_id（验证 UNIQUE 约束 + 幂等性）
- `get_or_create_chat_session_creates_distinct_for_different_identities` — (spec, "local") 与 (spec, "wechat_ilink:foo") 拿到不同 session_id
- `get_or_create_chat_session_cascades_on_session_delete` — 删除 agent_session → 索引行随 FK CASCADE 清除

### 6.2 路由层

- `scheduled_fire_writes_to_owner_chat_session` — scheduler fires → owner 的 `(spec, "local")` session 多出一条 assistant 消息，**不**新建 `automation:scheduled` session
- `im_inbound_routes_to_per_identity_session` — WeChat 用户 A 触发 → `(spec, "wechat_ilink:UIN_a")` session；B 触发 → `(spec, "wechat_ilink:UIN_b")`；两条互不混
- `legacy_automation_scheduled_session_not_touched_by_new_code` — 历史 `automation:scheduled` session 在新代码路径下保持只读、不被当作 chat session 使用

### 6.3 Delegate 行为

- `chatlike_delegate_streams_to_ui_when_streaming_handle_some` — UI 路径下 token 通过 streaming_handle 推出去
- `chatlike_delegate_silent_when_streaming_handle_none` — 自主路径下不调 streaming_handle
- `chatlike_delegate_sends_im_reply_after_loop_when_reply_handle_some` — agent loop 结束 → `reply_handle.send_text` 被调用一次，参数是最终 text
- `chatlike_delegate_logs_send_text_errors_not_silent` — IM 发送失败 → `tracing::error!` 被调（PR #186 设计延续）

### 6.4 notify_user 路由

- `notify_user_routes_to_originator_im_user_not_owner` — IM 触发的 run 里 spec 调 notify_user → 推回触发者 IM 而非 owner
- `notify_user_routes_to_owner_when_scheduled_triggered` — scheduled fire 里调 notify_user → owner 本地通知
- `notify_user_does_not_broadcast_across_identities` — 不发给其他 identity（验证非 spam）

### 6.5 集成

- `round_trip_im_user_a_triggers_spec_reply_persisted_and_sent` — 端到端：mockito mock WeChat sendmessage；触发 trigger phrase → 验证 (a) `(spec, "wechat_ilink:UIN_a")` session 存在 (b) 两条新消息持久化（user + assistant）(c) mock 收到 send_text 调用且文本与持久化的 assistant 一致

---

## 7 · 范围边界（明确不做）

| 项 | 留给 |
|---|---|
| `memory_schema` 结构化 Tier 1 | 集群 B（独立后续 spec）|
| Tier-2 memory promotion | 集群 B |
| "Continue in a new session" fork affordance | 集群 C（依赖本 spec 完成）|
| WeCom 群聊 | 独立 WeCom spec（PR #182 已明示 iLink DM-only）|
| IM 流式中间更新 | 永不做（设计决策，避免 IM spam）|
| 跨 identity 共享记忆 | 集群 B |
| 旧 `automation:scheduled` 数据迁移 | 不做（forward-only）|
| spec detail 页的 "Run history" tab 改造 | 不在本 spec，UI 侧改动只新增 "Chat threads" tab，"Run history" 保持现状 |

---

## 8 · 已知风险与公开问题

### 8.1 同 session 并发消息 burst

同一 `(spec, identity)` 上短时间 burst 多消息（用户连发 3 条）→ 当前 agent loop 不支持中断。**处理**：runtime service 内持 `Arc<Mutex<HashMap<session_id, Arc<tokio::sync::Mutex<()>>>>>`，进入 chat 路径前先取本 session 的 mutex；后续消息排队（重启后清空，可接受）。实施时验证锁的获取顺序与释放是 FIFO，避免饥饿。

### 8.2 identity_key 命名规范

`"wechat_ilink:UIN_a"` 直接用明文 chat_id。**接受风险**：`im_sessions` 表已经存明文 chat_id，本表保持一致便于 debug。如未来引入隐私要求，统一加 hash 层。

### 8.3 delegate 命名冲突

`HeadlessDelegate` 在通用 `run_agent_chat_via_im`（无 spec）路径仍在用。**处理**：保留 `HeadlessDelegate` 不动，**新加** `ChatlikeAutomationDelegate` 只用于 automation 路径。避免重命名引发的影响面扩散。

### 8.4 emit 给 owner 的 IM 线刷新

当 IM 用户 A 触发 run，owner 在 UI 里打开了 `(spec, A)` chat 线 → `chat:stream-complete` emit 后 owner 看到刷新。这要求 owner 的 UI 把 `(spec, A)` session 作为正常 agent session 渲染（AgentView 已能做到）。需要在 `list_chat_sessions_for_spec` 命令里返回所有 identity 的 session 元数据，不仅是 owner 自己的，让 owner 能在 spec detail 页看到全部 chat threads。

### 8.5 触发者归属在 delegate 中的传递

`notify_user` 路由依赖 delegate 知道"我是谁触发的"。**实现路径**：在 `ChatlikeAutomationDelegate` 构造时记录 `OriginContext { identity_key: String, reply_handle: Option<ReplyHandle> }`；`notify_user` 工具实现读取此上下文决定推送目标。

---

## 9 · 验收条件（本 spec 完成的判定）

1. **本地 owner 视角**：打开任意 spec → "Chat threads" tab 里至少能看到自己的 `local` 线；scheduled fire 一次 → 线里多一条 assistant 消息
2. **IM 用户视角**：WeChat 用户 A 发 spec trigger phrase → 收到 spec 处理后的回复（单条，包含完整 final text，不含历史 JSON 转储或截断）
3. **多 IM 用户隔离**：A 和 B 同时与同一 spec 对话 → owner 在 spec detail 页能看到两条独立 chat threads，互相内容不混
4. **notify_user 触发器正确**：IM 触发的 run 调 notify_user → 推到触发者 IM；scheduled fire 调 notify_user → 推到 owner 本地
5. **回归**：通用 IM agent chat（无 spec 路径）行为不受影响；历史 `automation:scheduled` session 在 UI 中仍可读
6. **测试**：本 spec 列出的 §6 测试全绿；`cargo test --lib` 全套通过；`npx tsc --noEmit` 干净
