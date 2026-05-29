# Pi-Convergence Gap Audit — Agent 框架深度审计与改造策略

**Date:** 2026-05-27
**Status:** Audit report (审计现况分析,非实施计划 — 落地需经 writing-plans)
**Method:** 7 个并行 subagent 代码级核实 + 三参考源(pi / hermes / openhuman,均在本地)真实对比 + 战略文档交叉验证
**Strategic baseline:** `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
**Related specs:** `docs/superpowers/specs/2026-05-26-agent-framework-pi-upgrade-design.md`、`docs/superpowers/specs/2026-05-20-agentic-loop-state-audit.md`
**References (local):** `/Users/ryanliu/Documents/pi`、`/Users/ryanliu/Documents/hermes-agent`、`/Users/ryanliu/Documents/openhuman`
**Author:** claude-opus-4-7 (synthesizer) + 7 subagents

---

## 第 0 部分:元诊断 —— 一句话解释所有问题

uClaw 现在事实上有**两套架构叠在一起**:

|          | **影子架构(应然)**                                                                                                                                                      | **在役架构(已然)**                                                                                                               |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| 形态     | ADR 设计的统一缝:`registries/`(Capability Mesh)、`memory_contract`、`memory_policy`、`plugin_manifest`、`skill_selection`、`harness/`(eval)、`TaskScheduler`、`workers` | `dispatcher.rs`(71 字段上帝对象)、`effective_system_prompt` 内联拼装、8 个并存记忆存储、3 套安全模型、仅靠 `heartbeat.rs` 的监督 |
| 状态     | **骨架 / 死代码 / 被绕过**——`select_top_k`、`MemoryAdapter`、`MemoryPolicyExecutor`、`registry_hub::resolve`、`HarnessRuntime`、`workers` 全部**零生产调用方**          | 真正跑的路径,但每个子系统**各自长出了平行实现**                                                                                  |
| 缝合状态 | "一个 registry、一个 memory 接口、一个 safety chokepoint、一个插件契约" 全部存在于设计,但没接线                                                                         | 每个子系统绕过统一缝,自己实现一份                                                                                                |

**这就是 uClaw 当前最深的结构债:不是缺少好设计,而是好设计停在了类型定义层,从未接入热路径,而热路径在等待期间各自野蛮生长。** 北极星 ADR §17 风险表里自己写下的 "memory splits again"、"Capability Mesh becomes plugin chaos / skeleton" 两条风险——**已经发生了**。

下面所有分模块缺陷,都是这一条元病理的不同切面。

---

## 第 1 部分:分模块缺陷清单(分级 + 证据)

### 1.1 Agent Loop —— Pi 收敛**基本是真的**,但 3 个高危隐患

这是全栈最健康的部分。双队列、迭代压缩 + split-turn、FileOps 都已落地并有单测。问题集中在耦合与隔离:

| 级别        | 缺陷                                                                                                                                                                                                                                                                                                                                             | 证据                                                  |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------- |
| 🔴 CRITICAL | **取消信号没接进真正的 await 点**。`CancellationToken` 只在 `call_llm` 返回**后**轮询(`agentic_loop.rs:89`),而 `stream_completion`(`dispatcher.rs:2214`)和 `ToolDispatcher::dispatch` 都不接收 token——30s 的 LLM 流、长 bash 命令**无法中途中断**。loop-state-audit R-6 要求的修复从未在 flight point 完成,其测试矩阵(#2:"100ms 内返回")无法通过 | `llm_stream.rs` 零 `CancellationToken`/`select!` 引用 |
| 🟠 MAJOR    | **迭代压缩重载后退化成 O(N)**。增量摘要 `previous_fold` 只存内存,`session.rs` 缺少 reconstruct-on-load 播种(设计文档 §6 承诺过)——重启长会话后静默回退到全量再摘要,正是该特性要消除的成本                                                                                                                                                         | `agentic_loop.rs:1154`                                |
| 🟠 MAJOR    | **TurnSnapshot 隔离"空心化"**。snapshot 冻结了 model/prompt/tools,但 cost 计算、context 长度、image policy、日志**全部仍读 `self.model`**(`dispatcher.rs:1206/1265/2096`)。当前 model 不变所以无害,但 Sprint-4 热切换上线时会 snapshot 说 model X、成本算 model Y——潜在正确性 bug                                                                |                                                       |
| 🟠 MAJOR    | `dispatcher.rs` 3853 行、`ChatDelegate` **71 个字段**的上帝对象;`run_turn_body` 有 5 处近乎复制的 `ContentBlock` 拼装,顺序不变量只靠注释维护                                                                                                                                                                                                     | `agentic_loop.rs:99-428`                              |

### 1.2 Prompt 构建 —— "单一缝"在 4+ 处被破坏

| 级别     | 缺陷                                                                                                                                                                                                                                                                                                                                | 证据                      |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| 🟠 MAJOR | **"模型看到什么"由 4 个互不知情的层拼成**,无一权威:`mode_prompts::compose_*`(看似缝)→ `dispatcher.rs:825` 追加 skills manifest → `:1914/:1938/:1942` 追加 plan-hint/项目规则/ladder padding → 另一条 `build_dynamic_context` 手拼 `<system_info>`/`<memory_context>` 等。B2 缓存的"系统 prompt 字节稳定"原则只靠注释维持,无类型保证 | `dispatcher.rs:892-1047`  |
| 🟠 MAJOR | `compose_system_prompt` 有 **4 个组合爆炸变体**(±persona ±injection),加第三个可选输入就再翻倍                                                                                                                                                                                                                                       | `mode_prompts.rs:142-206` |
| 🟡 MINOR | A4 注入通道**接了线但失效**:10 个块全 `Always`,`estimate_context_pressure_ratio` 硬编码 `0.0`,`is_first_act_turn` 自清并留 TODO——真机器,零效果                                                                                                                                                                                      | `dispatcher.rs:878`       |
| 🟠 MAJOR | **无法对真实 prompt 做快照测试**。`compose_*` 有 golden 测试,但下游 4 处变更依赖 `self.db`/atomics/`GLOBAL_FILE_CONTEXT_TRACKER`,不是纯 `state→string`。**未测面与缺陷面完全重合**                                                                                                                                                  |                           |

### 1.3 Skill 系统 —— 两套平行注入器,一个死选择器

| 级别        | 缺陷                                                                                                                                                                                                | 证据                                      |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| 🔴 CRITICAL | `agent/skill_selection/select_top_k` **完全死代码**,零生产调用方;live 路径是另一套 `skills_manifest::build_skills_manifest`,自带排序。两套独立 skill 选择算法,更干净的那套(topic/budget 感知)没人用 | `select.rs:82` vs `skills_manifest.rs:43` |
| 🟠 MAJOR    | **3 个竞争的 skill→prompt 渲染器**,只有 `format_for_system_prompt_xml` 被调用,`build_skill_prompt`/`combined_system_prompt` 是死的                                                                  | `skills.rs:674-712`                       |

### 1.4 插件统一性 —— **抽象不存在**(对"可插拔"目标最致命)

| 级别        | 缺陷                                                                                                                                                                                                                                                                                                                                                                                                                       | 证据 |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| 🔴 CRITICAL | `plugin_manifest` 是**有类型 schema、无安装器、零接线**。唯一引用是 `lib.rs:39` 的 `pub mod`,模块头自己承认 "Installer lives in M7-T1 commit 2"——那个 commit 从未落地                                                                                                                                                                                                                                                      |      |
| 🔴 CRITICAL | **四类插件 = 四个独立子系统,无共享缝**:Skills(SkillsRegistry + manifest + 死 selection)、MCP(`mcp.rs`,且只有 gbrain 一个 server 有硬编码 prompt 块 `gbrain_prompt.rs:91`)、Automation specs(自带 `marketplace/skill_install.rs` 安装器)、CLI tools(dispatcher + SafetyManager)。ADR §9 要求的 `registries/`(RegistryHub)是**意图中的统一缝,但 `registry_hub::resolve` 在 `src-tauri/src/agent/` 里零调用**——agent 从不查它 |      |

### 1.5 Memory —— 8 个存储拼装,"冻结"是装饰性的

| 级别        | 缺陷                                                                                                                                                                                                                                                                                                    | 证据                                                   |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| 🔴 CRITICAL | **至少 8 个并存记忆存储**(`memory.rs` kv / `memory_graph` / gbrain / memu / memorization / learning / `file_ops` / `world`),+ 2 个死抽象层。系统 prompt 由 `effective_system_prompt` **内联拼 4-5 个 store**,各自格式/预算/去重,互不知情。同一事实可同时存在于 gbrain、memory_nodes、memu、kv 表,无对账 | `tauri_commands.rs:2134/2184/2205/2209`                |
| 🔴 CRITICAL | **"gbrain 主、memory_graph 冻结只读"是假的**。pre-commit hook 只拦字面 token `memory_graph::write`,而真实写 API 是 `.create_node()`/`.create_entity_page()`——~20 处运行时仍在写;`MemoryRecallEngine` 每个 chat turn 还在读这个"冻结" store                                                              | `store.rs:65/1152`;hook `check-memory-graph-freeze.sh` |
| 🟠 MAJOR    | `memory_contract`(本应是统一 Memory 接口)与 `memory_policy::MemoryPolicyExecutor`(本应是唯一写入门控)**都是死代码**,零生产调用方。删掉什么都不坏                                                                                                                                                        | `adapter.rs:48`、`executor.rs:88`                      |
| 🟠 MAJOR    | **"类脑双层第二大脑"是 ADR 明确暂停的设计**;`importance_decay.rs`/`spaced_repetition.rs` 等"认知"文件作用在**已弃用的 memory_nodes** 上,即在给陈旧路径加 buff。没有 working/episodic/semantic 分离,只有挂了衰减算法的存储表                                                                             |                                                        |

### 1.6 Harness / Runtime —— 名字撞车 + 死脚手架

| 级别        | 缺陷                                                                                                                                                                                                                                                                        | 证据                       |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| 🔴 CRITICAL | **`src-tauri/src/harness/` 是离线 eval 跑机,不是 ADR 描述的运行时监督器**。`HarnessRuntime` 纯内存、无持久化、无生产摄取,**零非测试调用方**。真实"长任务监督"只有 `heartbeat.rs`                                                                                            | `harness/runtime.rs:13-79` |
| 🟠 MAJOR    | `TaskScheduler`(`runtime/task.rs`)、`workers/spec.rs`、`task_scheduler/queue.rs` 全是**没接线的类型试点**("orchestrator lives in commit 2"——不存在)。生产直接调 `run_agentic_loop`。CLAUDE.md 宣称的"统一 runtime fan-out"实为 **observe-only JSONL 发射**,只记录不统一控制 | `headless.rs:340`          |
| 🟠 MAJOR    | **4 套并行监督词汇**(harness/runtime/workers/heartbeat)靠手维护的 `From` impl + 往返测试桥接,是"用测试维护的重复",非共享内核                                                                                                                                                | `case.rs:157-246`          |

### 1.7 Browser —— Provider 是真的,但"工具里套了第二个 agent loop"

| 级别     | 缺陷                                                                                                                                                                                                             | 证据                                         |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| 🟠 MAJOR | `browser_task` 单个工具内**起了一整个第二 agent loop**(`BrowserAgentLoop::run`,自带 LLM/ask_user/memory)。外层 SafetyManager/budget/heartbeat **看不见**它内部的 N 次导航点击——只门控那 1 次 `browser_task` 调用 | `browser/agent_loop.rs:181`、`tools.rs:2328` |
| 🟡 MINOR | `browser/` 60+ 模块、`tools.rs` 105KB/`agent_loop.rs` 80KB/`recipes.rs` 84KB,面铺得很大;ADR §4.5 的 "thin CDP lane" 未实现(`loop_detector.rs` 标注 stub)                                                         |                                              |

> 注:browser provider/supervisor/playwright-cli 子进程执行是真实可用的,这是三个子系统里最连贯的一个;缺陷仅在 agent→browser 的缝(嵌套 loop)。

### 1.8 Safety —— **不是单一 chokepoint(全栈最高风险)**

| 级别        | 缺陷                                                                                                                                                                                                                                                                                        | 证据                                                                                                                   |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| 🔴 CRITICAL | **三套独立安全模型并存**:`SafetyManager.should_approve`(只在 `tool_dispatch` 交互路径)、browser 的 `boundary.rs` broker、automation 的 `PermissionSet`。**automation run 或 browser 子循环里发的破坏性 shell 命令(它们都注册了 live `BashTool`)不经过 chat 的 `should_approve` chokepoint** | `automation/runtime/` 零 SafetyManager 引用;`register_base_tools` 给每个 automation run 配 BashTool;`safety/mod.rs:96` |

### 1.9 Coding 能力 —— 通用而非专精(对标 hermes 的差距)

🟠 MAJOR:coding 路径只是通用 `run_agentic_loop` + builtin `edit`/`shell`/`search`,**无 coding 专属 harness、无 worktree-aware worker、无强制 per-domain capability profile**。多领域目标被前述 3-silo(tool 装配 / safety / 监督)分裂阻塞,而非缺功能。对比 hermes 的编辑可靠性机制(见 §3.3),uClaw 的 `file_ops.rs` 编辑路径缺少模糊匹配、读回校验、回滚 checkpoint。

---

## 第 2 部分:战略层盲点(单个模块审计看不到的)

### 盲点 ①:北极星 ADR 与产品目标**方向相反**

声明的目标是 **"pi 那种轻量化可插拔多领域 Agent"**;而 `north-star.md` 写的是 **"Agent OS for long-running work"**——重内核,含 Teams、Cluster、4 个 Registry、Capability Card、World Projection、Evolution Factory、自治阶梯 L0-L6。

这两者**不是同一个产品哲学**。Pi 的轻量恰恰来自:一个 `pi` 句柄涵盖所有插件种类、纯无状态 loop、依赖只向下、hooks 是闭包而非 trait。而 Agent OS v2 的 4-Registry + Capability Mesh + World Projection 正是 Pi 提取报告点名的"Rust 移植最容易做错的方向"。

**这是必须先做的决策。** 否则团队会在两个相反方向上同时投入——这也是"影子架构"的根因:ADR 让大家建重型缝(registries/contracts/harness),但 live 路径只需要轻量缝,于是重型缝没人接、轻量需求各自硬编码。

### 盲点 ②:"可插拔"缺一个根本答案 —— 第三方如何不改核心扩展?

Pi 用 jiti **运行时加载 `.ts`**,所以"不重编译核心即可加能力"是真的。**Rust 做不到这点**,而 uClaw **至今没有为这个问题选定机制**。`plugin_manifest` 只解决"描述插件",不解决"加载执行插件"。可选项必须尽早定:

- **子进程 / RPC 插件**(最像 MCP,最现实,跨语言)— 推荐
- **WASM 插件**(沙箱好,生态在成熟)
- **dylib 动态库**(性能最好,ABI 脆弱)
- **编译期注册宏**(最简单,但"插件" = 重编译,等于放弃目标)

不选,则"可插拔"会静默退化成"重编译核心"。

### 盲点 ③:大量死代码不是中性的——它在制造"已完成"的错觉

`select_top_k`、`MemoryAdapter`、`MemoryPolicyExecutor`、`registry_hub`、`HarnessRuntime`、`workers`、`plugin_manifest` 都是"看起来架构很完整"的骨架。它们让 milestone 看起来推进了,但热路径从未受益。**每存在一天,新人就更可能往骨架里加代码而非接线,债务复利增长。**

---

## 第 3 部分:三参考源 → uClaw 的可借鉴蓝图

### 3.1 Pi → 插件契约 + 轻量 loop(解决 1.4 / 盲点 ①②)

- **塌缩 4 个 Registry 为一个 `AgentApi` 句柄**:`register_tool` / `register_provider` / `register_command` / `register_renderer` / `on(event, handler)`。Rust 用一个 `&mut AgentApi` builder + 事件 enum + 返回 patch 结构(`ToolResultPatch`/`ContextResult`),**不要**建 30 方法的 `trait Plugin`。
- **loop 保持纯无状态**:返回事件流,不内嵌 DB 写/session 变更(uClaw loop 已接近此形,继续守住)。
- **hooks 是 `Option<Box<dyn Fn>>` 闭包**,不是 trait object 爆炸。
- **尽早选定运行时插件加载机制**(建议子进程 RPC,与现有 MCP 同构,迁移成本最低)。
- Pi 关键文件:`packages/agent/src/agent-loop.ts`(纯 loop)、`packages/agent/src/types.ts`(loop config + tool 契约)、`packages/coding-agent/src/core/extensions/types.ts:1084`(`ExtensionAPI` 单句柄)、`examples/extensions/preset.ts`(多领域 = data + 扩展,非 core 分支)。

### 3.2 openhuman → 用 bucket-seal 树替换 8 存储(解决 1.5)—— **且它就是纯 Rust,可近乎直接移植**

- **写时准入打分 + 便宜/LLM 边界带**:`≤0.15 drop / ≥0.85 keep / 仅中间带调 LLM`,`<0.3` 直接 tombstone(写时遗忘)。砍掉绝大部分 embedding + LLM 成本。
- **L0 缓冲按 token/数量预算封板 → 向上级联**的层级摘要树(per-source / per-topic-entity / global 日周月年),对应海马体→皮层巩固。直接复用 uClaw 现有 SQLite + `agent_turns`/`agent_messages` 切分。
- **hotness + recency_decay 纯函数**决定话题树懒生成/归档——可测、无新基础设施。
- **粗到细检索原语**(先摘要,按需 `drill_down`/`fetch_leaves`),取代当前平铺多源拼接。
- **深模块小接口**:`load_context(query) -> 2000 字符硬预算` + `ArchivistHook` PostTurnHook 写回——**正好是 `memory_contract::MemoryAdapter` 本应有的形状**(把死接口做实)。
- 存储:SQLite + FTS5 + bge-m3(1024 维,Ollama),cosine 在 Rust 算。无需向量/图 DB。
- openhuman 关键路径:`src/openhuman/memory/tree/`(`ingest.rs`/`score/`/`tree_source/bucket_seal.rs`/`tree_topic/hotness.rs`/`retrieval/`)、`agent/memory_loader.rs`、`agent/harness/archivist.rs`。

### 3.3 Hermes → coding 编辑可靠性(解决 1.9)

- **9 策略模糊匹配链**(exact → line-trimmed → whitespace → indent → escape → boundary → unicode → block-anchor → context-aware):吸收 LLM 输出的空白/缩进/转义漂移。**直接搬进 `agent/file_ops.rs` 编辑路径,ROI 最高。**
- **编辑工具自带三信号**:读回校验(byte 比对,catch 静默失败)+ 增量 lint(过滤既有错误,只报本次引入的)+ LSP delta。
- **影子 git checkpoint store**:每次 mutating 工具调用前 `ensure_checkpoint()`(per-turn 去重),快照入项目外 `GIT_DIR`,不碰用户 `.git`,支持整树/单文件回滚。即 uClaw `code_rescue.rs` 应有的形态。
- **按需读取契约**:分页 + 行号 read、硬 ~100K 字符上限、ripgrep search、read-dedup 缓存,工具描述里写反模式引导("用此工具而非 sed/cat")。
- **压缩后重注入 todo**:`agent/compaction.rs` 是自然落点。
- hermes 关键路径:`tools/fuzzy_match.py`、`tools/file_operations.py`、`tools/patch_parser.py`、`tools/checkpoint_manager.py`、`tools/todo_tool.py`、`agent/lsp/`。

---

## 第 4 部分:改善策略与里程碑计划

按**杠杆 × 风险**排序。前三阶段是"还债",后三阶段是"借鉴"。先给**进取方案**(推荐),再给保守回退。

### 阶段 0(决策,~0.5 天):锁定产品哲学

回答盲点 ①:**"Agent OS v2 重内核" 还是 "Pi 轻量可插拔"?** 建议——**正式收敛到 Pi-lightweight**,把 north-star 的 Teams/Cluster/World Projection/4-Registry 降级为"远期可选",写一份 ADR supersede。否则后续全部白做。同时锁定盲点 ② 的插件加载机制(建议子进程 RPC)。

### 阶段 1(还债 · 安全,1-2 周):合一 safety chokepoint + 接取消信号

- 把 browser/automation 的工具装配与 chat 统一走**一条 tool-assembly + 一个 `SafetyManager`**(解决 1.8 CRITICAL)。
- 把 `CancellationToken` 接进 `stream_completion` 与 `ToolDispatcher::dispatch`(解决 1.1 CRITICAL,补完 R-6)。
- 这两个是**安全/正确性级**,且阻塞多领域统一,必须最先。

### 阶段 2(还债 · 清骨架,1 周):杀死或接线影子架构

- **删**:`select_top_k`、`memory_contract`(待阶段 4 重建)、`MemoryPolicyExecutor`、死的 skill 渲染器、`plugin_manifest`(待阶段 3 重建)。09
- **接线或删**:`HarnessRuntime`、`TaskScheduler`、`workers`——二选一,不留半成品。
- 修真实 freeze:要么真把 `memory_graph` 写 API 封死,要么撤销"冻结"宣称(解决 1.5 CRITICAL 的认知错位)。

### 阶段 3(借鉴 Pi,2-3 周):一个 `AgentApi` 句柄

- 塌缩 registries 为单句柄;现有 tools/MCP/skills/automation 改为通过它注册;落地子进程 RPC 插件加载(一个真插件端到端跑通)。
- 顺手拆 `dispatcher.rs` 上帝对象、统一 prompt 单缝(解决 1.1/1.2 MAJOR)。

### 阶段 4(借鉴 openhuman,2-3 周):bucket-seal 记忆树

- 移植打分准入 + 封板级联树 + hotness/decay + 粗到细检索;做实 `MemoryAdapter` 作为唯一 Memory 缝;`effective_system_prompt` 改为单次 `memory.load_context()` 调用,退役 8-store 内联拼装。

### 阶段 5(借鉴 hermes,1-2 周):coding 可靠性

- 9 策略模糊匹配链 + 读回/lint/LSP 三信号 + 影子 git checkpoint + 按需读取契约。

**保守回退**(若不能停下来还债):只做阶段 1(安全 + 取消)+ 阶段 5 第一项(模糊匹配链)——用户可感知的"更安全 + 编辑更少失败",其余维持现状。但这会让影子架构继续积债,**不推荐**。

---

## 关键证据索引(供后续深挖)

- 插件/prompt:`mode_prompts.rs:142-206`、`dispatcher.rs:825/1914/1938`、`select.rs:82`、`plugin_manifest/schema.rs`、`registries/hub.rs`
- loop:`agentic_loop.rs:89/99-428/1154`、`llm_stream.rs`、`dispatcher.rs:2214`
- memory:`tauri_commands.rs:2134-2230`、`memory_graph/store.rs:65/1152`、`memory_contract/adapter.rs:48`、`memory_policy/executor.rs:88`、`scripts/git-hooks/checks/check-memory-graph-freeze.sh`
- harness/safety:`harness/runtime.rs:13-79`、`automation/runtime/execute.rs`、`browser/agent_loop.rs:181`、`safety/mod.rs:96`
- 参考源:pi `packages/agent/src/agent-loop.ts` + `coding-agent/.../extensions/types.ts:1084`;openhuman `src/openhuman/memory/tree/`;hermes `tools/fuzzy_match.py` + `checkpoint_manager.py`

---

## 核心判断

uClaw 的 agent loop 内部质量其实不错(Pi 收敛多数为真),真正的债在**"统一缝全是骨架、子系统各自平行实现"**,而"Pi 轻量"目标与现有"Agent OS 重内核" ADR 方向相反——**这个矛盾不解决,后面投入会互相抵消。**

> 本报告为现况审计,非实施计划。任何阶段落地前应经 `superpowers:writing-plans` 产出 bisectable 计划,并遵守 ADR §18 的 spec 设计规则。
