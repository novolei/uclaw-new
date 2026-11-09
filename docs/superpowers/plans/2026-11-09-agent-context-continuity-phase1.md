# Agent 上下文延续 — 阶段 1 实施计划（转录持久化 + 完整历史 + 128K 压缩生效）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让交互式 Agent 会话保留完整历史（含工具调用与结果）直到可配置的压缩阈值（默认 128K），压缩结果真正进入后续提示词，并让前端（设置、上下文用量、压缩提示）与之同步。

**Architecture:** 新增只追加的 `agent_transcript` 表，按迭代持久化"实际发给模型的消息"；`send_agent_message` 从转录表重放历史（旧会话先完整重放 `agent_messages` 文本），不再做头尾截断；`run_agentic_loop` 在 `transcript_mode` 下按绝对 token 阈值触发新压缩：复用现有摘要器生成 fold，写为 `session_block` 转录行并标记旧行 superseded，再从转录重建上下文。其他调用方不受影响（开关默认关闭）。

**Tech Stack:** Rust（tokio、rusqlite、serde_json）、Tauri 2 IPC、React 18 + Jotai + Vitest。

**Spec:** `docs/superpowers/specs/2026-11-09-agent-context-continuity-design.md`（§6.2、§6.3、§6.5 的压缩流程主干、§6.8 阈值设置、§6.9 生命周期、§7）。阶段 2–4 见文末"后续阶段大纲"，各自另写详细计划。

## Global Constraints

- 压缩阈值默认 **128_000** token，设置中可调；有效值 = clamp(配置值, 32_000, 模型窗口 − 16_384 输出预留 − 8_192 余量)。
- 新行为**只**在交互式 `send_agent_message` 启用（`AgenticLoopConfig.transcript_mode = true`）；其余 `run_agentic_loop` 调用方默认 `false`，行为不变。
- **绝不**在工具调用（`ToolUse`）与其结果（`ToolResult`）之间切分、持久化或压缩。
- 转录内容 = `uclaw_message_types::ChatMessage` 的 `serde_json` 序列化；重放必须与写入时字节一致。
- `agent_messages` 继续供界面显示；界面现有渲染（含 `CompactionFoldCard`、回退、分叉）不得回退。
- 迁移使用下一个空闲 V 号 **V63**，并更新 `CONTEXT.md` 迁移登记表。
- 新 Tauri 命令同时在 `tauri_commands.rs` 定义并在 `main.rs` 的 `invoke_handler!` 注册。
- 新 Rust 文件带 `// SPDX-License-Identifier: Apache-2.0`。
- **前后端同步**：本阶段所有面向用户的变化都在本阶段的前端任务中落地（设置项、上下文用量阈值、压缩提示）。
- 短对话不回退：`scripts/bench/` 7 条消息基准下首 token 中位数不劣于 1.14 s、每次调用输入中位数不劣于约 27K。
- 每个任务一个提交；提交前跑该任务的测试；`git add` 明确列出文件。

## Review Focus

1. **单个超大工具结果**（如一次读入 100K token 的文件）让请求本身就超过阈值：压缩后仍超限时不得反复压缩（防抖），应退到硬截断且保留本轮用户消息与 `session_block`。→ Task 6 测试 `compaction_does_not_thrash_on_oversized_tail`。
2. **工具执行中途取消/出错**：转录里不能留下没有结果的 `ToolUse`，下一轮请求必须对 OpenAI 兼容服务商合法。→ Task 5 测试 `finalize_synthesizes_results_for_dangling_tool_use`。
3. **会话中途重启应用**：重启后重放的历史与重启前完全一致（含 `session_block`）。→ Task 1 `roundtrip_is_byte_identical` + Task 7 `history_after_restart_matches_transcript`。
4. **升级前的长会话**（只有 `agent_messages`，可能含旧 `/compact` 摘要行）：首次新路径必须带上全部旧文本，不截断，且旧摘要行按原顺序出现。→ Task 7 `legacy_session_replays_all_text_then_transcript`。
5. **会话中切换到小窗口模型**（如本地 MiniCPM）：有效阈值按新窗口收紧，超出时先压缩再发送，而不是请求溢出。→ Task 2 `effective_threshold_clamps_to_small_window` + Task 4 `decision_compacts_when_window_shrinks`。

---

## 文件结构

| 文件 | 职责 | 新建/修改 |
|---|---|---|
| `src-tauri/src/agent/transcript.rs` | 转录表 DAO：追加、读取活跃行、supersede、回退截断、分叉复制 | 新建 |
| `src-tauri/src/agent/transcript_history.rs` | 组装 Agent 历史（旧文本 + 转录）、持久化辅助（完整前缀判断、补写取消结果） | 新建 |
| `src-tauri/src/agent/compaction_policy.rs` | 纯函数：有效阈值、压缩决策、切分点选取 | 新建 |
| `src-tauri/src/agent/mod.rs` | 注册上述模块 | 修改 |
| `src-tauri/src/db/migrations.rs` | `SQL_V63` + 运行器 + 测试 | 修改 |
| `src-tauri/src/agent/types.rs` | `AgenticLoopConfig` 新字段、`ReasoningContext` 新字段、`LoopDelegate::persist_transcript` 默认方法 | 修改 |
| `src-tauri/src/agent/agentic_loop.rs` | transcript_mode 下的持久化调用、压缩决策、新压缩路径 | 修改 |
| `src-tauri/src/agent/dispatcher/turn_runner.rs` | 实现 `persist_transcript`；写入请求开销估算 | 修改 |
| `src-tauri/src/agent/dispatcher/observability.rs` | `agent:context_stats` 增加 `compactionThresholdTokens` | 修改 |
| `src-tauri/src/memubot_config.rs` | `agent_compaction_threshold_tokens` 配置项 | 修改 |
| `src-tauri/src/tauri_commands.rs` | 历史组装改造、`/compact` 改走新路径、回退/分叉同步、新设置命令 | 修改 |
| `src-tauri/src/main.rs` | 注册新命令 | 修改 |
| `CONTEXT.md` | V63 登记 | 修改 |
| `ui/src/lib/tauri-bridge.ts`、`ui/src/lib/types.ts` | 设置读写、`ContextStats.compactionThresholdTokens`、`onContextCompacted` 监听 | 修改 |
| `ui/src/components/settings/ContextCompactionSettings.tsx` | "上下文与压缩"设置卡片 | 新建 |
| `ui/src/components/settings/IntelligenceTab.tsx` | 挂载上述卡片 | 修改 |
| `ui/src/components/agent/ContextUsageBadge.tsx` | 用后端阈值替代写死比例 | 修改 |
| `ui/src/hooks/useGlobalAgentListeners.ts` | 处理 `agent:context_compacted` | 修改 |

---

### Task 1: V63 转录表与 DAO

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`（新增 `SQL_V63`，运行器紧随 V62）、`CONTEXT.md`
- Create: `src-tauri/src/agent/transcript.rs`；Modify: `src-tauri/src/agent/mod.rs`

**Interfaces:**
- Produces:
  - 表 `agent_transcript(id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, seq INTEGER NOT NULL, turn_id TEXT, kind TEXT NOT NULL, message_json TEXT NOT NULL, est_tokens INTEGER NOT NULL DEFAULT 0, superseded INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, UNIQUE(session_id, seq))` + 索引 `(session_id, superseded, seq)`。
  - `pub enum TranscriptKind { Message, SessionBlock }`（存 `"message"` / `"session_block"`）。
  - `pub struct TranscriptRow { pub id: i64, pub seq: i64, pub turn_id: Option<String>, pub kind: TranscriptKind, pub message: ChatMessage, pub est_tokens: u32 }`
  - `pub fn append(conn: &Connection, session_id: &str, turn_id: Option<&str>, kind: TranscriptKind, msgs: &[ChatMessage]) -> anyhow::Result<Vec<i64>>`（seq 连续递增，事务内写入）
  - `pub fn load_active(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<TranscriptRow>>`（`superseded = 0`，按 seq）
  - `pub fn has_transcript(conn: &Connection, session_id: &str) -> anyhow::Result<bool>`
  - `pub fn supersede_up_to(conn: &Connection, session_id: &str, max_seq_inclusive: i64) -> anyhow::Result<usize>`
  - `pub fn import_legacy_text_rows(conn: &Connection, session_id: &str) -> anyhow::Result<usize>`：仅当该会话尚无转录行时，把 `agent_messages` 中 `compacted = 0` 的 `user`/`assistant` 文本行按 `created_at` 顺序一次性写入转录（`kind = message`；user 行的 `turn_id` = 该行 id，assistant 行沿用前一条 user 行的 id）；已有转录时返回 0
  - `pub fn truncate_after_turn(conn: &Connection, session_id: &str, keep_through_turn_id: &str) -> anyhow::Result<usize>`（删除该 turn 之后的行；若被删的行中有 `session_block`，同时把被它 supersede 的行恢复为 `superseded = 0`）
  - `pub fn copy_through_turn(conn: &Connection, from_session: &str, to_session: &str, through_turn_id: &str) -> anyhow::Result<usize>`

- [ ] **Step 1: 写失败测试**（`transcript.rs` 内 `#[cfg(test)]`，用 `db::migrations::run` 建库）
  - `v63_creates_agent_transcript`：`pragma_table_info('agent_transcript')` 含上述列；`run` 两次不报错。
  - `roundtrip_is_byte_identical`：含 Text/ToolUse/ToolResult（含中文与嵌套 JSON input）的消息，`append` 后 `load_active`，逐条 `serde_json::to_string` 与写入前相等。
  - `append_assigns_contiguous_seq_per_session`：两个会话交错写入，各自 seq 为 1..n。
  - `supersede_hides_rows`：`supersede_up_to(3)` 后 `load_active` 只剩 seq>3。
  - `truncate_after_turn_restores_rows_superseded_by_removed_block`：t1、t2 消息 → session_block（supersede t1、t2 的行）→ t3；`truncate_after_turn(t2)` 后 session_block 与 t3 消失，t1/t2 恢复可见。
  - `copy_through_turn_copies_active_rows_only`。
- [ ] **Step 2: 运行** `cd src-tauri && cargo test --lib agent::transcript` → 预期编译失败（模块不存在）。
- [ ] **Step 3: 实现** 迁移 `SQL_V63` 与上述 DAO；`est_tokens` 用 `uclaw_message_types::estimate_tokens` 对消息文本估算（与 `ReasoningContext::estimate_token_count` 同口径）。
- [ ] **Step 4: 运行** 同上 → 全部 PASS；再跑 `cargo test --lib db::migrations` → PASS。
- [ ] **Step 5: 更新 `CONTEXT.md`** 登记 `| V63 | agent_transcript — append-only per-session model transcript (context continuity phase 1) | this PR |`。
- [ ] **Step 6: 提交** `git add src-tauri/src/db/migrations.rs src-tauri/src/agent/transcript.rs src-tauri/src/agent/mod.rs CONTEXT.md && git commit -m "feat(agent): V63 agent_transcript table + DAO"`

### Task 2: 压缩阈值设置与有效阈值

**Files:**
- Create: `src-tauri/src/agent/compaction_policy.rs`
- Modify: `src-tauri/src/memubot_config.rs`、`src-tauri/src/tauri_commands.rs`（仿照 `set_plan_mode_suggest_enabled` 写一对命令）、`src-tauri/src/main.rs`

**Interfaces:**
- Produces:
  - `MemubotConfig.agent_compaction_threshold_tokens: u32`，`#[serde(default = "default_agent_compaction_threshold_tokens")]` → `128_000`
  - `pub const MIN_COMPACTION_THRESHOLD: usize = 32_000; pub const OUTPUT_RESERVE: usize = 16_384; pub const SAFETY_MARGIN: usize = 8_192;`
  - `pub fn effective_compaction_threshold(configured: usize, model_window: usize) -> usize`
  - Tauri: `get_agent_compaction_threshold() -> Result<u32, String>`、`set_agent_compaction_threshold(tokens: u32) -> Result<u32, String>`（返回保存后的值；拒绝 `< 32_000` 与 `> 2_000_000`）

- [ ] **Step 1: 写失败测试**（`compaction_policy.rs`）
  - `effective_threshold_uses_configured_when_window_large`：`(128_000, 1_000_000) == 128_000`
  - `effective_threshold_clamps_to_small_window`：`(128_000, 32_768)` → `32_768 - 16_384 - 8_192 = 8_192`，且结果 `<= window`（小窗口时允许低于 32K 下限，下限只约束配置值）
  - `effective_threshold_enforces_minimum_on_config`：`(10_000, 1_000_000) == 32_000`
  - `effective_threshold_unknown_window_uses_configured`：`model_window == 0` → 配置值
  - memubot_config 测试 `agent_compaction_threshold_defaults_to_128k`：旧 JSON（无该字段）反序列化后为 128_000。
- [ ] **Step 2: 运行** `cargo test --lib compaction_policy memubot_config` → FAIL。
- [ ] **Step 3: 实现** 上述函数、配置字段与两个命令，并在 `main.rs` 注册。
- [ ] **Step 4: 运行** 同上 → PASS；`cargo build` 无错误。
- [ ] **Step 5: 提交** `feat(agent): configurable compaction threshold (default 128K) + effective clamp`

### Task 3: 完整请求的 token 估算

**Files:** Modify `src-tauri/src/agent/types.rs`、`src-tauri/src/agent/dispatcher/turn_runner.rs`

**Interfaces:**
- Produces: `ReasoningContext.request_overhead_tokens: usize`（默认 0）——工具定义 + 组装后的系统提示词 + 动态块，相对 `system_prompt` 的额外部分；`pub fn estimate_request_tokens(&self) -> usize { self.estimate_token_count() + self.request_overhead_tokens }`
- Consumes: `turn_runner.rs` 的 `create_turn_snapshot`（组装出的系统提示词、`snapshot.tools`、`snapshot.dynamic_context`）

- [ ] **Step 1: 写失败测试** `estimate_request_tokens_includes_overhead`（types.rs）：overhead=5_000 时比 `estimate_token_count()` 多 5_000；turn_runner 单测 `snapshot_sets_request_overhead`：构造快照后 `request_overhead_tokens > 0` 且约等于工具定义 JSON + 组装系统提示词 + 动态块的 `estimate_tokens` 之和减去 `system_prompt` 的估算。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现** 在 `create_turn_snapshot` 末尾写入 `reason_ctx.request_overhead_tokens`。
- [ ] **Step 4: 运行** → PASS。
- [ ] **Step 5: 提交** `feat(agent): estimate full request tokens (tools + assembled system + dynamic block)`

### Task 4: 压缩决策（transcript_mode 开关）

**Files:** Modify `src-tauri/src/agent/compaction_policy.rs`、`src-tauri/src/agent/types.rs`、`src-tauri/src/agent/agentic_loop.rs:770-811`

**Interfaces:**
- Produces:
  - `AgenticLoopConfig.transcript_mode: bool`（默认 `false`）、`AgenticLoopConfig.compaction_threshold_tokens: usize`（默认 `128_000`）
  - `pub enum CompactionDecision { None, Compact, HardTruncate }`
  - `pub fn decide_compaction(estimated_request_tokens: usize, threshold: usize, model_window: usize) -> CompactionDecision`：`< threshold` → None；`>= threshold` 且 `< hard_limit` → Compact；`>= hard_limit` → HardTruncate，其中 `hard_limit = model_window - OUTPUT_RESERVE`（window 为 0 时视为无硬上限）
- Consumes: Task 2 `effective_compaction_threshold`；Task 3 `estimate_request_tokens`

- [ ] **Step 1: 写失败测试**（`compaction_policy.rs`）`decision_none_below_threshold`、`decision_compacts_at_threshold`、`decision_hard_truncates_past_window`、`decision_compacts_when_window_shrinks`（同一估算值在 1M 窗口下 None、在 32K 窗口下 Compact/HardTruncate）。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现** `decide_compaction`；在 `compress_context_if_needed` 开头：`if config.transcript_mode { 用 decide_compaction(reason_ctx.estimate_request_tokens(), effective_compaction_threshold(config.compaction_threshold_tokens, model_window), model_window) 分派到 Task 6 的新压缩 / 硬截断；return }`，否则保持原逻辑不变。
- [ ] **Step 4: 运行** `cargo test --lib compaction_policy agentic_loop` → PASS（原有 agentic_loop 测试全部仍通过，证明非 transcript_mode 行为未变）。
- [ ] **Step 5: 提交** `feat(agent): absolute-threshold compaction decision behind transcript_mode`

### Task 5: 按迭代持久化转录

**Files:** Create `src-tauri/src/agent/transcript_history.rs`；Modify `src-tauri/src/agent/types.rs`、`src-tauri/src/agent/agentic_loop.rs`（每个调用 `delegate.after_iteration(iteration)` 之前、以及循环返回前）、`src-tauri/src/agent/dispatcher/turn_runner.rs`

**Interfaces:**
- Produces:
  - `ReasoningContext.transcript_persisted_len: usize`（不变式：`messages[..transcript_persisted_len]` 已在转录中）
  - `LoopDelegate::persist_transcript(&self, new_msgs: &[ChatMessage]) -> anyhow::Result<()>`，**默认实现返回 `Ok(())`**（其他 delegate 无需改动）
  - `pub fn complete_prefix_len(msgs: &[ChatMessage]) -> usize`：最长的、末尾不含"无对应结果的 ToolUse"的前缀长度
  - `pub fn synthesize_missing_tool_results(msgs: &[ChatMessage]) -> Option<ChatMessage>`：为末尾悬空的 ToolUse 生成一条 `user_tool_result` 消息，每个结果文本为 `[工具调用已取消]`、`is_error = true`
  - `pub async fn flush_transcript(delegate: &dyn LoopDelegate, ctx: &mut ReasoningContext)`：写入 `messages[transcript_persisted_len..transcript_persisted_len + complete_prefix_len(tail)]`（跳过 `compacted` 消息），成功后推进计数；失败只记 warn，不中断循环
  - `pub async fn finalize_transcript(delegate: &dyn LoopDelegate, ctx: &mut ReasoningContext)`：先追加 `synthesize_missing_tool_results` 的结果（若有），再 `flush_transcript`
  - `TurnRunner::with_transcript_sink(self, sink: TranscriptSink) -> Self`，`pub struct TranscriptSink { pub db: Arc<std::sync::Mutex<rusqlite::Connection>>, pub session_id: String, pub turn_id: String }`；`TurnRunner::persist_transcript` 调用 `transcript::append(.., Some(&turn_id), TranscriptKind::Message, msgs)`
- Consumes: Task 1 `append`

- [ ] **Step 1: 写失败测试**（`transcript_history.rs`）
  - `complete_prefix_excludes_dangling_tool_use`：`[user, assistant(ToolUse a)]` → 1；补上 `ToolResult a` → 3。
  - `complete_prefix_requires_all_parallel_results`：一条 assistant 含 ToolUse a、b，只有 a 的结果 → 不含该 assistant。
  - `finalize_synthesizes_results_for_dangling_tool_use`：用记录型 fake delegate，`finalize_transcript` 后写入的最后一条是 a、b 两个 `[工具调用已取消]` 结果，且 `transcript_persisted_len == messages.len()`。
  - `flush_is_noop_when_transcript_mode_off`：agentic_loop 层测试——`transcript_mode = false` 时 fake delegate 的 `persist_transcript` 从未被调用。
- [ ] **Step 2: 运行** `cargo test --lib transcript_history agentic_loop` → FAIL。
- [ ] **Step 3: 实现** 上述函数；在 `agentic_loop.rs` 中，`config.transcript_mode` 为真时于每处 `delegate.after_iteration(iteration).await` 之前调用 `flush_transcript`，并在 `run_agentic_loop` 每个返回点之前调用 `finalize_transcript`。
- [ ] **Step 4: 运行** → PASS；全部原有 agentic_loop 测试仍通过。
- [ ] **Step 5: 提交** `feat(agent): persist transcript per iteration with tool-call pairing guard`

### Task 6: 新压缩路径（session_block）

**Files:** Modify `src-tauri/src/agent/compaction_policy.rs`、`src-tauri/src/agent/agentic_loop.rs`（新增 transcript_mode 分支，不改旧 `soft_compress_context` 的非 transcript 行为）、`src-tauri/src/agent/types.rs`（`LoopDelegate` 新默认方法）、`src-tauri/src/agent/dispatcher/turn_runner.rs`

**Interfaces:**
- Produces:
  - `pub const KEEP_RECENT_TOKENS: usize = 20_000; pub const KEEP_RECENT_USER_TURNS: usize = 3;`
  - `pub fn choose_cut_index(msgs: &[ChatMessage]) -> Option<usize>`：返回保留尾部的起点下标——尾部至少含最近 `KEEP_RECENT_USER_TURNS` 条用户文本消息、或累计 ≥ `KEEP_RECENT_TOKENS`（取两者中保留更多者）；起点必须是一条用户文本消息（因此不会切开 ToolUse/ToolResult）；可压缩部分为空时返回 `None`
  - `LoopDelegate::replace_with_session_block(&self, block: &ChatMessage, superseded_through: usize) -> anyhow::Result<()>`，默认 `Ok(())`；TurnRunner 实现：在同一事务里 `supersede_up_to(load_active 结果中第 superseded_through 条的 seq)`（依据 Task 7 的一一对应不变式；`superseded_through = cut`） 并 `append(.., TranscriptKind::SessionBlock, [block])`，再向 `agent_messages` 插入一条显示行（role `user`、content 为 `block` 文本、`created_at` = 被压缩的最早一行与保留尾部第一行之间，使界面的 `CompactionFoldCard` 出现在正确位置），最后 `emit("agent:context_compacted", {sessionId, compactedMessages, summaryTokens, thresholdTokens})`
  - `async fn transcript_compact(delegate, reason_ctx, config, llm, model_id) -> Result<(), CompactError>`（agentic_loop.rs 私有）：① `flush_transcript`；② `choose_cut_index`；③ `previous_fold` 存在则 `update_fold_incremental`，否则 `summarize_to_fold`，失败用 `extractive_fallback_fold`；④ `block = ChatMessage::user(&fold.to_markdown())`（以 `## Earlier conversation (compacted)` 开头）；⑤ `delegate.replace_with_session_block`；⑥ `reason_ctx.messages = [block] + messages[cut..]`，`transcript_persisted_len = reason_ctx.messages.len()`，`compaction_state.previous_fold = Some(fold)`
  - 防抖：同一迭代内压缩后若 `decide_compaction` 仍非 None，则不再压缩，改走硬截断；硬截断在 transcript_mode 下**不得**移除最后一条用户文本消息与首条 `session_block`
- Consumes: Task 4 `decide_compaction`；Task 5 `flush_transcript`；现有 `summarize_to_fold` / `update_fold_incremental` / `extractive_fallback_fold` / `StructuredFold::to_markdown`

- [ ] **Step 1: 写失败测试**
  - `choose_cut_index_keeps_last_three_user_turns`、`choose_cut_index_never_splits_tool_pair`（尾部起点处是 ToolResult 时向前移到对应用户消息）、`choose_cut_index_none_when_nothing_to_compact`（compaction_policy.rs）。
  - `transcript_compact_rebuilds_context_with_session_block`（agentic_loop.rs，fake LLM 返回固定 fold）：结果 `messages[0]` 文本以 `## Earlier conversation (compacted)` 开头、其后是原尾部、fake delegate 收到一次 `replace_with_session_block`。
  - `compaction_does_not_thrash_on_oversized_tail`：尾部单条消息本身超过阈值 → 只压缩一次，随后硬截断，保留最后一条用户消息与 session_block。
  - turn_runner 测试 `replace_with_session_block_supersedes_and_appends`（内存库）：之后 `load_active` = `[session_block, 尾部...]`，`agent_messages` 多一条以 fold 标题开头的行。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现**。
- [ ] **Step 4: 运行** `cargo test --lib compaction_policy agentic_loop turn_runner` → PASS。
- [ ] **Step 5: 提交** `feat(agent): transcript-mode compaction writes session_block and rebuilds context`

### Task 7: `send_agent_message` 改用转录历史

**Files:** Modify `src-tauri/src/agent/transcript_history.rs`、`src-tauri/src/tauri_commands.rs`（历史读取 ~9780-9797、ctx 构造 ~10190-10208、loop config、TurnRunner 构造）

**Interfaces:**
- Produces: `pub fn build_agent_history(conn: &Connection, session_id: &str) -> anyhow::Result<(Vec<ChatMessage>, usize)>` → `(messages, persisted_len)`：
  - 先调用 `transcript::import_legacy_text_rows`（升级前的会话在首次走新路径时一次性导入全部旧文本，**不截断**；之后所有内容都只来自转录，压缩/回退/分叉只处理一种来源）
  - 再返回 `load_active` 的全部消息，`persisted_len = messages.len()`（不变式：`messages[i]` 与第 i 条活跃转录行一一对应）
  - 本轮用户消息由调用方追加，并立即写入转录（turn_id = 本轮用户消息在 `agent_messages` 的 id）
- Consumes: Task 1、Task 5 `TranscriptSink`、Task 2 设置

- [ ] **Step 1: 写失败测试**（transcript_history.rs，内存库）
  - `new_session_history_is_empty`。
  - `legacy_session_replays_all_text_then_transcript`：60 条旧文本行（总量超过原 32K 窗口）、无转录 → 首次调用后转录有 60 行、结果包含全部 60 条（无 `[Earlier context omitted` 标记）；再追加新轮次后调用，顺序为旧文本在前。
  - `legacy_import_runs_once`：第二次调用 `build_agent_history` 不再重复导入（转录行数不变）。
  - `compaction_after_legacy_import_does_not_duplicate`：导入后经 Task 6 压缩，再次 `build_agent_history` 的结果首条为 session_block，且不再出现被压缩的旧文本。
  - `legacy_compact_fold_row_kept_in_order`：旧 `/compact` 写入的以 `## Earlier conversation (compacted)` 开头的 user 行按原位置出现。
  - `history_after_restart_matches_transcript`：经 Task 6 压缩后，再次调用 `build_agent_history` 的结果与压缩后的 `reason_ctx.messages`（去掉本轮未持久化部分）逐条序列化相等。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现** `build_agent_history`；在 `send_agent_message` 中：用它替换 `history_budget_window` 路径；构造 `ReasoningContext` 时设置 `transcript_persisted_len`；本轮用户消息追加后立即 `transcript::append`；`AgenticLoopConfig { transcript_mode: true, compaction_threshold_tokens: cfg.agent_compaction_threshold_tokens as usize, .. }`；TurnRunner `.with_transcript_sink(..)`。
- [ ] **Step 4: 运行** `cargo test --lib transcript_history tauri_commands` → PASS；`cargo build` 无错误。
- [ ] **Step 5: 提交** `feat(agent): replay full transcript history in agent mode (drop 32K head+tail window)`

### Task 8: `/compact` 改走新压缩

**Files:** Modify `src-tauri/src/tauri_commands.rs`（`/compact` 分支 ~9354-9671）

**Interfaces:** Consumes Task 6 `transcript_compact`（为此在 agentic_loop.rs 暴露 `pub async fn compact_now(delegate, reason_ctx, config, llm, model_id)` 包装）

- [ ] **Step 1: 写失败测试** `manual_compact_writes_session_block`：对一个已有转录的会话执行 `/compact` 路径的核心函数（把分支体抽成 `async fn run_manual_compact(state_like, session_id) -> Result<ManualCompactResult>`），之后 `load_active` 首行为 `session_block`，`agent_messages` 有 fold 显示行，且旧的 `compacted = 1` 批量 UPDATE 不再执行。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现**；无转录的旧会话执行 `/compact` 时先用 Task 7 规则构建历史，再走同一函数。删除旧 fold 构造代码。
- [ ] **Step 4: 运行** → PASS。
- [ ] **Step 5: 提交** `refactor(agent): /compact uses transcript compaction (single compaction path)`

### Task 9: 回退与分叉同步转录

**Files:** Modify `src-tauri/src/tauri_commands.rs`（`fork_agent_session` 10916、`rewind_session` 10955）

**Interfaces:** Consumes Task 1 `truncate_after_turn` / `copy_through_turn`

- [ ] **Step 1: 写失败测试** `rewind_truncates_transcript_to_turn`、`rewind_before_compaction_restores_raw_rows`、`fork_copies_transcript_through_turn`（对两个命令抽出的同步函数做内存库测试；turn_id 取回退/分叉点对应的用户消息 id）。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现**：在现有 `agent_messages` 截断/复制之后，同一个 DB 锁内调用对应 DAO。
- [ ] **Step 4: 运行** → PASS；`ui` 侧现有回退/分叉测试 `npm test -- --run` 不回退。
- [ ] **Step 5: 提交** `feat(agent): keep transcript in sync on rewind/fork`

### Task 10: 前端 — "上下文与压缩"设置

**Files:** Create `ui/src/components/settings/ContextCompactionSettings.tsx`（+ `.test.tsx`）；Modify `ui/src/lib/tauri-bridge.ts`、`ui/src/components/settings/IntelligenceTab.tsx`

**Interfaces:**
- Produces: `getAgentCompactionThreshold(): Promise<number>`、`setAgentCompactionThreshold(tokens: number): Promise<number>`（invoke `get_agent_compaction_threshold` / `set_agent_compaction_threshold`）
- UI：`SettingsCard` 标题"上下文与压缩"，说明文案："对话超过该长度时，会把较早的内容压缩成摘要，并保留最近几轮原文。数值越大记得越多，费用也越高。"；数值输入（单位 K，步长 8K，范围 32K–1000K，默认 128K）+ "恢复默认"按钮；保存失败用 `sonner` toast 提示。

- [ ] **Step 1: 写失败测试**（Vitest + `renderWithProviders`，mock bridge）：`loads current threshold and shows 128K`、`saves new value on change`（输入 200 → 调用 `setAgentCompactionThreshold(200000)`）、`rejects below 32K`（不调用 set，显示错误提示）、`reset restores 128K`。
- [ ] **Step 2: 运行** `cd ui && npm test -- --run ContextCompactionSettings` → FAIL。
- [ ] **Step 3: 实现**组件并挂到 `IntelligenceTab`（`AgentSettings` 之后）。
- [ ] **Step 4: 运行** → PASS；`npx tsc --noEmit` 无错误。
- [ ] **Step 5: 提交** `feat(ui): context & compaction settings card (threshold, default 128K)`

### Task 11: 前端 — 上下文用量使用后端阈值

**Files:** Modify `src-tauri/src/agent/dispatcher/observability.rs`（`ContextStats` 加 `compaction_threshold_tokens`，serde camelCase）、`ui/src/lib/types.ts`（`ContextStats.compactionThresholdTokens?: number`）、`ui/src/components/agent/ContextUsageBadge.tsx`（+ 测试）

**Interfaces:** Produces 事件字段 `compactionThresholdTokens`（= Task 2 有效阈值，仅 transcript 会话发送）

- [ ] **Step 1: 写失败测试**：Rust `context_stats_includes_compaction_threshold`；Vitest `uses backend compaction threshold when provided`（窗口 1M、阈值 128K、已用 110K → 处于警告态）、`falls back to ratio when absent`、详情中显示"距离自动压缩"剩余 token。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现**：徽标以 `compactionThresholdTokens ?? floor(window × COMPACT_THRESHOLD_RATIO)` 为准。
- [ ] **Step 4: 运行** Rust 与 Vitest → PASS。
- [ ] **Step 5: 提交** `feat(ui): context meter reflects configured compaction threshold`

### Task 12: 前端 — 自动压缩提示

**Files:** Modify `ui/src/lib/tauri-bridge.ts`（`onContextCompacted(cb)` 监听 `agent:context_compacted`）、`ui/src/lib/types.ts`（`ContextCompactedEvent { sessionId: string; compactedMessages: number; summaryTokens: number; thresholdTokens: number }`）、`ui/src/hooks/useGlobalAgentListeners.ts`（+ 测试）

**Interfaces:** Consumes Task 6 事件

- [ ] **Step 1: 写失败测试** `context_compacted refreshes the session messages`（触发后调用现有的会话消息刷新，使 `CompactionFoldCard` 出现）与 `shows a toast "已自动压缩较早的对话，保留了最近几轮原文"`；仅对当前会话生效。
- [ ] **Step 2: 运行** → FAIL。
- [ ] **Step 3: 实现**（复用现有刷新函数与 toast）。
- [ ] **Step 4: 运行** → PASS。
- [ ] **Step 5: 提交** `feat(ui): notify and refresh on automatic context compaction`

### Task 13: 阶段 1 验证与 PR

- [ ] **Step 1:** `cd src-tauri && cargo test --lib`：除已知失败（shell daemon 1 个、browser runtime/provider_execution 4 个、playwright_cli 偶发）外全部通过。`cd ui && npm test -- --run && npx tsc --noEmit` 通过。
- [ ] **Step 2: 回归基准** `scripts/bench/`：7 条消息，首 token 中位数 ≤ 1.14 s 附近、每次调用输入中位数 ≤ 约 27K（应用日志 `agent:turn_cost`）。
- [ ] **Step 3: 长对话手动验证**（阈值设为 32K）：持续对话触发压缩 → 界面出现压缩卡片与提示、上下文用量徽标回落；重启应用后继续对话，压缩摘要仍在；回退到压缩前的一轮后，原始历史恢复。
- [ ] **Step 4:** 推送分支、开 PR（含 `## Commits (bisectable)` 表），等 CI 通过后合并。

---

## 后续阶段大纲（每阶段开始前另写详细计划）

**阶段 2 — 文件工作集与压缩质量**（依赖阶段 1）
- 后端：持久化 `SessionFileOps`（读：路径/范围/时间；写/编辑：改动摘要/diff 截断/时间/回退标记）；文件类工具执行后更新；结构化压缩模板（全部用户消息要点、关键决定、错误与解决、未完成事项、下一步，目标约 3K）；压缩后重载（工作集、计划正文、最近改过的最多 5 个文件、单个 ≤5K）；复用主对话前缀的摘要调用；旧工具结果批量清理（保留 3 组、下限 20K、占位保留路径/范围、写回转录）。
- 前端：Agent 视图"本会话文件"面板（改过的在前、读过的在后、已回退轮次的改动带标记）；设置卡片增加"保留最近工具结果组数 / 批量清理下限 / 摘要长度 / 压缩后重载文件数"；压缩卡片展示摘要中的文件清单；被清理的工具结果在消息流里显示为"已收起"。
- 验收：设计 §7.A 主验收（含重启、两次压缩、32K 阈值）。

**阶段 3 — 提示词分层、缓存与记忆**（依赖阶段 1，可与阶段 2 并行）
- 后端：系统提示词按会话冻结并持久化（移出 gene 块、项目规则、计划建议提示）；动态块随用户消息写入转录；记忆差量注入（`<memory_update>`）；召回查询结合最近用户消息、修正 `"default"` space；T3：`build_skills_manifest` 限量清单（1500 token / 200 条，会话内固定）+ 动态块 `<relevant_skills>`（词法 + 语义，≤400 ms）；会话中途模式/插件变化以提醒追加；`cost_store` 写入缓存命中字段。
- 前端：上下文用量详情增加"系统提示词 / skills / 记忆 / 历史"分项与缓存命中率；skills 调试面板（`compute_active_manifest_entries`）对齐新清单；模式切换提示文案；设置中"提示词"页显示冻结后的系统提示词预览。
- 验收：§7.B 长对话缓存命中率提升、每轮未命中 ≤ 2K；§7.C 前缀字节一致性测试。

**阶段 4 — 系统提示词打磨与全量验收**（依赖阶段 2、3）
- 后端：按 Claude 规范审查基础/模式/基线提示词（可用 `/claude-api prompt-audit` 起草 diff）；新增"关于上下文"段落。
- 前端：提示词设置页文案与预览同步；帮助说明"压缩如何工作"。
- 验收：§7 全部条目 + bench 回归。
