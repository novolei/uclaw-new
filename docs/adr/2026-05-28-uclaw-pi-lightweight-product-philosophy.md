# ADR — uClaw Pi-Lightweight Product Philosophy

- **Status:** Accepted (产品哲学基线;取代 Agent OS v2 North Star 的"重内核"定位)
- **Date:** 2026-05-28
- **Deciders:** Ryan (user, decider) + claude-opus-4-7 (grill-me 共商,7 项决策见 §6)
- **One-line target:** 一个 **Pi 式轻量、可插拔、领域无关的 agent 内核**,服务**日常大众用户 + vibe coding 用户**,并以 openhuman 的现代化记忆理念做记忆层。
- **Scope:** 产品哲学与框架方向。覆盖:内核形态、harness 含义、插件模型与加载机制、多领域机制、重支柱(Teams / World Projection / Evolution Factory / Cluster)的处置、记忆现代化方向。
- **Supersedes:** `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`(Agent OS v2 North Star)的**重内核产品定位**(§1-§5、§14 Teams/Cluster、§4.4/§4.5)。北极星 ADR 正文保留作历史与设计探索参考。
- **Superseded by:** none
- **Related code:** `src-tauri/src/agent/`(内核)、`src-tauri/src/harness/`(将重命名为 `eval/`)、`src-tauri/src/mcp.rs` + `mcp_server/`(子进程插件协议基础)、`src-tauri/src/registries/`(将塌缩为单一 `AgentApi` 句柄)、`src-tauri/src/memory*`(将收敛到单一 `MemoryAdapter`)
- **Related docs:** `docs/superpowers/specs/2026-05-27-pi-convergence-gap-audit.md`(代码级差距审计 + 5 阶段改造)、`docs/superpowers/specs/2026-05-26-agent-framework-pi-upgrade-design.md`(Pi 8 轴对标)、`docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md`(记忆专项将与之协调)
- **References (local):** `/Users/ryanliu/Documents/pi`(内核+插件)、`/Users/ryanliu/Documents/openhuman`(纯 Rust 类脑记忆)、`/Users/ryanliu/Documents/hermes-agent`(coding 编辑可靠性)

---

## 1. 决策摘要

uClaw 的产品定位从 **"Agent OS for long-running work"(重内核:Teams / Cluster / 4-Registry / World Projection / Evolution Factory / 自治阶梯 L0-L6)** 转向:

> **一个 Pi 式轻量、可插拔、领域无关的 agent 内核 —— 默认表面简单(服务日常大众用户),底层有纪律(支撑 vibe coding 用户),能力通过插件与 preset 横向扩展,记忆采用 openhuman 的现代化理念。**

转向的根据是 `2026-05-27-pi-convergence-gap-audit.md` 的核心发现:**uClaw 存在"影子架构 vs 在役架构"双层叠加**——ADR 要求的统一缝(registries / memory_contract / memory_policy / plugin_manifest / skill_selection / harness / TaskScheduler / workers)全是骨架/死代码/被绕过,而热路径各自平行实现。北极星 ADR §17 自列的 "memory splits again""Capability Mesh becomes skeleton" 两条风险已经发生。继续在"重内核"方向投入,会与"轻量大众"目标互相抵消。

本 ADR 不重复审计内容,只**锁定产品哲学与 7 项框架决策**,作为后续所有 agent 工作的判断基线。详细实施序列见审计 §4(5 阶段);任何阶段落地前经 `superpowers:writing-plans` 产出 bisectable 计划。

---

## 2. 为什么取代 Agent OS v2 重内核定位

| Agent OS v2(被取代的定位) | 本 ADR(新定位) |
|---|---|
| 大内核:intent/task 生命周期、4 个 Registry、Capability Card、World Projection 为内核真相、Evolution Factory、自治阶梯 L0-L6、Teams、Cluster 都是一等内核职责 | 小内核:纯无状态 loop + 一个 `AgentApi` 句柄 + Pi `AgentHarness` 层;一切领域/重能力在内核**之上**作为可选层与插件 |
| "能做很多事" | "默认简单、按需变强";大众用户开箱即用,vibe coding 用户渐进解锁 |
| 4 个 Registry 分治 tools/providers/plugins/profiles | **一个句柄**统一注册(Pi 证明一个句柄足够;4-Registry 正是 Pi 提取报告点名"Rust 移植最易做错"的方向) |

北极星 ADR 的许多**子理念仍然优秀且与轻量不冲突**——human-boundary 安全边界、capability profile 的"任务可见能力子集"、observability/trace、isolation——这些**收进小内核或作为内核之上的薄层保留**,不随重内核定位一起被取代。

---

## 3. 产品画像与目标用户

- **日常大众用户(office / 非开发者)**:默认 UX 必须守**简单预算**——首次运行不被任何高级能力复杂化;重能力(Teams、诊断、Evolution 审阅等)一律在**渐进披露**之后,永不出现在默认路径。
- **vibe coding 用户**:底层要有纪律——可靠的编辑(借鉴 hermes:9 策略模糊匹配 + 读回/lint/LSP 三信号 + 影子 git checkpoint)、按需读取、可中断的长任务。
- **产品信条**(沿用北极星收尾句,作为本 ADR 的 UX 第一原则):**"表面简单,因为底层的内核有纪律。"**

两类用户通过 **capability preset**(§6.6)表达,而非内核分支。

---

## 4. 核心设计原则(Pi 轻量内核)

1. **内核领域无关、无状态**:loop 返回事件流,不内嵌 DB 写 / session 变更。coding / office / 自动化 / 浏览器都是**工具 + preset**,不是 loop 里的 if 分支。(uClaw 的 `run_agentic_loop` 已接近此形,继续守住。)
2. **一个句柄,不是四个 Registry**:`AgentApi` 提供 `register_tool / register_provider / register_command / register_renderer / on(event, handler)`。hooks 是 `Option<Box<dyn Fn>>` 闭包,**不**建 30 方法的 `trait Plugin`。
3. **依赖只向下**:provider/LLM 调用隔离在一个可换函数后(Pi 的 `streamSimple` 形)。
4. **可插拔优先于可配置**:能力靠插件与 preset 横向加,不靠内核分支纵向堆。
5. **Pi `AgentHarness` 是内核的会话层**:纯 loop ← 有状态 `Agent`(prompt/abort/subscribe)← `AgentHarness`(session + compaction + skills + hooks)。
6. **重能力在内核之上**:Teams / World Projection / Evolution Factory 都是消费内核事件流的上层,**不进 loop**。

---

## 5. 架构分层

```
┌─────────────────────────────────────────────────────────────┐
│  可选上层 (Optional Layers — 消费内核事件流,不进 loop)        │
│   • Teams (sub-agent/worker 协调层)                          │
│   • World Projection (事件流之上的只读 UI 投影)               │
│   • Evolution Factory (trace+eval 的事后门控管线)             │
├─────────────────────────────────────────────────────────────┤
│  capability presets (数据/插件):office/大众 · vibe coding     │
├─────────────────────────────────────────────────────────────┤
│  AgentHarness 层 (Pi):session + compaction + skills + hooks   │
├─────────────────────────────────────────────────────────────┤
│  有状态 Agent:prompt / abort / subscribe / state             │
├─────────────────────────────────────────────────────────────┤
│  纯无状态 loop:领域无关,返回事件流                            │
├─────────────────────────────────────────────────────────────┤
│  一个 AgentApi 句柄  ←  tools / providers / commands /         │
│                          renderers / hooks 统一注册            │
├─────────────────────────────────────────────────────────────┤
│  插件加载:子进程/RPC(第三方代码) · 编译期(首方) · 文件(声明式) │
├─────────────────────────────────────────────────────────────┤
│  记忆:单一 MemoryAdapter 缝  →  [openhuman 现代化理念,专项定]  │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. 七项决策(grill-me,2026-05-28)

### 6.1 文档容器 → 新 ADR 取代北极星
本文件即该新 ADR。北极星 ADR 加 `Superseded by` 头,正文保留作历史。CLAUDE.md / BEHAVIOR.md 等入口指针更新指向本 ADR。沿用 `gbrain-primary-freeze` ADR 的取代先例。

### 6.2 重支柱处置 → 保留三,降一
- **保留**(作为内核之上的可选层,§6.3):Teams、World Projection、Evolution Factory。
- **降为远期 / 移出**:Cluster(分布式 worker 调度)——最重、最不服务大众用户,移出当前路线图,文档保留作历史。

### 6.3 三者与内核的关系 → 内核之上的可选层
- **Teams** = 用 sub-agent / worker 与工具面搭的协调层,不往 loop 加分支。
- **World Projection** = 跳在事件流之上的**只读 UI 投影**(read-model),**不是内核真相源**。
- **Evolution Factory** = 消费 trace + eval 的**事后门控管线**,提案需 gate,不直接 mutate 生产行为。

### 6.4 harness 语义 → 采用 Pi AgentHarness,重命名 eval,清三义撞名
"harness" 在 uClaw 语境曾同时指三样东西,本 ADR 钉死为 ①:
- ① **Pi `AgentHarness`**(session+compaction+skills+hooks over 纯 loop)→ **采纳为内核会话层**。
- ② uClaw 现有 `src-tauri/src/harness/`(实为离线 **eval 跑机**)→ **重命名为 `eval/`**。
- ③ "autonomy 监督"(长任务运行时监督)→ 内核之上的**薄层**,不再叫 harness。

### 6.5 插件模型 → 一个句柄 + 子进程/RPC 加载
- 4 个 Registry **塌缩为一个 `AgentApi` 句柄**作为所有插件统一注册点。
- **第三方代码插件(tools/providers)**:走**子进程 / RPC**(把现有 MCP 模式泛化为统一插件协议),跨语言、与 MCP 同构、风险最低。
- **首方内置**:编译期注册。
- **声明式插件**(skills=markdown、automation spec=YAML、prompt 模板):保持运行时文件加载。

### 6.6 多领域 → capability preset
"office / 大众" 与 "vibe coding" 形式化为两个 **capability preset**(provider + model + tools + skills + 指令 的捆绑包),作为数据 / 插件加载、运行时可切;内核保持领域无关。默认 UX 守简单预算,重能力渐进披露(§3)。

### 6.7 记忆现代化 → 方向定,详细架构开专项
- **方向**:用 openhuman 的现代化理念(写时准入打分 + bucket-seal 级联巩固树 + hotness/recency 衰减 + 粗到细检索原语,纯 Rust + SQLite + FTS5,与 uClaw 同栈),收敛到**单一 `MemoryAdapter` 缝**,退役"8 存储内联拼装"。
- **gbrain ↔ openhuman 的详细取舍**(取代主记忆 / 分工共存 / 仅借理念)**后期开专项**单独定,需与 `gbrain-primary-freeze` ADR 协调。本 ADR 不预决。

---

## 7. 处置总表

| 模块 / 概念 | 处置 | 去向 |
|---|---|---|
| 纯无状态 loop | **保留并守护** | 内核 |
| 双队列 / 迭代压缩+split-turn / FileOps | **保留**(已落地为真) | 内核(补完 §见审计 1.1) |
| 4 个 Registry | **塌缩** | 一个 `AgentApi` 句柄 |
| `plugin_manifest`(死骨架) | **重建** | 接入 `AgentApi` + 子进程/RPC |
| `skill_selection::select_top_k`(死) / 多余 skill 渲染器(死) | **删** | — |
| `memory_contract` / `memory_policy`(死) | **重建为 MemoryAdapter** | 记忆专项 |
| `harness/`(eval 跑机) | **重命名** | `eval/` |
| Pi `AgentHarness` | **采纳** | 内核会话层 |
| 三套 safety 模型 | **合一** | 单一 `SafetyManager` chokepoint(审计 1.8 CRITICAL) |
| `CancellationToken` 未接 flight point | **补完** | 内核(审计 1.1 CRITICAL) |
| Teams / World Projection / Evolution Factory | **保留** | 内核之上可选层 |
| Cluster | **降为远期/移出** | 历史文档 |
| 自治阶梯 L0-L6 / human-boundary / capability profile | **收进内核/薄层** | 保留好的部分 |
| 8 个记忆存储 | **收敛** | 单一 MemoryAdapter(专项) |

---

## 8. 非目标(Non-Goals)

- **不**把内核做成 Agent OS 大内核;不在 loop 里加领域分支。
- **不**做分布式 Cluster(本阶段)。
- **不**建 4 个并行 Registry;不建 30 方法的 `trait Plugin`。
- **不**让任何能力复杂化大众用户的首次运行。
- **不**保留"看起来完整、热路径从未受益"的死骨架——要么接线,要么删。
- **不**在本 ADR 预决 gbrain 的去留(留给记忆专项)。
- **不**做语言迁移——保持 Rust + Tauri,借 Pi/hermes/openhuman 的**设计**。

---

## 9. 实施路径

实施序列遵循审计 `2026-05-27-pi-convergence-gap-audit.md` §4 的 5 阶段:

1. **阶段 1(安全/正确性,先)**:合一 safety chokepoint + 接 `CancellationToken` 到 flight point。
2. **阶段 2(清骨架)**:删死代码 / 接线或删 `HarnessRuntime`·`TaskScheduler`·`workers` / 修真实 freeze。
3. **阶段 3(Pi 插件)**:塌缩为一个 `AgentApi` 句柄 + 子进程/RPC 加载 + 拆 `dispatcher.rs` + 统一 prompt 单缝;eval 重命名。
4. **阶段 4(记忆专项)**:openhuman 理念 + 单一 `MemoryAdapter`(独立 grill + plan)。
5. **阶段 5(coding 可靠性)**:hermes 9 策略模糊匹配 + 三信号 + 影子 git checkpoint + 按需读取。

每阶段落地前经 `superpowers:writing-plans`;遵守北极星 ADR §18 的 spec 设计规则(仍适用)。

---

## 10. 未决 / 待专项

- **记忆架构专项**:gbrain ↔ openhuman 取舍、`MemoryAdapter` 接口形、迁移路径(§6.7)。
- **Teams / World Projection / Evolution Factory 的上层接口**:三者作为可选层各自的事件流消费契约,待各自 spec。
- **capability preset 的 schema 与切换 UX**:待 §6.6 落地 spec。
- 子进程/RPC 统一插件协议与 MCP 的关系(泛化 or 并存),待阶段 3 spec。

---

## 核心判断

uClaw 的 agent loop 内部质量不错(Pi 收敛多数为真),真正的债在"统一缝全是骨架、子系统各自平行实现"。本 ADR 把产品方向正式锁定为 **Pi 轻量可插拔**,使后续投入收敛到同一方向——先还安全/骨架债,再依次借鉴 Pi(插件)、openhuman(记忆)、hermes(coding)。
