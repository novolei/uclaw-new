# Agent 上下文延续 — 阶段 2（文件工作集与压缩质量）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 agent 在任意时刻（含压缩后、重启后、回退后）都能准确说出本会话读过、改过哪些文件以及改了什么；压缩摘要结构化且复用主对话缓存；旧工具结果按批清理；前端同步展示。

**Architecture:** 新增按会话持久化的**文件事件日志**（V64 `agent_file_events`，只追加），由工具执行后的统一出口写入，聚合成 `FileWorkingSet` 后渲染为 `<file_working_set>`。压缩时会话固定块 = 结构化摘要 + 文件工作集 + 当前计划正文 + 最近改过的文件内容；摘要调用复用主对话完全相同的 tools/system/messages 前缀。旧工具结果清理以**覆盖层列**（`agent_transcript.cleared_json`）实现，原始 `message_json` 永不改写。

**Tech Stack:** Rust（Tauri、rusqlite、tokio）、React + Jotai + Vitest。

**Spec:** `docs/superpowers/specs/2026-11-09-agent-context-continuity-design.md`（§6.4、§6.5、§6.8、§6.9 回退/分叉、§6.10、§7.A）。阶段 1 已合并（PR #688，main @ bd8e146a）。

## Global Constraints

- 工作目录：worktree `.claude/worktrees/ctx-p2`，分支 `feat/agent-context-continuity-p2`（基于 main bd8e146a）。不推送，直到最终任务。
- 新行为仅在 transcript 模式（交互式 `send_agent_message`，`AgenticLoopConfig.transcript_mode == true`）生效；headless / symphony / teams 等调用方行为不变（只需回归测试）。
- 迁移号 **V64**（V63 为最后一个；开工前用 `gh pr list --state open` 与 `CONTEXT.md` 迁移登记表确认无冲突，并在 `CONTEXT.md` 登记 V64）。遵循 `uclaw-migrations` skill 的幂等规则（`ALTER TABLE … ADD COLUMN` 需先查 `pragma_table_info`）。
- 新 Tauri 命令必须同时在 `tauri_commands.rs` 定义并在 `main.rs` 的 `invoke_handler!` 注册。
- 新 Rust 文件以 `// SPDX-License-Identifier: Apache-2.0` 开头。
- `agent_transcript.message_json` 永不改写（阶段 1 不变量）；清理只写覆盖层列。
- 设置默认值（§6.8，原样使用）：保留最近工具结果组数 **3**；批量清理下限 **20_000** token；摘要目标长度 **3_000** token；压缩后重载文件数 **5**，单文件上限 **5_000** token。
- 清理占位前缀固定为 `[已清理的工具结果：`（前端据此识别）。
- 界面文案为简体中文；代码、提交信息为英文。每个后端可见变化都要有对应前端（用户要求：前后端同步）。
- 每个任务一个提交（`git add <paths>` 显式暂存，禁止 `git commit -am`）；TDD 必须有 RED→GREEN 证据。
- 验证命令：`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`；`cargo test --lib <filter>`；`cd ui && npm test -- --run <pattern>`；`npx tsc --noEmit`（main 基线 41 个错误，不得增加）。已知失败：shell `test_daemon_mode_approval_unchanged`、`runtime_pack_ipc::print_real_status_diagnostics`、3× `provider_execution::*cli_route*`、偶发 `playwright_cli::*worker*`、偶发 `recall_semantic_is_fast_under_connection_contention`；UI `KaleidoscopeShell`、`MemoryModule` MemoryGraphView。

## Review Focus

1. 同一文件以相对路径、绝对路径、`./` 前缀或 `..` 形式出现 → 工作集里只有一条（Task 2 测试 `normalizes_equivalent_paths_to_one_entry`）。
2. 最近改过的文件在压缩时已被删除、是二进制/非 UTF-8、或超过 5K token → 不读入正文，只给路径并注明原因，不 panic（Task 7 测试 `reload_skips_deleted_binary_and_oversized_files`）。
3. 清理不得触碰最近 3 组工具调用、本迭代的结果、已清理过的结果；可清理总量不足 20K 时什么都不做（Task 8 测试 `clearing_respects_recent_groups_floor_and_idempotence`）。
4. 复用前缀的摘要调用返回了工具调用、空文本或非 JSON → 回退到独立摘要器，压缩照常完成（Task 6 测试 `prefix_summary_falls_back_on_tool_call_or_bad_json`）。
5. 回退到更早的轮次后再问"改过哪些文件" → 被回退轮次的改动仍列出但标注"来自已回退的轮次，磁盘上仍保留"；分叉后新会话带同样标注（Task 4 测试 `rewind_marks_events_and_fork_copies_marks`）。

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `src-tauri/src/agent/file_working_set.rs`（新） | `FileEvent` / `FileWorkingSet` 类型、V64 表 DAO、聚合、路径规范化、渲染 `<file_working_set>` |
| `src-tauri/src/agent/file_events.rs`（新） | 由 `ToolDispatchOutcome` 推导 `FileEvent`（纯函数） |
| `src-tauri/src/agent/session_block.rs`（新） | 组装会话固定块（摘要 + 工作集 + 计划 + 最近文件），读盘与截断 |
| `src-tauri/src/agent/tool_result_clearing.rs`（新） | 清理选择（纯函数）+ 占位文本 |
| `src-tauri/src/agent/compact/prefix_summary.rs`（新） | 复用前缀的摘要请求构造与解析 |
| `src-tauri/src/agent/transcript.rs` | 覆盖层列读写、`delete_session` 级联新表 |
| `src-tauri/src/agent/transcript_compaction.rs` | 改用 session_block 组装、前缀摘要、清理前置 |
| `src-tauri/src/agent/transcript_branch.rs` | 回退标记、分叉复制文件事件 |
| `src-tauri/src/agent/dispatcher/turn_runner.rs` | 工具出口写文件事件；新 delegate 方法实现；请求构造抽取 |
| `src-tauri/src/agent/compact/{fold.rs,summarize.rs,render.rs}` | 结构化模板字段、提示词、目标长度 |
| `src-tauri/src/memubot_config.rs` | 4 个新设置 |
| `src-tauri/src/db/migrations.rs`、`CONTEXT.md` | V64 |
| `ui/src/components/settings/ContextCompactionSettings.tsx` | 4 个新设置行 |
| `ui/src/components/files-rail/changes/FileChangesPanel.tsx` | 填充"文件改动"面板 |
| `ui/src/components/agent/AgentMessages.tsx`、`ui/src/components/chat/ChatToolBlock.tsx` | 压缩卡片文件清单、"已从上下文清理"标记 |
| `ui/src/lib/{tauri-bridge.ts,types.ts}`、`ui/src/hooks/useGlobalAgentListeners.ts` | 新命令与事件 |

---

### Task 1: 上下文设置项（后端 + 设置卡片）

**Files:**
- Modify: `src-tauri/src/memubot_config.rs`、`src-tauri/src/tauri_commands.rs`、`src-tauri/src/main.rs`
- Modify: `ui/src/components/settings/ContextCompactionSettings.tsx`（+ `.test.tsx`）、`ui/src/lib/tauri-bridge.ts`、`ui/src/lib/types.ts`

**Interfaces:**
- Produces（Rust，`memubot_config.rs`）：
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct AgentContextSettings {
      pub keep_tool_result_groups: u32,   // 默认 3，范围 1..=20
      pub tool_clear_floor_tokens: u32,   // 默认 20_000，范围 5_000..=200_000
      pub summary_target_tokens: u32,     // 默认 3_000，范围 1_000..=8_000
      pub reload_recent_files: u32,       // 默认 5，范围 0..=10
  }
  impl AgentContextSettings { pub fn clamped(self) -> Self; }
  impl Default for AgentContextSettings { /* 上述默认值 */ }
  ```
  `MemubotConfig` 新字段 `pub agent_context: AgentContextSettings`（`#[serde(default)]`）与 `pub fn set_agent_context_persisted(&mut self, s: AgentContextSettings, data_dir: &Path) -> Result<AgentContextSettings, crate::error::Error>`（先 clamp、先保存克隆、成功后才改内存，同阶段 1 阈值写法）。
- Produces（Tauri）：`get_agent_context_settings() -> Result<AgentContextSettings, String>`、`set_agent_context_settings(settings: AgentContextSettings) -> Result<AgentContextSettings, String>`（返回 clamp 后的值）。
- Produces（TS）：`interface AgentContextSettings { keepToolResultGroups: number; toolClearFloorTokens: number; summaryTargetTokens: number; reloadRecentFiles: number }`；`getAgentContextSettings()`、`setAgentContextSettings(s)`。
- 单文件重载上限 5_000 token 为常量 `RELOAD_FILE_MAX_TOKENS`（Task 7），不进设置。

- [ ] **Step 1: 写失败测试**（Rust）：`agent_context_settings_default_values`（3 / 20_000 / 3_000 / 5）；`agent_context_settings_clamps_out_of_range`（`{0, 1, 99_999, 50}` → `{1, 5_000, 8_000, 10}`）；`set_agent_context_persisted_keeps_old_value_on_save_failure`（data_dir 指向文件使保存失败，内存值不变）；旧配置 JSON 无该字段时反序列化为默认值。
- [ ] **Step 2: 运行确认失败** `cd src-tauri && cargo test --lib agent_context_settings` → 编译失败（类型不存在）。
- [ ] **Step 3: 实现** 上述类型、字段、setter、两个命令并在 `main.rs` 注册。
- [ ] **Step 4: 写失败测试**（Vitest，`ContextCompactionSettings.test.tsx`）：加载后显示 4 个输入（`aria-label`：`保留最近工具结果组数`、`批量清理下限 (K)`、`摘要目标长度 (K)`、`压缩后重载文件数`），值为 3 / 20 / 3 / 5；改"保留最近工具结果组数"为 5 并失焦 → `setAgentContextSettings` 收到 `keepToolResultGroups: 5` 且其余字段不变；后端返回 clamp 值时输入显示 clamp 后的值；保存失败 → `toast.error('保存上下文设置失败')` 并回退。
- [ ] **Step 5: 运行确认失败** `cd ui && npm test -- --run ContextCompactionSettings`。
- [ ] **Step 6: 实现** 在同一 `SettingsCard` 增加 4 个 `SettingsRow`，复用现有加载状态（`loadState`：未加载前禁用、加载失败显示重试）。K 单位字段保存时 ×1000。可把现有"加载/提交/回退"逻辑抽成本文件内的小 hook，不新建文件。每行附一句说明：
  - 保留最近工具结果组数："更早的工具结果在上下文接近阈值时会被替换为一行摘要"
  - 批量清理下限："可清理内容少于此值时不清理，避免频繁打断缓存"
  - 摘要目标长度："压缩时生成的摘要长度"
  - 压缩后重载文件数："压缩后重新放入最近改过的文件内容（单个超过 5K 只给路径）"
- [ ] **Step 7: 运行全部相关测试通过**；`npx tsc --noEmit` 仍为 41。
- [ ] **Step 8: 提交** `feat(agent,ui): agent context settings (tool-result groups, clear floor, summary length, reload files)`

---

### Task 2: V64 文件事件日志与工作集聚合

**Files:**
- Create: `src-tauri/src/agent/file_working_set.rs`（在 `agent/mod.rs` 声明）
- Modify: `src-tauri/src/db/migrations.rs`、`CONTEXT.md`、`src-tauri/src/agent/transcript.rs`（`delete_session` 级联）

**Interfaces:**
- V64 SQL：
  ```sql
  CREATE TABLE IF NOT EXISTS agent_file_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    path TEXT NOT NULL,          -- 规范化后的路径（工作区内为相对路径）
    op TEXT NOT NULL,            -- 'read' | 'write' | 'edit' | 'delete' | 'plan'
    detail TEXT NOT NULL DEFAULT '',  -- 读取范围或改动摘要，≤ 600 字符
    rewound INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_agent_file_events_session ON agent_file_events(session_id, id);
  ```
  同一迁移内为 `agent_transcript` 增加 `cleared_json TEXT` 与 `cleared_est_tokens INTEGER`（先查 `pragma_table_info`，幂等），供 Task 8 使用。
- Produces（`file_working_set.rs`）：
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum FileOp { Read, Write, Edit, Delete, Plan }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct FileEvent { pub path: String, pub op: FileOp, pub detail: String }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct ModifiedFile {
      pub path: String,
      pub created: bool,           // 首个非读事件为 write 且之前无 read
      pub deleted: bool,           // 最后一个非读事件为 delete
      pub edit_count: u32,
      pub changes: Vec<String>,    // 最近 5 条改动摘要，旧→新
      pub last_modified_at: i64,
      pub rewound: bool,           // 存在来自已回退轮次的改动
  }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct ReadFile { pub path: String, pub ranges: Vec<String>, pub last_read_at: i64 }

  #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct FileWorkingSet {
      pub modified: Vec<ModifiedFile>, // 按 last_modified_at 降序
      pub read: Vec<ReadFile>,         // 只含未被修改过的文件，按 last_read_at 降序
      pub active_plan: Option<String>, // 最近一个未回退 'plan' 事件的路径
  }

  pub fn normalize_path(raw: &str, workspace_root: Option<&Path>) -> String;
  pub fn record(conn: &Connection, session_id: &str, turn_id: Option<&str>, events: &[FileEvent]) -> anyhow::Result<()>;
  pub fn load(conn: &Connection, session_id: &str) -> anyhow::Result<FileWorkingSet>;
  pub fn render(ws: &FileWorkingSet, max_tokens: usize) -> String; // 空工作集返回 ""
  pub fn delete_session(conn: &Connection, session_id: &str) -> anyhow::Result<()>;
  ```
- `render` 格式：`<file_working_set>` 包裹；先"已修改的文件"（每个：路径、`新建`/`已删除`/`修改 N 次` 标签、`来自已回退的轮次，磁盘上仍保留` 标注、改动摘要逐行），后"读过的文件"（路径 + 范围）；超过 `max_tokens` 时先丢读过的文件尾部，再截断改动摘要，最后只留路径；被截断处写 `…另有 N 个文件`。

- [ ] **Step 1: 写失败测试**：`v64_migration_is_idempotent`（跑两次迁移不报错，三列/表均存在）；`normalizes_equivalent_paths_to_one_entry`（`src/a.rs`、`./src/a.rs`、`<root>/src/a.rs`、`src/x/../a.rs` → 一条；工作区外绝对路径保持绝对）；`load_orders_modified_first_and_excludes_modified_from_read`；`load_marks_created_and_deleted`；`load_keeps_last_five_changes`；`load_active_plan_ignores_rewound`；`render_empty_is_empty_string`；`render_truncates_read_list_first`；`delete_session_removes_file_events`（经 `transcript::delete_session`）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib file_working_set`。
- [ ] **Step 3: 实现** 迁移 V64（含 `CONTEXT.md` 登记）、模块、`transcript::delete_session` 内调用 `file_working_set::delete_session`（自动化/symphony 清理路径已复用 `transcript::delete_session`，确认之）。
- [ ] **Step 4: 运行通过**；全量 `cargo test --lib` 无新失败。
- [ ] **Step 5: 提交** `feat(agent): V64 agent_file_events + persisted file working set`

---

### Task 3: 工具执行后写入文件事件 + 查询命令与事件

**Files:**
- Create: `src-tauri/src/agent/file_events.rs`
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs`（`execute_tool_calls` 的 `for o in &outcomes` 循环，约 L1065）、`tauri_commands.rs`、`main.rs`

**Interfaces:**
- Consumes: Task 2 `FileEvent`、`record`、`load`、`normalize_path`；`ToolDispatchOutcome`（`tool_dispatch/mod.rs:88`：`tool_name`、`arguments`、`paths_touched`、`is_error`、`rejected`）。
- Produces：
  ```rust
  pub fn file_events_from_outcome(o: &ToolDispatchOutcome, workspace_root: Option<&Path>) -> Vec<FileEvent>;
  ```
  规则（失败或被拒绝的调用不产生事件）：
  - `read_file` → `Read`，detail = `第 {offset+1}–{offset+limit} 行` 或 `全文`。
  - `write_file` → `Write`，detail = `写入 {行数} 行`。
  - `edit` → 对 `paths_touched` 中每个文件一条 `Edit`，detail = 每个 edit 的 `old`→`new` 首行各截 80 字符，形如 `「old…」→「new…」`，多处以 `；` 连接，总长 ≤ 600 字符（兼容单文件 `{path, edits}` 与批量 `files:[{path, edits}]`）。
  - `plan_write` / `plan_update` → `Plan`，path = 计划文件（`.uclaw/plans/<filename>`），detail = `创建计划` / `更新计划`。
  - `bash` → 仅识别命令中简单的 `rm <paths>`（→ `Delete`）与 `mv <a> <b>`（→ `Delete a` + `Write b`，detail `由 a 移动而来`）；其他命令不产生事件。
  - 其余工具：若 `was_mutation` 且有 `paths_touched` → 每个路径一条 `Edit`，detail = 工具名；否则无事件。
- `ChatDelegate`：transcript 模式下（有 `transcript_sink`）在 outcomes 循环后调用 `record` 一次（同一锁内），然后 emit `agent:file_working_set_changed { sessionId }`（仅当本批有事件）。保留现有 `reason_ctx.file_ops.track_tool_call` 不动（Task 7 改用工作集后再删其用途）。
- Tauri：`get_session_file_working_set(session_id: String) -> Result<FileWorkingSet, String>`。

- [ ] **Step 1: 写失败测试**（`file_events.rs`）：`read_file_event_records_range`；`write_file_event_counts_lines`；`edit_single_and_batch_forms_record_each_file`（批量 `files:[…]` 两个文件 → 两条）；`edit_detail_is_truncated_to_600_chars`；`plan_tools_record_plan_event`；`bash_rm_and_mv_are_recognised_other_commands_ignored`；`failed_or_rejected_calls_record_nothing`。turn_runner 集成测试：`transcript_mode_records_file_events_after_tool_batch`（用现有 sink 测试夹具执行一个 `write_file` 结果 → `file_working_set::load` 有该文件）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib file_events`。
- [ ] **Step 3: 实现** 模块、接线、命令（两处注册）、事件。
- [ ] **Step 4: 运行通过**；全量无新失败。
- [ ] **Step 5: 提交** `feat(agent): record file events from tool outcomes; get_session_file_working_set`

---

### Task 4: 回退标记与分叉复制

**Files:**
- Modify: `src-tauri/src/agent/transcript_branch.rs`（`rewind_with_transcript` L54、`fork_with_transcript` L93）、`src-tauri/src/agent/file_working_set.rs`

**Interfaces:**
- Produces（`file_working_set.rs`）：
  ```rust
  pub fn mark_rewound_after(tx: &Connection, session_id: &str, kept_turn_ids: &HashSet<String>) -> anyhow::Result<usize>;
  pub fn clone_for_fork(tx: &Connection, src: &str, dst: &str, turn_id_map: &HashMap<String, String>) -> anyhow::Result<usize>;
  ```
  - 回退：在回退事务提交前，把 `turn_id` 不在保留集合内的事件标 `rewound = 1`（不删除；`turn_id IS NULL` 的事件不动）。
  - 分叉：复制源会话中 `turn_id` 在映射内（含其 `rewound` 标记）的事件到新会话，`turn_id` 按 `message_id_map` 映射；不在映射内的事件（分叉点之后）**也复制**并标 `rewound = 1`（磁盘改动仍在，§6.9）。
- 回退/分叉成功后 emit `agent:file_working_set_changed`（调用方 `tauri_commands.rs` 约 L10935/L10981）。

- [ ] **Step 1: 写失败测试** `rewind_marks_events_and_fork_copies_marks`：t1 改 a、t2 改 b、t3 改 c → 回退到 t2 之后 → `load` 中 c 的 `rewound == true`，a/b 为 false；从 t2 分叉 → 新会话 a/b 未标、c 标记；再在新会话改 c → c 的 `rewound` 仍为 true（存在被回退的改动），`edit_count` 累加。另测回退事务失败时事件不被标记（事务一致性）。
- [ ] **Step 2: 运行确认失败**。
- [ ] **Step 3: 实现**。
- [ ] **Step 4: 运行通过**（含阶段 1 的 `transcript_branch` 全部测试）。
- [ ] **Step 5: 提交** `feat(agent): rewind marks and fork copies file events`

---

### Task 5: "文件改动"面板（前端）

**Files:**
- Modify: `ui/src/components/files-rail/changes/FileChangesPanel.tsx`（+ 新测试 `FileChangesPanel.test.tsx`）、`ui/src/lib/tauri-bridge.ts`、`ui/src/lib/types.ts`

**Interfaces:**
- Consumes: Task 3 命令 `get_session_file_working_set` 与事件 `agent:file_working_set_changed`；`agentMessageRefreshAtom`；当前会话 id atom；`ChangeRow`（`badge: 'created'|'modified'|'removed'|'renamed'`）；`openPreviewTabAction`。
- Produces（TS）：`FileWorkingSet`、`ModifiedFile`、`ReadFile` 类型（camelCase，与 Rust 一致）；`getSessionFileWorkingSet(sessionId)`；`onFileWorkingSetChanged(cb)`。

行为：
- 无会话或工作集为空 → 保留现有空状态"这个会话还没有文件改动"。
- "已修改"区（标题 `已修改 · N`）：每行 `ChangeRow`（`created`→新建、`deleted`→removed、否则 modified），右侧 `修改 N 次`；`rewound` 行加灰色标签 `已回退的轮次 · 磁盘仍保留`；点击行展开改动摘要列表（最近 5 条）；点击路径打开预览（已删除文件不可点）。
- "读过"区（标题 `读过 · N`，默认折叠）：路径 + 范围。
- "当前计划"：`activePlan` 存在时在顶部显示一行 `当前计划：<文件名>`，点击打开预览。
- 刷新：挂载时、`agent:file_working_set_changed`（sessionId 匹配）、`agentMessageRefreshAtom` 该会话版本变化时重新拉取。

- [ ] **Step 1: 写失败测试**：空状态；修改区排序与标签（新建/已删除/修改 N 次/已回退标签）；读过区默认折叠、点击展开；收到匹配 sessionId 的事件后重新调用 `getSessionFileWorkingSet`，不匹配时不调用；已删除文件路径不可点击。
- [ ] **Step 2: 运行确认失败** `npm test -- --run FileChangesPanel`。
- [ ] **Step 3: 实现**。
- [ ] **Step 4: 运行通过**；`tsc` 仍 41。
- [ ] **Step 5: 提交** `feat(ui): session file changes panel backed by the persisted working set`

---

### Task 6: 结构化摘要模板 + 复用前缀的摘要调用

**Files:**
- Create: `src-tauri/src/agent/compact/prefix_summary.rs`
- Modify: `src-tauri/src/agent/compact/{fold.rs,summarize.rs,render.rs}`、`src-tauri/src/agent/types.rs`（`LoopDelegate`）、`src-tauri/src/agent/dispatcher/turn_runner.rs`（`call_llm` 请求构造抽取，约 L632）、`src-tauri/src/agent/transcript_compaction.rs`、`src-tauri/src/agent/agentic_loop.rs`（两个调用点传入 snapshot）

**Interfaces:**
- `StructuredFold` 新增（均 `#[serde(default)]`）：`user_requests: Vec<String>`（全部用户消息的要求、决定、纠正，短句保留原话）、`current_state: Option<String>`（当前进展与下一步之前的状态）；`FailedAttempt` 新增 `resolution: Option<String>`。`render.rs` 渲染顺序固定：用户的要求与决定 → 关键决定 → 错误与解决 → 未完成事项 → 当前进展 → 下一步 → 其余轴。标题 `SESSION_BLOCK_HEADER` 不变。
- `summarize.rs`：目标长度由参数传入（来自 Task 1 `summary_target_tokens`），`SUMMARIZER_MAX_TOKENS` 改为 `target * 2`（下限 2048）；JSON schema 显式列出新字段（并移除"merge file_ops"的指示——文件信息由工作集提供）。
- `turn_runner.rs`：把 `call_llm` 内"由 `ReasoningContext` + `TurnSnapshot` 构造实际发送的消息列表"（动态块前置、跳过 `compacted`、截图剥离）抽成
  ```rust
  pub(crate) fn build_request_messages(reason_ctx: &ReasoningContext, snapshot: &TurnSnapshot, model_supports_images: bool) -> Vec<ChatMessage>;
  ```
  `call_llm` 改为调用它（行为不变，已有测试兜底）。
- `LoopDelegate` 新增默认方法：
  ```rust
  async fn summarize_with_prefix(&self, _reason_ctx: &ReasoningContext, _snapshot: &TurnSnapshot, _cut: usize, _target_tokens: usize) -> Option<StructuredFold> { None }
  ```
  `ChatDelegate` 实现：请求 = 与 `call_llm` **相同**的 tools、system、`build_request_messages(...)`，末尾追加一条用户消息（`prefix_summary::instruction(first_kept_preview, target_tokens)`）：要求只总结"在以「{first_kept_preview}」开头的那条用户消息之前"的内容（`first_kept_preview` = `messages[cut]` 文本前 80 字符；`cut == len` 时总结全部）、不要调用工具、只输出 JSON。解析用 `parse_fold_from_text`；返回含工具调用、空文本或 JSON 解析失败 → `None`。记录日志：`prefix_summary cache_read={} input={} output={} ms={}`（§6.10）。
- `transcript_compact` 新签名：
  ```rust
  pub(crate) async fn transcript_compact(delegate: &dyn LoopDelegate, reason_ctx: &mut ReasoningContext, snapshot: Option<&TurnSnapshot>, threshold: usize, trigger: CompactionTrigger, settings: &AgentContextSettings) -> anyhow::Result<bool>;
  ```
  设置来源：`AgenticLoopConfig` 新增 `pub context_settings: AgentContextSettings`（`Default`），`send_agent_message` 从 `MemubotConfig.agent_context` 填入；循环内经 `reason_ctx`/config 传给 `transcript_compact` 与 Task 8 的清理。`run_manual_compact` 读同一配置。
  顺序：有 snapshot → `summarize_with_prefix`；`None` → 现有 `update_fold_incremental` / `summarize_to_fold`（传入目标长度）→ `extractive_fallback_fold`。自动压缩两个调用点（`run_agentic_loop` 的 `snap`、`run_turn_body` 溢出重试的 `snapshot`）传 `Some`；手动 `/compact`（`run_manual_compact`）传 `None`（它没有真实 system/tools，阶段 3 冻结系统提示词后再接入——在代码注释中写明）。

- [ ] **Step 1: 写失败测试**：`fold_roundtrips_new_fields_and_old_json_still_parses`；`render_orders_user_requests_first`；`summarize_prompt_lists_new_fields_and_target_length`；`build_request_messages_matches_call_llm_input`（同一 ctx/snapshot，抽取前后构造结果一致——先在抽取前用现有 `call_llm` 的捕获型测试 delegate 录下请求作为基准）；`prefix_summary_request_is_prefix_plus_instruction`（捕获请求：除最后一条外与主请求逐字节相同，tools/system 相同）；`prefix_summary_falls_back_on_tool_call_or_bad_json`（三种返回各自导致回退到独立摘要器，压缩仍完成）；`manual_compact_uses_standalone_summarizer`。
- [ ] **Step 2: 运行确认失败** `cargo test --lib prefix_summary fold render summarize`。
- [ ] **Step 3: 实现**。
- [ ] **Step 4: 运行通过**；全量无新失败。
- [ ] **Step 5: 提交** `feat(agent): structured compaction template and prefix-reusing summary call`

---

### Task 7: 压缩后重载（工作集 + 计划 + 最近文件）

**Files:**
- Create: `src-tauri/src/agent/session_block.rs`
- Modify: `src-tauri/src/agent/transcript_compaction.rs`（session_block 组装处，约 L61）、`src-tauri/src/agent/types.rs`（`LoopDelegate` 新默认方法）、`turn_runner.rs`、`agent/plan_state.rs`

**Interfaces:**
- Consumes: Task 2 `load`/`render`；Task 1 `reload_recent_files`；Task 6 新 `transcript_compact` 签名。
- Produces：
  ```rust
  pub const RELOAD_FILE_MAX_TOKENS: usize = 5_000;
  pub enum ReloadedFile { Content { path: String, text: String }, PathOnly { path: String, reason: &'static str } } // reason: "已删除" | "二进制文件" | "超过 5K token"
  pub fn reload_recent_files(ws: &FileWorkingSet, workspace_root: Option<&Path>, max_files: usize) -> Vec<ReloadedFile>;
  pub fn build_session_block(fold_markdown: &str, working_set_md: &str, plan: Option<(&str, &str)>, files: &[ReloadedFile], budget_tokens: usize) -> String;
  ```
  `plan_state.rs` 新增 `pub fn read_plan(workspace_root: Option<&Path>, rel_path: &str) -> Option<String>`（超过 5K token 返回 `None`，由调用方只给路径）。
- `LoopDelegate` 新增默认方法 `fn session_block_context(&self) -> Option<SessionBlockContext> { None }`，其中 `pub struct SessionBlockContext { pub working_set: FileWorkingSet, pub workspace_root: Option<PathBuf> }`；`ChatDelegate` 从 sink 的 db 读取 `file_working_set::load`。
- 会话固定块 = `fold.to_markdown()`（不再附 `file_ops`）+ `\n\n` + `render(ws, …)` + `<current_plan path="…">…</current_plan>`（或只给路径）+ `<recent_files>` 中每个文件 `<file path="…">内容</file>` 或 `<file path="…" omitted="原因"/>`。`budget_tokens = min(25_000, threshold / 4)`；超预算按"最近文件 → 计划正文 → 工作集"顺序裁剪。只修改过、未读取的文件也可重载；`deleted` 的不读盘。
- 每次压缩记录一行日志（§6.10）：`transcript_compact trigger={} cut={} summary_tokens={} block_tokens={} reloaded_files={} ms={} prefix_summary={}`。
- `StructuredFold.with_file_ops` 在 transcript 路径不再调用；`reason_ctx.file_ops` 保留给非 transcript（legacy）路径。

- [ ] **Step 1: 写失败测试**：`reload_skips_deleted_binary_and_oversized_files`；`reload_takes_most_recent_modified_up_to_max`（max=0 时为空）；`session_block_contains_summary_working_set_plan_and_files`；`session_block_respects_budget_trim_order`；`manual_compact_block_includes_session_working_set`（`/compact` 也能拿到 DB 中的工作集——修复阶段 1 遗留的"手动压缩文件记录为空"）；端到端：`compaction_block_lists_files_modified_in_earlier_turns`（t1 改 a、t2 改 b，t5 触发压缩 → 块内同时出现 a 与 b，而不仅是本轮）。
- [ ] **Step 2: 运行确认失败**。
- [ ] **Step 3: 实现**。
- [ ] **Step 4: 运行通过**；全量无新失败。
- [ ] **Step 5: 提交** `feat(agent): session block reloads file working set, plan and recent files`

---

### Task 8: 旧工具结果批量清理

**Files:**
- Create: `src-tauri/src/agent/tool_result_clearing.rs`
- Modify: `src-tauri/src/agent/transcript.rs`（覆盖层列）、`src-tauri/src/agent/agentic_loop.rs`（`apply_transcript_compaction_decision` L902 与溢出重试 L78–99）、`src-tauri/src/agent/types.rs`（`LoopDelegate`）、`turn_runner.rs`

**Interfaces:**
- Produces（纯函数）：
  ```rust
  pub const CLEARED_PREFIX: &str = "[已清理的工具结果：";
  pub struct ClearPlan { pub indices: Vec<usize>, pub tool_call_ids: Vec<String>, pub freed_tokens: usize }
  pub fn plan_clearing(msgs: &[ChatMessage], persisted_len: usize, keep_groups: usize, floor_tokens: usize) -> Option<ClearPlan>;
  pub fn placeholder(tool_name: &str, args: &serde_json::Value, original_tokens: usize) -> String;
  pub fn apply_clearing(msgs: &mut [ChatMessage], plan: &ClearPlan);
  ```
  - 一"组" = 一条含工具调用的助手消息及其全部结果。保留最近 `keep_groups` 组；只考虑 `< persisted_len` 的消息（本迭代未落库的不动）；跳过已以 `CLEARED_PREFIX` 开头的结果；可清理总量 `< floor_tokens` → `None`。
  - 占位：`[已清理的工具结果：{工具名} {关键参数}（约 {n}K token）——如需内容请重新读取]`。关键参数：`read_file` → `path 第 a–b 行`；`grep`/`glob` → `"pattern" 于 path`；`bash` → 命令前 60 字符；其他 → 首个字符串参数前 60 字符。
- `transcript.rs`：
  ```rust
  pub fn set_cleared(conn: &Connection, session_id: &str, cleared: &[(usize /*active index*/, ChatMessage)]) -> anyhow::Result<()>;
  ```
  写 `cleared_json` / `cleared_est_tokens`；`load_active` 有覆盖层时返回覆盖版本与其 token 数；`message_json` 不变；`clone_through_turn_within`（分叉）复制覆盖层；回退不撤销覆盖层（保留下来的旧结果仍为占位，模型可重新读取——在模块文档中写明）。
- `LoopDelegate` 新增默认方法 `async fn persist_cleared(&self, _cleared: &[(usize, ChatMessage)], _tool_call_ids: &[String], _freed_tokens: usize) -> anyhow::Result<()> { Ok(()) }`；`ChatDelegate` 调 `set_cleared` 并 emit `agent:tool_results_cleared { sessionId, toolCallIds, freedTokens }`。
- 接线：在 `apply_transcript_compaction_decision` 中，`Compact` / `HardTruncate` 决策时**先**尝试清理（读 `AgentContextSettings`），清理成功则重新估算；低于阈值 → 返回 `true`（让调用方重建 snapshot）且不压缩；否则继续压缩。溢出重试路径同样先清理再压缩。
- Tauri：`get_cleared_tool_call_ids(session_id: String) -> Result<Vec<String>, String>`（供前端重载后恢复标记）。

- [ ] **Step 1: 写失败测试**：`clearing_respects_recent_groups_floor_and_idempotence`；`placeholder_formats_read_grep_bash`；`clearing_never_touches_unpersisted_messages`；`set_cleared_overlays_without_touching_message_json`（直接查 `message_json` 不变）；`fork_copies_cleared_overlay`；`clearing_avoids_compaction_when_enough_is_freed`（循环测试：估算超阈值，清理后低于阈值 → 无 session_block 写入、`persist_cleared` 被调用一次）；`clearing_then_compaction_when_still_over`；`cleared_history_is_stable_prefix_next_turn`（清理后下一轮 `build_agent_history` 的前缀与清理当轮相同）。
- [ ] **Step 2: 运行确认失败**。
- [ ] **Step 3: 实现**（含命令两处注册）。
- [ ] **Step 4: 运行通过**；全量无新失败。
- [ ] **Step 5: 提交** `feat(agent): batch-clear old tool results via transcript overlay before compacting`

---

### Task 9: 消息流同步（压缩卡片文件清单 + "已从上下文清理"标记）

**Files:**
- Modify: `ui/src/components/agent/AgentMessages.tsx`（`CompactionFoldCard` L321–407）、`ui/src/components/chat/ChatToolBlock.tsx`、`ui/src/hooks/useGlobalAgentListeners.ts`、`ui/src/atoms/agent-atoms.ts`、`ui/src/lib/{tauri-bridge.ts,types.ts}`（+ 对应测试）

**Interfaces:**
- Consumes: Task 7 块格式（`<file_working_set>`、`<current_plan>`、`<recent_files>`）；Task 8 事件 `agent:tool_results_cleared` 与命令 `get_cleared_tool_call_ids`。
- Produces（TS）：`ToolResultsClearedEvent { sessionId: string; toolCallIds: string[]; freedTokens: number }`；`onToolResultsCleared(cb)`；`getClearedToolCallIds(sessionId)`；atom `clearedToolCallIdsMapAtom: Map<string /*sessionId*/, Set<string>>`；纯函数 `parseSessionBlock(markdown) -> { summary: string; modifiedFiles: string[]; readCount: number; plan?: string; reloadedFiles: string[] }`。

行为：
- `CompactionFoldCard`：折叠态文案改为 `上下文已压缩 · 保留了 N 个改动文件的记录`（无文件时保持现有分项文案）；展开态标题改为 `上下文摘要`（去掉 "M2-G StructuredFold"）；摘要正文下方新增"改动过的文件"芯片列表（点击打开预览）、`当前计划：…`、`已重新载入：a、b…`；原始 XML 标签不直接显示给用户。
- 工具卡片：工具调用 id 在 `clearedToolCallIdsMapAtom[sessionId]` 中，或结果文本以 `[已清理的工具结果：` 开头 → 标题右侧显示灰色标签 `已从上下文清理`（tooltip：`为节省上下文，模型侧已用一行摘要替代；此处仍保留原文`），仍可展开查看原文。
- `useGlobalAgentListeners`：收到 `agent:tool_results_cleared` → 合并到 atom；当前会话时 `toast.info('已清理较早的工具结果，释放约 {K}K token')`。`AgentView` 加载会话消息时调用 `getClearedToolCallIds` 初始化该会话的集合。

- [ ] **Step 1: 写失败测试**：`parseSessionBlock` 解析三类标签与无标签旧块（旧块返回空列表，不抛错）；卡片折叠/展开文案与文件芯片；工具卡片在 id 命中时显示标签且仍可展开；事件处理更新 atom、仅当前会话 toast。
- [ ] **Step 2: 运行确认失败**。
- [ ] **Step 3: 实现**（工具 id 在 `NativeBlockRenderer` 与 `ChatToolActivityIndicator` 两条渲染路径都要传到 `ChatToolBlock`；若某路径拿不到 id，至少按结果前缀识别，并在报告中注明）。
- [ ] **Step 4: 运行通过**；`tsc` 仍 41。
- [ ] **Step 5: 提交** `feat(ui): compaction card lists kept files; mark tool results cleared from context`

---

### Task 10: 验证与交付

- [ ] **Step 1: 自动化** `cargo build`（0 错误）、`cargo test --lib`（仅已知失败）、`npm test -- --run`（仅已知失败）、`npx tsc --noEmit`（41）。
- [ ] **Step 2: 回归基准**（用户执行）`scripts/bench/` 7 条消息：首 token 中位数 ≲ 1.14 s、每次调用输入中位数 ≲ 27K。
- [ ] **Step 3: §7.A 主验收**（用户执行，阈值设为 32K）：前期修改 ≥ 5 个文件（跨目录，含新建与删除），中间大量读取使上下文超阈值，至少一次压缩；问"这次会话改过哪些文件、各改了什么"→ 无遗漏、描述准确。变体：重启后再问；两次以上压缩后再问；同时问一条早期用户要求。界面检查："文件改动"面板与回答一致、压缩卡片列出文件、被清理的工具卡片带标记。
- [ ] **Step 4:** 推送、开 PR（含 `## Commits (bisectable)` 表），CI 通过后合并（需用户确认）。
