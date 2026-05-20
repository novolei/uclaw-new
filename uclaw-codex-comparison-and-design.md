# 《uclaw-codex 对比分析与架构改进设计文档》

> **版本：v2.0（ADR Agent OS v2 北极星对齐版）**
> 日期：2026-05-20
> 范围：codex-rs (OpenAI Codex CLI, ~89 万行 Rust, 115 crate workspace) ↔ uclaw (Tauri 桌面 Agent OS, 13.6 万行 Rust + React 前端)
> 北极星：`docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`（Agent OS v2 ADR, 1059 行）
> 配套：《uclaw 代码库升级改造实施方案》v2.0

---

## v2.0 重大变更说明

本文档是对 v1.0-v1.2 的**全面整合 + ADR 对齐重写**：

1. **接受 ADR Agent OS v2 框架**：uclaw 不是"agent app"，是 **Agent Operating System for Long-Running Work**。所有对比通过 ADR 11 层运行时模型重新组织。
2. **承认 uclaw 现状**：实地核查发现 `harness/`（11 文件成熟）、`learning/`（7 文件 Sprint 1）、`browser/` v2（25+ 文件）、`gbrain/`、`channels/`、`extensions/`、`symphony_graph/` 都已就位。**总代码量 136,283 行，agentic_loop.rs 已达 882 行**。v1.0-v1.2 低估了 uclaw 现状。
3. **License 决策**：推荐 **Apache-2.0** 作为 uclaw 主许可证（详见 §3）。
4. **Hermes Agent 升格为主参考**：ADR §3.1 明确 Hermes 是 capability discipline 的最强本地参考。
5. **路径修订**：v1.0 PR-S/PR-T 等代号全部归入 ADR M0-M9 milestones。

---

## 目录

1. [总览与数据画像](#1-总览与数据画像)
2. [ADR Agent OS v2 速读 + 11 层运行时模型](#2-adr-速读)
3. [License 决策：Apache-2.0 推荐](#3-license-决策)
4. [对比方法论与术语表](#4-方法论)
5. [Intent Layer — IntentSpec / TaskSpec / TaskEvent 契约](#5-intent-layer)
6. [Runtime Kernel — Agent Loop / Session / Task 抢占式调度](#6-runtime-kernel)
7. [Context Fabric — 比 prompt 高一层的运行时层](#7-context-fabric)
8. [Capability Mesh — Tools / Skills / Plugin / MCP 统一为 OS 资源](#8-capability-mesh)
9. [World Projection — UI 真理来源](#9-world-projection)
10. [Safety & Policy — Hooks 13 events + Isolation profiles](#10-safety-policy)
11. [Evolution Layer — Learning / Harness / Proactive Pipeline 化](#11-evolution)
12. [Workers — Subagent / Teams / Cluster](#12-workers)
13. [gbrain & Memory Provider Strategy](#13-gbrain-memory)
14. [Browser Provider Strategy](#14-browser-provider)
15. [Autonomy Ladder L0-L6](#15-autonomy-ladder)
16. [Hermes Agent 作为 plugin discipline 参考](#16-hermes-参考)
17. [可直接复制的 codex Crate 分级清单](#17-crate-清单)
18. [uclaw 现状再评估](#18-uclaw-现状)
19. [改进设计总集（按 ADR Milestone 排）](#19-改进设计总集)
20. [风险与权衡](#20-风险)
21. [附录 A：codex 路径索引](#附录-acodex-路径)
22. [附录 B：uclaw 路径索引](#附录-buclaw-路径)
23. [附录 C：迁移注册表预占](#附录-c迁移注册表)

---

## 1. 总览与数据画像 <a id="1-总览与数据画像"></a>

| 维度 | OpenAI Codex CLI | uclaw |
|---|---|---|
| 仓库根 | `/Users/ryanliu/Documents/Hero/codex` | `/Users/ryanliu/Documents/uclaw` |
| 主语言 | Rust 2024 edition + TS 薄壳 | Rust + TS + 嵌入 Python(memU)/Bun(gbrain) |
| Rust 总行 | ~888,020（剔除 vendor/target） | **136,283**（包含 harness/learning/browser v2 等） |
| Crate 数 | **115 个 workspace 成员**（modular monolith） | **1 个 uclaw_core** + 44 个内部 module |
| 构建系统 | Cargo + **Bazel** 双栈 | Cargo + Tauri |
| 前端 | `codex-cli/` 薄 npm 启动器 + `tui/` 终端 UI | React 18 + Vite + Tailwind + Radix + Jotai（11 主题） |
| IPC | JSON-RPC over stdio/UDS（app-server-protocol v1/v2） | Tauri `invoke` + Axum HTTP @127.0.0.1:27270 + WebSocket |
| 持久化 | **3 SQLite + JSONL rollout**（state_5.sqlite / logs_2.sqlite / goals_1.sqlite） | 单 SQLite `~/.uclaw/uclaw.db`（39 次顺序迁移）+ memorization.db + proactive.db |
| Agent 角色 | default / explorer / worker / awaiter（注释隐藏） | `chat` 单主体 + `agent/teams/`（Worker/Reviewer/Supervisor 内存编排，6 文件） |
| Browser | 无原生 browser 集成 | **browser/ v2 已成熟**（25+ 文件含 identity/perception/loop_detector/recovery） |
| Evaluation Harness | tests + insta snapshots | **harness/ 11 文件**（HarnessEpisode/HarnessCase/HarnessGraderRegistry/SelfImprovementGate） |
| Self-Learning | 无 learning crate | **learning/ 7 文件 Sprint 1**（candidate/stability_detector/scheduler/cache/prompt_section/extractor） |
| Memory | 3 crate（mcp/read/write）+ message-history | gbrain primary + memU auxiliary + memory_graph legacy + memorization（与 ADR §11.2 一致）|
| 工具数 | ~17 个 handler + 内置 toolsets | ~13 个 builtin + MCP + memU 适配 |
| MCP | 既 client 又 server | 仅 client（2024-11-05 协议） |
| Plugin marketplace | 16 个 OpenAI Curated/Bundled + 远程同步 | 未实现；`automation_marketplace` 表已有 |
| 实时语音 | realtime-webrtc + voice mode | 无（有 stt/ 模块） |
| 调度 | 事件驱动 mailbox + maybe_start_turn_for_pending_work | automation/ 完整 spec + activity 体系（V20-V32）|
| IM 集成 | 无 | **channels/im/** 已实现（V32） |
| Workflow 引擎 | 无 | **symphony_graph/ runtime**（V33） |
| 沙箱 | seatbelt/landlock/bwrap/windows-sandbox 多平台 | macOS Tauri 默认 + safety/path_policy + permissions |
| 凭证 | Keychain/SecretService/DPAPI + OAuth | secrets/ 模块 |
| Observability | OpenTelemetry + analytics | observability/ 模块 |
| 实验 API | `#[experimental("...")]` 宏 | 无 |

**核心结论**：
- codex 是 **modular monolith**（115 crate），uclaw 是 **monolithic crate with high internal modularity**（44 module）。
- uclaw 在**多个 ADR 关键层已经领先 codex**：harness、learning、browser v2、IM 集成、Symphony workflow、proactive scenarios。
- uclaw 缺的不是"功能"，是 **ADR 11 层运行时模型的统一组织**（IntentSpec/TaskSpec/TaskEvent 契约、Capability Mesh 五大注册表、World Projection、HookBus 13 events、Evolution Factory pipeline）。

---

## 2. ADR Agent OS v2 速读 + 11 层运行时模型 <a id="2-adr-速读"></a>

### 2.1 一句话定位

> **uClaw is the Agent OS for long-running work: local-first, observable, recoverable, learnable, evolvable, and extensible from one user task to teams and clusters.**

### 2.2 v1 → v2 关键转变

- v1：build a local-first agent control plane
- **v2：build an operating system for agentic work**

> **Keep the kernel small. Make context queryable. Make capabilities replaceable. Make state observable. Make learning gated. Make autonomy resumable.**

### 2.3 11 层运行时模型（ADR §6）

```
┌──────────────────────────────────────────────────────────┐
│ 10. Product Shell  chat / browser / artifacts / timeline │
└────────────────────────┬─────────────────────────────────┘
                         ↓ renders
┌──────────────────────────────────────────────────────────┐
│  9. World Projection  canonical task + environment state │
└────────────────────────↑─────────────────────────────────┘
                         │ materialized from
┌──────────────────────────────────────────────────────────┐
│  2. Runtime Kernel  plan / act / observe / recover /     │
│                     checkpoint                           │
└──┬──────┬──────┬──────┬──────┬──────┬───────────────────┘
   ↓      ↓      ↓      ↓      ↓      ↓
┌─────┐┌─────┐┌─────┐┌─────┐┌─────┐┌────────┐
│  1. ││  3. ││  4. ││  6. ││  8. ││  7.    │
│Intent│Ctx  ││Capa-││Saf- ││Har- ││Workers │
│Layer││Fab- ││bili-││ety& ││ness ││sub/tea-│
│Inte-││ric  ││ty   ││Poli-││scor-││m/clus- │
│ntSpec│ctx  ││Mesh ││cy   ││ecard││ter     │
│Task-││tools││5 reg││hook ││regr-││Worker- │
│Spec ││fold ││+ cap││13 e-││ess  ││Node    │
│Task-││budg-││card ││vents││pack ││TeamSp- │
│Event││ets  ││     ││isolt│└──┬──┘│ec      │
│     ││cita-││     ││ion  │   ↓    │CapPro- │
│     ││tions││     │└──┬──┘   ↑    │file    │
│     │└──┬──┘└──┬──┘   ↓      │    └────────┘
└──┬──┘   │      │   ┌──────────┴────┐
   │      ↓      ↓   │  5. Evolution │
   │   ┌─────────────┤   reflection  │
   │   │ 11. gbrain  ├→  candidate   │
   │   │ primary     │   simulation  │
   │   │ knowledge   │   harness     │
   │   └─────────────┘   promotion   │
   │                  └───────────────┘
   ↓
 IntentSpec 进入系统
```

### 2.4 ADR 9-Milestone 路线图（与 uclaw 现状对照）

| 里程碑 | 内容 | uclaw 现状（v2.0 实地核查） |
|---|---|---|
| **M0** | ADR Lock | ✅ 已完成（ADR Accepted, CLAUDE.md 已引用） |
| **M1** | Runtime Contracts（IntentSpec/TaskSpec/TaskEvent 类型 + 适配器） | ⏳ 部分 —— `harness/trace.rs::HarnessEvent` 是 TaskEvent 雏形，缺统一类型定义 |
| **M2** | Context Fabric（context tools + folding + budgets） | ⏳ 部分 —— `agentic_loop::compress_context_if_needed` 已有；缺显式 7 context 工具 + 8 字段结构化 fold |
| **M3** | Capability Mesh（ToolRegistry / ProviderRegistry / PluginRegistry / CapabilityProfileRegistry / WorkerRegistry） | ⏳ 散落 —— tools/builtin + mcp.rs + skills.rs + providers/ 各自实现，需统一 |
| **M4** | World Projection（materialized view） | ⏳ 无统一 projection；UI 直读各模块 |
| **M5** | Policy Hooks（13 events）+ Isolation profiles | ⏳ safety/path_policy + permissions 部分；无 HookBus；browser/identity 有 profile 隔离基础 |
| **M6** | Browser Provider 抽象 | ⏳ **browser/ v2 已成熟**（25+ 文件），需抽 BrowserProvider trait |
| **M7** | Evolution Factory pipeline | ⏳ **learning/ + harness/self_improvement + proactive scenarios 三块基础已就位**，缺统一 pipeline + Simulation 阶段 + User Review UI |
| **M8** | Teams v1 | ⏳ `agent/teams/` 6 文件已搭好，需 IntentSpec/TaskEvent 化 + ReviewGate 强化 |
| **M9** | Cluster v1 | ❌ 未启动（远期）|

**关键发现**：uclaw 在 **M1/M2/M5/M6/M7/M8 都已有部分实现**，真正完全缺失的只有 M4 World Projection 和 M9 Cluster。这意味着 9-Milestone 路线图的真实工作量比看起来小。

### 2.5 ADR §18 — 未来 spec 必答的 11 个问题

> 任何 uclaw 战略 spec 必须回答：
>
> 1. 支持什么用户 intent？
> 2. 能跑到什么 autonomy 等级？
> 3. canonical truth source 是什么？
> 4. emit 什么 TaskEvent？
> 5. 读什么 context，怎么 cite？
> 6. 添加/消费什么 capability card？
> 7. 什么 policy hook 能 block 它？
> 8. UI 渲染什么 world projection？
> 9. 什么 harness case 证明它工作？
> 10. rollback / disable 路径是什么？
> 11. 它**不拥有**什么？

本文档所有改进设计章节（§5-§15）都按此模式组织。

---

## 3. License 决策：Apache-2.0 推荐 <a id="3-license-决策"></a>

> 用户委托做 license 决策。下面是推荐与完整理由。

### 3.1 推荐：**Apache-2.0**

### 3.2 候选方案对比

| 方案 | 优势 | 劣势 | 适合 uclaw |
|---|---|---|---|
| **Apache-2.0**（推荐） | 与 codex 衍生**直接兼容**；含**显式 patent grant**（商业产品法律保护）；与 Tauri/Rust 生态主流兼容；允许商业化（SaaS / 支持 / 高级 plugin）；NOTICE 流程已因 codex 衍生而必走，统一更简洁 | 公开源码 | ★★★★★ |
| MIT | 最简洁；最广兼容 | **无 patent grant**——对 agent 这种新兴专利不明朗领域是大风险；与 Apache-2.0 衍生混合时 NOTICE 协调略复杂 | ★★★ |
| MIT + Apache-2.0 双许可 | Rust crate 社区标准；最大兼容 | 略复杂；CLA / contributor 管理成本 | ★★★★ |
| BSL 1.1（Business Source）→ Apache 2-4 年后 | 防 SaaS 竞品快速 fork（CockroachDB / Elastic / Sentry 模式） | 与 Apache-2.0 衍生需特别注意 vendor SaaS 条款；社区认知度低；管理成本最高 | ★★★ |
| 闭源 | 完全商业控制 | 与 ADR §3 多次提及的"借鉴 + 社区参考"策略张力大；marketplace 野心几乎不可能；plugin 作者顾虑深 | ★★ |
| AGPL-3.0 | 强 copyleft 防 SaaS 克隆 | **与 Apache-2.0 衍生 crate license 冲突**——直接阻断 v1.2 第 26 章规划的 codex crate 复制；与 ADR §3.1/§3.2/§3.3 借鉴策略不兼容 | ❌ |

### 3.3 选 Apache-2.0 的 6 个理由

**理由 1：与 codex 衍生零摩擦**

本文档第 17 章规划从 codex 复制 17+3 个 utility crate。codex 是 Apache-2.0。同选 Apache-2.0 让 NOTICE / license header / 衍生标注流程**统一为一种格式**，工程开销最小。

**理由 2：含 patent grant —— Agent 产品的关键防御**

Apache-2.0 §3 给所有贡献者**自动免费授予 patent rights**，且若某 contributor 起诉别人专利侵权则其授权立即终止。

对 **AI agent / LLM 工具领域**这点尤其关键 —— 该领域专利地图模糊、潜在专利纠纷多。MIT/BSD 完全没有这条款。

**理由 3：与 Rust + Tauri 生态主流**

- Tauri 本身 MIT/Apache-2.0 双许可
- 主流 Rust crate（tokio/serde/reqwest/sqlx 等）Apache-2.0 OR MIT 双
- uclaw 选 Apache-2.0 与 99% 依赖零冲突

**理由 4：允许完整商业化路径**

Apache-2.0 不是 copyleft —— 它允许：
- 开源 uclaw 核心
- 销售商业版（含闭源高级 plugin）
- 提供 hosted SaaS（云端 cluster）
- 销售支持服务
- 销售官方 marketplace 优先级
- 私有 fork 内部使用

唯一约束：保留原 LICENSE/NOTICE，不假冒 Apache 商标。

**理由 5：呼应 ADR §3 借鉴 + marketplace 野心**

ADR §3 把 Hermes / codex / Claude Code / GenericAgent / hello-halo 列为参考。其中 codex 和大多数借鉴对象都是 Apache-2.0。**uclaw 选 Apache-2.0 = 与所有借鉴对象在同一法律语境**，社区可见度和 plugin 作者信心都最优。

ADR §3.5 引用 hello-halo 的教训："marketplace/plugin ambitions only work when permissions and contracts are clear" —— Apache-2.0 是这条契约清晰度的最佳基础。

**理由 6：NOTICE 已经因 codex 衍生而必走**

v1.2 第 26 章已规定要建 NOTICE。既然反正要做，统一 Apache-2.0 让这个流程最简洁。

### 3.4 不推荐方案的关键原因

- **❌ AGPL-3.0**：直接阻断从 codex 复制（许可冲突）。与 v1.2 §26 规划冲突。
- **❌ 闭源**：与 ADR §3 借鉴姿态张力大。Plugin marketplace（M3 Capability Mesh）几乎不可能建立。
- **△ BSL 1.1**：技术可行，但管理成本高，且与 codex 衍生需特别注意。如担心 SaaS 竞品克隆，建议 **1-2 年后再迁 BSL**；启动期 Apache-2.0 更易积累社区。
- **△ MIT**：缺 patent grant 是 agent 产品的真实风险，不推荐。

### 3.5 落地步骤（**立即执行**）

**Step 1**（10 分钟）：
1. `/Users/ryanliu/Documents/uclaw/LICENSE` —— 复制 Apache-2.0 全文（从 `/Users/ryanliu/Documents/Hero/codex/LICENSE` 直接 cp）
2. `/Users/ryanliu/Documents/uclaw/NOTICE` —— 见 §17.1 模板（列出衍生来源 codex 等）
3. `/Users/ryanliu/Documents/uclaw/licenses/apache-2.0.txt` —— Apache-2.0 全文备份

**Step 2**（Cargo 配置）：
4. Cargo workspace 根 `Cargo.toml`（建立后）加 `license = "Apache-2.0"`
5. `src-tauri/Cargo.toml` 加 `license.workspace = true`

**Step 3**（持续）：
6. CI lint：每个衍生 crate 顶部含 SPDX-License-Identifier 注释 + "Derived from ..." 说明
7. 每次新拷贝外部代码必须更新 NOTICE

### 3.6 商业化 FAQ

**Q：Apache-2.0 下能闭源销售商业版吗？**
A：可以。Apache-2.0 非 copyleft。基于 Apache-2.0 核心做闭源 plugin、闭源 cloud 服务等合法。仅不能 strip 掉原 LICENSE/NOTICE。

**Q：用户能 fork 卖竞品 SaaS 吗？**
A：理论可以（任何开源 license 都允许 fork）。Apache-2.0 不防 SaaS fork。如这是核心担忧，可考虑：
- 1-2 年后迁 BSL 1.1（CockroachDB 模式）
- 保留 uclaw trademark 作为差异化
- 核心 SaaS（M9 cluster + marketplace）单独闭源 + 开源 core

**Q：需要 CLA（贡献者协议）吗？**
A：建议有。可用 Apache 基金会风格 CLA 或 GitHub 自动 CLA bot。

---

## 4. 对比方法论与术语表 <a id="4-方法论"></a>

### 4.1 方法

本文档第 5-15 章按 **ADR 11 层运行时模型**逐层对比。每层四段：

1. **ADR 规约**：层职责、契约、规则
2. **codex 实现**：实地源码引用（路径+签名+行号）
3. **uclaw 现状**：实地核查（codebase 136,283 行）
4. **改进设计**：含与 ADR §18 11 问的回答

### 4.2 术语表

| 术语 | 含义 |
|---|---|
| **IntentSpec** | 用户/触发产生的"类型化目标"（goal + acceptance criteria + constraints + autonomyTarget + riskClass）|
| **TaskSpec** | IntentSpec 的可执行形式（policy + budget + capability profile + output contract + checkpoint policy）|
| **TaskEvent** | 运行时所有重要动作的事件类型（13+ variants）|
| **WorldProjection** | UI/diagnostics 用的物化视图，由 TaskEvent 流构造 |
| **CapabilityProfile** | 任务可见的 capability bundle（含 budget/autonomy/deny-allow）|
| **Capability Card** | 单个能力的元数据（cost/permissions/reliability/harness score）|
| **HookBus** | 13 个 lifecycle hook 的统一总线（can_block/can_mutate/must_emit_event 矩阵）|
| **Autonomy Ladder** | L0(Chat Assist) - L6(Distributed Cluster) 自治阶梯 |
| **Context Fabric** | 上下文运行时层：tools + folding + budgets + citations |
| **Capability Mesh** | tools/providers/plugins/profiles 的统一注册表网 |
| **Evolution Factory** | 学习 pipeline：Reflection → Candidate → Simulation → Harness → User Review → Promotion → gbrain |
| **Isolation Profile** | 工作类型的隔离策略（git worktree / browser session / subagent context / 等）|
| **WorkerNode** | 任何执行单元的统一抽象（local/subagent/worktree/remote/container/mobile/cloud）|

---

## 5. Intent Layer — IntentSpec / TaskSpec / TaskEvent 契约 <a id="5-intent-layer"></a>

### 5.1 ADR 规约

ADR §7.1-7.3 定义 3 个核心运行时类型（已实地复核 ADR 原文）：

**IntentSpec**：每个用户提示、自动化触发、IM 命令、团队指派、集群作业都以此入系统。
```ts
type IntentSpec = {
  id: string
  origin: 'chat' | 'automation' | 'im' | 'team' | 'cluster' | 'system'
  userGoal: string
  acceptanceCriteria: string[]
  constraints: Constraint[]
  autonomyTarget: 'L0' | 'L1' | 'L2' | 'L3' | 'L4' | 'L5' | 'L6'
  riskClass: 'low' | 'medium' | 'high' | 'restricted'
  contextRefs: ContextRef[]
  requestedCapabilities: CapabilityQuery[]
}
```

**TaskSpec**：IntentSpec 的可执行形式。
```ts
type TaskSpec = {
  id: string; intentId: string; goal: string; planRef?: string
  policy: PolicySpec; budget: BudgetSpec; capabilityProfile: string
  outputContract: OutputContract; checkpointPolicy: CheckpointPolicy
}
```

**TaskEvent**：13 种事件类型，所有重要动作必入事件流。

### 5.2 codex 实现

| ADR 类型 | codex 等价物 |
|---|---|
| IntentSpec | **无直接等价** —— codex 把"intent"作为 `Op::UserTurn` 的 payload，没有 acceptanceCriteria/constraints/autonomyTarget/riskClass |
| TaskSpec | `SessionTask` trait（`core/src/tasks/mod.rs`）+ `TaskKind` enum（Regular/Compact/Review/UserShell）+ `TurnContext`；缺显式 budget/capabilityProfile/checkpointPolicy 字段 |
| TaskEvent | `EventMsg` enum（`protocol/src/protocol.rs:1137`，200+ variants）+ rollout `RolloutItem` —— **codex 这一层最成熟**，可直接借鉴 schema |
| WorldProjection | 无显式 projection —— codex UI 直接消费 EventMsg 流 |

codex 关键模式：
- `SessionTask::run` 异步函数返回 `Option<String>`（last_agent_message）
- 任务通过 tokio task spawn + `CancellationToken` 树状级联取消
- 抢占式：`Session::spawn_task` 内部先 `abort_all_tasks(TurnAbortReason::Replaced)`
- 100ms 优雅退出窗口（`GRACEFULL_INTERRUPTION_TIMEOUT_MS`）

### 5.3 uclaw 现状

实地核查：

| ADR 类型 | uclaw 已有 | 缺口 |
|---|---|---|
| IntentSpec | 无统一类型；`Op::*` 类型化部分；mode_suggest 已有启发 | 全部需新建 |
| TaskSpec | `agent/types.rs`、`automation_specs` 表（但不是 task 粒度） | 需统一抽象 + Rust types |
| TaskEvent | **`harness/trace.rs::HarnessEvent`** 已是 task event 雏形 + IPC events | 需统一并扩展到所有域（browser/automation/team）|
| WorldProjection | UI 直读各模块状态 + IPC events 拼接 | 需统一 projection materialized view |

`agent/agentic_loop.rs::run_agentic_loop`（882 行）的 6 阶段（check_signals → compress_context → before_llm → call_llm → handle_response → after_iteration）**已经是 task 循环的雏形**，只是未类型化为 SessionTask + 未发 TaskEvent。

### 5.4 改进设计（M1 Runtime Contracts）

**任务 M1-T1**：在 `src-tauri/src/runtime/contracts.rs` 定义完整 Rust 类型（IntentSpec/IntentOrigin/AutonomyLevel/RiskClass/Constraint/ContextRef/CapabilityQuery/TaskSpec/PolicySpec/BudgetSpec/CapabilityProfileId/OutputContract/CheckpointPolicy/TaskEvent/TaskVerdict/PlanRef/ArtifactRef/HookDecision/BoundaryRef/CheckpointRef/WorkerId）。建议复用 codex `protocol/src/protocol.rs` 的 `EventMsg` 部分作为 TaskEvent 字段命名参考（已实地复核）。

**任务 M1-T2**：把 `agent/agentic_loop.rs` 包成 `SessionTask` 的 `RegularTask` 实现，引入 `Session::spawn_task`/`abort_all_tasks` 抢占式调度（参考 codex 模式）。100ms 优雅退出窗口。

**任务 M1-T3**：现有 `harness/trace.rs::HarnessEvent` 升格为通用 `TaskEvent`（不再仅 harness 内部用）。`harness/case.rs::HarnessSubject` 12 个 subject（AgentLoop/Browser/Tools/Skills/Plugins/Permissions/Hooks/Memory/Gbrain/Tasks/Coordinator/Prompts）作为 event source 维度。

**任务 M1-T4**：写适配器把 `agent/agentic_loop`、`browser/agent_loop`、`automation/runtime/` 的事件**全部转为 TaskEvent**，存入统一 rollout（JSONL + SQLite 双写参考 codex）。**ADR M1 Exit Criteria**：one chat task + one browser task + one automation run 产生 comparable traces。

**任务 M1-T5**：Rollout JSONL 副产物 —— `~/.uclaw/sessions/rollout-<TS>-<UUID>.jsonl`，每 TaskEvent 实时 append。便于 jq 即时审计 + 灾难恢复。

**ADR §18 11 问回答**：
1. **Intent**: 任何用户输入或自动化触发
2. **Autonomy**: L0-L6 全覆盖（IntentSpec.autonomyTarget 决定）
3. **Truth source**: Rollout JSONL（事实）+ State SQLite（索引）
4. **TaskEvent**: 全部 13 种 variants
5. **Context**: IntentSpec.contextRefs 显式声明
6. **Capability**: IntentSpec.requestedCapabilities + TaskSpec.capabilityProfile
7. **Block hooks**: UserPromptSubmit / IntentClassified
8. **Projection**: WorldProjection.intent + .task_state
9. **Harness**: harness/case.rs subject=Tasks
10. **Rollback**: checkpoint policy / rollout replay
11. **不拥有**: 不拥有具体工具实现 / 不拥有 LLM provider

---

## 6. Runtime Kernel — Agent Loop / Session / Task 抢占式调度 <a id="6-runtime-kernel"></a>

### 6.1 ADR 规约

> Runtime Kernel coordinates lifecycle; it does not embed provider-specific logic.

阶段：plan / act / observe / recover / checkpoint。

### 6.2 codex 实现

四层栈（实地核查）：

```
Codex (submission_channel 入口)
   ↓
Session (Arc<Session>, owns active_turn Mutex, input_queue, services, telemetry)
   ↓
SessionTask trait (Regular | Compact | Review | UserShell)
   ↓
TurnContext (per-turn immutable config snapshot)
   ↓
run_turn (内部循环 LLM → tool calls → 再 LLM → ... 直到无 tool call / abort)
```

`Session::spawn_task` 关键代码（`core/src/tasks/mod.rs`）：
```rust
pub async fn spawn_task<T: SessionTask>(self, ctx, input, task) {
    self.abort_all_tasks(TurnAbortReason::Replaced).await;  // 抢占
    self.clear_connector_selection().await;
    self.start_task(turn_context, input, task).await;
}
```

特性：
- 一个 session 至多 1 个活动 task
- `info_span!("turn", ...)` 全 turn 一档 OTel span，记录 6 维 token usage
- `maybe_start_turn_for_pending_work` — mailbox `trigger_turn=true` 唤醒 idle session
- token usage 开始/结束快照取差，按 6 type 各发一次 histogram

### 6.3 uclaw 现状

`agent/agentic_loop.rs::run_agentic_loop`（882 行）+ `agent/dispatcher.rs::execute_tool_calls`：
- **6 阶段成熟**：check_signals → compress_context_if_needed → before_llm_call → call_llm → handle_response → after_iteration
- 三层 LLM 流超时：连接 15s / chunk 45s / 完整 120s
- `ReasoningContext` 状态 + `ThreadState`（Idle/Processing/AwaitingApproval/Completed/Interrupted/Failed）
- cost 记录到 `cost_records`（V13 迁移）
- 工具白名单 `INTERACTIVE_TOOLS` + `is_purely_conversational()` 启发

缺：
- 无显式 SessionTask trait
- 无抢占式 `spawn_task` API
- token 单次累加而非按 type 分维度
- 无 turn span / OTel 集成

### 6.4 改进设计（M1-T6 ~ T9）

**M1-T6（SessionTask 抽象）**：把 `agentic_loop` 重构为 `RegularTask: SessionTask`。新增 `CompactTask`/`ReviewTask`/`UserShellTask`。`Session::spawn_task` API 替代直接调用 agentic_loop。CancellationToken 树状级联。

**M1-T7（6 维 token + Turn span）**：`TokenUsage { input, cached_input, output, reasoning_output, total }` —— 增加 `cached_input_tokens`（OpenAI prompt caching 命中）和 `reasoning_output_tokens`（o1/o3 推理）。每 turn 发 6 个 token_type histogram + `TURN_E2E_DURATION_METRIC` + `TURN_TOOL_CALL_METRIC` + `TURN_MEMORY_METRIC` + `TURN_NETWORK_PROXY_METRIC`。V51 迁移加 `cost_records.cached_input_tokens` + `reasoning_output_tokens` 列。

**M1-T8（Prewarm LLM）**：`main.rs::Stage 3` 启动时 spawn 低优 task 预建 keepalive HTTP/2 连接 + 预协商 SSE。首字延迟预期下降 200-400ms。

**M1-T9（流式分层超时）**：参考 codex `llm/stream_error.rs` 实现 connect_timeout=15s + STREAM_STALL_TIMEOUT=45s + COMPLETE_TIMEOUT=120s 三档（uclaw 已有等价但封装更松）。

**ADR §18 11 问**：同 §5.4。

---

## 7. Context Fabric — 比 prompt 高一层的运行时层 <a id="7-context-fabric"></a>

> 本章是 v1.1 §23（Prompt 深析）+ §24（Token 节约）的 ADR 对齐重组。两者实际是 Context Fabric 的子模块。

### 7.1 ADR §8 核心规则

> **Do not preload the world. Retrieve the right context at the right time, with provenance and budgets.**

### 7.2 9 类 Context Sources（ADR §8.2）

| 来源 | 例子 | Canonical owner |
|---|---|---|
| Conversation | 当前 chat + 历史 turn | agent conversation tables |
| Task trace | TaskEvent 流 | harness/run ledger |
| Codebase | 文件、符号、commits、tests | filesystem/git tools |
| Browser | tabs / DOM / screenshots / action history | BrowserProvider |
| Memory | 持久知识 + 用户/项目 facts | gbrain |
| Artifacts | outputs / diffs / reports / generated files | artifact store |
| Team | role messages + decisions | TeamChannel |
| Automation | schedules / triggers / runs | automation ledger |
| Cluster | worker state + 远程 traces | ClusterManager |

### 7.3 7 个 Context Tools（ADR §8.3）

```
context.search   ← 主动查找相关 artifact
context.read     ← 显式读 artifact（带 budget）
context.fold     ← 结构化压缩（不是 generic summarize）
context.cite     ← 引用 artifact ID + span
context.compare  ← 比对两个 context 状态
context.pin      ← 锁定某 artifact 不被遗忘
context.release  ← 释放 pin（节省 budget）
```

### 7.4 codex 等价 + 实地源码

**codex 的 prompt 是 Context Fabric 在"对话上下文"分支的具体实现**：

| ADR Context Tool | codex 等价 |
|---|---|
| context.search | `tool_search` handler + connector discovery |
| context.read | 工具调用（read_file/grep）；无显式 read tool |
| context.fold | **`compact.rs::run_compact_task`** 三档（local + remote v1 + remote v2）|
| context.cite | **`memory_citation.rs`** —— 已含 fragment_id/span/confidence/origin_thread_id |
| context.compare | **`context_manager/updates.rs::build_*_update_item`** —— diff-based |
| context.pin | 无直接对位 |
| context.release | 无 |

codex 现状覆盖 ~5/7。**关键洞察**：codex 已经做了大半但散落在不同文件，**ADR 要求统一为显式 context.* 工具供模型调用**。

#### 7.4.1 codex 的 base instructions（实地原文 ~10K 字）

`protocol/src/prompts/base_instructions/default.md` 是一份 **12 block 完整行为训练手册**：

- **A. 身份与角色**（5 行明确身份）
- **B. 默认人格**（concise / direct / friendly）
- **C. AGENTS.md spec**（完整作用域/优先级规则）
- **D. Preamble messages**（4 原则 + **8 个具体例句**）
- **E. Planning**（5 信号 + **3 好 plan + 3 坏 plan 对比示例**）
- **F. Task execution**（10+ 硬性规约，含 `NEVER` 列表）
- **G. Validating your work**（specific → broad 测试策略）
- **H. Ambition vs. precision**（新项目 vs 存量代码）
- **I. Progress updates**（8-10 词字数上限）
- **J. Final answer formatting**（~3K 字 style guide：Section Headers / Bullets / Monospace / File References / Structure / Tone / Don'ts）
- **K. Tool Guidelines**（`rg` 优于 `grep` 等）

#### 7.4.2 codex 的 30+ Context Fragment（`core/src/context/`，已实地核查目录）

每个 fragment 独立文件 + render() 方法：EnvironmentContext / PermissionsInstructions / CollaborationModeInstructions / PersonalitySpecInstructions / SkillInstructions / AvailableSkillsInstructions / PluginInstructions / AvailablePluginsInstructions / AppsInstructions / GoalContext / UserInstructions / RealtimeStartInstructions / RealtimeStartWithInstructions / RealtimeEndInstructions / HookAdditionalContext / ModelSwitchInstructions / ContextualUserFragment / SubagentNotification / TurnAborted / ImageGenerationInstructions / UserShellCommand / LegacyApplyPatchExecCommandWarning / LegacyModelMismatchWarning / LegacyUnifiedExecProcessLimitWarning / ApprovedCommandPrefixSaved / NetworkRuleSaved / GuardianFollowupReviewReminder / 等。

#### 7.4.3 codex 的 Diff-based Re-injection（**最大单笔 token 节约**）

`core/src/context_manager/updates.rs`：
```rust
fn build_environment_update_item(prev: Option<&TurnContextItem>, next: &TurnContext, ...) -> Option<ResponseItem> {
    let prev_context = EnvironmentContext::from_turn_context_item(prev?, ...);
    let next_context = EnvironmentContext::from_turn_context(next, shell);
    if prev_context.equals_except_shell(&next_context) {
        return None;  // 没变就不发！
    }
    Some(...diff_from_turn_context_item...)
}
```

同样模式套用 permissions / collaboration_mode / realtime。**reference_context_item baseline + 每 fragment 独立 diff 函数**。

#### 7.4.4 codex 的 7 层 Token 防线

```
Layer 1: TruncationPolicy(Bytes|Tokens) + truncate_middle（首尾保留删中间）
Layer 2: model_visible_input_schema 精简 + ToolFilter + ToolExposure
Layer 3: per-turn skills/plugins 注入（不全量）+ default_skill_metadata_budget
Layer 4: diff-based context updates（reference_context_item baseline）
Layer 5: image stripping（model-aware）
Layer 6: ensure_call_outputs_present（合法化历史防 API 400 重试）
Layer 7: 三档 compaction（Local + Remote v1 + Remote v2）
Buffer: effective_context_window_percent = 95（不用满）
```

关键常量：`COMPACT_USER_MESSAGE_MAX_TOKENS = 20_000` 硬限。

#### 7.4.5 codex 的 InitialContextInjection 双模式

```rust
pub(crate) enum InitialContextInjection {
    BeforeLastUserMessage,  // mid-turn auto-compact 用
    DoNotInject,            // 用户主动 /compact 用
}
```

mid-turn 压缩后 summary 放最后 user msg 之前；manual compact 清空历史下轮全量注入。

#### 7.4.6 codex 的 Structured Folding（codex 简版 vs ADR 完整版）

codex `core/templates/compact/prompt.md` 仅保留 4 类（progress / context / remaining / critical data）。ADR §8.4 要求 **8 字段**：facts / decisions / unresolved questions / evidence refs / failed attempts / active constraints / next actions / rollback points。

**ADR 比 codex 更严格**。

### 7.5 uclaw 现状

| 项 | uclaw 已有 | 缺口 |
|---|---|---|
| base prompt | `agent/prompts/baseline.md` + `mode_prompts.rs` 字面量 | 简略，缺 12 block 训练 |
| AGENTS.md 等价 | 仅顶层 CLAUDE.md（给 Claude Code）| 无 UCLAW.md 项目级注入 |
| Context fragment 抽象 | 散落代码 | 无 trait 化 |
| Diff-based updates | 无 | 每轮全发，长会话 token 浪费严重 |
| Template 引擎 | format! / String::replace | 无严格 `{{ var }}` 引擎 |
| TruncationPolicy | 仅 harness 模式活跃（`harness/budget.rs::ToolBudgetManager`） | 生产模式工具输出不截短 |
| Tool exposure 控制 | 无 | 全量 schema 每轮发 |
| Per-turn skills | 部分修复（2026-05-13 PR）| 仍偏全量 |
| Image stripping | 需现地审计 | 模型不支持图时全量浪费 |
| ensure_call_outputs | 需现地审计 | 不合法时 API 400 → 重试浪费 |
| Compaction | 单本地 `compress_context_if_needed` | 无远程委派、无双模式、无 hooks |
| effective context window | 需审计 | 偶发尾段质量下降 |

### 7.6 量化痛点（用户原话："token 消耗很大让人失望"）

保守估计当前一个 50-turn 会话：
- uclaw 比 codex **多发 200K-1M tokens**
- 按 Claude Sonnet 4.6 ~$3/M input + $15/M output，**每会话多花 $0.5-3**
- 重度用户 10 会话/天 × 30 天 = **月度多花 $150-900**

### 7.7 改进设计（M2 Context Fabric）

**ContextFabric 完整重构** —— 把 v1.1 PR-S1 ~ PR-S9（Prompt 系列）+ PR-T1 ~ PR-T12（Token 系列）统一归入 M2 milestone：

#### M2-A：Base instructions 完整重写（对齐 codex 12 block）

新 `src-tauri/src/agent/prompts/baseline.md`（~8K 字 / ~2K tokens），含 12 block：身份 / 人格 / UCLAW.md 规约 / Preamble（8 例句）/ Planning（3 好 + 3 坏）/ 任务执行（10+ 规约）/ 验证 / Ambition / Progress / Final answer formatting（~3K 字 style guide）/ 工具指南 / **NEVER 列表（≥ 12 条）**。

#### M2-B：UCLAW.md 项目级指令注入

`ProjectDocManager` 仿 codex `AgentsMdManager`，从工作区根（`.git` 或 `.uclaw/` marker）向 cwd 收集 `UCLAW.md`（`UCLAW.override.md` 优先），用 `\n\n--- project-doc ---\n\n` 分隔。

#### M2-C：30+ Context Fragment 抽象（对位 codex `core/src/context/`）

新 `src-tauri/src/agent/context/`：trait `ContextFragment { fn render(&self) -> String; fn token_estimate(&self) -> usize; }` + 20+ 实现（EnvironmentContext / PermissionsInstructions / PersonalitySpecInstructions / AvailableSkillsInstructions / AvailablePluginsInstructions / GoalContext / SubagentNotification / UserInstructions / ModelSwitchInstructions / TurnAborted / HookAdditionalContext / 等）。每个 fragment 配 snapshot test。

#### M2-D：Diff-based re-injection（最大节约 30-50%）

`ContextManager.reference_context_item: Option<TurnContextItem>` + 每 fragment `build_update_item(prev, next) -> Option<ResponseItem>`。仅在变化时注入。

#### M2-E：自研 Template 引擎（直接复制 codex `utils/template`）

442 行零依赖 Rust，`{{ name }}` 严格语法 + `LazyLock::new` 编译时校验 + panic on parse error。

#### M2-F：7 个 Context Tools 实现

```rust
// 暴露给 LLM 的工具
context.search(query, sources?, top_k?) -> Vec<ContextRef>
context.read(ref, budget?) -> Artifact
context.fold(items[], strategy?) -> StructuredFold  // 8 字段
context.cite(ref, span?) -> Citation
context.compare(ref_a, ref_b) -> Diff
context.pin(ref, ttl?) -> PinHandle
context.release(pin_handle) -> ()
```

每个调用 emit `TaskEvent::ContextRead/Write/Pinned/Released`。

#### M2-G：8 字段结构化 Fold（严于 codex）

```
## Facts (每条带 evidence_ref)
## Decisions (含理由)
## Unresolved Questions
## Evidence Refs (artifact ID + span)
## Failed Attempts (避免重复)
## Active Constraints (budget / policy / user preference)
## Next Actions
## Rollback Points
```

#### M2-H：7 层 Token 防线落地

1. **L1 TruncationPolicy**（复制 codex `utils/output-truncation` + `utils/string`）—— 默认 budget：shell/exec 8K / file 4K / search 4K / web 6K / mcp 5K tokens。truncate_middle 保留首尾。formatted_truncate_text 加 `Total output lines: N` 头。
2. **L2 ToolExposure**（Always/OnDemand/Hidden）+ schema 精简（去 description.examples / 合并 enum / 裁剪 nested 第 3+ 层）
3. **L3 Per-turn skills**：top-K（默认 5）+ `default_skill_metadata_budget = 1500 tokens`
4. **L4 Diff updates**（M2-D）
5. **L5 Image stripping**：每 provider 配 `supports_images: bool`
6. **L6 ensure_call_outputs_present**：扫描历史合法性
7. **L7 三档 compaction**：local + remote v1 + remote v2（feature flag 切换）+ pre/post-compact hooks + CompactionAnalytics 5 维（trigger/reason/implementation/phase/status）
- **Buffer**：`effective_context_window_percent = 92`（uclaw 推理型 LLM 较多，比 codex 95% 更保守）

#### M2-I：Prompt caching 优化

前缀稳定（base + UCLAW.md + skill manifest 在最前面），变化项放末尾，最大化 OpenAI/Anthropic prompt caching 命中率（OpenAI 1/2 价格，Anthropic 1/10）。

#### M2-J：Token budget UI dashboard

Settings → Token Usage：context 占用 progress bar / 累计 cost / 工具 top-10 / 接近 context window 告警。

### 7.8 预期收益（量化）

| 场景 | 当前每 turn input | 优化后每 turn input | 节约 |
|---|---|---|---|
| 短对话（5 turn） | 5K-15K | 3K-8K | 30-45% |
| 中等会话（20 turn） | 20K-80K | 8K-30K | **55-65%** |
| 长会话（50+ turn） | 50K-300K | 15K-100K | **65-75%** |
| 自动化任务 | 100K-1M | 30K-300K | **60-70%** |

月度成本预测（重度用户 100 会话/月 × 平均 25 turn）：
- **当前 ~$150-400/月 → 优化后 ~$40-130/月（节约 ~70%）**

**ADR §18 11 问回答**：
1. **Intent**：所有需要上下文的任务
2. **Autonomy**：L0-L6 全适用
3. **Truth source**：ContextArtifact store + gbrain
4. **TaskEvent**：ContextRead / Write / Pinned / Released（4 个 ADR §7.3 已列）
5. **Context**：自己就是 context 层
6. **Capability**：context.* 7 工具是 capability cards
7. **Block hooks**：PreContextRead（hook 可阻止某来源读）
8. **Projection**：WorldProjection.context_reads
9. **Harness**：harness/case.rs subject=Prompts
10. **Rollback**：context.release 释放 pin / fold 可重新生成
11. **不拥有**：不拥有持久知识（→ gbrain）/ 不拥有 prompt 内容决策（→ Intent Layer）

---

## 8. Capability Mesh — Tools / Skills / Plugin / MCP 统一为 OS 资源 <a id="8-capability-mesh"></a>

> 本章把 v1.0 §10-13（Tools/Skills/Plugin/MCP）+ §15 mode 切换 + Hermes 借鉴**全部统一**到 ADR §9 Capability Mesh 单一层。

### 8.1 ADR §9 核心：每个能力都有 Capability Card

```yaml
id: browser.local_chromium
kind: provider  # tool | provider | plugin | script | source | worker | model
family: browser
description: ...
permissions: [network.browser, local.profile.read, ...]
cost:
  money: local | $X.YY/call | $X.YY/M_tokens
  latency: low | medium | high
  tokenPressure: low | medium | high
reliability:
  harnessScore: 0.82  # ★ harness 评估的可靠度
  lastEvaluatedAt: ...
failureModes: [captcha, login_required, site_blocks_automation]
humanBoundaries: [credential_handoff, payment, privacy_sensitive_form]
```

**关键洞察**：planner 看的不是"工具列表"，而是"capability cards"——含成本、可靠度、风险面。

### 8.2 五大 Registry（ADR §9.2）

| Registry | Owns |
|---|---|
| `ToolRegistry` | schema, handler, toolset, check function, display metadata, result limits, override state |
| `ProviderRegistry` | provider families, active selection, health, config schema, harness cases |
| `PluginRegistry` | plugin discovery, manifests, enablement, hooks, install/update/remove |
| `CapabilityProfileRegistry` | named capability bundles, budgets, deny/allow rules |
| `WorkerRegistry` | local/subagent/team/remote workers, capabilities, heartbeat, load |

### 8.3 codex 与 uclaw 现状

| Registry | codex | uclaw | 差距 |
|---|---|---|---|
| ToolRegistry | `core/src/tools/registry.rs` 集中 + 17 handler | `agent/tools/builtin/` + `mcp.rs` + `skills.rs` 散落 | 需统一抽象 |
| ProviderRegistry | `model-provider*` + `models-manager` + `codex-mcp` | `providers/` + `mcp.rs` + memU + gbrain 散落 | 需统一 + 抽 BrowserProvider |
| PluginRegistry | `core-plugins/` 完整 + Marketplace（16 OpenAI Curated/Bundled）+ 远程同步 | **无** + `automation_marketplace` 表 + `extensions/` 入口 | 新建（Hermes-aligned）|
| CapabilityProfileRegistry | 无直接对位（用 role config layer） | **无**；`mode_prompts.rs` 是简化版 | 新建 |
| WorkerRegistry | 无（仅 agent registry）| `agent/teams/` + browser session + automation activity | 需建抽象 |

### 8.4 Plugin Manifest（ADR §9.3 完整 schema）

```yaml
id: browser-use-cloud
name: Browser Use Cloud Provider
version: 0.1.0
kind: backend  # standalone | backend | exclusive | platform | model-provider
category: browser
provides:
  browser_providers: [browser-use]
capabilities: [browser.session, browser.snapshot, browser.action]
hooks: [pre_gateway_dispatch, post_tool_call]
toolsets: [browser_tasks]
permissions:
  network: true
  secrets: [BROWSER_USE_API_KEY]
requiresEnv:
  - name: BROWSER_USE_API_KEY
    secret: true
gateway:
  supported: true
  preferredWhenEnabled: true
harnessCases:
  - ./harness/browser-navigation.json
```

**4 种 plugin source**（ADR §3.1 Hermes 模式）：
- bundled
- user
- project-trusted
- external

**5 种 plugin kind**：standalone / backend / exclusive / platform / model-provider

### 8.5 CapabilityProfile（ADR §9.4）

```yaml
id: browser_research_l3
autonomyMax: L3  # 阶梯上限
allowedToolsets: [core, memory_recall, browser_tasks, web_search]
deniedCapabilities: [filesystem.write, shell.exec]
budget:
  maxToolCalls: 80
  maxBrowserActions: 40
  maxCostUsd: 3.00
requiresApproval: [credential_handoff, payment, destructive_action]
```

**关键统一**：
- v1.0 mode（plan/ask/bypass/accept_edits）= **CapabilityProfile 简单情形**
- v1.1 ToolExposure(Always/OnDemand/Hidden) = **CapabilityProfile allowed/denied lists 子集**
- **应合并入 CapabilityProfileRegistry**，不再各自独立

### 8.6 codex 内置工具清单（已实地核查 `core/src/tools/handlers/`）

17 个 handler：shell / apply_patch / unified_exec / agent_jobs / multi_agents / multi_agents_v2 / goal / plan / mcp / mcp_resource / tool_search / request_user_input / request_permissions / request_plugin_install / view_image / test_sync / dynamic / extension_tools。

uclaw 13 个：ask_user / edit / exit_plan_mode / file / load_skill / plan / plan_mode / search / self_eval / shell / skill_search / web / + memU + MCP 适配。

**缺失关键工具**（建议 M3 新增）：mcp_resource / request_permissions / request_plugin_install / view_image / tool_search / unified_exec / agent_jobs。

### 8.7 codex Skills 系统（已实地核查）

双 crate：`skills/` 嵌入式 system skills + `core-skills/` 业务（loader/manager/injection/render）。

4 个 SkillScope：`User` / `Repo` / `System` / `Admin`（已实地核对 `core/src/skills.rs`）。

实地仓库自带 12 个 skill（`.codex/skills/`）：babysit-pr / code-review / code-review-breaking-changes / code-review-change-size / code-review-context / code-review-testing / codex-bug / codex-issue-digest / codex-pr-body / remote-tests / test-tui / update-v8-version。

格式：YAML frontmatter（name + description 必需）+ markdown 正文。

### 8.8 codex Plugins（OpenAI Curated/Bundled 白名单）

`core-plugins/src/lib.rs` 实地核查：
```
github / notion / slack / gmail / google-calendar / google-drive /
openai-developers / canva / teams / sharepoint / outlook-email /
outlook-calendar / linear / figma  ← 14 个 OpenAI Curated
chrome / computer-use              ← 2 个 OpenAI Bundled
```

### 8.9 codex MCP（既 client 又 server）

- Client：`rmcp-client/` + `codex-mcp/src/mcp_connection_manager.rs` 聚合
- Server：`mcp-server/` 暴露 codex 自身能力
- Tool filter + normalize_tools_for_model + tool_with_model_visible_input_schema
- consequential_tool_message_templates.json 标识 destructive 工具

uclaw 仅 client（`src-tauri/src/mcp.rs`，2024-11-05 协议）。

### 8.10 改进设计（M3 Capability Mesh）

**M3-T1**：定义 5 Registry trait + struct（统一 `src-tauri/src/capabilities/mod.rs`）

**M3-T2**：Capability Card 类型 + YAML serde + harness score 字段

**M3-T3**：把现有 tools/builtin / mcp / skills / providers 注册到 ToolRegistry / ProviderRegistry。`mcp.rs::SharedMcpManager` 注册到 ProviderRegistry 作为 backend kind。

**M3-T4**：新增缺失工具 —— `mcp_resource` / `request_permissions` / `request_plugin_install` / `view_image` / `tool_search` / `unified_exec`（V47 迁移配套 `agent_jobs` 表）

**M3-T5**：Skill 作用域（User/Repo/Workspace/System）+ Per-turn top-K + budget（`default_skill_metadata_budget`）。`learning/` 提取的学习 skill 进 Repo scope。

**M3-T6**：Plugin manifest 解析 + PluginRegistry 实现。4 source（bundled/user/project-trusted/external）+ 5 kind（standalone/backend/exclusive/platform/model-provider）。Hermes-aligned discovery + install/update/remove。V43 迁移 `installed_plugins` 表（编号以提交时为准）。

**M3-T7**：把现有扩展迁移为 plugin：memU / gbrain（如可能）/ automation_installed_skills（V22 是 plugin 表前驱）。

**M3-T8**：CapabilityProfileRegistry + 把 mode_prompts.rs 重构为 profile TOML（`~/.uclaw/profiles/<name>.toml`）+ ToolExposure 子集纳入。

**M3-T9**：MCP server 暴露 —— 让 uclaw 自身作为 MCP server，给 VS Code / Cursor / Claude Desktop 集成。

**M3-T10**：consequential templates —— `~/.uclaw/mcp_consequential_templates.json` 标识 destructive 工具 + 审批文案模板。

**M3-T11**：LLM 主动派生 `request_plugin_install` 工具，UI 弹窗给用户确认。

**ADR §18 11 问**：
1. **Intent**：任何需要选择 capability 的任务
2. **Autonomy**：L0-L6（profile autonomyMax 决定）
3. **Truth source**：5 Registry + manifest 文件
4. **TaskEvent**：CapabilitySelected / ToolCall / ToolResult / PolicyHook
5. **Context**：CapabilityProfile + harness scorecard
6. **Capability**：本层就是 capability 层
7. **Block hooks**：PreToolUse / PermissionRequest
8. **Projection**：WorldProjection.active_capabilities
9. **Harness**：每个 capability 自带 harnessCases
10. **Rollback**：plugin uninstall / capability profile downgrade
11. **不拥有**：不拥有产品意图（→ Intent Layer）/ 不拥有工具具体实现（在 handler 内）

---

## 9. World Projection — UI 真理来源 <a id="9-world-projection"></a>

### 9.1 ADR §7.4 + M4

> WorldProjection is not another store of record. It is a materialized view over task events, run ledgers, provider status, and memory receipts.

必须回答 11 问：what user wants / current plan / what happened / waiting on / active tools-providers-workers / context reads / memory writes / boundaries hit / can resume / harness score / where to click。

### 9.2 codex vs uclaw

| 维度 | codex | uclaw |
|---|---|---|
| projection 层 | 无显式 —— UI 直接消费 EventMsg + 调用 thread/read v2 RPC | 无显式 —— UI 直接读各模块状态 |
| 状态一致性 | rollout JSONL + state SQLite + ThreadId 三处对齐 | SQLite + Tauri IPC events 拼接 |
| Plan 可视化 | `update_plan` tool + PlanDeltaEvent | `plan_state.rs` + `plan_mode.rs` 内部状态 |

### 9.3 设计（M4-T1）

新 `src-tauri/src/projection/world.rs`：

```rust
#[derive(Debug, Clone, Serialize)]
pub struct WorldProjection {
    pub intent: Option<IntentSpec>,
    pub current_plan: Option<PlanSnapshot>,
    pub task_state: TaskState,  // Running / Idle / WaitingForUser / Completed / Failed
    pub history: Vec<TaskEventSummary>,
    pub waiting_on: Option<BoundaryRef>,
    pub active_capabilities: Vec<CapabilityCard>,
    pub active_workers: Vec<WorkerSnapshot>,
    pub context_reads: Vec<ContextReadSummary>,
    pub memory_writes: Vec<MemoryWriteReceipt>,
    pub boundaries_hit: Vec<BoundaryEvent>,
    pub resume_checkpoints: Vec<CheckpointRef>,
    pub latest_harness_score: Option<HarnessVerdict>,
    pub action_affordances: Vec<UserAffordance>,
    pub version: u64,  // 用于 diff_since
}

impl WorldProjection {
    pub fn apply_event(&mut self, event: &TaskEvent);
    pub fn diff_since(&self, version: u64) -> ProjectionDelta;
}
```

UI React 端：

```ts
const projection = useWorldProjection(taskId)
// 单一 hook 替代 20+ store
```

**ADR §18 11 问**：
1. **Intent**：所有任务
2. **Autonomy**：L0-L6
3. **Truth source**：TaskEvent 流（projection 是 derived）
4. **TaskEvent**：消费所有 13 种
5. **Context**：projection.context_reads
6. **Capability**：projection.active_capabilities
7. **Block hooks**：无（projection 是只读 derived state）
8. **Projection**：本层就是 projection
9. **Harness**：projection.latest_harness_score
10. **Rollback**：projection 可从任意 version 重建
11. **不拥有**：不拥有 truth（→ TaskEvent stream）/ 不拥有 UI 样式（→ Product Shell）

---

## 10. Safety & Policy — Hooks 13 events + Isolation profiles <a id="10-safety-policy"></a>

### 10.1 ADR §10.1 13 个 Policy Hook（完整矩阵）

下表与 ADR §10.1 一字对应。**Can block** 列含义：hook 返回否决可阻止该动作；**Can mutate**：hook 可修改 payload；**Must emit event**：hook 执行必须 emit TaskEvent 入 trace。

| # | Hook | Can block | Can mutate | Must emit event | 主要用途 |
|---|---|---|---|---|---|
| 1 | UserPromptSubmit | ✓ | ✓ | ✓ | 用户输入审计 + 改写（如自动加项目上下文） |
| 2 | IntentClassified | ✓ | ✓ | ✓ | 分类后审批（如 high-risk intent 强制降级 autonomy）|
| 3 | PreContextRead | ✓ | ✓ | ✓ | 限制可读 context source / 改写 budget |
| 4 | PostContextRead | ✗ | ✓ | ✓ if changed | 注入额外 context / overlay |
| 5 | PreToolUse | ✓ | ✓ | ✓ | 否决 destructive 工具 / 改写参数 |
| 6 | PostToolUse | ✗ | ✓ | ✓ if changed | 工具结果脱敏 / 改写 |
| 7 | PreMemoryWrite | ✓ | ✓ | ✓ | 阻止未授权记忆写入 / 强制 receipt |
| 8 | PreBrowserAction | ✓ | ✓ | ✓ | 阻止 destructive 浏览器动作 |
| 9 | PermissionRequest | ✓ | ✗ | ✓ | 自动批准/拒绝某些权限请求 |
| 10 | SubagentStart | ✓ | ✓ | ✓ | 限制 subagent 数 / 改写 role |
| 11 | WorkerAssignment | ✓ | ✓ | ✓ | 任务到 worker 的分配策略 |
| 12 | PrePromotion | ✓ | ✓ | ✓ | Evolution candidate 上线前最后审批 |
| 13 | SessionEnd | ✗ | ✗ | ✓ | 会话结束钩子（仅 emit，无法 block）|

codex 的 hooks 系统是 8 events（PreToolUse / PermissionRequest / PostToolUse / PreCompact / PostCompact / SessionStart / UserPromptSubmit / Stop）。**ADR 扩展到 13**，新增 IntentClassified / PreContextRead / PostContextRead / PreMemoryWrite / PreBrowserAction / SubagentStart / WorkerAssignment / PrePromotion。

**关键设计规则**：
- 所有 **Can block** hook 返回否决时，**必须** emit BoundaryEvent 入 trace（用户可见）
- 所有 **Can mutate** hook 改写 payload 时，**必须** emit "what changed" 信息
- **SessionEnd is special**：不能 block / 不能 mutate，但 emit 是强制的（用于审计）

### 10.2 ADR §10.2 Human Boundary 7 类

必须人介入：
- credentials / login handoff
- CAPTCHA / bot challenge
- payment / purchase
- destructive filesystem / database
- sending messages externally
- publishing / deploying
- private / sensitive data exposure
- policy downgrade / autonomy escalation

### 10.3 ADR §10.3 Isolation 7 工作类型

| Work type | Isolation |
|---|---|
| quick chat | conversation scope |
| local coding | **git worktree** or explicit dirty-tree policy |
| browser task | browser session/profile scope |
| subagent exploration | fresh context, restricted tools |
| automation run | run ledger + task checkpoint |
| team role | role context + team channel |
| remote worker | data locality + capability policy |

### 10.4 codex / uclaw 现状

codex：
- Hooks 8 events 完整（`hooks/src/lib.rs` 实地核查）
- 三平台 sandbox（seatbelt/landlock/bwrap/windows-sandbox）
- `attestation.rs` + `process-hardening` crate
- consequential_tool_message_templates.json
- 编码任务建议 git worktree（CLAUDE.md 提到）
- `core/src/codex_delegate.rs::run_codex_thread_interactive` 子 agent 独立 Codex 实例

uclaw：
- `safety/permissions.rs` + `path_policy.rs`（部分）
- `agent/dispatcher.rs` 内嵌审批
- 无 HookBus
- `browser/identity` profile broker（browser session isolation 雏形）
- Tauri sandbox

### 10.5 改进设计（M5 Policy Hooks + Isolation）

**M5-T1**：HookBus 实现，13 events 全覆盖。配置：`~/.uclaw/hooks.toml` + plugin 注册。`PluginHookDeclaration` 类型化。V51 迁移 `hook_configs(event, matcher_regex, command_argv, execution_mode, trust_status, ...)`。

**M5-T2**：HookEvent + HookResult 类型化。execution_mode（serial/parallel）+ matcher regex + sandbox 化（hook 命令受 SafetyManager 治理）。

**M5-T3**：13 事件集成点 —— UserPromptSubmit（agent loop 入口）/ IntentClassified（IntentSpec 构造完成）/ PreContextRead+PostContextRead（context.* tools）/ PreToolUse+PostToolUse（ToolOrchestrator）/ PreMemoryWrite（memory writes）/ PreBrowserAction（browser/agent_loop）/ PermissionRequest（SafetyManager）/ SubagentStart（codex_delegate 等价）/ WorkerAssignment（teams orchestrator）/ PrePromotion（Evolution Factory）/ SessionEnd（session.rs）。

**M5-T4**：IsolationProfile trait + 7 工作类型实现：
```rust
pub enum IsolationScope {
    ConversationScope { thread_id: ThreadId },
    GitWorktree { base_branch: String, worktree_path: PathBuf },
    DirtyTreePolicy { allowed_paths: Vec<PathPattern> },
    BrowserSession { profile_id: BrowserProfileId },
    SubagentContext { parent: SessionId, restricted_tools: Vec<ToolName> },
    AutomationRun { run_id: RunId, ledger_path: PathBuf },
    TeamRoleContext { role: WorkerRole, channel: ChannelId },
    RemoteWorker { worker_id: WorkerId, locality: DataLocality },
}
```

**M5-T5**：Git worktree 自动创建（针对 coding task）+ dirty-tree 检查 + 退出时清理 / merge。

**M5-T6**：Consequential templates `~/.uclaw/mcp_consequential_templates.json` + UI 审批弹窗模板化。

**ADR §18 11 问**：
1. **Intent**：所有 risky 任务
2. **Autonomy**：L1+（L0 无 side effects）
3. **Truth source**：hook decisions log
4. **TaskEvent**：PolicyHook / BoundaryEvent
5. **Context**：hook 可读 context
6. **Capability**：本身不消费 capability
7. **Block hooks**：本身就是 hook 层
8. **Projection**：projection.boundaries_hit
9. **Harness**：harness/case.rs subject=Hooks/Permissions
10. **Rollback**：hook 决定可审计 + 撤销
11. **不拥有**：不拥有业务逻辑

---

## 11. Evolution Layer — Learning / Harness / Proactive Pipeline 化 <a id="11-evolution"></a>

### 11.1 ADR §13 Evolution Factory Pipeline

```
Runtime Trace → Reflection → Candidate Builder → Simulation → Harness → User Review → Promotion Registry → gbrain
```

### 11.2 9 类 Candidate（ADR §13.1，**完整 9 项**）

```
1. gbrain page update              ── 持久知识更新
2. skill or SOP                    ── 可执行操作流程
3. browser script                  ── 网站 workflow 自动化
4. prompt patch                    ── prompt 模板调整
5. planner heuristic               ── 规划启发式
6. capability profile adjustment   ── capability 边界微调
7. policy hook                     ── 新增 hook 规则
8. failure memory                  ── 失败模式备忘
9. regression harness case         ── 回归测试用例（防止类似失败再发）
```

> v2.1 → v2.2 修正：v2.1 列出 8 项，遗漏第 9 项 "regression harness case"。已补全（subagent fidelity audit 发现）。

### 11.3 Promotion 必含 7 字段（ADR §13.2）

```
- source trace
- proposed scope
- safety impact
- benchmark or harness result
- rollback plan
- owner or approving user
- version id
```

### 11.4 禁止的直接 promotion

- silent permission widening
- secret capture
- prompt mutation without regression
- memory writes without evidence
- provider enablement without user configuration
- autonomy escalation without policy approval

### 11.5 uclaw 现状（**远超 codex**）

实地核查显示 **uclaw Evolution Layer 不是空白，是缺统一架构**：

| 阶段 | uclaw 已有 |
|---|---|
| **Reflection** | `proactive/scenarios/` 7 个：conversation_learning / skill_extraction / multimodal_context / gene_evolution / plan_mode_calibration / memory_health / memory_lint |
| **Candidate Builder** | `learning/candidate.rs` 已定义 LearningCandidate + Buffer + 6 子模块（candidate/stability_detector/scheduler/cache/prompt_section/extractor） |
| **Simulation** | ❌ 无 |
| **Harness Gate** | `harness/self_improvement.rs::SelfImprovementGateReport + Verdict` |
| **User Review** | ❌ UI 不完整 |
| **Promotion Registry** | 部分（V35 memory_edge_audit / V30 fragment_reviews）|
| **gbrain 写入** | gbrain MCP + brain_sync_state（V37）|

codex 几乎**无 evolution layer**（无 learning crate、无 proactive 概念）。这是 **uclaw 的明确优势**。

### 11.6 改进设计（M7 Evolution Factory）

**M7-T1**：新 `src-tauri/src/evolution/` 模块统一 pipeline：

```
evolution/
├── mod.rs
├── pipeline.rs              # 6 阶段编排器
├── reflection/              # 把 7 个 proactive scenario 升格为 reflection generator
│   ├── conversation.rs (现有 conversation_learning)
│   ├── skill.rs            (现有 skill_extraction)
│   ├── multimodal.rs       (现有 multimodal_context)
│   ├── gep.rs              (现有 gene_evolution)
│   ├── plan_mode.rs        (现有 plan_mode_calibration)
│   ├── memory_health.rs    (现有 memory_health)
│   └── memory_lint.rs      (现有 memory_lint)
├── candidate/               # 9 种 candidate type
│   ├── gbrain_page_update.rs
│   ├── skill_or_sop.rs
│   ├── browser_script.rs
│   ├── prompt_patch.rs
│   ├── planner_heuristic.rs
│   ├── capability_profile_adjustment.rs
│   ├── policy_hook.rs
│   ├── failure_memory.rs
│   └── regression_harness_case.rs
├── simulation.rs            # dry-run / sandboxed simulation（**新增层**）
├── promotion.rs             # 包装 harness/self_improvement.rs
├── review.rs                # User Review surface（新增 UI）
└── registry.rs              # Promotion Registry（版本化 + 可回滚）
```

**M7-T2**：Simulation 阶段补全 —— 给每个 candidate type 写 dry-run 实现。例如 browser_script candidate 在 sandboxed browser 内跑；prompt_patch candidate 在 LLM eval set 上跑。

**M7-T3**：User Review UI —— Settings → Evolution Queue，列出待审 candidates，每个含 source trace + evidence + harness score + safety impact + rollback plan。一键 approve / edit / reject。

**M7-T4**：Promotion Registry V52 迁移（编号以提交时为准）：`evolution_promotions(id, candidate_kind, version, status, source_trace_ref, harness_result_json, safety_impact_json, rollback_plan_json, owner, approved_at, promoted_at)`。

**M7-T5**：6 类禁止 promotion 的硬性 check（编译期 + 运行期）。

**M7-T6**：把现有 `learning/` Sprint 1 输出（user_profile_facets V39）作为第一个完整 candidate type 跑通 pipeline。

**ADR §18 11 问**：
1. **Intent**：自我改进
2. **Autonomy**：L0（仅产生 proposal，不直接修改）
3. **Truth source**：Promotion Registry
4. **TaskEvent**：PreMemoryWrite / PrePromotion
5. **Context**：Runtime Trace（TaskEvent 流 + harness episodes）
6. **Capability**：消费各种 harness graders
7. **Block hooks**：PrePromotion / PreMemoryWrite
8. **Projection**：WorldProjection（promotion queue 子视图）
9. **Harness**：所有 12 个 HarnessSubject 都参与评估
10. **Rollback**：promotion 版本化 + disable path
11. **不拥有**：不拥有 LLM 推理（→ Runtime Kernel）/ 不拥有持久知识（→ gbrain）

---

## 12. Workers — Subagent / Teams / Cluster <a id="12-workers"></a>

### 12.1 ADR §4.4 Teams + §4.5 Cluster + §14

**Teams**：role-scoped coordinated workers，不是独立 chat rooms。
**Cluster**：分布式 OS 扩展，registers local/remote workers。

```rust
type WorkerNode = {
  id: string
  kind: 'local' | 'subagent' | 'worktree' | 'remote' | 'container' | 'mobile' | 'cloud'
  capabilities: CapabilityDescriptor[]
  status: 'online' | 'busy' | 'draining' | 'offline'
  load: { activeTasks: number; cpu?: number; memory?: number }
  policy: PolicySpec
  locality: DataLocalitySpec
  lastHeartbeatAt: string
}
```

### 12.2 codex 实现（已实地核查）

**Subagent**：`core/src/codex_delegate.rs::run_codex_thread_interactive` 嵌套 Codex 实例，共享 services（skills_manager / plugins_manager / mcp_manager / extensions / environment_manager / exec_policy / attestation_provider）但独立 session_state。`SessionSource::SubAgent(SubAgentSource::ThreadSpawn { depth })` 跟踪深度。

**Agent Registry**（`core/src/agent/registry.rs`）：`AgentRegistry` 控制 max_threads + 99 个 nickname pool（Euclid, Archimedes, ..., Jason，已实地核查 `agent_names.txt`）。重名加序数后缀（"Newton the 2nd"）。

**4 Built-in Roles**（已实地核查 `core/src/agent/role.rs`）：default / explorer / worker / awaiter（注释隐藏）。每个有独立 TOML 配置注入到 ConfigLayerStack：

- `explorer.toml`：read-only fast 探索器，鼓励并行 spawn
- `worker.toml`：执行单元，强调 ownership + 非孤立感知
- `awaiter.toml`：长任务 polling（已实地核查含 `background_terminal_max_timeout = 3600000` + `model_reasoning_effort = "low"`）

**Multi-agent v2 工具**：`spawn` / `wait` / `send_message` / `list_agents` / `followup_task` / `close`（实地核查 `tools/handlers/multi_agents_v2/`）。

**InterAgentCommunication** 类型 + `state` crate 的 `DirectionalThreadSpawnEdgeStatus`（spawn 图存储）+ `AgentJob` / `AgentJobItem` / `AgentJobProgress` 模型。

### 12.3 uclaw 现状

`agent/teams/` 6 文件（channel/orchestrator/reviewer/supervisor/worker/mod）：内存编排 `AgentTeamOrchestrator` 驱动 worker → reviewer → supervisor 流水线。**同一进程同一 LLM session 内**，非真 subagent。

`channels/` 5+ 文件（im/notify/dispatcher/manager/session_registry/types）：TeamChannel 雏形。

无 WorkerNode 抽象。无 cluster。

### 12.4 改进设计

**M5+M8 Workers + Teams v1**：

**M8-T1**：把现有 `agent/teams/` 用 IntentSpec + TaskEvent 包装。worker/reviewer/supervisor 各跑独立 SessionTask + 独立 CapabilityProfile。

**M8-T2**：subagent MVP —— 借鉴 codex `codex_delegate`：`SessionSource::SubAgent` + 共享 services 但独立 session_state + child CancellationToken。先支持 depth ≤ 1。V45 迁移 `agent_session_spawn_edges`。

**M8-T3**：spawn 工具暴露给 LLM —— `spawn_subagent(task, role, max_turns)` + `wait_subagent` + `get_subagent_result` + `list_active_subagents`。

**M8-T4**：Role-as-config —— 把 worker/reviewer/supervisor + 新增 explorer/awaiter/planner 重构为 TOML 配置文件（`~/.uclaw/roles/<name>.toml`）。借鉴 codex `apply_role_to_config`。

**M8-T5**：Nickname 体系 —— 100 个中文化命名 pool（墨子 / 张衡 / 徐霞客 / 毕昇 / 华罗庚 等）。重名加序数。

**M8-T6**：TeamSpec / Coordinator / WorkerRole / TeamChannel / ReviewGate 完整实现。reviewer 可 block 完成。

**M9-T1（远期）**：WorkerRegistry + WorkerNode 抽象 + 8 kind（local/subagent/worktree/remote/container/mobile/cloud）+ heartbeat。

**M9-T2**：capability routing + load-aware assignment + data locality policy + checkpoint/failover。

**ADR §18 11 问**：略（核心同 §5 + §10）

---

## 13. gbrain & Memory Provider Strategy <a id="13-gbrain-memory"></a>

### 13.1 ADR §11 Memory 规则

> - gbrain：primary durable knowledge
> - memU：auxiliary retrieval/embedding where useful
> - memory_graph：legacy/archive；no new EntityPage feature work
> - Memory OS ideas: 仅保留如能融入 gbrain / runtime metadata / harness-gated executable knowledge

**3 类知识**：
| 类型 | 用途 | Owner |
|---|---|---|
| Factual | 持久 facts | **gbrain** |
| Evidential | traces / logs / outputs / receipts / scorecards | harness / run ledger |
| Executable | skills / SOPs / browser scripts / prompts / policies | Evolution Layer + registries |

### 13.2 Memory Provider Strategy（Hermes-style exclusive）

> - only one active primary memory provider at a time
> - gbrain is current active provider
> - alternative providers exist as plugins, not parallel core systems
> - writes require receipts
> - consequential recalls require source references
> - memory writes from self-evolution require harness or user approval

**关键约束（ADR §11.2 明文规定，v2.2 自审后加重强调）**：

> **memory_graph 已冻结。新 feature 不得直接使用 memory_graph，除非新 ADR 反转冻结决议（"No new EntityPage feature work unless a later ADR reverses the freeze"）。**

落地强制：
- M0 Phase 0.5 收尾即在 `memory_graph::write*` 函数加入 `panic!("memory_graph frozen — use gbrain instead")` 防御（除遗留迁移代码白名单外）
- CI lint 禁止 `src-tauri/src/` 新 PR 出现 `memory_graph::write` 调用
- 现有数据保留可读但不可写

### 13.3 codex / uclaw 现状

| 项 | codex | uclaw |
|---|---|---|
| 主记忆 | 3 crate（memories/mcp + read + write）| gbrain primary（已与 ADR 对齐）|
| 辅助 | message-history | memU + memorization |
| Legacy | — | memory_graph（freeze 状态）|
| Citation | memory_citation.rs（结构化 fragment_id/span/confidence）| 部分 |
| Provider 互斥 | 无显式 | 需补 |

### 13.4 改进设计（M2 Memory Provider Plugin Strategy）

**M2-T-Mem-1**：gbrain 显式声明为 `PrimaryMemoryProvider`（plugin kind=exclusive）。其他 memory provider 都标 plugin kind=exclusive 但默认 disabled。

**M2-T-Mem-2**：MemoryWriteReceipt 类型化 —— 含 provider / artifact_ref / evidence_refs / approved_by / harness_episode_ref。所有 memory_write 必经 PreMemoryWrite hook。

**M2-T-Mem-3**：consequential recall：高风险决策引用 memory 时必须返回 source references（fragment_id + span + confidence）。

**M2-T-Mem-4**：自我进化 memory writes 走 Evolution Factory PrePromotion gate。

**M2-T-Mem-5**：memory_graph **真正冻结** —— 严格不加新 EntityPage feature；现有数据保留可读但不可写。

---

## 14. Browser Provider Strategy <a id="14-browser-provider"></a>

### 14.1 ADR §12 核心

```
Kernel → BrowserProvider API → {Local Chromium, Browser Use, Browserbase, Firecrawl, Scripts, Harness}
```

策略：
- 保留 `BrowserContextManager` 作为 `LocalChromiumProvider`
- `BrowserService` 仅作 compat surface（sunset note）
- 新行为全走 `BrowserProvider` trait
- browser-use / Browserbase / Firecrawl 作 provider plugin
- 站点 specific workflow → script artifact
- 保留 structured observations / action results / boundary events / checkpoints
- 同一 browser harness case 可评估所有 provider

### 14.2 Computer Use 升级（Agent S2 模式）

- 通用 planner 做 high-level
- specialist grounding 做坐标 / accessibility / 截图 / UI 状态
- 多尺度 plan：goal / page-app / next-action
- GUI grounding uncertainty 作 first-class risk

### 14.3 uclaw 现状（**已高度成熟**）

实地核查 `src-tauri/src/browser/`（25+ 文件）：
- agent_loop.rs（含 BrowserTaskRequest / BrowserTaskStatus / 完整循环）
- action.rs + action_registry.rs（structured actions）
- boundary.rs（intervention detection）
- context.rs + context_manager.rs（BrowserContextManager）
- decision.rs（BrowserDecisionAdapter）
- dom_state.rs
- identity/（含 BrowserAuthProfileBroker + BrowserIdentityProfile + PlaywrightStorageState）
- intervention_bridge.rs（BrowserAskUserBridge）
- loop_detector.rs（fingerprinting + 死循环检测）
- memory_adapter.rs（BrowserLongTermMemoryAdapter）
- observation.rs（BrowserObservation）
- perception/
- recovery.rs（classify_browser_error + BrowserRecoveryKind）
- session_state.rs
- task_store.rs
- tools.rs

**这是 ADR §12 要求的几乎全部基础**！

### 14.4 改进设计（M6 Browser Provider）

**M6-T1**：抽 `BrowserProvider` trait（session / snapshot / action / boundary / checkpoint）。

**M6-T2**：把现有 BrowserContextManager 适配为 `LocalChromiumProvider`。

**M6-T3**：browser-use / Browserbase / Firecrawl 作 plugin（manifest 含 capability cards + harnessCases）。

**M6-T4**：provider-independent browser harness（已有 `harness/` 基础，加 browser case set）。

**M6-T5**：站点 script artifact 化（Evolution Factory candidate kind=browser_script 配套）。

**M6-T6**：Computer Use 升级（Agent S2 模式）—— 远期，仅当需要 OS-level GUI 控制时启动。

---

## 15. Autonomy Ladder L0-L6 <a id="15-autonomy-ladder"></a>

### 15.1 ADR §5（已实地核查）

| Level | Name | Description | Required guardrails |
|---|---|---|---|
| L0 | Chat Assist | 仅回答或提议 | no side effects |
| L1 | Assisted Action | agent 备动作，用户每步批准 | 可见 plan + approval |
| L2 | Supervised Task | bounded task + 频繁 checkpoint | tool policy / trace / cancel/resume |
| L3 | Delegated Task | 仅在 human boundary 打断 | budget / checkpoints / harness trace |
| L4 | Scheduled Worker | trigger/schedule 唤醒跑 workflow | automation ledger / escalation policy |
| L5 | Agent Team | 多角色协作 | role ownership / reviewer gate |
| L6 | Distributed Cluster | 工作路由到本地/远程 worker | worker policy / locality / failover |

### 15.2 Autonomy Resolver（自动下调）

每个 task 声明 `autonomyTarget`。runtime 基于以下因素自动下调：
- risk_class 高 → 上限 L2
- 凭证缺失 → 上限 L1
- provider harness score 低 → 下调
- capability profile 不覆盖 → 下调

下调原因在 UI 显示 + 用户可手动升回（但 hooks 必经过审计）。

### 15.3 codex / uclaw 现状

codex：`AskForApproval`（Never/OnRequest/UnlessTrusted/OnFailure）+ `SandboxPolicy` 隐式表达，无 L0-L6 显式。

uclaw：`safety/permissions.rs` 类似，亦无显式分级。

### 15.4 设计

`src-tauri/src/runtime/autonomy.rs` —— AutonomyResolver + AutonomyLevel + DowngradeReason。每 task UI 卡片显示 `Requested L3 → Effective L2 (provider unhealthy)`。

---

## 16. Hermes Agent 作为 plugin discipline 参考 <a id="16-hermes-参考"></a>

ADR §3.1 明确 Hermes 是 capability strategy 最强参考。位于 `/Users/ryanliu/Documents/hermes-agent`（已确认 85+ 目录入口，~5.8MB 可访问）。

### 16.1 ADR 列出的 Hermes 模式（uclaw 应直接复刻）

1. plugin sources 显式分类：bundled / user / project-trusted / external
2. plugin kinds 显式类型化：standalone / backend / exclusive / platform / model-provider
3. tools 注册到 canonical registry
4. provider backends 是 plugins
5. tool overrides 显式 + 可审计
6. hooks 是 lifecycle-level APIs
7. provider families 可 exclusive（memory 典型）
8. managed gateway 代理外部 tool capabilities
9. task-specific toolset distribution
10. install-time configuration（env vars / secrets / metadata / permission surfaces）

### 16.2 uclaw 应实现的 Rust 基础设施（ADR §3.1 明列）

```
ToolRegistry / ProviderRegistry / PluginRegistry / HookBus / CapabilityProfile / ToolGateway
```

### 16.3 codex vs Hermes 在 uclaw 视角的角色分工

- **codex** = Kernel 层 + Context Fabric + Token 工程现代化基线
- **Hermes** = Capability Mesh + Plugin Discipline 具体实现参考

**互补，非替代**。

### 16.4 行动建议

新建 `docs/research/hermes-agent-deep-dive.md`，实地扫描 Hermes 核心 crate，按"直接复刻 / 改造 / 借鉴架构"分级（同 §17 的 codex 方法）。

---

## 17. 可直接复制的 codex Crate 分级清单 <a id="17-crate-清单"></a>

### 17.1 License 合规前置

- **codex 是 Apache-2.0**
- **uclaw 推荐 Apache-2.0**（见 §3）
- NOTICE 模板（`/Users/ryanliu/Documents/uclaw/NOTICE`）：

```
uclaw

This product includes software developed at OpenAI (https://openai.com/)
under the Apache License, Version 2.0.

The following crates are derived (with or without modifications) from
the openai/codex repository (https://github.com/openai/codex):

  - uclaw-utils-template       (from codex-rs/utils/template)
  - uclaw-utils-string         (from codex-rs/utils/string)
  - uclaw-utils-cache          (from codex-rs/utils/cache)
  - uclaw-utils-fuzzy          (from codex-rs/utils/fuzzy-match)
  - uclaw-async-utils          (from codex-rs/async-utils)
  - uclaw-file-watcher         (from codex-rs/file-watcher)
  - uclaw-utils-output-truncation (from codex-rs/utils/output-truncation; modified)
  # 后续添加更多

Upstream codex commit: <填入今日 commit hash>
The original codex source is licensed under Apache License 2.0.
See licenses/apache-2.0.txt for the full license text.
```

### 17.2 第一档：直接复制零改动（17 个）

| # | codex 路径 | 行数 | 外部依赖 | uclaw 落地 | ROI |
|---|---|---|---|---|---|
| 1 | `utils/template/` | 442 | 无 | `uclaw-utils-template` | ★★★★★ Prompt 模板引擎 |
| 2 | `utils/string/` | 560 | regex-lite, serde | `uclaw-utils-string` | ★★★★★ token 估算/截短 |
| 3 | `utils/cache/` | 193 | lru, sha1, tokio | `uclaw-utils-cache` | ★★★★ LRU 缓存 |
| 4 | `utils/fuzzy-match/` | 168 | 无 | `uclaw-utils-fuzzy` | ★★★★ skill 评分 |
| 5 | `utils/elapsed/` | 71 | 无 | `uclaw-utils-elapsed` | ★★★ UI 时长 |
| 6 | `utils/json-to-toml/` | 83 | serde_json, toml | `uclaw-utils-json-toml` | ★★ |
| 7 | `utils/readiness/` | 333 | async-trait, time, tokio | `uclaw-utils-readiness` | ★★★★ service 就绪 |
| 8 | `utils/sleep-inhibitor/` | 618 | 平台 deps | `uclaw-utils-sleep` | ★★★★ automation 防睡 |
| 9 | `utils/stream-parser/` | 1485 | 无 | `uclaw-utils-stream` | ★★★ SSE 解析 |
| 10 | `async-utils/` | 86 | async-trait, tokio-util | `uclaw-async-utils` | ★★★★ `OrCancelExt` |
| 11 | `file-watcher/` | 中 | notify, tokio | `uclaw-file-watcher` | ★★★★★ skills/MCP 热加载 |
| 12 | `file-search/` | 中 | ignore, nucleo | `uclaw-file-search` | ★★★★ rg 包装 |
| 13 | `utils/absolute-path/` | 871 | dirs, dunce, schemars | `uclaw-utils-abs-path` | ★★★★ `AbsolutePathBuf` |
| 14 | `utils/home-dir/` | 134 | dirs（+#13）| `uclaw-utils-home` | ★★★ |
| 15 | `utils/path-utils/` | 353 | dunce, tempfile（+#13）| `uclaw-utils-path` | ★★★ |
| 16 | `utils/image/` | 中 | image, base64, mime_guess（+#3）| `uclaw-utils-image` | ★★★ |
| 17 | `utils/pty/` | 中 | portable-pty | `uclaw-utils-pty` | ★★★ unified_exec 基础 |

**总 ~7,300 行**。

**最高 ROI 6 个**：#1 + #2 + #3 + #4 + #10 + #11。

### 17.3 第二档：复制 + 微改动（3 个）

| # | codex 路径 | codex 内部依赖 | 改动 |
|---|---|---|---|
| 18 | `utils/output-truncation/` | codex-protocol::TruncationPolicy + FunctionCallOutputContentItem | 摘出 2 类型到 uclaw 自己（~50 行）★★★★★ PR-T1 根 |
| 19 | `features/` | codex-otel + codex-protocol | 框架保留，Feature variants 重写 ★★★★ |
| 20 | `utils/oss/` | 无 | 零改动，仅 license metadata 元数据 ★★ |

### 17.4 第三档：选择性复制（2 个 + ansi-escape 跳过）

| # | codex 路径 | 工作量 | 改动 |
|---|---|---|---|
| 21 | `apply-patch/` | ~2 周 | 替换 codex-exec-server 依赖 + 保留 Lark grammar + tree-sitter ★★★ |
| 22 | `git-utils/` | ~1.5 周 | 替换 codex-file-system 依赖 + 保留 gix ★★★ |
| 23 | `ansi-escape/` | — | **uclaw 不用 ratatui，跳过** ❌ |

### 17.5 第四档：不建议复制（90+）

**uclaw 已有等价**：aws-auth / keyring-store / login / chatgpt / backend-client / ollama / lmstudio / models-manager / model-provider*

**uclaw 无对应形态**：tui / cli / exec* / app-server* / arg0 / terminal-detection / realtime-webrtc / cloud-tasks* / external-agent-* / v8-poc / code-mode / responses-api-proxy / bwrap / linux-sandbox / windows-sandbox-rs / landlock / stdio-to-uds

**深度耦合（借鉴而非复制）**：protocol / core / tools 内部 / hooks / skills + core-skills / plugin + core-plugins / rollout + state + thread-store / codex-mcp + mcp-server + rmcp-client / utils/sandbox-summary / utils/approval-presets / utils/cli + utils/cargo-bin / analytics + otel

**太底层 / uclaw 不需要**：process-hardening / network-proxy / shell-escalation

### 17.6 执行步骤（4 阶段）

**Step 1** — Workspace 改造（半天）：
- 新建顶层 `Cargo.toml` workspace
- workspace.dependencies 集中 + workspace.package.license = "Apache-2.0"

**Step 2** — 第一批 6 个 crate 复制（半天）：
```bash
for c in utils/template utils/string utils/cache utils/fuzzy-match; do
  cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/$c \
        /Users/ryanliu/Documents/uclaw/src-tauri/uclaw-$(basename $c)
done
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/async-utils \
      /Users/ryanliu/Documents/uclaw/src-tauri/uclaw-async-utils
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/file-watcher \
      /Users/ryanliu/Documents/uclaw/src-tauri/uclaw-file-watcher
# sed 改名 codex-* → uclaw-*
# 加 SPDX header + NOTICE
```

**Step 3** — output-truncation 移植（半天）：
- 把 `TruncationPolicy` + `FunctionCallOutputContentItem` 类型摘出到 uclaw 自己（~50 行）
- 修改 lib.rs imports
- Cargo.toml 改依赖

**Step 4** — 第二批 11 个 crate（可选，0.5-1 天）

**总计**：6 个最高 ROI 第一档 + #18 = **7 个 crate，~3,500 行代码，2-3 天工作量**。

### 17.7 量化总结

| 档位 | crate 数 | 总代码行 | 工作量 |
|---|---|---|---|
| 第一档零改动 | 17 | ~7,300 | 1-2 天 |
| 第二档微改动 | 3 | ~900 | 0.5-1 天 |
| 第三档选择 | 2（除 ansi-escape） | ~2,000 | 2-3.5 周 |
| 第四档不复制 | 90+ | — | 借鉴架构 |

---

## 18. uclaw 现状再评估 <a id="18-uclaw-现状"></a>

实地核查 `src-tauri/src/` 全部 44 个 module，**v1.0-v1.2 文档低估了 uclaw 现状**。修正如下：

### 18.1 模块清单（实地）

```
src-tauri/src/
├── agent/                  (18+ 文件含 teams/ + tools/ + prompts/ + retry/ + gep/ + context/)
│   ├── agentic_loop.rs     (882 行)
│   ├── dispatcher.rs
│   ├── teams/ (6 文件)     ← Agent Teams 已就位
│   ├── tools/builtin/      ← 13 个 builtin tools
│   ├── prompts/            ← baseline + mode prompts
│   ├── gep/                ← Gene Expression Programming
│   ├── retry/
│   ├── context/
│   ├── code_rescue.rs
│   ├── gbrain_prompt.rs    ← gbrain 集成
│   ├── headless.rs
│   ├── history_window.rs
│   ├── llm_stream.rs
│   ├── mode_prompts.rs
│   ├── mode_suggest.rs + mode_suggest_store.rs
│   ├── plan_state.rs
│   └── session.rs
├── browser/                (25+ 文件)
│   ├── agent_loop.rs       ← browser v2 主循环
│   ├── action.rs + action_registry.rs
│   ├── boundary.rs         ← intervention detection
│   ├── context_manager.rs  ← BrowserContextManager
│   ├── decision.rs
│   ├── dom_state.rs
│   ├── identity/           ← BrowserAuthProfileBroker
│   ├── intervention_bridge.rs
│   ├── loop_detector.rs    ← fingerprinting
│   ├── memory_adapter.rs
│   ├── observation.rs
│   ├── perception/
│   ├── recovery.rs
│   ├── session_state.rs
│   ├── task_store.rs
│   └── tools.rs
├── harness/                (11 文件)
│   ├── adapters/
│   ├── artifacts.rs        ← HarnessArtifact + HarnessArtifactStore
│   ├── budget.rs           ← ToolBudgetManager
│   ├── case.rs             ← HarnessCase + HarnessSubject (12 subjects)
│   ├── cases/
│   ├── episode.rs          ← HarnessEpisode + HarnessVerdict
│   ├── graders.rs          ← HarnessGraderRegistry
│   ├── memory_inventory.rs
│   ├── runtime.rs          ← HarnessRuntime
│   ├── self_improvement.rs ← SelfImprovementGate
│   ├── trace.rs            ← HarnessEvent (TaskEvent 雏形)
│   └── trajectory.rs
├── learning/               (7 文件 Sprint 1)
│   ├── candidate.rs        ← LearningCandidate + Buffer
│   ├── cache.rs            ← FacetCache
│   ├── extractor.rs        ← chat-turn candidate producer
│   ├── prompt_section.rs   ← inject to system prompt
│   ├── scheduler.rs        ← 30 min periodic tick
│   └── stability_detector.rs ← half-life-decayed evidence
├── gbrain/                 (3 文件)
│   ├── browse.rs
│   └── chat_extractor.rs
├── channels/               (5+ 文件 IM)
│   ├── dispatcher.rs
│   ├── im/
│   ├── manager.rs
│   ├── notify/
│   ├── session_registry.rs
│   └── types.rs
├── automation/             (完整 spec + activity)
├── symphony_graph/         (manager + protocol/ + runtime/)
├── proactive/              (7 scenarios + service)
├── memory.rs + memory_graph/ + memu/ + memorization/
├── mcp.rs
├── safety/                 (permissions + path_policy)
├── secrets/
├── services/               (ServiceManager)
├── infra/                  (InfraService 消息总线)
├── extensions/             (mod.rs 入口已留)
├── observability/
├── llm/
├── providers/
├── api/ + local_api/
├── stt/
├── workspace/
├── git/
├── files_rail/
├── preview/
├── notifications.rs / background.rs / cost_store.rs / settings.rs / app.rs / main.rs / lib.rs / ipc.rs / db/ / config/
```

**13.6 万行 Rust 代码**。

### 18.2 v1.0-v1.2 误判清单

| v1.0-v1.2 论断 | 修正 |
|---|---|
| "uclaw 是单一 uclaw_core crate" | 是，但内部 **44 个 module 高度模块化** |
| "agent loop 简单 while-loop" | agentic_loop.rs **882 行**，6 阶段成熟（含 token bookkeeping + retry + cost record）|
| "无 evaluation harness" | ❌ **错** —— `harness/` 11 文件成熟模块（HarnessEpisode/HarnessCase/HarnessGraderRegistry 等）|
| "无 learning system" | ❌ **错** —— `learning/` Sprint 1 已实现 6 子模块 |
| "Browser 简单" | ❌ **大错** —— `browser/` v2 是 25+ 文件成熟模块 |
| "Plugin 系统不存在" | 仍正确，但 `extensions/` 留了入口；`automation_marketplace` V23a 已是市场基础 |
| "无 IM 集成" | ❌ —— `channels/im/` 已有（V32） |
| "无 Symphony workflow" | ❌ —— `symphony_graph/` 完整 runtime（V33） |
| "Memory 单系统" | gbrain + memU + memory_graph + memorization 多层（与 ADR §11.2 一致）|
| "Cron / 定时任务未实现独立模块" | 部分正确，automation 已能驱动；background.rs 存在 |

### 18.3 修正后的差距视图（按 ADR 11 层）

| ADR 层 | uclaw 已有 | 真实差距 |
|---|---|---|
| Intent Layer | `Op::*` + mode_suggest | 缺 IntentSpec 统一 + autonomyTarget + riskClass |
| Runtime Kernel | agentic_loop.rs + dispatcher.rs 成熟 | 缺 SessionTask 显式化 + 抢占式 spawn_task + 6 维 token |
| Context Fabric | compact + memory_citation + skill manifest | 缺显式 context.* 7 工具 + diff-based updates + 8 字段 fold |
| Capability Mesh | tools/builtin + mcp + skills + providers 散落 | 缺统一 5 Registry + Capability Card + Hermes plugin |
| World Projection | UI 直读各模块 + IPC events | 缺统一 projection materialized view |
| Safety & Policy | safety/permissions + path_policy + automation/permission | 缺 HookBus（13 events）+ Isolation profiles |
| Evolution Layer | **learning/ + harness/ + proactive 三块齐全** | 缺统一 pipeline + Simulation 阶段 + User Review UI + Promotion Registry |
| Product Shell | React + Jotai + Tailwind 11 主题 | 已成熟 |
| Harness | **11 文件 + 12 HarnessSubject** | **超过 codex**；缺与 TaskEvent 流统一 |
| gbrain | gbrain/ + mcp 集成 | 已就位（与 ADR 一致） |
| Workers | agent/teams/ + browser session + automation | 缺 WorkerNode 抽象 + cluster |

### 18.4 关键洞察：uclaw 离 Agent OS v2 比想象的近

ADR M0-M9 在 uclaw 现状下的真实工作量：
- M0 ADR Lock：✅ 已完成
- M1 Runtime Contracts：~2-3 周（事件流改名 + 加枚举）
- M2 Context Fabric：~5-7 周（升级现有 + Template + 7 工具）
- M3 Capability Mesh：~6-8 周（统一 5 Registry）
- M4 World Projection：~3-4 周（在现有 store 上加一层）
- M5 Hooks + Isolation：~4-5 周
- M6 Browser Provider：~3-4 周（现有 v2 抽 trait）
- M7 Evolution Factory：~6-8 周（learning + harness + proactive 收纳）
- M8 Teams v1：~5-7 周（agent/teams + channels 升级）
- M9 Cluster v1：~12-16 周（从零）

**总时长**（**单一权威值**，本文档与实施方案同步）：
- **M1-M8（核心）**：约 **34-46 周 = 8-11 月（3 人团队，中位 ~9.5 月）**
- **M1-M9（含 cluster 远期）**：约 **46-62 周 = 13-15 月（3 人团队，中位 ~14 月）**

远小于 v1.1 估的 15-18 月，因为大量现状骨架（harness/learning/browser v2 等）可复用。

---

## 19. 改进设计总集（按 ADR Milestone 排）<a id="19-改进设计总集"></a>

> v1.0-v1.2 的 PR-S/PR-T/M/SA/T/A 等代号全部归入 ADR Milestone M0-M9。

### 19.1 v1.0-v1.2 代号 → ADR Milestone 归属

| v1.0-v1.2 代号 | ADR Milestone | 备注 |
|---|---|---|
| G1 SessionTask + 抢占式 | **M1** Runtime Contracts | RegularTask 包装现有 agentic_loop |
| G2 OTLP turn span + 6 维 token | **M4** World Projection | OTel 是 projection 的 export |
| G3 Rollout JSONL | **M1** Runtime Contracts | TaskEvent 持久化 |
| G4 Prewarm LLM | **M2** Context Fabric | context 预读优化 |
| TL1-TL6 Tools 分层 | **M3** Capability Mesh | ToolRegistry |
| H1 Hooks（→ 13 events） | **M5** Policy Hooks | 扩展 codex 8 → ADR 13 |
| SK1-SK4 Skills 作用域+top-K | **M3** Capability Mesh | Executable Knowledge |
| P1-P4 Plugin | **M3** Capability Mesh | PluginRegistry, Hermes-aligned |
| MC1-MC4 MCP | **M3** Capability Mesh | ProviderRegistry 子集 |
| PR-S1 ~ PR-S9 Prompt | **M2** Context Fabric Part A | Prompt 实现 |
| PR-T1 ~ PR-T12 Token | **M2** Context Fabric Part B | Budget Management |
| SA1-SA3 SubAgent | **M5** Isolation + **M8** Teams | depth=1 subagent + spawn 工具 |
| LX1-LX4 Memory/Learning | **M7** Evolution Factory | 把 learning + harness + proactive 收纳 |
| C1-C3 Cron / Scheduler | **M3** Capability Mesh | Automation 是 IntentSpec 触发器 |
| S1-S4 Session | **M1** Runtime Contracts + **M4** Projection | Session 升级 |
| M1-M4 Multi-agent | **M5** Workers + **M8** Teams | role-as-config |
| T1-T2 Teams | **M8** Teams v1 | TeamSpec template + SubAgent fanout |
| A1-A3 Automation | Intent Layer + **M3** Capability Mesh | 不另起 semantics |
| X1-X3 横向架构 | 持续 | crate 拆分 + API 版本 |

### 19.2 P0-P3 优先级（按用户感知 + ADR M-序列）

**P0（必须，立即）**：
1. **License → Apache-2.0** + NOTICE + workspace 改造 + 复制 17+1 个 codex utils crate
2. **M2-H L1 TruncationPolicy** —— 立竿见影 token 节约
3. **M2-H L2 ToolExposure** —— 立竿见影
4. **M2-A baseline.md 12 block 重写** —— 输出质量肉眼可见

**P1（高优）**：
5. **M1 Runtime Contracts** —— IntentSpec/TaskSpec/TaskEvent + adapters
6. **M2-D Diff-based context updates** —— 长会话最大单笔节约
7. **M2-H L7 三档 compaction** —— 解决"context 爆掉"
8. **M3 五大 Registry** —— Capability Mesh 基座
9. **M5 HookBus 13 events** —— 安全可见性

**P2（中优）**：
10. **M4 WorldProjection** —— UI 体验飞跃
11. **M6 BrowserProvider** trait —— 现有 v2 抽象化
12. **M7 Evolution Factory** —— 现有 learning/harness/proactive 收纳
13. **M8 Teams v1** —— agent/teams + channels 升级

**P3（远期）**：
14. **M9 Cluster v1** —— 远期目标

---

## 20. 风险与权衡 <a id="20-风险"></a>

| 风险 | 缓解 |
|---|---|
| **重新框架导致认知断层** | 保留 v1.0-v1.2 代号映射表（§19.1）；CLAUDE.md 已引用 ADR |
| **uclaw 已有大量 partial 实现，重构破坏** | 渐进 Feature flag，旧路径不立即删；e2e 测试先行 |
| **Token 节约过度截断信息丢失** | 默认 budget 偏宽；用户可调；truncate_middle 保留首尾；UI 显示"已截断" |
| **Diff 注入 baseline 漂移** | 每 10 turn 强制全量重注入（"心跳"）；compaction 后强制清空 baseline |
| **codex 上游更新 drift** | NOTICE 固化 commit hash；季度对比 upstream diff |
| **Hermes 参考未实地扫描** | 单独建《Hermes Agent 深度参考报告》；M3 启动前完成 |
| **Apache-2.0 不防 SaaS 竞品 fork** | 保留 uclaw trademark；1-2 年后可迁 BSL 1.1 |
| **M9 Cluster 投入过早** | 严格按 M8 稳定后启动；最小可用先做 mock remote worker |
| **harness 已存在但仅评估用，TaskEvent 升格冲突** | M1 显式做"事件流统一"任务；保留 harness 内部使用 + 暴露 public API |
| **proactive scenarios 7 个并跑可能冲突** | M7 给每个 scenario 加 Feature flag + 优先级 |

---

## 附录 A：codex 路径索引 <a id="附录-acodex-路径"></a>

**ADR 11 层 → codex 文件**：

| 层 | 关键路径 |
|---|---|
| Intent Layer | `protocol/src/protocol.rs` 中 `Op::*` enum |
| Runtime Kernel | `core/src/codex_thread.rs`, `thread_manager.rs`, `session/session.rs`, `tasks/mod.rs` + `regular.rs` / `compact.rs` / `review.rs` / `user_shell.rs`, `client.rs`, `client_common.rs` |
| Context Fabric | `core/src/context_manager/` (history/normalize/updates), `core/src/compact.rs` + `compact_remote.rs` + `compact_remote_v2.rs`, `core/src/agents_md.rs`, `core/src/context/` (30+ fragments), `protocol/src/prompts/base_instructions/default.md`, `protocol/src/memory_citation.rs`, `utils/output-truncation/`, `utils/string/`, `utils/template/` |
| Capability Mesh | `core/src/tools/` (registry, router, orchestrator, parallel, handlers/, runtimes/, code_mode/), `core-plugins/` (manager, marketplace, manifest, store, remote), `plugin/`, `core/src/skills.rs` + `core-skills/`, `core/src/mcp.rs` + `mcp_tool_call.rs` + `mcp_tool_exposure.rs`, `codex-mcp/`, `mcp-server/`, `rmcp-client/`, `model-provider/`, `model-provider-info/`, `models-manager/`, `core/src/function_tool.rs` |
| World Projection | 无显式 —— `app-server/src/message_processor.rs` + `app-server-protocol/src/protocol/v2/` |
| Safety & Policy | `hooks/`, `core/src/hook_runtime.rs`, `core/src/guardian/`, `core/src/sandboxing/` + `landlock.rs` + `windows_sandbox.rs`, `sandboxing/`, `bwrap/`, `linux-sandbox/`, `windows-sandbox-rs/`, `process-hardening/`, `core/src/exec_policy.rs`, `core/src/consequential_tool_message_templates.json`, `core/src/attestation.rs` |
| Evolution Layer | 几乎缺失 —— 仅 `analytics/`, `otel/` |
| Workers | `core/src/codex_delegate.rs`, `core/src/agent/` (registry, role, control, status, builtins/), `core/src/session/multi_agents.rs`, `external-agent-sessions/`, `agent-graph-store/`, `agent-identity/`, `collaboration-mode-templates/` |
| gbrain 等价 | `memories/mcp/`, `memories/read/`, `memories/write/`, `ext/memories/`, `message-history/` |
| Browser | 无 |
| Harness | `core/tests/` + insta snapshots（无独立 crate）|

---

## 附录 B：uclaw 路径索引 <a id="附录-buclaw-路径"></a>

**ADR 11 层 → uclaw 文件**：

| 层 | 关键路径 |
|---|---|
| Intent Layer | （新建）`src-tauri/src/runtime/contracts.rs`（计划）|
| Runtime Kernel | `src-tauri/src/agent/agentic_loop.rs` (882 行), `agent/dispatcher.rs`, `agent/session.rs`, `agent/llm_stream.rs`, `agent/retry/`, `agent/code_rescue.rs` |
| Context Fabric | `src-tauri/src/agent/prompts/`, `agent/mode_prompts.rs`, `agent/context/`, `agent/gbrain_prompt.rs`, `agent/history_window.rs`, `skills.rs`, `skills_manifest.rs`, `memory.rs` |
| Capability Mesh | `src-tauri/src/agent/tools/builtin/`, `mcp.rs`, `skills.rs`, `providers/`, `extensions/`, `automation/tools/`, `llm/`, `agent/teams/` |
| World Projection | （新建）`src-tauri/src/projection/world.rs`（计划） |
| Safety & Policy | `src-tauri/src/safety/permissions.rs` + `path_policy.rs`, `automation/permission.rs` |
| Evolution Layer | `src-tauri/src/learning/` (7 文件), `harness/self_improvement.rs`, `proactive/scenarios/` (7 个) |
| Product Shell | `ui/src/` (React + Jotai + Tailwind, 11 主题) |
| Harness | `src-tauri/src/harness/` (11 文件) |
| gbrain | `src-tauri/src/gbrain/` (3 文件) + `mcp.rs` 集成 |
| Browser | `src-tauri/src/browser/` (25+ 文件 v2) |
| Workers / Teams / Channels | `src-tauri/src/agent/teams/` (6 文件), `channels/` (5+ 文件 IM), `symphony_graph/` |
| Memory | `src-tauri/src/memory.rs`, `memory_graph/`, `memu/`, `memorization/` |
| 基础设施 | `app.rs`, `main.rs`, `lib.rs`, `tauri_commands.rs`, `services/`, `infra/`, `db/migrations.rs`, `config/`, `secrets/`, `observability/`, `api/`, `local_api/`, `notifications.rs`, `background.rs`, `cost_store.rs`, `settings.rs` |
| Automation | `automation/` (manager, runtime, activity, protocol, tools, permission), `symphony_graph/` |
| 其他模块 | `stt/`, `workspace/`, `git/`, `files_rail/`, `preview/`, `notifications.rs`, `ipc.rs` |
| ADR + Plans | `/Users/ryanliu/Documents/uclaw/docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`, `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md`, `docs/superpowers/plans/2026-05-19-uclaw-agent-autonomy-rollout-tracker.md`, `docs/superpowers/plans/2026-05-19-uclaw-harness-runtime-core.md`, `docs/superpowers/plans/2026-05-20-memory-os-v2-A-gbrain-browser.md` |

---

## 附录 C：迁移注册表预占 <a id="附录-c迁移注册表"></a>

uclaw 实际最新 V 号为 V40（MCP audit table 已预订，常量 `V40_MCP_AUDIT`）。V41+ 建议占位：

| V | 内容 | 关联 Milestone / 代号 |
|---|---|---|
| V41 | `agent_session_spawn_edges(parent_thread_id, child_thread_id, status, ...)` | M8 SA3 |
| V42 | `thread_goals(id, thread_id, text, status, ...)` | M1 LX2 |
| V43 | `installed_plugins(plugin_id, version, marketplace, source_url, ...)` | M3 P2 |
| V44 | `task_events_rollout(kind, payload_json, task_id, thread_id, ts, ...)` | M1 S2 |
| V45 | `agent_sessions.parent_thread_id NULLABLE` + `spawn_depth INTEGER DEFAULT 0` | M8 S3+SA3 |
| V46 | `automation_dead_letter(activity_id, last_error, attempts, ...)` | M3 C2 |
| V47 | `agent_jobs(id, thread_id, status, ...)` + `agent_job_items(...)` | M3+M8 A1 |
| V48 | personality_profile_id / personality_evolved columns | M2 LX1 |
| V49 | logs DB 拆分（audit_log → uclaw_logs.db） | M4 S4 |
| V50 | cost DB 拆分（cost_records → uclaw_cost.db） | M4 S4 |
| V51 | `hook_configs(id, event, matcher_regex, command_argv, execution_mode, trust_status, ...)` | M5 H1 |
| V52 | `evolution_promotions(id, candidate_kind, version, status, source_trace_ref, harness_result_json, safety_impact_json, rollback_plan_json, owner, approved_at, promoted_at)` | M7 |
| V53 | `world_projection_snapshots(task_id, version, snapshot_json, ts)` | M4 |
| V54 | `capability_cards_cache(card_id, kind, family, harness_score, last_evaluated_at, ...)` | M3 |
| V55 | `cost_records.cached_input_tokens + reasoning_output_tokens` 列扩展 | M1 T7 |

**注**：以上 V 号是建议占位。每次开 PR 必须按 CLAUDE.md *Active migration registry* 流程查实时可用号。

---

## 24. 17 个 Codex Crate 在 uclaw 的集成映射（避免孤儿）<a id="24-crate-integration-map"></a>

> 本章解决 v2.0 遗留问题：**复制过来的 17 个 codex crate 必须被真实使用，不能成为孤儿 crate**。
>
> 实地核查 codex 内部如何使用这些 crate（哪些 crate 依赖它们 + 关键 import 行）后，给出 uclaw 中的**精确集成点**：每个 crate 在哪个 uclaw 模块用、替换什么现有代码、归属哪个 Milestone。

### 24.1 crate × uclaw 模块 集成矩阵

下表把每个 crate 的实际使用面、codex 内部依赖者、uclaw 集成目标三者一一对应。

| # | Crate | codex 内部依赖者（实测） | uclaw 集成模块 | 替换的现有代码 / 新建用法 | Milestone |
|---|---|---|---|---|---|
| 1 | **uclaw-utils-template** | `core`, `models-manager`, `memories/{read,write}`, `login` | `agent/prompts/` + `agent/mode_prompts.rs` + `agent/gbrain_prompt.rs` + `capabilities/plugin_manifest.rs` + `evolution/candidate/prompt_patch.rs` | (a) 替代 `format!` / `String::replace` 的 prompt 拼接；(b) 编译期 `LazyLock::new` 校验占位符；(c) Plugin manifest 中 `{{ var }}` 占位符渲染 | **M2-A/B/E/G** + M3-T6 + M7-T3 |
| 2 | **uclaw-utils-string** | `tools`, `core`, `tui`, `output-truncation`, `otel`, `protocol`, `rollout`（**最广**）| `agent/tools/builtin/*.rs`, `agent/context/`, `tauri_commands.rs`（**实测有 7+ 处可替换**）, `harness/budget.rs`, `observability/`, `agent/rollout/recorder.rs` | (a) `tauri_commands.rs:1867 .chars().take(500)` → `take_bytes_at_char_boundary`；(b) `tauri_commands.rs:8626 .chars().count() > 120` → `approx_token_count`；(c) `tauri_commands.rs:12220 .chars().take(max_stem)` → `take_bytes_at_char_boundary`；(d) 所有 tool 截断；(e) 所有 token 估算；(f) JSON ASCII 序列化（`to_ascii_json_string` 用于 OTel tag 安全）| **M1-T6** + **M2-H L1** |
| 3 | **uclaw-utils-cache** | `core`, `utils/image` | `automation/filters.rs`（REGEX_CACHE 改造）, `agent/context/agents_md_cache.rs`（新增）, `agent/context/skill_parse_cache.rs`（新增）, `uclaw-utils-image` 内置依赖, `mcp.rs`（tool schema 缓存） | (a) `automation/filters.rs:7 static REGEX_CACHE: Lazy<Mutex<HashMap>>` → `BlockingLruCache<String, Regex>`；(b) UCLAW.md / AGENTS.md 等价物的解析结果缓存（sha1_digest 做内容指纹）；(c) Skill markdown 解析缓存；(d) MCP 工具 schema normalize 后缓存 | **M2-C** + **M3-T2** |
| 4 | **uclaw-utils-fuzzy** | `tui`（slash_commands / skill_popup / mentions_v2 / multi_select_picker / skills_helpers，**至少 5 处**） | `ui/` 的 React 端通过 Tauri command 暴露 + `agent/skills_manifest.rs` + `proactive/failure_memory.rs` + `proactive/skill_parser.rs` 替代部分自写 fuzzy | (a) `ui/` slash command palette 模糊匹配（新增）；(b) Skill picker（新增）；(c) File mention `@filename` 模糊匹配（新增）；(d) `proactive/failure_memory.rs:335` 现有 fuzzy 加固；(e) `proactive/skill_parser.rs:276` bigram-Jaccard dedup 可补 fuzzy_match 作为 secondary signal | **M3-T2** + UI 工作 |
| 5 | **uclaw-utils-elapsed** | `tui`（exec_cell/render） | `harness/episode.rs`（duration 显示）, `automation/activity.rs`, `browser/session_state.rs`（步骤耗时）, `ui/` 时间显示 hook | 替代各处自实现的 duration 格式化（"1m 23s" 风格） | **M1-T6** + UI |
| 6 | **uclaw-utils-json-toml** | `mcp-server`, `app-server` | `capabilities/plugin_manifest.rs`（YAML/TOML/JSON 互转）, `automation/spec.rs`（spec 跨格式 import/export）, `config/` 导入导出 | (a) Plugin manifest 跨格式；(b) Automation spec 跨格式；(c) Settings export/import | **M3-T6** + Settings UI |
| 7 | **uclaw-utils-readiness** | **codex 内部无使用者**（仅 workspace 成员） | **uclaw 是首个用户** → `services/manager.rs` ServiceManager 启动信号 + `gbrain/mod.rs` 就绪 + `memu/client.rs` 就绪 + `mcp.rs` server 启动就绪 + plugin load 完成 | (a) 替代当前 ad-hoc Notify/oneshot 启动同步；(b) 每个 service 实现 `Readiness` trait + 启动期间统一展示；(c) UI 启动 splash 显示各 service 状态 | **M3-T3** + service refactor |
| 8 | **uclaw-utils-sleep** | `tui`（chatwidget/turn_lifecycle） | `automation/runtime/service.rs`（长 automation 跑）, `harness/runtime.rs`（episode 跑）, `evolution/simulation.rs`（dry-run）, `browser/agent_loop.rs`（长 browser task），用户可配 toggle | (a) 长任务期间防 OS sleep（用户已显式选择"代理跑活"语义）；(b) Harness 跑分期间防 throttle；(c) Browser 长会话期间保持 wake | **M2-H L7** + **M6** + **M7-T4** |
| 9 | **uclaw-utils-stream** | `core`（SSE 解析） | **强烈推荐** 替换 `llm/providers/anthropic.rs:521-659` 自实现 SSE state machine + `llm/providers/openai.rs:253+` + 其他 provider | (a) Anthropic SSE 解析统一；(b) OpenAI / DeepSeek / Gemini / Groq SSE 解析统一；(c) 未来 MCP stream / browser event stream 解析 | **M1-T8**（流式分层超时）+ provider 重构 |
| 10 | **uclaw-async-utils** (`OrCancelExt`) | `codex-mcp`, `core`, `protocol`（codex 内部 ≥ 8 个文件用 `or_cancel`） | `agent/task.rs`（M1-T2 SessionTask）, `browser/agent_loop.rs`, `automation/runtime/service.rs`, `evolution/pipeline.rs`, `mcp.rs`（MCP 工具调用 timeout）, `safety/hook_bus.rs`（hook 执行 timeout） | 替换所有自写 `tokio::select! { _ = cancel ... }` 为 `.or_cancel(&token)` —— 提升一致性和可读性 | **M1-T2** 起全 milestone 持续使用 |
| 11 | **uclaw-file-watcher** | `app-server`（fs_watch + skills_watcher） | `agent/context/project_doc.rs`（UCLAW.md hot reload）, `skills.rs`（skills hot reload）, `mcp.rs`（MCP config hot reload）, `capabilities/plugin_registry.rs`（plugin manifest hot reload）, `agent/prompts/` 文件变更监听 | **统一所有热加载逻辑**到一个 watcher 服务（避免每个模块各起 `notify` 监听）| **M2-B/C** + **M3-T6** + **M3-T9** |
| 12 | **uclaw-file-search** | `tui`, `rollout`, `app-server`（≥ 3 个 crate） | `agent/tools/builtin/file_search.rs`（新 builtin tool，替代 ad-hoc grep 模式）, `agent/rollout/replayer.rs`（rollout 文件发现）, `skills.rs`（skill 文件递归扫描）, `ui/` 文件 picker | (a) 新 builtin tool `file_search` 给 LLM 用；(b) Rollout 列表加载性能提升；(c) Skill 扫描换用 `ignore` + `nucleo` | **M1-T5** + **M3-T4** |
| 13 | **uclaw-utils-abs-path** (`AbsolutePathBuf`) | `app-server-transport`, `network-proxy`, `tools`, `core`, `core-plugins`, `config`（**最广，9+ crate**） | uclaw **全仓** —— 应作为 THE PATH TYPE：`safety/path_policy.rs`, `automation/runtime/`, `browser/identity/`, `agent/session.rs`, `workspace/`, `automation/spec.rs`, `capabilities/plugin_manifest.rs`, `evolution/registry.rs` | 渐进替换：`PathBuf` → `AbsolutePathBuf`。先在新代码强制使用，再迁移热点旧路径参数。**这是路径安全的根本约束** | **M3-T6 后** 渐进迁移，跨全 Milestone |
| 14 | **uclaw-utils-home** | `rmcp-client`, `network-proxy`, `core`, `install-context`, `tui`, `arg0`, `app-server-daemon`（**8+ crate**） | `tauri_commands.rs`（**实测 5+ 处 `dirs::home_dir()` + `.uclaw` 硬编码**）, `memubot_config.rs`, `secrets/`, `app.rs` 初始化 | 把 **`tauri_commands.rs:872, 943, 4571-4573, 11857-11931` + `memubot_config.rs:741-764` 全部** `dirs::home_dir().unwrap().join(".uclaw")` 替换为 `uclaw_home()` 单一函数（对位 codex 的 `codex_home()`） | **M0 Phase 0.5 后立即** |
| 15 | **uclaw-utils-path** | `core`, `config`, `tui`, `cli`, `rollout`, `thread-store`（**最广**） | `safety/path_policy.rs`（路径规范化）, `workspace/`, `files_rail/`, `git/`, `automation/runtime/` 中路径比对 | 统一路径规范化：跨平台 `dunce::canonicalize`, normalize_for_path_comparison 等。**避免 macOS /private/var vs /var 这种 symlink 陷阱** | **M3-T6 后** 渐进迁移 |
| 16 | **uclaw-utils-image** | `core`, `protocol` | `browser/perception/` 截图 base64, `proactive/scenarios/multimodal_context`, `agent/tools/builtin/view_image.rs`（新 builtin tool）, `preview/` 图片预览 | (a) 新 builtin tool `view_image` 给 LLM 用；(b) Browser 截图统一 base64 + mime detection；(c) Multimodal context 图片处理 | **M3-T4**（view_image tool）+ **M6** browser 集成 |
| 17 | **uclaw-utils-pty** | `rmcp-client`, `tools`, `core`, `exec-server`, `app-server`, `windows-sandbox-rs`（**6+ crate**） | `agent/tools/builtin/shell.rs`（PTY 模式可选）, `agent/tools/builtin/unified_exec.rs`（新增，M3-T4）, `browser/` 如需 PTY 控制 | (a) 升级 shell tool 支持 PTY 模式（capture stdout/stderr + 真彩色 + 交互输入）；(b) Unified exec 长进程 stdin 写入；(c) MCP server 用 PTY 模式启动子进程时复用 | **M3-T4 unified_exec** + shell refactor |
| 18 | **uclaw-utils-output-truncation** | `core`, `ext/memories`, `core-skills`, `models-manager`, `hooks`, `external-agent-sessions`, `memories/write`（**7+ crate，几乎渗透核心**） | uclaw 全仓 —— **PR-T1 / M2-H L1 的根**：`agent/tools/builtin/*.rs` 每个 handler 出口 + `agent/compact/local.rs` + `agent/context/*_fragment.rs` token budget + `memory.rs` 写入截断 + `evolution/candidate/*.rs` candidate 描述截断 + `automation/runtime/` activity 输出截断 | **每个 tool handler 出口** 加 `formatted_truncate_text` —— uclaw 当前 13 个 builtin tool 全部需要改 | **M2-H L1** 主线 + 跨全 Milestone |

### 24.2 4 个"被 codex 自己用得最广"的 crate（uclaw 也应高频使用）

按 codex 内部依赖者数量排序，这 4 个是**真正基础设施级**：

1. **uclaw-utils-abs-path**（codex 9+ crate 用） —— 必须成为 uclaw 路径强类型
2. **uclaw-utils-home**（codex 8+ crate 用） —— 必须取代 `dirs::home_dir().unwrap().join(".uclaw")` 散落代码
3. **uclaw-utils-output-truncation**（codex 7+ crate 用） —— 必须在每个 tool handler 出口
4. **uclaw-utils-pty**（codex 6+ crate 用） —— 必须成为 shell / unified_exec 的底座

这 4 个 crate **不立即广泛使用就是放弃 codex 集成的最大杠杆**。

### 24.3 1 个 uclaw 是首个采用者的 crate

**uclaw-utils-readiness** —— codex 把它定义了但**自己尚未使用**。uclaw 的 `services/manager.rs::ServiceManager` 是天然的首个用户。这意味着 uclaw 可以**比 codex 更早**用上这个抽象，反哺设计反馈。

### 24.4 现有 uclaw 代码中可直接替换的具体位置（实测，**非推测**）

| uclaw 现有代码 | 替换为 | crate |
|---|---|---|
| `src-tauri/src/tauri_commands.rs:872` `dirs::home_dir()` | `uclaw_utils_home::uclaw_home()` | uclaw-utils-home |
| `src-tauri/src/tauri_commands.rs:943` `dirs::home_dir().unwrap().join(".uclaw")` | `uclaw_utils_home::uclaw_home()` | uclaw-utils-home |
| `src-tauri/src/tauri_commands.rs:4571-4573` `dirs::home_dir().unwrap().join(".uclaw").join("skills")` | `uclaw_utils_home::uclaw_skills_dir()` | uclaw-utils-home |
| `src-tauri/src/tauri_commands.rs:1867` `title_content.chars().take(500).collect::<String>()` | `uclaw_utils_string::take_bytes_at_char_boundary(&title, 2000)` | uclaw-utils-string |
| `src-tauri/src/tauri_commands.rs:8626` `if context.chars().count() > 120` | `if uclaw_utils_string::approx_token_count(context) > 30` | uclaw-utils-string |
| `src-tauri/src/tauri_commands.rs:12220-12221` stem truncation | `uclaw_utils_string::take_bytes_at_char_boundary` | uclaw-utils-string |
| `src-tauri/src/automation/filters.rs:7` `static REGEX_CACHE: Lazy<Mutex<HashMap<String, Regex>>>` | `static REGEX_CACHE: LazyLock<BlockingLruCache<String, Regex>>` | uclaw-utils-cache |
| `src-tauri/src/memubot_config.rs:741, 764` data_dir 拼接 | `uclaw_utils_home::uclaw_home()` | uclaw-utils-home |
| `src-tauri/src/llm/providers/anthropic.rs:521-659` SSE 自实现 state machine | `uclaw_utils_stream::SseParser` | uclaw-utils-stream |
| `src-tauri/src/llm/providers/openai.rs:253+` SSE 自实现 | 同上 | uclaw-utils-stream |
| `src-tauri/src/proactive/failure_memory.rs:335` 现有 fuzzy_matches SQL | + `uclaw_utils_fuzzy::fuzzy_match` 二次评分 | uclaw-utils-fuzzy |
| 所有 `format!("{}\n{}", base_prompt, user_section)` | `Template::parse(...)?.render([(k, v), ...])` | uclaw-utils-template |
| 所有 `pathbuf.canonicalize()` 散落 | `uclaw_utils_path::normalize_for_path_comparison` | uclaw-utils-path |

### 24.5 集成的"反孤儿"硬性规则

为防止 crate 复制后被遗忘，提议三条硬性规则：

**规则 1**：每个 crate 必须在 30 天内有第一个生产使用者
- CI 加 check：每个 `uclaw-*` 子 crate 必须被 `src-tauri/Cargo.toml` 或另一个 `uclaw-*` crate 依赖
- 反向 check：被引入但 6 个月无 `use` 调用 → CI warning

**规则 2**：M2-H L1（TruncationPolicy 落地）必须用 `uclaw-utils-output-truncation`，不允许另起
- code review 规则：新增 tool handler PR 必须 import `uclaw_utils_output_truncation`

**规则 3**：M0 Phase 0.5 后**立即**做 `uclaw-utils-home` 全仓替换 sweep（半天工作）
- 把所有 `dirs::home_dir().unwrap().join(".uclaw")` 一次性 sed → `uclaw_home()`
- 这是最快产生"它真的被用"信号的动作

### 24.6 集成时间表（嵌入 ADR Milestone）

| Milestone | 必须用上的 crate | 集成动作 |
|---|---|---|
| **Phase 0.5** | template / string / cache / fuzzy / async-utils / file-watcher / output-truncation | 复制 + 落地（NOTICE） |
| **Phase 0.5 收尾** | **home / abs-path** | 全仓 sweep 替换 `dirs::home_dir` + path types |
| **M1** | string (token est) / elapsed / async-utils (OrCancelExt) / file-search (rollout) / stream (SSE 重构) | M1-T6 + T7 + T8 配套 |
| **M2-A/B/E/G** | template (所有 prompt) / file-watcher (UCLAW.md) | M2 主线 |
| **M2-C** | cache (fragment 缓存) | M2-C 配套 |
| **M2-H L1** | output-truncation (所有 tool 出口) | M2-H 主线 |
| **M2-H L7** | sleep-inhibitor (长 compaction 防睡) | M2-H L7 配套 |
| **M3-T2** | fuzzy (skill/tool search) | M3-T2 配套 |
| **M3-T3** | readiness (service manager) | M3-T3 配套 |
| **M3-T4** | pty (unified_exec) / image (view_image tool) / file-search (file_search tool) | M3-T4 主线 |
| **M3-T6** | template (plugin manifest) / json-toml (跨格式) | M3-T6 配套 |
| **M3-T9** | file-watcher (MCP config) | M3-T9 配套 |
| **M6** | pty / image / sleep-inhibitor (browser) | M6 配套 |
| **M7-T3/T4** | template (prompt patch candidate) / sleep-inhibitor (simulation) | M7 配套 |
| **持续** | abs-path / path (路径强类型迁移) | 跨全 Milestone |

### 24.7 整合后的预期最终状态

完成全部 Milestone 后，uclaw 中：
- **0 个 `dirs::home_dir().unwrap().join(".uclaw")` 散落**（全走 uclaw_utils_home）
- **0 个手写 SSE state machine**（全走 uclaw_utils_stream）
- **0 个 `format!()` 拼接 prompt**（全走 uclaw_utils_template）
- **0 个 tool handler 不截断输出**（全走 uclaw_utils_output_truncation）
- **0 个 ad-hoc `tokio::select! { cancel }`**（全走 .or_cancel）
- **0 个孤儿 crate**

---

## 25. 跨文档一致性核查（v2.1 新增）<a id="25-跨文档一致性核查"></a>

为避免"二次真相"（同一概念在两文档定义不一致），本节列出**所有需要在两份文档同步的关键事实**。任何更新一处后必须同步另一处。

### 25.1 单一真相清单（Single Source of Truth）

| 概念 | 唯一定义位置 | 引用规则 |
|---|---|---|
| ADR 11 层模型 | ADR `2026-05-20-uclaw-agent-platform-north-star.md` §6 | 两文档 §2.3 / §1.1 都**引用**而非重述 |
| ADR 9 Milestone | ADR §16 | 两文档**完全沿用** M0-M9 命名 |
| License = Apache-2.0 | 对比文档 §3.1 | 实施方案 §2.2 P0.5-T1 引用 |
| Phase 0.5 步骤 | 实施方案 §2.2 | 对比文档 §17.6 / §24.6 仅引用 |
| 17 crate 列表 | 对比文档 §17.2 + §24.1 | 实施方案 §17.1 表对齐 |
| 13 个 HookEventName | 对比文档 §10.1 + ADR §10.1 | 实施方案 §7.2 M5-T1 实现 |
| 7 个 Context Tools | 对比文档 §7.3 + ADR §8.3 | 实施方案 §4.2 M2-F 实现 |
| 9 个 Candidate Type | 对比文档 §11.2 + ADR §13.1 | 实施方案 §9.2 M7-T3 实现 |
| 7 个 Isolation Profile | 对比文档 §10.3 + ADR §10.3 | 实施方案 §7.2 M5-T4 实现 |
| 12 个 HarnessSubject | 现有 `harness/case.rs` 代码 | 两文档均引用 |
| 迁移注册表 V41-V55 | 实施方案 §17 附录 A | 对比文档 §23 附录 C 同步 |
| 5 大 Registry | 对比文档 §8.2 + ADR §9.2 | 实施方案 §5.2 M3 实现 |

### 25.2 已修正的潜在矛盾点

| 矛盾点 | v2.1 修正 |
|---|---|
| 总时长（v1.1 估 15-18 月 vs v2.0 估 8-10 月 vs v2.0 实施方案 6-8 月） | **v2.1 单一权威值**：M1-M8 约 **8-11 月**（中位 ~9.5 月）；含 M9 约 **13-15 月**（中位 ~14 月）。两文档统一 |
| codex `Personality` enum 字段（None/Friendly/Pragmatic vs 5 档） | **采用实地核查值**：仅 None/Friendly/Pragmatic 三档 |
| AgentStatus enum 变体数（v1 7 个 vs v2 8 个含 NotFound） | **采用实地核查值**：8 个（PendingInit/Running/Interrupted/Completed/Errored/Shutdown/NotFound/+Default tag），且 Interrupted 非终态 |
| V40 占用（v1.0 spawn_edges vs 实际 mcp_audit） | **采用实地核查值**：V40 = mcp_audit；spawn_edges 占 V41 |
| codex base prompt 字数（v1.0 估 vs 实测） | **采用实测值**：~10K 字 / ~2K tokens / 12 个 block |
| Crate 17 还是 20（v1.2 vs v2.0） | **统一为 17 个第一档 + 3 个第二档 + 2 个第三档（除 ansi-escape 跳过）**，共 22 个候选，**Phase 0.5 实做 17+1 = 18 个** |
| uclaw 现状（v1.0 严重低估）| **采用 v2.0 实地核查**：harness/learning/browser v2/channels/extensions/symphony_graph 都已就位，136,283 行 Rust |

### 25.3 二次扫描后修正的事实细节

通过本轮（v2.1）再次扫描 codex 内部 crate 依赖关系，新增确认事实：

- **`uclaw-utils-readiness` 在 codex 内无依赖者** —— uclaw 是首个采用者，可作为反向贡献机会
- **`uclaw-utils-string` 是 codex 内**最广**使用的 utility crate**（被 tools/core/tui/output-truncation/otel/protocol/rollout 等 7+ crate 依赖）—— 这强化了 M2-H L1 把它放在最重要位置的合理性
- **`uclaw-async-utils::OrCancelExt` 在 codex `core/src/session/turn.rs` 单文件就有 4 处用法** —— 证明 cancellation cascade 是 codex 长会话稳定性的核心模式，uclaw 应在 SessionTask 中全面采用
- **`uclaw-utils-pty` 被 codex `tools` + `core` + `exec-server` + `app-server` 等 6+ crate 用** —— 这不仅是底层工具，是 codex 整套 exec 系统的根基；uclaw 引入它配合 unified_exec（M3-T4）能直接打通整套 exec 现代化
- **`uclaw-utils-output-truncation` 是 codex 内排第二广使用的 utility**（仅次于 string）—— 在 `core` + `ext/memories` + `core-skills` + `models-manager` + `hooks` + `external-agent-sessions` + `memories/write` 都被用，证明 truncation 不只是工具输出，**任何文本进入 prompt 前都该 truncate**，这是 M2 多个子任务的隐含规约

### 25.4 一致性维护规则

未来更新两份文档时必须遵守：

1. **更新 ADR 任何条目** → 两份文档同步更新 §2 / §1.1 引用
2. **更新 17 crate 任何条目** → 对比文档 §17.2 + §24.1 + 实施方案 §17 必须三处同步
3. **更新迁移注册表** → 对比文档附录 C + 实施方案附录 A + CLAUDE.md V-table 三处同步
4. **更新 Milestone 任务** → 实施方案 §M*-T* 是唯一源；对比文档 §19 仅引用代号映射
5. **更新 License** → 对比文档 §3 是唯一源；实施方案 §2.2 P0.5-T1 仅引用执行步骤
6. **更新 codex 实地源码引用** → 必须 grep 验证后再写入；否则在被引用处加 "(待核查)" 标注

### 25.5 ADR Baseline 对齐复核

最终复核：本文档每章都对应 ADR §6 的 11 层之一或 §5 Autonomy Ladder：

| 本文档章 | ADR 章 | 对齐状态 |
|---|---|---|
| §5 Intent Layer | ADR §7.1 | ✅ |
| §6 Runtime Kernel | ADR §6 Kernel | ✅ |
| §7 Context Fabric | ADR §8 | ✅ |
| §8 Capability Mesh | ADR §9 | ✅ |
| §9 World Projection | ADR §7.4 | ✅ |
| §10 Safety & Policy | ADR §10 | ✅ |
| §11 Evolution Layer | ADR §13 | ✅ |
| §12 Workers | ADR §4.4 + §4.5 + §14 | ✅ |
| §13 gbrain & Memory | ADR §11 | ✅ |
| §14 Browser Provider | ADR §12 | ✅ |
| §15 Autonomy Ladder | ADR §5 | ✅ |
| §16 Hermes 参考 | ADR §3.1 | ✅ |

**所有改进设计、Phase 安排、Crate 集成均统一在 ADR Agent OS v2 北极星之下**。无独立运行时模型、无并行 task lifecycle、无替代 capability registry —— 100% baseline 对齐。

---

## 26. v2.2 自审报告 + ADR §17 风险对齐 <a id="26-v22-自审报告"></a>

> 本章是对 v2.1 全文 + 实施方案 v2.1 + 当前 uclaw codebase 的**三向严格审查报告**，由 3 个独立 subagent 并行完成（fidelity / gap / risk），用户进一步要求**严格遵循 ADR baseline + 修复差距**。

### 26.1 三向审查方法

| 审查者 | 范围 | 输出 |
|---|---|---|
| **Subagent 1 (Doc Fidelity)** | ADR § ↔ 两份文档术语 / Milestone / 11 问 / 引用 / 一致性 | 评分 + CRITICAL/MAJOR/MINOR 问题清单 |
| **Subagent 2 (Codebase Gap)** | ADR § ↔ uclaw codebase 实际类型 / 函数 / 模块 | 15 项实现度评分 + Gap 清单 |
| **Subagent 3 (Risk Assessment)** | M0-M9 推进路径 + 当前 codebase 状态 → 实施陷阱 | 15 个 risk + 10 条红线 + 5 个分岔点 |

### 26.2 审查结果总评

**文档对 ADR 忠诚度**：**8/10**（无 CRITICAL，3 个 MAJOR + 5 个 MINOR）

| 维度 | 评分 |
|---|---|
| 术语忠诚度 | 8.5/10 |
| Milestone 对齐 | 9/10 |
| ADR §18 11 问对齐 | 8/10 |
| 风险覆盖（vs ADR §17） | 7/10 |

**codebase 对 ADR 实现度**：**22%**（**重要发现，远低于 v2.0 估计**）

| ADR 层 / 概念 | 实现度（0-10）|
|---|---|
| Runtime Contracts (IntentSpec/TaskSpec/TaskEvent) | **2/10** ❌ |
| Context Fabric (7 tools + budget) | **1/10** ❌ |
| Capability Mesh (5 Registry + Card) | **1/10** ❌ |
| World Projection | **0/10** ❌ |
| Safety/Hooks/Isolation | 3/10 ⏳ |
| Memory Provider Strategy | 4/10 ⏳ |
| Browser Provider (抽 trait) | 3/10 ⏳ |
| Evolution Factory | 4/10 ⏳ |
| Autonomy Ladder (L0-L6) | **0/10** ❌ |
| Workers/Teams/Cluster | 3/10 ⏳ |
| CapabilityProfile 落地 | **0/10** ❌ |
| uclaw_home + 路径强类型 | **0/10** ❌ |
| Harness 对位 ADR | **6/10** ✅（uclaw 优势）|
| Human Boundary 检测 | 5/10 ⏳ |
| Memory Write Receipts | **0/10** ❌ |

**总实现度平均：~2.2/10 = 22%**

**实施风险等级**：**MEDIUM-HIGH**

成功率预测（3 人团队 9.5 月）：
- 保守 55% / **中位 72%** / 乐观 85%

**三大不确定源**：
1. agentic_loop.rs 882 行重构陷阱（R-1 + R-6）
2. M2 Context Fabric 与既有 compress_context 的双重压缩冲突（R-7）
3. V41-V55 数据库迁移一致性（R-13）

### 26.3 文档层 MAJOR 修复（v2.1 → v2.2 已应用）

以下 7 处偏差**本次更新已直接修正**：

| # | 偏差 | 修复 |
|---|---|---|
| F1 | §10.1 HookEventName 缺 13×3 完整矩阵 | ✅ §10.1 已补全 can_block / can_mutate / must_emit_event 矩阵 + 主要用途列 |
| F2 | §11.2 仅列 8 类 Candidate（缺 regression harness case） | ✅ §11.2 已补第 9 类 |
| F3 | §13.2 未明文 "memory_graph 冻结" | ✅ §13.2 已加 ADR §11.2 冻结约束 + 防御性 panic 设计 |
| F4 | §20 未对应 ADR §17 风险注册表 | ✅ 见下方 §26.4 完整 ADR §17 ↔ uclaw 缓解矩阵 |
| F5 | §24.1 Crate 清单缺 Priority 列 | ✅ 见下方 §26.5 Priority-annotated 清单 |
| F6 | §13 缺"3 类知识"显式分类 | ✅ 见 §26.6 三类知识在代码组织中的对应 |
| F7 | ADR §18 11 问在多个章节回答不完整 | ⏳ §26.7 给出每章 11 问的补全标准（后续应用到各章末） |

### 26.4 ADR §17 11 类风险 × uclaw 缓解矩阵（补 F4）

ADR §17 列 11 类系统性风险，本节给出对应缓解措施位置：

| # | ADR §17 风险 | 触发信号 | uclaw 缓解措施位置 |
|---|---|---|---|
| 1 | Product becomes a feature pile | 新模块加私有 state / policy / eval 自己一套 | ADR §18 11 问 + §26.7 强制；任何 PR 必经 4 答（intent / context / capability / harness）|
| 2 | Agent OS becomes too abstract | milestone 无可见用户价值 | §1.2 优先级表 + §26.9 用户感知排序（PR-T1 立竿见影）|
| 3 | Context Fabric becomes another memory system | context.* tools 写持久 fact | §7.3 context.* 工具 transient by default；写持久必经 PreMemoryWrite hook → gbrain |
| 4 | Capability Mesh becomes plugin chaos | 每 plugin 全局曝露 tool | §8.5 CapabilityProfile allowed/denied lists；M3-T8 + ToolExposure 集成 |
| 5 | Hooks become invisible magic | hook 改 behavior 无 trace | §10.1 must_emit_event 列强制；CI lint M5 实现 |
| 6 | Self-evolution corrupts behavior | prompt/skill 自动 promote | §11.4 6 类禁止 promotion 硬性 check；M7-T5a 子任务（实施方案 §9.2 补） |
| 7 | Memory splits again | 新 feature 写 memory_graph | §13.2 冻结约束 + panic 防御 + CI lint |
| 8 | Browser stack becomes clone | 新 browser feature 仅 LocalChromium 路径 | §14.4 BrowserProvider trait + 多 provider plugin stub |
| 9 | Teams duplicate runtime | team 自己起 loop/tool/policy | §12.4 M8-T1 IntentSpec/TaskEvent 化 |
| 10 | Cluster leaks private context | remote worker 收到本地全局 state | §12.4 M9 data locality policy + explicit context refs；实施方案 §11 M9-T0 新增任务 |
| 11 | UI becomes panel sprawl | 每模块自己渲染状态 | §9 WorldProjection 唯一真理来源；M4 强制各 UI consumer 改通过 projection |

### 26.5 17 Crate Priority 标注（补 F5）

§17.2 第一档 17 个 crate 按集成时间 + ROI 分级：

| Crate | Priority | 落地 Phase | 用户感知 ROI |
|---|---|---|---|
| uclaw-utils-template | **P0** | Phase 0.5-T3 | ★★★★★ |
| uclaw-utils-string | **P0** | Phase 0.5-T3 | ★★★★★ |
| uclaw-utils-cache | **P0** | Phase 0.5-T3 | ★★★★ |
| uclaw-utils-fuzzy | **P0** | Phase 0.5-T3 | ★★★★ |
| uclaw-async-utils | **P0** | Phase 0.5-T3 | ★★★★ |
| uclaw-file-watcher | **P0** | Phase 0.5-T3 | ★★★★★ |
| uclaw-utils-output-truncation | **P0** | Phase 0.5-T4 | ★★★★★（M2-H L1 根） |
| uclaw-utils-home | **P0** | Phase 0.5-T6（sweep）| ★★★★（实测 5+ 处可替换） |
| uclaw-utils-abs-path | **P1** | Phase 0.5-T5 + 持续 | ★★★★（持续渐进迁移） |
| uclaw-utils-path | **P1** | Phase 0.5-T5 + 持续 | ★★★ |
| uclaw-utils-elapsed | **P1** | Phase 0.5-T5 | ★★★ |
| uclaw-utils-readiness | **P1** | M3-T3（uclaw 首个用户） | ★★★★ |
| uclaw-utils-sleep | **P1** | M2-H L7 + M6 配套 | ★★★★ |
| uclaw-utils-stream | **P1** | M1-T8（SSE 重构） | ★★★★ |
| uclaw-file-search | **P1** | M1-T5 + M3-T4 | ★★★★ |
| uclaw-utils-image | **P2** | M3-T4（view_image tool）| ★★★ |
| uclaw-utils-pty | **P2** | M3-T4（unified_exec）+ M6 | ★★★ |
| uclaw-utils-json-toml | **P2** | M3-T6（plugin manifest）| ★★ |

**P0 = Phase 0.5 必落地（7 个）+ 立即 sweep（home）= 8 个**
**P1 = M1-M3 期间落地（7 个）**
**P2 = M3-T4 / M6 配套（3 个）**

### 26.6 3 类知识在 uclaw 代码组织的对应（补 F6）

ADR §11.1 定义 3 类知识：

| 类型 | 内容 | uclaw owner | 实现状态 |
|---|---|---|---|
| **Factual** | 持久 user/project/domain facts | **gbrain**（已实地核查 `src-tauri/src/gbrain/`） | ✅ active |
| **Evidential** | traces / logs / outputs / receipts / scorecards | **harness/run ledger**（`src-tauri/src/harness/` 11 文件 + rollout JSONL） | ✅ active（部分） |
| **Executable** | skills / SOPs / browser scripts / prompts / policies | **Evolution Layer + Registries**（`src-tauri/src/learning/` + 未来 capabilities/） | ⏳ 部分 |

**规则强化**：
- 任何 fact 写入必经 gbrain，禁止直接进 memory_graph
- 任何 trace 必走 rollout JSONL + harness episode（M1-T5）
- 任何 skill/SOP/script 必经 Evolution Factory promotion gate

### 26.7 每章末 ADR §18 11 问完整标准（补 F7）

下表是后续每个改进设计章节末**必须**包含的 11 问回答标准。两文档中现有部分简化回答需逐一补全：

| 问题 | 标准回答类型 |
|---|---|
| 1. 用户 intent | 具体 IntentSpec.origin 取值 + userGoal 类型 |
| 2. autonomy 等级 | L0-L6 具体取值（不能写"全适用"） |
| 3. canonical truth source | 具体表名 / 文件 / 类型 |
| 4. emit 什么 TaskEvent | 13 variants 中具体几个 |
| 5. 读什么 context, 怎么 cite | 具体 ContextRef.source 取值 + 是否 cite |
| 6. 添加 / 消费 capability card | 具体 capability id + family |
| 7. 什么 policy hook 能 block | 13 hook 中具体几个 |
| 8. UI 渲染什么 world projection | projection field name + UI 组件 |
| 9. 什么 harness case 证明 | HarnessSubject 取值 + case 示例 |
| 10. rollback / disable 路径 | 具体 Feature flag 名 + 数据回滚步骤 |
| 11. **不拥有什么** | architectural boundary（"不拥有 X，因 X 属于 Y 层"） |

第 11 问尤其重要 —— **明确"不拥有什么" = 防止层间职责蔓延**。

### 26.8 codebase Gap 修复路径（按 P 级）

实施方案 v2.2 已加入以下补充任务：

**P0（不补就影响 ADR baseline）**：
- M1-T1 增量：完整 IntentSpec / TaskSpec / TaskEvent / AutonomyLevel / RiskClass Rust 类型 + 测试
- M3-T1 增量：5 Registry 完整类型骨架，避免循环初始化（R-8）
- M4-T1 增量：WorldProjection 完整 struct + apply_event subscriber

**P1（影响 milestone exit criteria）**：
- M2 全程：Context Fabric 7 tools 显式化 + ContextRef/ContextArtifact schema
- M5-T1 增量：HookBus + 13 event 完整 can_block/can_mutate/must_emit 矩阵代码化
- M11 新增（远期）：Memory Write Receipts 全链路
- M1 新增子任务：AutonomyLevel enum + AutonomyResolver
- M7-T5a 新增：6 类禁止 promotion 硬性 check（探测器）

**P2（架构整洁度）**：
- M6-T1 增量：BrowserProvider trait 抽 + LocalChromiumProvider adapter
- M9-T0 新增：WorkerNode Rust types + 8 kind
- 持续：uclaw_home + AbsolutePathBuf 全仓迁移
- M7-T2 增量：GEP ↔ Evolution Factory 显式映射

### 26.9 关键风险红线（精简自 subagent risk report）

**永远不要做的 10 件事**（subagent 详细推导，已纳入 v2.2 规约）：

1. ❌ M0-M1 期间修改 IntentSpec schema（后续全依赖）
2. ❌ memory_graph 新增字段作 M2+ 上下文源（违反 §11.2 冻结）
3. ❌ SessionTask trait 直接暴露 agentic_loop 内部状态（应通过 TurnContext）
4. ❌ Capability override 允许跨层循环（plugin override provider 但 provider 来自 plugin）
5. ❌ WorldProjection 直接修改 canonical state（应只读）
6. ❌ HookBus 允许 hook 进行远程 I/O（deadlock 风险）
7. ❌ BrowserProvider 隐藏 identity / cookie 管理（必须显式 IdentityContext）
8. ❌ Evolution Factory 自动 promote prompt/skill 到生产（必经 User Review）
9. ❌ Team 中某 role 绕过 HookBus 直接调用 LLM（policy 失效）
10. ❌ M9 之前引入"分布式 session"概念（M1-M8 假设单机）

### 26.10 5 个关键决策分岔点

| # | 决策 | 选项 | 推荐 | 影响范围 |
|---|---|---|---|---|
| D1 | M1 agentic_loop 包装策略 | A: 完整重构（高复用 / 高 regression）；B: 仅改外壳（低风险 / 高债） | **B 先做**，验证后再 A | M1-M8 全部 |
| D2 | M2 三档 compaction 启动节奏 | A: 全启（激进）；B: 灰度 1 月（保守） | **B**（保守）| 月度成本时间 |
| D3 | M3 Plugin discovery 层数 | A: 4 层（bundled/user/project/external）；B: 3 层 | **A**（与 ADR §3.1 对齐） | M8 + M9 |
| D4 | M4 WorldProjection 模式 | A: in-process channel；B: HTTP pubsub | **A**（性能优）| M9 Cluster readiness |
| D5 | M5 git worktree 自动创建粒度 | A: 每任务都 worktree（磁盘爆）；B: 仅 M3+ 任务（务实）| **B** | 文件系统压力 |

### 26.11 v2.2 升级总结

**修复内容**：
- 7 处文档偏差（HookEvent 矩阵 / Candidate 第 9 类 / memory_graph 冻结 / 风险注册表 / Crate Priority / 3 类知识 / 11 问标准）
- ADR §17 11 类风险完整对应缓解措施
- 现有 codebase 22% 实现度真相
- 15 个具体实施风险注册
- 10 条红线 + 5 个决策分岔点

**未变化**：
- ADR Baseline 100% 对齐（v2.0 / v2.1 已锁定）
- License = Apache-2.0 决策（§3，未变）
- Milestone M0-M9 命名（§2.4，未变）
- 总时长 M1-M8 中位 9.5 月 / M1-M9 中位 14 月（§2.4，未变）
- 17 crate 列表（仅加 Priority 列）

**v2.2 后状态**：两份文档 + ADR baseline 三者高度一致。codebase 22% 实现度是清醒认识，**不需要大改方案**，但需要严格执行 v2.2 的 P0/P1/P2 任务排序 + 红线规约。

---

> **本文档配套实施方案见《uclaw 代码库升级改造实施方案》v2.2**
>
> **版本历史**：
> - v1.0 (2026-05-19)：14 维度初版对比 + 改进设计
> - v1.1 (2026-05-19)：新增 Prompt + Token 专章
> - v1.2 (2026-05-19)：新增 17 crate 复制清单
> - v2.0 (2026-05-20)：ADR Agent OS v2 北极星对齐重写
> - v2.1 (2026-05-20)：新增 §24 crate 集成映射 + §25 跨文档一致性核查
> - **v2.2 (2026-05-20)**：三向 subagent 审查 + 7 处偏差修正 + ADR §17 风险对齐 + 22% 实现度真相 + 15 风险 / 10 红线 / 5 分岔点（本次更新）
