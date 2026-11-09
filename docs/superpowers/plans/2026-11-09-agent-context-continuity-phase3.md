# Agent 上下文延续 — 阶段 3（冻结系统提示词 + 动态块持久化）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交互式 Agent 会话的请求变成"只追加"：系统提示词按会话冻结并持久化，每轮动态块随用户消息写入转录并原样重放，记忆只注入新增部分，召回查询带上近期上下文并使用会话真实 space，skills 清单限量且会话内不变、本轮相关 skill 进动态块；前端能看到"提示词已冻结 / 缓存命中率 / 工具集或模式变化"。

**Architecture:** 新增按会话持久化的**提示词框架**（V67 `agent_session_frames`：冻结的系统提示词 + 已公布的工具名 + 规则源哈希），由 `agent/session_frame.rs` 管理；`create_turn_snapshot` 在 transcript 模式直接使用它。每轮动态块由纯模块 `agent/turn_context.rs` 渲染为 `<turn_context>…</turn_context>`，在本轮第一次迭代前拼进本轮用户消息（内存）并写入 `agent_transcript.dynamic_prefix` 覆盖层列（V67，`message_json` 仍永不改写）；`load_active` 读出时合并，因此重放、重启、压缩、分叉看到的字节完全一致。"哪些内容模型已经看过"（记忆行、粘性块、最近一次模式/工具说明）全部从活动转录行的 `dynamic_prefix` 推导（`TurnContextLedger`），不另存状态。

**Tech Stack:** Rust（Tauri、rusqlite、tokio、sha2）、React + Jotai + Vitest。

**Spec:** `docs/superpowers/specs/2026-11-09-agent-context-continuity-design.md`（§6.1、§6.6、§6.9"会话中途的变化"、§6.10、§7.C；§8 第 3 行）。阶段 1（#688）、阶段 2（#692）已合并；T3 前置工作 #690 / #691 已合并（见审计）。

---

## 审计（main @ c67100ee，逐项核实）

### A. 每轮进入 `[system]` 的输入（解释 system_prompt_tokens 14336 → 15360 → 16384）

系统提示词在 `create_turn_snapshot`（`agent/dispatcher/turn_runner.rs:572-669`）里**每次迭代**重新组装（`assemble_prompt` → `content_assembler.rs:374` → `assemble_system_prompt` `:79-125`）。所有输入：

| # | 输入 | 位置 | 会话内是否会变 |
|---|---|---|---|
| S1 | 用户提示词模板（`prompt_id` → `resolve_user_system_prompt`） | `tauri_commands.rs:9814` | 换模板/编辑模板时变 |
| S2 | `<workspace>/uclaw.md`，**每次组装都读盘** | `mode_prompts.rs:63-69,180` | 文件改动即变 |
| S3 | `[WORKSPACE]` 路径块 | `mode_prompts.rs:198-209` | 稳定 |
| S4 | persona 块，**每次组装都查库** | `content_assembler.rs:408-427` | 全局语音档案改动即变 |
| S5 | baseline（`render_with_context(InjectionContext)`，含 `context_pressure_ratio`、`last_error_kind`、`is_first_act_turn`） | `content_assembler.rs:338-342`、`baseline_blocks.rs:63-75` | 生产 block 全是 `Always`（`baseline_blocks.rs` 只有测试 block 覆盖 policy），**目前**不变；是潜在变化源 |
| S6 | 模式附加段（每次迭代 `resolve_effective_mode`） | `turn_runner.rs:584`、`mode_prompts.rs:49-57` | 切模式即变（含轮内切换） |
| S7 | skills 清单：`format_for_system_prompt_xml()`，**不限量**，按名称排序的完整 XML | `tauri_commands.rs:10277-10278`、`skills.rs:710-754` | 装/卸 skill 即变；**用过 `skill_search` 后整块被隐藏**（`turn_runner.rs:983`、`content_assembler.rs:89,345`）→ 轮内突然少 ~9.5K |
| S8 | GEP gene 块：按最后一条用户文本 + 本轮工具错误匹配，每次迭代重算 | `content_assembler.rs:265-289` | 随消息/工具错误变 |
| S9 | plan-suggest 提示：7 天滑动窗口统计 | `content_assembler.rs:291-318` | 随统计翻转 |
| S10 | 项目规则：`.uclawrules` glob 对**全局** `GLOBAL_FILE_CONTEXT_TRACKER`（`HashSet`，迭代顺序不定，跨会话共享）匹配 + `AGENTS.md`/`.cursorrules` 段落抽取，每次读盘 | `content_assembler.rs:320-328`、`rule_context_builder.rs:24-89`、`anchor_state.rs:284,343` | 任何会话碰到新文件即变；顺序不确定 |
| S11 | `align_to_1024_tokens` 填充 | `content_assembler.rs:112-120` | 把以上变化量化到 1024 的整数倍 |

**测算**：日志中的值全是 1024 的整数倍（14×/15×/16×1024），即 S11 的量化结果；真实变化量来自 S7/S8/S9/S10/S2。今天日志 `~/.uclaw/logs/uclaw.log.2026-11-09`：同一会话 `tool_count=34` 时 `system_prompt_tokens` 在 15360/16384 之间来回，说明变化不是单调增长（S8 gene 块出现/消失、S10 规则集合变化最符合）。按 `~/.uclaw` 下 132 个 `SKILL.md` 估算，S7 完整 XML 约 38K 字符 ≈ **9.5K token，占系统提示词约 60%**。工具定义从 7989 跳到 11759（34 → 59 个工具）来自 `registry_build.rs:138-183`：会话出现浏览器上下文后一次性多注册 25 个浏览器工具；上下文关闭后又会消失（第二次失效）。

### B. 阶段 3 各项现状

| 项 | main 上已有 | 缺口 | 本计划 |
|---|---|---|---|
| 冻结系统提示词（§6.1、§6.9） | 无；每次迭代重组（见 A） | 全部 | Task 2–4 |
| gene / 规则 / plan 提示移出 `[system]` | 无（`content_assembler.rs:93-111` 仍拼进 system） | 全部 | Task 4、5 |
| 动态块持久化与原样重放（§6.6、D8） | 动态块只拼进请求副本（`turn_runner.rs:539-561`），注释明确"persisted session messages stay clean"；每次迭代重算（时间按分钟变，轮内也会断缓存）；`<rewound_file_changes>`（#692，`turn_runner.rs:610-616,1697-1730`）**每轮重复注入** | 全部 | Task 1、5、6 |
| 记忆差量注入（§6.6） | M2-D 只**记录**漂移（`content_assembler.rs:154-216`，日志 `[M2-D] … emitted=full cache_state=miss added=..`），始终注入完整块（实测首轮 5.4–5.7K）；`<memory_context_changes>` 注释已停用 | 差量注入本身 | Task 5（复用 `context_diff` 的行切分思想，账本改由转录推导） |
| 召回查询改进 + space（D10） | 图召回 `recall_space_id = "default"`（`tauri_commands.rs:9875`）；查询只用本轮原文（`:9877`、bucket_seal `:10103`）；会话 space 已算好但只给提取器用（`:9803-9807`） | 全部 | Task 8 |
| T3 限量清单 | **未做**：#690（skill 全文）/#691 不碰清单；agent 路径（`tauri_commands.rs:10277`）与另一调用点（`:1031`）都是不限量 XML（S7）。`skills_manifest::build_skills_manifest` 有上限参数但生产路径未调用，且 builtin 段按名称排序、header 写的是"已学习的技能" | 限量、确定性、会话内冻结 | Task 3 |
| T3 `<relevant_skills>` | **部分**：#691 `agent/relevant_skills.rs` 从 bucket_seal 召回中拆出已学习 skill，渲染 `<relevant_learned_skills>` 进动态块（`content_assembler.rs:224-231`）；不含内置/插件 skill 的词法匹配 | 内置/插件 skill 的词法匹配、统一块名 | Task 9 |
| 中途变化（§6.9） | 工具列表变化只打 warn（`turn_runner.rs:648-658`） | 冻结 + 模式/工具说明 + 模板切换重建 | Task 2、4、5、10 |
| 可观测性（§6.10） | `cost_store::record`（`cost_store.rs:14`）不写 `cached_input_tokens`（V49 列恒 0）；`agent:turn_cost` 已带 `cacheReadTokens`；`context_stats.system_prompt_tokens` 统计的是**基础**提示词而非实际发送的 | 写缓存字段、按会话命中率、框架信息 | Task 11–13 |
| §7.C 字节一致性测试 | 无 | 全部 | Task 7 |

### C. 已完成、不再列任务的

- 工具定义确定性排序：`ToolRegistry::list_definitions` 已按名称排序（`agent/tools/tool.rs:502-514`）。
- 前一轮推理块不重放：阶段 1 `without_thinking`（`transcript_history.rs:131-144`）。
- 模型切换后阈值重校验：阶段 1 `effective_compaction_threshold(configured, window)` 每轮按当轮模型窗口计算（`compaction_policy.rs:17`）。
- #693/#695 只涉及 fold 宽松约束与压缩抖动，与本阶段无交集。

---

## Decisions（规格未定、由本计划决定）

1. **动态块存为覆盖层列 `agent_transcript.dynamic_prefix`**，不改 `message_json`。原因：用户行在召回之前就已在事务里写入（`tauri_commands.rs:9646-9659`），且阶段 1 的"`message_json` 永不改写"不变量要保留。
2. **动态块每轮只算一次**（第一次迭代的 `before_llm_call`），本轮所有迭代共用。原因：轮内前缀也稳定；代价是轮内的模式切换、本轮新碰到的文件所匹配的规则在下一轮才出现。
3. **线上格式不变**：仍是同一个 Text 块 `"{prefix}\n\n{user}"`，`prefix` 以 `<turn_context>` 开头、`</turn_context>` 结尾；凡读"用户原话"的地方改用 `turn_context::strip`。原因：与当前请求形状相同，零服务商改动。
4. **"模型已看过什么"由活动转录行推导**（`TurnContextLedger::from_prefixes`），不另建状态。原因：重启、压缩（被替代的行不再活动）、回退、分叉自动正确。
5. **记忆完整注入的时机** = 活动上下文里没有任何记忆行时（会话首轮、或压缩/回退把它们移出了活动上下文）；否则只注入新增行（`<memory_update>`）。不渲染"已过时"条目：召回没命中不代表过时。
6. **框架键** = `FRAME_VERSION` + sha256(解析后的模板文本) + 工作区路径 + sha256(编译期静态文本：baseline + 模式文案)。只有这些变化才重建；模式、persona、skills 注册表、规则文件、模型变化**不重建**。
7. **模型切换不重建框架**：缓存按模型隔离，阈值已按当轮窗口重算（阶段 1）。
8. **persona 随框架冻结**：改了语音档案在新会话或模板切换时生效。
9. **规则拆两半**：`uclaw.md` + `AGENTS.md`/`.cursorrules` 段落（与文件无关）进框架；`.uclawrules` glob 规则改用**本会话**文件工作集（V64 `agent_file_events`）匹配，每条规则只在动态块出现一次（`<project_rule id=…>`）。规则源文件哈希变化时，下一轮动态块带一次 `<project_rules_update>`（新的完整静态规则文本，声明取代系统提示词中的同名部分）。原因：去掉跨会话共享的全局追踪器与不确定顺序。
10. **transcript 模式不再做 1024 对齐填充**：冻结后前缀天然稳定，填充平均多花 ~512 token/次。
11. **框架用 `InjectionContext::baseline()`**：生产 block 全为 `Always`，行为不变。
12. **限量清单**：`SYSTEM_PROMPT_MANIFEST_MAX_TOKENS = 1500`、`_MAX_ENTRIES = 200`；排序 = Bundled（按名称）→ 用户安装的 User/Project/Marketplace/Plugin（按名称）→ 已学习且 promoted（E3 顺序）；末尾固定一句"共 N 个技能，此处列出 M 个；其余用 `skill_search` 查找、`load_skill` 加载"。只在创建框架时构建一次（`bump_manifest_appearance` 也只在那时调用）。
13. **transcript 模式取消 `skill_search` 隐藏清单**（框架不可变）；非 transcript 路径保持原行为。
14. **浏览器工具一旦在本会话公布就保持注册**（框架 `tool_names` 含 `browser_go_back` 即视为已公布），避免关闭上下文后第二次失效；无上下文时这些工具返回工具错误。
15. **MCP 等其他工具集合变化**：接受一次失效，并在下一轮动态块写 `<tools_update>`（新增/移除的工具名），同时通知前端。
16. **召回**：查询 = 本轮原文 + 最近 2 条用户消息（取自 `agent_messages`，天然不含 `<turn_context>`，每条截 200 字，总长 ≤ 1200 字）；图召回在会话 space 与 `"default"` 上各跑一次并按 `node_id` 去重（会话 space 优先，二者相同时只跑一次）。bucket_seal 召回用同一查询。
17. `skill_search` / `load_skill` 工具的 space 仍为 `"default"`（已学习 skill 在那里；`split_skill_entries` 已同时接受会话 space 与 default）。
18. **缓存命中率口径**：Anthropic 家族（模型名以 `claude` 开头）`input_tokens` 不含缓存 → 命中率 = cached /(input + cached)；其余（OpenAI 兼容，含 DeepSeek）`prompt_tokens` 已含缓存 → cached / input。
19. **GEP**：每轮匹配一次（本轮用户原文 + 上一轮留下的 `recent_tool_errors`），`gep.last_matches` 副作用保留；渲染为 `<gene_signals>`。
20. **`<system_info>` 只保留当前时间**：说明性文字（"不要用 bash date…"）移入框架，工作区路径已在 `[WORKSPACE]`。
21. **粘性块只在变化时出现**：`learned_profile`、`plan_suggest`、`rewound_file_changes`、`context_fragment`（按 id）、`project_rule`（按 id）、`project_rules_update`；每轮都出现的只有 `system_info`、`gene_signals`、`relevant_skills`、`memory_*`、`mode_update`/`tools_update`（仅变化时）。
22. headless / symphony / teams / regular_task 等非 transcript 调用方完全走旧路径（`assemble_system_prompt` 不删），只做回归。

---

## Global Constraints

- 工作目录：worktree `.claude/worktrees/ctx-p3`，分支 `feat/agent-context-continuity-p3`（基于 main c67100ee）。不推送，直到最终任务。
- 新行为仅在 transcript 模式（交互式 `send_agent_message`，`AgenticLoopConfig.transcript_mode == true`，`ChatDelegate.transcript_sink.is_some()`）生效；其他调用方行为不变（只需回归测试）。
- 迁移号 **V67**（V65 已被 #696 memory growth、V66 已被 #699 memory_core BM25 索引占用；开工前 `gh pr list --state open` 与 `CONTEXT.md` 迁移登记表确认无冲突，并登记 V67）。遵循 `uclaw-migrations` skill：`ALTER TABLE … ADD COLUMN` 先查 `pragma_table_info`，整段幂等。
- `agent_transcript.message_json` 永不改写；`dynamic_prefix` 每行最多写一次（已有值则不覆盖）。
- 动态块包裹标记固定为 `<turn_context>` / `</turn_context>`（前端与压缩提示词据此识别）。
- 常量原样使用：`SYSTEM_PROMPT_MANIFEST_MAX_TOKENS = 1500`、`SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES = 200`、`FRAME_VERSION = 1`、召回近期用户消息 **2** 条 / 每条 **200** 字 / 总长 **1200** 字、内置 skill 词法匹配最多 **3** 个、召回期限沿用 `TURN_RECALL_DEADLINE`（400 ms）。
- 修改既有符号前按 `CLAUDE.md` 运行 `gitnexus_impact`（至少：`assemble_system_prompt`、`build_request_messages`、`create_turn_snapshot`、`cost_store::record`、`build_tool_registry`、`load_active`），提交前运行 `gitnexus_detect_changes`；HIGH/CRITICAL 先告知用户。
- 新 Tauri 命令同时在 `tauri_commands.rs` 定义并在 `main.rs` 的 `invoke_handler!` 注册。新 Rust 文件以 `// SPDX-License-Identifier: Apache-2.0` 开头。
- 界面文案简体中文；代码、提交信息英文。每个后端可见变化都要有对应前端（用户要求：前后端同步）。
- 每个任务一个提交（`git add <paths>` 显式暂存，禁止 `git commit -am`；提交后 `git show --stat HEAD` 核对新文件都在）；TDD 必须有 RED→GREEN 证据。
- 验证命令：`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`；`cargo test --lib <filter>`；`cd ui && npm test -- --run <pattern>`；`npx tsc --noEmit`（main 基线 **41** 个错误，不得增加）。已知失败：shell `test_daemon_mode_approval_unchanged`、`runtime_pack_ipc::print_real_status_diagnostics`、3× `provider_execution::*cli_route*`、偶发 `playwright_cli::*worker*`、偶发 `recall_semantic_is_fast_under_connection_contention`；UI `KaleidoscopeShell`、`MemoryModule` MemoryGraphView。

## Review Focus

1. 用户原话里本身含 `<turn_context>` 字样、或以 `</turn_context>` 结尾 → `strip` 只去掉**行首由我们生成**的那一段，用户原文一字不差（Task 5 测试 `strip_only_removes_leading_generated_prefix`）。
2. 轮内插入的 steering 消息与 follow-up 消息 → 动态块只挂在本轮第一条用户行（`current_turn_start`）上，不挂到后来的用户消息，也不被重复写入（Task 6 测试 `turn_context_attaches_only_to_turn_user_row`）。
3. 回退到注入完整记忆那一轮之前、或压缩把它移出活动上下文 → 下一轮重新注入完整 `<memory_context>`；分叉后新会话的账本与源会话截断点一致（Task 5 测试 `ledger_reinjects_full_memory_when_no_memory_line_is_active`，Task 6 测试 `fork_and_rewind_keep_prefix_and_ledger_consistent`）。
4. 升级前就存在的 transcript 会话（有转录行、无框架、行无 `dynamic_prefix`）→ 首轮创建框架（原因 `new`），旧行重放字节与升级前完全相同（Task 1 测试 `rows_without_prefix_replay_unchanged`，Task 4 测试 `existing_session_without_frame_gets_new_frame`）。
5. 浏览器上下文关闭后，已公布的浏览器工具仍在列表中且调用时返回工具错误而非 panic；MCP 服务器断开 → 下一轮 `<tools_update>` 列出移除的工具（Task 10 测试 `sticky_browser_tools_without_context_return_tool_error`、`tools_update_lists_removed_mcp_tools`）。

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `src-tauri/src/agent/session_frame.rs`（新） | V67 `agent_session_frames` DAO、框架键与重建判定、分叉复制 |
| `src-tauri/src/agent/turn_context.rs`（新） | `<turn_context>` 渲染/解析/剥离、`TurnContextLedger`、记忆差量、粘性块、模式/工具说明（纯函数） |
| `src-tauri/src/agent/recall_query.rs`（新） | 召回查询构造（纯函数）+ 读取近期用户消息 |
| `src-tauri/src/agent/prefix_consistency_tests.rs`（新，`#[cfg(test)]`） | §7.C 字节一致性测试 |
| `src-tauri/src/agent/transcript.rs` | `dynamic_prefix` 覆盖层：写入、读出合并、随压缩/分叉复制、级联删除框架 |
| `src-tauri/src/agent/transcript_branch.rs` | 分叉复制框架 |
| `src-tauri/src/skills_manifest.rs` | `build_session_manifest`（限量、确定性） |
| `src-tauri/src/agent/rule_context_builder.rs` | 拆出静态规则 / glob 规则 / 规则源哈希 |
| `src-tauri/src/agent/dispatcher/{mod.rs,content_assembler.rs,turn_runner.rs,observability.rs}` | 框架装载、冻结组装、动态块应用、指标 |
| `src-tauri/src/agent/relevant_skills.rs` | 内置/插件 skill 词法匹配 + 统一 `<relevant_skills>` |
| `src-tauri/src/agent/tools/registry_build.rs` | 浏览器工具粘性公布 |
| `src-tauri/src/memory_graph/recall.rs` | 多 space 召回合并 |
| `src-tauri/src/cost_store.rs`、`src-tauri/src/tauri_commands.rs`、`src-tauri/src/main.rs` | 缓存字段、汇总、会话缓存命中命令、接线 |
| `src-tauri/src/db/migrations.rs`、`CONTEXT.md` | V67 |
| `ui/src/components/agent/ContextUsageBadge.tsx`（+ `.test.tsx`） | "提示词缓存"分区 |
| `ui/src/components/settings/UsageSettings.tsx` | 缓存命中列 |
| `ui/src/components/agent/PromptFrameNotice.tsx`（新，+ `.test.tsx`）、`ui/src/components/agent/AgentMessages.tsx` | 工具集/模式/重建提示 |
| `ui/src/lib/{tauri-bridge.ts,types.ts}`、`ui/src/atoms/agent-atoms.ts`、`ui/src/hooks/useGlobalAgentListeners.ts` | 新字段、新事件、新命令 |

---

### Task 1: V67 迁移 + 转录动态块覆盖层

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`、`CONTEXT.md`、`src-tauri/src/agent/transcript.rs`

**Interfaces:**
- V67 SQL（`run_v67`，与 `run_v64` 同写法）：
  ```sql
  -- agent_transcript.dynamic_prefix TEXT  (pragma_table_info 检查后 ALTER)
  CREATE TABLE IF NOT EXISTS agent_session_frames (
    session_id      TEXT PRIMARY KEY,
    frame_key_json  TEXT NOT NULL,      -- FrameKey (Task 2)
    system_prompt   TEXT NOT NULL,
    mode            TEXT NOT NULL,      -- 冻结时的模式，SafetyMode 的 snake_case
    rules_hash      TEXT NOT NULL,
    tool_names_json TEXT NOT NULL DEFAULT '[]',
    manifest_json   TEXT NOT NULL DEFAULT '{}',  -- {"listed":N,"total":M,"tokens":T}
    rebuild_reason  TEXT NOT NULL,      -- new | prompt_changed | workspace_changed | version_changed
    rebuilt_turn_id TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
  );
  ```
- Produces（`transcript.rs`）：
  ```rust
  pub struct TranscriptRow { /* 既有字段 */ pub dynamic_prefix: Option<String> }  // message 已合并 prefix
  pub fn merge_dynamic_prefix(prefix: &str, msg: ChatMessage) -> ChatMessage;     // 首个 Text 块前加 "{prefix}\n\n"；无 Text 块则在 0 位插入 Text{prefix}
  pub fn set_dynamic_prefix(conn: &Connection, session_id: &str, turn_id: &str, prefix: &str) -> anyhow::Result<bool>;
  //   目标 = 该 turn_id 下 seq 最小、kind=message、role=user、未 superseded 的行；仅当 dynamic_prefix IS NULL 时写入；返回是否写入
  ```
  `load_active` / `last_active` 读出时用 `merge_dynamic_prefix` 合并，`est_tokens` 加上 `estimate_tokens(prefix)`；压缩重新追加保留尾部（`transcript.rs:225` 的 INSERT…SELECT）与分叉克隆（`:527`）把 `dynamic_prefix` 一起复制；`delete_session` 级联删除 `agent_session_frames` 行。

- [ ] **Step 1: 写失败测试**（`transcript.rs` tests）：`v67_adds_dynamic_prefix_and_frames_table_idempotently`（跑两次 `run` 不报错，列与表存在）；`set_dynamic_prefix_is_set_once`（第二次返回 false 且值不变）；`dynamic_prefix_is_merged_on_load_active_and_last_active`（文本 = `"<turn_context>x</turn_context>\n\nhi"`，est 增加）；`message_json_is_never_rewritten_by_dynamic_prefix`（直接查列，原 JSON 不变）；`dynamic_prefix_survives_compaction_reappend_and_fork_clone`；`rows_without_prefix_replay_unchanged`（无 prefix 的行与改动前逐字节相同）；`delete_session_removes_frame_row`。
- [ ] **Step 2: 运行确认失败** `cd src-tauri && cargo test --lib transcript::` → 编译失败/断言失败。
- [ ] **Step 3: 实现** V67 迁移（在 `run` 里 V66 之后调用 `run_v67`）、上述函数与复制/级联；`CONTEXT.md` 登记 V67。
- [ ] **Step 4: 运行通过** `cargo test --lib transcript:: && cargo test --lib migrations`。
- [ ] **Step 5: 提交** `feat(agent): V67 transcript dynamic_prefix overlay + agent_session_frames table`

---

### Task 2: 会话提示词框架 DAO（`session_frame.rs`）

**Files:**
- Create: `src-tauri/src/agent/session_frame.rs`（在 `agent/mod.rs` 声明）
- Modify: `src-tauri/src/agent/transcript_branch.rs`（`fork_with_transcript` 在同一事务内复制框架）

**Interfaces:**
- Produces：
  ```rust
  pub const FRAME_VERSION: u32 = 1;

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct FrameKey { pub version: u32, pub prompt_sha: String, pub workspace: String, pub static_sha: String }
  impl FrameKey { pub fn compute(base_prompt: &str, workspace_root: Option<&Path>) -> Self; }
  //   prompt_sha = sha256(base_prompt)；static_sha = sha256(karpathy baseline 渲染 + 全部 mode_addition 文案)；hex 小写

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum FrameRebuildReason { New, PromptChanged, WorkspaceChanged, VersionChanged }

  #[derive(Debug, Clone, PartialEq)]
  pub struct SessionFrame {
      pub session_id: String, pub key: FrameKey, pub system_prompt: String, pub mode: String,
      pub rules_hash: String, pub tool_names: Vec<String>, pub manifest: ManifestStats,
      pub rebuild_reason: FrameRebuildReason, pub rebuilt_turn_id: Option<String>,
      pub created_at: i64, pub updated_at: i64,
  }
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
  pub struct ManifestStats { pub listed: u32, pub total: u32, pub tokens: u32 }

  pub fn rebuild_reason(existing: Option<&SessionFrame>, key: &FrameKey) -> Option<FrameRebuildReason>; // None = 复用
  //   优先级：无 → New；version/static_sha 不同 → VersionChanged；workspace 不同 → WorkspaceChanged；prompt_sha 不同 → PromptChanged
  pub fn load(conn: &Connection, session_id: &str) -> anyhow::Result<Option<SessionFrame>>;
  pub fn upsert(conn: &Connection, frame: &SessionFrame) -> anyhow::Result<()>;
  pub fn update_tool_names(conn: &Connection, session_id: &str, names: &[String], now: i64) -> anyhow::Result<()>;
  pub fn update_rules_hash(conn: &Connection, session_id: &str, hash: &str, now: i64) -> anyhow::Result<()>;
  pub fn copy_to(conn: &Connection, from: &str, to: &str) -> anyhow::Result<bool>;
  pub fn delete_session(conn: &Connection, session_id: &str) -> rusqlite::Result<()>;   // Task 1 的级联改为调用它
  ```

- [ ] **Step 1: 写失败测试**：`frame_key_is_stable_and_hex`（同输入两次相同）；`rebuild_reason_priority`（四种原因各一例 + 相同键返回 None）；`frame_roundtrip_upsert_load`（`tool_names` 顺序保留）；`update_tool_names_and_rules_hash_touch_only_those_columns`；`fork_copies_frame`（`fork_with_transcript` 后新会话有同样的 `system_prompt`）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib session_frame`。
- [ ] **Step 3: 实现** 上述类型与函数；`fork_with_transcript` 调 `copy_to`。
- [ ] **Step 4: 运行通过** `cargo test --lib session_frame && cargo test --lib transcript_branch`。
- [ ] **Step 5: 提交** `feat(agent): persisted per-session prompt frame (key, rebuild reason, fork copy)`

---

### Task 3: 限量、确定性的会话 skills 清单（T3）

**Files:**
- Modify: `src-tauri/src/skills_manifest.rs`

**Interfaces:**
- Produces：
  ```rust
  pub const SYSTEM_PROMPT_MANIFEST_MAX_TOKENS: usize = 1500;
  pub const SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES: usize = 200;
  pub struct SessionManifest { pub block: String, pub listed: Vec<String>, pub total_available: usize, pub learned_node_ids: Vec<String> }
  pub fn build_session_manifest(registry: &SkillsRegistry, store: &MemoryGraphStore, space_id: &str) -> SessionManifest; // 纯读，不 bump
  ```
  选择与排序见 Decision 12（沿用 `collect_entries` 的启用/工作区标签/Playwright 兼容性过滤；Plugin skill 按 `plugin_skill_disabled` 过滤）。每行 `- **{name}** — {summary≤100 字}`；header `## 可用技能`；footer 固定：`共 {total} 个技能，此处列出 {listed} 个；其余用 skill_search 查找，用 load_skill 按名称加载全文。`（`listed == total` 时省略"其余…"半句）。估算用本文件现有 `APPROX_TOKENS_PER_CHAR`，总长不超过 1500 token。

- [ ] **Step 1: 写失败测试**：`session_manifest_respects_token_and_entry_caps`（造 300 个 skill → `listed.len() <= 200` 且 `block` 估算 ≤ 1500）；`session_manifest_is_byte_identical_across_calls`；`session_manifest_orders_bundled_then_installed_then_learned`；`session_manifest_footer_reports_total_and_listed`；`session_manifest_excludes_disabled_and_disabled_plugin_skills`；`load_skill_resolves_builtin_and_plugin_skill_by_name`（放在 `load_skill.rs` tests，若 `builtin_skill_loads_from_registry` 已覆盖 builtin，则只补 plugin 一例）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib skills_manifest`。
- [ ] **Step 3: 实现** `build_session_manifest` 与专用 formatter；不改 `build_skills_manifest` 的现有行为。
- [ ] **Step 4: 运行通过** `cargo test --lib skills_manifest && cargo test --lib load_skill`。
- [ ] **Step 5: 提交** `feat(skills): bounded deterministic session skills manifest (T3)`

---

### Task 4: 冻结系统提示词并在 transcript 模式使用

**Files:**
- Modify: `src-tauri/src/agent/rule_context_builder.rs`、`src-tauri/src/agent/dispatcher/content_assembler.rs`、`src-tauri/src/agent/dispatcher/mod.rs`、`src-tauri/src/agent/dispatcher/turn_runner.rs`、`src-tauri/src/tauri_commands.rs`（agent 路径传入清单与模板文本）

**Interfaces:**
- Consumes：Task 2 `SessionFrame`/`FrameKey`/`rebuild_reason`/`load`/`upsert`；Task 3 `build_session_manifest`。
- Produces（`rule_context_builder.rs`）：
  ```rust
  impl RuleContextBuilder {
      pub fn build_static_context(workspace_root: &Path) -> String;                        // 仅 AGENTS.md/.cursorrules 段落，格式同现有 ACTIVE PROJECT RULES 块
      pub fn glob_matched_rules(workspace_root: &Path, files: &[PathBuf]) -> Vec<(String, String)>; // (rule_id = sha256(instructions) 前 12 位, instructions)，按 .uclawrules 中出现顺序、去重
      pub fn rules_source_hash(workspace_root: &Path) -> String;                           // sha256(uclaw.md ‖ .uclawrules ‖ .cursorrules ‖ AGENTS.md 内容，缺失视为空)
  }
  ```
  `build_context` 保持原签名与行为（非 transcript 路径继续用）。
- Produces（`content_assembler.rs`）：
  ```rust
  pub(super) struct FrameInputs { pub base_system_prompt: String, pub workspace_root: Option<PathBuf>, pub mode: SafetyMode,
                                  pub persona_block: Option<String>, pub skills_manifest_block: String, pub static_rules_block: String }
  pub(super) fn assemble_frozen_system_prompt(i: &FrameInputs) -> String;
  //   = compose_system_prompt_with_injection_and_persona(.., &InjectionContext::baseline(), ..) + manifest + static rules + SYSTEM_INFO_GUIDANCE
  //   不含 gene / plan 提示 / glob 规则 / 时间；不做 align_to_1024_tokens
  pub(super) const SYSTEM_INFO_GUIDANCE: &str; // 现 build_dynamic_block 中"注意: 以上时间和工作区路径由系统直接提供……"整段，改为"每轮 <system_info> 提供当前时间……"
  ```
- Produces（`ChatDelegate`，`mod.rs`）：字段 `frame: tokio::sync::OnceCell<SessionFrame>`、`session_manifest: Option<SessionManifest>`、`frame_base_prompt: Option<String>`；`pub fn with_frame_inputs(self, manifest: SessionManifest, base_prompt: String) -> Self`；`pub(super) async fn session_frame(&self) -> &SessionFrame`（读库 → `rebuild_reason` → 需要时用 `assemble_frozen_system_prompt` 新建并 `upsert`，新建时调用 `store.bump_manifest_appearance(&learned_node_ids)`，记 `rebuilt_turn_id = sink.turn_id`，`mode` = 当前 `resolve_effective_mode`，`rules_hash` = `rules_source_hash`，`tool_names` = 当前工具面）。
- `create_turn_snapshot`：`transcript_sink.is_some()` 时 `system_prompt = frame.system_prompt`、`dynamic_context = String::new()`；否则走原 `assemble_prompt`。
- 新建框架时打一条 `[FRAME]` info 日志：`reason`、各段 token（base / uclaw_md / persona / baseline / mode / manifest / static_rules / total）、`manifest_listed`、`manifest_total`。
- `tauri_commands.rs:10274-10279`：transcript 模式改为 `build_session_manifest(&registry, &memory_graph_store, &space_id_for_extractor)` 并 `with_frame_inputs(manifest, resolved_system_prompt.clone())`；非 transcript 模式保持 XML 清单。

- [ ] **Step 1: 写失败测试**（`content_assembler.rs` tests，构造 `FrameInputs`）：`frozen_system_prompt_excludes_gene_plan_glob_rules_and_time`；`frozen_prompt_has_no_cache_align_padding`；`frozen_prompt_contains_system_info_guidance_and_bounded_manifest`；（`rule_context_builder.rs`）`glob_matched_rules_are_deterministic_and_session_scoped`（输入文件顺序打乱结果相同）、`rules_source_hash_changes_when_agents_md_changes`；（`turn_runner.rs` tests，用 in-memory DB + sink）`frame_reused_across_turns_byte_identical`、`frame_rebuilt_on_prompt_change_with_reason`、`frame_not_rebuilt_on_mode_or_persona_change`、`existing_session_without_frame_gets_new_frame`、`non_transcript_snapshot_unchanged`（现有 `snapshot_*` 测试保持通过即可）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib content_assembler && cargo test --lib rule_context_builder && cargo test --lib turn_runner`。
- [ ] **Step 3: 实现** 上述函数、字段、快照分支与接线。
- [ ] **Step 4: 运行通过**（同上命令）+ `cargo build`。
- [ ] **Step 5: 提交** `feat(agent): freeze the system prompt per session in transcript mode`

---

### Task 5: `turn_context.rs` — 动态块渲染、账本与记忆差量（纯函数）

**Files:**
- Create: `src-tauri/src/agent/turn_context.rs`（在 `agent/mod.rs` 声明）

**Interfaces:**
- Consumes：Task 2 `SessionFrame`（`mode`、`tool_names`）。
- Produces：
  ```rust
  pub const OPEN: &str = "<turn_context>";
  pub const CLOSE: &str = "</turn_context>";

  pub struct TurnContextInputs {
      pub now: chrono::DateTime<chrono::Local>,
      pub memory_context: Option<String>,
      pub learned_profile: String,
      pub relevant_skills: String,              // 已渲染的 <relevant_skills> 块或空
      pub gene_block: String,                   // format_gene_injection 输出或空
      pub plan_suggest_hint: String,
      pub glob_rules: Vec<(String, String)>,    // Task 4 glob_matched_rules
      pub rules_update: Option<String>,         // 规则源哈希变化时的新静态规则全文
      pub context_fragments: Vec<crate::runtime::context::ContextArtifact>,
      pub rewound_changes: String,              // 现 rewound_changes_from_sink 的块体
      pub current_mode: crate::safety::SafetyMode,
      pub current_tool_names: Vec<String>,
  }

  #[derive(Debug, Default)]
  pub struct TurnContextLedger {
      pub seen_memory_lines: std::collections::HashSet<String>,
      pub sticky: std::collections::BTreeMap<String, String>,  // key → 最近一次正文
      pub last_mode: Option<String>,
  }
  impl TurnContextLedger { pub fn from_prefixes<'a>(prefixes: impl IntoIterator<Item = &'a str>) -> Self; }

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "snake_case")]
  pub enum MemoryInjection { Full, Delta, None }

  #[derive(Debug, Clone, PartialEq, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct TurnContextReport {
      pub memory: MemoryInjection, pub memory_tokens: u32, pub total_tokens: u32,
      pub mode_changed: Option<(String, String)>, pub tools_added: Vec<String>, pub tools_removed: Vec<String>,
  }

  pub fn render(i: &TurnContextInputs, ledger: &TurnContextLedger, frame: &SessionFrame) -> (String, TurnContextReport);
  pub fn strip(text: &str) -> &str;
  pub fn memory_content_lines(text: &str) -> Vec<String>;  // trim 后非空、非 `<…>` 标签行、非 `#` 标题行
  ```
- 渲染规则（顺序固定）：`OPEN` → `<system_info>当前时间: {YYYY年M月D日 周X HH:MM}</system_info>` → 记忆（账本无任何记忆行 → `<memory_context>` 全文，`Full`；否则新增内容行非空 → `<memory_update>`（首行 `以下是本轮新召回、此前未出现过的记忆：`），`Delta`；否则 `None`）→ 粘性块（键：`learned_profile`、`plan_suggest`、`rewound_file_changes`、`project_rules_update`、`project_rule:{id}`、`context_fragment:{id}`；正文与账本中该键最近一次相同则省略）→ `<gene_signals>` → `<relevant_skills>`（原样）→ `<mode_update mode="{snake}">`（当前模式 ≠ 账本 `last_mode`（无则 frame.mode）时：`mode_addition` 文本，Supervised 为 `自动模式：无额外约束。`，末句 `此说明取代系统提示词中的模式部分。`）→ `<tools_update>`（`current_tool_names` 与 `frame.tool_names` 的差集，`新增: a, b` / `移除: c`）→ `CLOSE`。每个子块 `<tag …>\n{body}\n</tag>`，子块之间空一行。`from_prefixes` 解析同一套标签（只认 `OPEN…CLOSE` 内的顶层子块）。
- `strip`：仅当 `text` 以 `OPEN` 开头且存在第一个 `CLOSE + "\n\n"` 时返回其后部分，否则原样返回。

- [ ] **Step 1: 写失败测试**：`render_parse_roundtrip_ledger_sees_all_sticky_keys`；`first_turn_emits_full_memory_then_delta_then_none`（三轮：全文 → 仅新增 2 行 → 无新增不出块）；`ledger_reinjects_full_memory_when_no_memory_line_is_active`（账本为空即 Full）；`sticky_block_emitted_only_on_change`（`learned_profile` 两轮相同第二轮不出现，第三轮变化再出现）；`mode_update_only_when_mode_differs_from_last_noted`；`tools_update_lists_added_and_removed_against_frame`；`strip_only_removes_leading_generated_prefix`（用户原文含 `<turn_context>`、以 `</turn_context>` 结尾、无前缀三种）；`render_is_deterministic_for_same_inputs`；`delta_turn_context_under_2k_tokens_for_typical_inputs`（5.4K 记忆 + 3 行新增 → 估算 ≤ 2_000）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib turn_context`。
- [ ] **Step 3: 实现** 模块。
- [ ] **Step 4: 运行通过** `cargo test --lib turn_context`。
- [ ] **Step 5: 提交** `feat(agent): turn_context renderer with transcript-derived ledger and memory delta`

---

### Task 6: 把动态块接入 transcript 模式（只算一次、持久化、原样重放）

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs`、`src-tauri/src/agent/dispatcher/content_assembler.rs`、`src-tauri/src/agent/dispatcher/mod.rs`、`src-tauri/src/agent/compact/summarize.rs`（或阶段 2 摘要提示词所在文件）、`src-tauri/src/tauri_commands.rs`（relevant skills / gene 输入透传不变，仅确认）

**Interfaces:**
- Consumes：Task 1 `set_dynamic_prefix`/`merge_dynamic_prefix`/`load_active`；Task 2 `update_tool_names`/`update_rules_hash`；Task 4 `session_frame`/`glob_matched_rules`/`rules_source_hash`/`build_static_context`；Task 5 `render`/`TurnContextLedger`/`strip`。
- Produces（可在无 `ChatDelegate` 时测试的纯入口，`turn_runner.rs`）：
  ```rust
  pub(crate) fn apply_turn_context(
      conn: &Connection, session_id: &str, turn_id: &str,
      ctx: &mut ReasoningContext, inputs: &TurnContextInputs, frame: &SessionFrame,
  ) -> anyhow::Result<Option<TurnContextReport>>;
  //   账本 = load_active 中 turn_id ≠ 本轮的行的 dynamic_prefix；render；
  //   把 prefix 拼进 ctx.messages[ctx.current_turn_start]（merge_dynamic_prefix）并 set_dynamic_prefix；
  //   该行已有 prefix（如重试/恢复）→ 不重算，返回 None
  ```
- `ChatDelegate::before_llm_call`：transcript 模式且本轮尚未应用（`turn_context_applied: AtomicBool`）时，组装 `TurnContextInputs`（GEP 用 `strip(当前用户文本)` + `recent_tool_errors` 匹配一次并写 `gep.last_matches`；plan 提示沿用现逻辑；glob 规则用 `file_working_set::load(conn, session).paths()`；`rules_update` = 规则源哈希 ≠ `frame.rules_hash` 时 `build_static_context` + `uclaw.md` 全文；`rewound_changes` 用现有 `rewound_changes_from_sink`；工具名取 `PerTurnToolSurface::from_registry(&self.tools)`）→ `apply_turn_context` → 若报告中工具/规则有变化则 `update_tool_names` / `update_rules_hash` → 结果存 `last_turn_context_report: Mutex<Option<TurnContextReport>>`，并发 `agent:prompt_frame` 事件（payload 见 Task 13；本任务先定义并发送）。
- 非 transcript 模式：`create_turn_snapshot` / `build_request_messages` 行为不变（`snapshot.dynamic_context` 非空时仍拼进副本）。transcript 模式下 `with_rewound_changes` 不再调用（已由粘性块承担）。
- 读"用户原话"的地方改为 `turn_context::strip`：至少 `create_turn_snapshot` 的 `last_user_text`、`steering`/学习管线中从 `reason_ctx.messages` 取用户文本处（实施时 `grep -rn "MessageRole::User" src-tauri/src/agent/dispatcher src-tauri/src/agent/agentic_loop.rs src-tauri/src/learning` 逐个核对并在提交说明列出）。
- 阶段 2 摘要提示词加一句：`用户消息开头的 <turn_context> 块是系统注入的上下文（时间、记忆、提示），不是用户原话，不要当作用户要求记录。`
- `agent:prompt_frame` 事件（camelCase）：`{ conversationId, frozen: true, rebuilt: bool, rebuildReason: "new"|"prompt_changed"|"workspace_changed"|"version_changed"|null, systemPromptTokens, toolCount, modeChanged: {from,to}|null, toolsAdded: string[], toolsRemoved: string[], memory: "full"|"delta"|"none", memoryTokens, turnContextTokens }`；`rebuilt` 只在本轮新建/重建框架时为 true。

- [ ] **Step 1: 写失败测试**（`turn_runner.rs` tests，in-memory DB）：`transcript_turn_context_applied_once_per_turn_and_persisted`（两次迭代只写一次，`load_active` 读出与内存消息字节相同）；`turn_context_attaches_only_to_turn_user_row`（steering / follow-up 消息不带前缀）；`second_turn_replays_first_turn_context_verbatim`；`restart_replay_includes_persisted_turn_context`（丢弃内存，重新 `build_agent_history` 得到相同消息）；`fork_and_rewind_keep_prefix_and_ledger_consistent`；`non_transcript_mode_keeps_ephemeral_splice`；`gep_matches_once_per_turn_on_stripped_user_text`；`summary_prompt_mentions_turn_context`。
- [ ] **Step 2: 运行确认失败** `cargo test --lib turn_runner`。
- [ ] **Step 3: 实现** 上述接线。
- [ ] **Step 4: 运行通过** `cargo test --lib turn_runner && cargo test --lib transcript && cargo test --lib agentic_loop` + `cargo build`。
- [ ] **Step 5: 提交** `feat(agent): persist the per-turn context with the user message and replay it verbatim`

---

### Task 7: §7.C 字节一致性测试（前一轮请求是后一轮请求的前缀）

**Files:**
- Create: `src-tauri/src/agent/prefix_consistency_tests.rs`（`#[cfg(test)] mod prefix_consistency_tests;` 声明于 `agent/mod.rs`）
- Modify: `src-tauri/src/llm/providers/openai.rs`、`src-tauri/src/llm/providers/anthropic.rs`（`build_request_body` 改为 `pub(crate)`，仅可见性）

**Interfaces:**
- Consumes：Task 1、2、4、5、6 的全部公开入口（`build_agent_history`、`persist_current_user_turn`、`apply_turn_context`、`assemble_frozen_system_prompt`、`build_request_messages`、`flush_transcript` 写入路径或 `transcript::append`）。
- 测试内辅助（不导出）：`fn request_body(provider: Provider, frame: &SessionFrame, tools: &[ToolDefinition], ctx: &ReasoningContext) -> serde_json::Value`；`fn assert_prefix(prev: &Value, next: &Value)`：`tools` 与 system 逐字节相等，`prev.messages` 每个元素 `serde_json::to_string` 后与 `next.messages` 对应元素相等，且 `next.messages.len() > prev.messages.len()`。Anthropic 比较前去掉所有 `cache_control` 字段（断点每轮移动，不属于前缀内容）。

- [ ] **Step 1: 写测试**（应直接通过，若失败即是 Task 1–6 的缺陷，回到对应任务修）：
  - `consecutive_turns_request_is_prefix_openai`：3 轮纯文本，每轮记忆不同（含新增行）。
  - `tool_iterations_within_turn_are_prefix`：一轮 2 次工具迭代，第 1 次迭代请求是第 2 次的前缀。
  - `restart_between_turns_preserves_prefix`：轮间丢弃全部内存状态，从 DB 重建。
  - `rewound_changes_and_sticky_blocks_do_not_break_prefix`。
  - `consecutive_turns_request_is_prefix_anthropic_messages`。
  - 豁免场景断言"仅在预期位置断开"：`compaction_turn_breaks_prefix_only_at_session_block`、`frame_rebuild_changes_only_system`、`tools_change_changes_only_tools`。
- [ ] **Step 2: 运行** `cargo test --lib prefix_consistency` → PASS；临时把 Task 6 的 `set_dynamic_prefix` 调用注释掉再跑，确认 `consecutive_turns_request_is_prefix_openai` 失败（RED 证据记录在提交说明），然后恢复。
- [ ] **Step 3: 提交** `test(agent): byte-level prefix consistency across turns, iterations and restarts (spec §7.C)`

---

### Task 8: 召回查询改进 + 会话 space

**Files:**
- Create: `src-tauri/src/agent/recall_query.rs`（在 `agent/mod.rs` 声明）
- Modify: `src-tauri/src/memory_graph/recall.rs`、`src-tauri/src/tauri_commands.rs`（`:9846-9878` 图召回、`:10101-10114` bucket_seal 召回）

**Interfaces:**
- Produces：
  ```rust
  pub const PRIOR_USER_MESSAGES: usize = 2;
  pub const PRIOR_MAX_CHARS: usize = 200;
  pub const QUERY_MAX_CHARS: usize = 1200;
  pub fn build_recall_query(current: &str, prior_user_texts: &[String]) -> String; // 本轮在前，之后按新→旧，"\n" 连接，按字符截断（不切坏 UTF-8）
  pub fn recent_user_texts(conn: &Connection, session_id: &str, exclude_message_id: &str, n: usize) -> Vec<String>; // agent_messages role='user' 按 created_at DESC
  ```
  `recall.rs`：`pub async fn build_recall_plan_for_spaces(&self, spaces: &[&str], user_input: &str, is_group_chat: bool) -> anyhow::Result<MemoryRecallPlan>`（逐 space 调 `build_recall_plan`，各层按 `node_id` 去重，先出现者优先；`spaces` 内重复项只跑一次）。
- 接线：查询在锁内与 `transcript_boot` 同时读取（已持有 `conn`）；图召回 `spaces = [space_id_for_extractor, "default"]`；bucket_seal 召回与 `split_skill_entries` 用同一查询；`agent:memory-recall` 事件额外带 `query`（前端暂不显示，便于排查）与 `spaces`。

- [ ] **Step 1: 写失败测试**：`recall_query_includes_recent_user_messages_truncated`；`recall_query_current_only_when_no_history`；`recall_query_respects_total_cap_on_cjk`；`recent_user_texts_excludes_current_and_orders_newest_first`；`recall_plan_for_spaces_merges_and_dedupes_session_first`；`recall_plan_for_spaces_runs_default_once`。
- [ ] **Step 2: 运行确认失败** `cargo test --lib recall_query && cargo test --lib memory_graph::recall`。
- [ ] **Step 3: 实现** 并接线。
- [ ] **Step 4: 运行通过**（同上）+ `cargo build`。
- [ ] **Step 5: 提交** `feat(memory): recall with recent user context and the session's real space`

---

### Task 9: 统一 `<relevant_skills>`（内置/插件词法 + 已学习语义）

**Files:**
- Modify: `src-tauri/src/agent/relevant_skills.rs`、`src-tauri/src/tauri_commands.rs`（`:10118-10140`）、`src-tauri/src/agent/dispatcher/turn_runner.rs`（`:983` transcript 模式不再置 `skill_search_used`）

**Interfaces:**
- Produces：
  ```rust
  pub const MAX_RELEVANT_REGISTRY_SKILLS: usize = 3;
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub enum RelevantSkillSource { Learned, Registry }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct RelevantSkill { pub name: String, pub when_to_use: String, pub source: RelevantSkillSource }
  pub fn registry_matches(registry: &SkillsRegistry, current_message: &str, max: usize) -> Vec<RelevantSkill>; // match_skills 顺序，禁用/禁用插件已被 registry 过滤
  pub fn merge(learned: Vec<RelevantSkill>, registry: Vec<RelevantSkill>) -> Vec<RelevantSkill>;             // 已学习在前，按 name 去重
  pub fn render_block(skills: &[RelevantSkill]) -> Option<String>;  // 标签改为 <relevant_skills>，说明句保持现文案语义并改为"技能"
  ```
  `split_skill_entries` 返回值改为 `(Vec<MemoryEntry>, Vec<RelevantSkill>)`（内部仍解析 `Skill`），`recalled_event_payload` 接收 `&[RelevantSkill]`。词法匹配用本轮原文（不用扩展查询）。bucket_seal 召回超时（返回 `None`）时只用词法结果。

- [ ] **Step 1: 写失败测试**：`relevant_skills_merges_registry_and_learned_dedup_by_name`；`relevant_skills_registry_only_when_recall_times_out`；`render_block_uses_relevant_skills_tag`；`registry_matches_skips_disabled_plugin_skill`；`transcript_mode_does_not_suppress_manifest_after_skill_search`（非 transcript 的 `manifest_suppressed_after_skill_search_used` 仍通过）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib relevant_skills && cargo test --lib content_assembler`。
- [ ] **Step 3: 实现** 并接线；前端 `agent:skill-recalled` 的消费方若读字段名，同步更新（`grep -rn "skill-recalled" ui/src`）。
- [ ] **Step 4: 运行通过**（同上）+ `cd ui && npm test -- --run skill` + `cargo build`。
- [ ] **Step 5: 提交** `feat(agent): unified <relevant_skills> from registry lexical match and learned recall`

---

### Task 10: 工具集稳定（浏览器工具粘性公布）

**Files:**
- Modify: `src-tauri/src/agent/tools/registry_build.rs`、`src-tauri/src/tauri_commands.rs`（`:9738` 调用处）

**Interfaces:**
- Consumes：Task 2 `session_frame::load`。
- Produces：`build_tool_registry(…, announce_full_browser: bool)` 新增参数；`registry_build.rs:151` 条件改为 `browser_active || announce_full_browser`；日志字段增加 `announce_full_browser`。调用处：`announce_full_browser = session_frame::load(&conn, session_id)?.map_or(false, |f| f.tool_names.iter().any(|n| n == "browser_go_back"))`（工具名以实际注册名为准，测试中断言）。其他调用方传 `false`。

- [ ] **Step 1: 写失败测试**：`sticky_browser_tools_registered_when_frame_announced`（无上下文 + 标志 → 28 个浏览器工具）；`sticky_browser_tools_without_context_return_tool_error`（执行 `browser_go_back` 返回 `ToolError`/错误结果，不 panic）；`tools_update_lists_removed_mcp_tools`（Task 5 `render` 对 frame 有 `mcp__x__y`、当前无 → `移除: mcp__x__y`，并经 Task 6 入口写入 `update_tool_names`）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib registry_build && cargo test --lib turn_runner`。
- [ ] **Step 3: 实现**。
- [ ] **Step 4: 运行通过**（同上）+ `cargo build`。
- [ ] **Step 5: 提交** `feat(agent): keep browser tools announced once a session has used them`

---

### Task 11: 可观测性（后端）：缓存字段、会话命中率、框架信息

**Files:**
- Modify: `src-tauri/src/cost_store.rs`、`src-tauri/src/agent/dispatcher/observability.rs`、`src-tauri/src/agent/types.rs`（`ContextStats`）、`src-tauri/src/tauri_commands.rs`、`src-tauri/src/main.rs`

**Interfaces:**
- Produces（`cost_store.rs`）：
  ```rust
  pub fn record_usage(state: &AppState, session_id: &str, model: &str, input_tokens: u32, output_tokens: u32, cached_input_tokens: u32);
  pub fn record(state: &AppState, session_id: &str, model: &str, input_tokens: u32, output_tokens: u32); // 委托 record_usage(.., 0)，其他调用方不动
  pub fn prompt_tokens_total(model: &str, input_tokens: u64, cached_input_tokens: u64) -> u64;            // Decision 18
  pub fn cache_hit_ratio(model: &str, input_tokens: u64, cached_input_tokens: u64) -> f64;                // 0.0..=1.0，分母 0 → 0.0
  ```
  `emit_turn_cost` 改调 `record_usage(.., usage.cache_read_tokens)`，并在 `agent:turn_cost` 增加 `cacheHitRatio`（单次调用）。
- `ContextStats` 增加（全部 `Option`，`skip_serializing_if = "Option::is_none"`，仅 transcript 模式填）：`frozen_system_prompt: Option<bool>`、`tool_def_tokens: Option<u32>`、`tool_count: Option<u32>`、`frame_rebuild_reason: Option<String>`、`frame_created_at: Option<i64>`、`memory_injection: Option<MemoryInjection>`、`turn_context_tokens: Option<u32>`；`system_prompt_tokens` 在 transcript 模式改为冻结提示词的估算。`build_context_stats` 增加一个 `frame: Option<&FrameStatsInput>` 参数（`struct FrameStatsInput { system_prompt: &str, tool_def_tokens: u32, tool_count: u32, rebuild_reason: FrameRebuildReason, created_at: i64, report: Option<&TurnContextReport> }`）。
- Tauri：`get_session_cache_stats(session_id: String) -> Result<SessionCacheStats, Error>`，`SessionCacheStats { calls: u32, input_tokens: u64, cached_input_tokens: u64, hit_ratio: f64 }`（camelCase；按 `cost_records` 该会话逐行用 `prompt_tokens_total` 汇总）；`DailyCostRollup` / `ModelCostRollup` 增加 `cached_input_tokens: i64` 与 `hit_ratio: f64`。

- [ ] **Step 1: 写失败测试**：`record_usage_writes_cached_input_tokens`；`cache_hit_ratio_anthropic_vs_openai_semantics`（claude: 600 input + 400 cached → 0.4；deepseek: 1000 input 含 400 cached → 0.4）；`session_cache_stats_aggregates_per_session`；`daily_rollup_includes_cached_and_ratio`；`context_stats_includes_frame_fields_only_in_transcript_mode`（复用 `context_stats_tests` 模块）。
- [ ] **Step 2: 运行确认失败** `cargo test --lib cost_store && cargo test --lib context_stats`。
- [ ] **Step 3: 实现** 并在 `main.rs` 注册命令。
- [ ] **Step 4: 运行通过**（同上）+ `cargo build`。
- [ ] **Step 5: 提交** `feat(observability): record cached input tokens; session cache hit rate; frame fields in context stats`

---

### Task 12: 前端 — 上下文指示器"提示词缓存"分区 + 用量页缓存列

**Files:**
- Modify: `ui/src/components/agent/ContextUsageBadge.tsx`（+ `.test.tsx`）、`ui/src/components/agent/AgentView.tsx`、`ui/src/atoms/agent-atoms.ts`、`ui/src/hooks/useGlobalAgentListeners.ts`、`ui/src/lib/tauri-bridge.ts`、`ui/src/lib/types.ts`、`ui/src/components/settings/UsageSettings.tsx`

**Interfaces:**
- Consumes：Task 11 `agent:context_stats` 新字段、`agent:turn_cost.cacheHitRatio`、`get_session_cache_stats`、rollup 新字段。
- Produces（TS）：`interface SessionCacheStats { calls: number; inputTokens: number; cachedInputTokens: number; hitRatio: number }`；`getSessionCacheStats(sessionId: string): Promise<SessionCacheStats>`；streaming state 增加 `frozenSystemPrompt?`、`systemPromptTokens?`、`toolDefTokens?`、`toolCount?`、`frameRebuildReason?`、`memoryInjection?`、`turnContextTokens?`、`cacheHitRatio?`；`ContextUsageBadge` 新 props 同名 + `sessionCacheHitRatio?: number`。

- [ ] **Step 1: 写失败测试**（`ContextUsageBadge.test.tsx`）：有 `frozenSystemPrompt` 时 Popover 出现"提示词缓存"分区，含行 `系统提示词` 值 `15.4k · 已冻结`、`工具定义` 值 `59 个 · 11.8k`、`本轮动态块` 值含 `记忆：增量`（`memoryInjection='delta'`）、`本会话命中率` 值 `82%`；`cacheHitRatio` 存在时"命中"徽标用它（不再用前端自算的 totalSent 口径）；无新字段时分区不出现（非 transcript 会话不变）。（`UsageSettings` 测试若存在则加一例：表格出现"缓存命中"列。）
- [ ] **Step 2: 运行确认失败** `cd ui && npm test -- --run ContextUsageBadge`。
- [ ] **Step 3: 实现**：监听器写入新字段；`AgentView` 在 `agent:turn_cost` 后（每轮结束）调用 `getSessionCacheStats` 取会话命中率并传入；`frameRebuildReason` 为 `prompt_changed`/`workspace_changed`/`version_changed` 时 `系统提示词` 行副文本显示 `本会话已重建：提示词模板变化` / `工作区变化` / `应用升级`。`UsageSettings` 日/模型表增加"缓存命中"列（`hitRatio` 百分比）。
- [ ] **Step 4: 运行通过** `npm test -- --run ContextUsageBadge UsageSettings`；`npx tsc --noEmit` 仍为 41。
- [ ] **Step 5: 提交** `feat(ui): prompt-cache section in the context meter and cache column in usage`

---

### Task 13: 前端 — 工具集 / 模式 / 提示词重建提示

**Files:**
- Create: `ui/src/components/agent/PromptFrameNotice.tsx`（+ `.test.tsx`）
- Modify: `ui/src/components/agent/AgentMessages.tsx`、`ui/src/hooks/useGlobalAgentListeners.ts`、`ui/src/atoms/agent-atoms.ts`、`ui/src/lib/types.ts`

**Interfaces:**
- Consumes：Task 6 `agent:prompt_frame` payload。
- Produces（TS）：`interface PromptFrameEvent { conversationId: string; frozen: boolean; rebuilt: boolean; rebuildReason: 'new'|'prompt_changed'|'workspace_changed'|'version_changed'|null; systemPromptTokens: number; toolCount: number; modeChanged: { from: string; to: string } | null; toolsAdded: string[]; toolsRemoved: string[]; memory: 'full'|'delta'|'none'; memoryTokens: number; turnContextTokens: number }`；atom 按会话保存本轮最近一次事件（`promptFrameEventsAtom: Record<sessionId, PromptFrameEvent[]>`，挂在触发它的那一轮用户消息之后，仅当前运行期保留）。
- `PromptFrameNotice({ event })`：只在有可见变化时渲染一行弱化提示（与压缩卡片同风格）：
  - 工具：`工具集已变化：新增 {n} 个（{前 3 个名字}…）· 移除 {m} 个 —— 本轮缓存重建一次`
  - 模式：`权限模式已切换为「{中文模式名}」，已告知模型`
  - 重建（`rebuilt && rebuildReason !== 'new'`）：`系统提示词已重建（{原因}）—— 本轮缓存重建一次`
  - 三者都无 → 返回 `null`。

- [ ] **Step 1: 写失败测试**（`PromptFrameNotice.test.tsx`）：工具新增 25 个时文案含 `新增 25 个` 且只列 3 个名字；模式变化文案；`rebuildReason='new'` 且无其他变化 → 不渲染；`prompt_changed` → `系统提示词已重建（提示词模板变化）`。
- [ ] **Step 2: 运行确认失败** `cd ui && npm test -- --run PromptFrameNotice`。
- [ ] **Step 3: 实现** 组件、监听器与 `AgentMessages` 插入点。
- [ ] **Step 4: 运行通过** `npm test -- --run PromptFrameNotice AgentMessages`；`npx tsc --noEmit` 仍为 41。
- [ ] **Step 5: 提交** `feat(ui): notice when the session's tool set, mode or system prompt changes`

---

### Task 14: 验证与交付

- [ ] **Step 1: 自动化** `cargo build`（0 错误）、`cargo test --lib`（仅已知失败）、`npm test -- --run`（仅已知失败）、`npx tsc --noEmit`（41）、`gitnexus_detect_changes`（只影响预期符号）。
- [ ] **Step 2: 回归基准**（用户执行）`scripts/bench/` 7 条消息：首 token 中位数 ≲ 1.14 s、每次调用输入中位数 ≲ 27K；日志中同一会话所有调用的 `system_prompt_tokens` 恒定、无 `[L2] Tool list changed mid-turn` 警告。
- [ ] **Step 3: 长对话缓存验收**（用户执行，DeepSeek）：同一会话 ≥ 8 轮、含一次浏览器使用与一次模式切换。期望：除首轮、浏览器工具首次公布那一轮、模式无关（模式切换不应失效）外，每轮首次调用的未命中 token ≤ 2K（`agent:turn_cost` input − cacheRead）；`[M2-D]` 日志第 2 轮起为 `emitted=delta` 或 `none`；上下文指示器显示"已冻结"与会话命中率；工具变化与模式切换各出现一次提示。
- [ ] **Step 4: 行为抽查**（用户执行）：问"现在几点"直接回答不调工具；`/compact` 后问早先的要求仍能答出；重启应用后继续对话，第一条请求的命中与重启前一致（缓存未过期时）；新装一个 skill 后问相关任务，`<relevant_skills>` 能指向它（日志或回答中提到用 `load_skill`）。
- [ ] **Step 5:** 推送、开 PR（含 `## Commits (bisectable)` 表与"Adjacent edits"说明：`main.rs` 命令注册、`registry_build` 新参数、`build_request_body` 可见性），CI 通过后合并（需用户确认）。
