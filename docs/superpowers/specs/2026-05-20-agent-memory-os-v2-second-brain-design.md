# Agent Memory OS v2 — 第二大脑（North-Star 总览 spec）

**Date:** 2026-05-20
**Status:** Approved (program-level umbrella; each sub-project gets its own spec)
**Supersedes (partial):** [ADR 2026-05-20 — gbrain primary, freeze L2 Cognitive](../adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md) §"L1 Foundation maintenance mode" — see §5 below.

---

## 0. 这份文档是什么

这是 Agent Memory OS v2 的**北极星总览**。它锁定 3 个横切全局的地基决策、定义双层架构、把工作分解成 5 个独立子项目（A–E）并排序。**它本身不是详细实现 spec** —— 每个子项目会各自走 `brainstorming → writing-plans → 实现` 的循环，产出自己的 `docs/superpowers/specs/*.md`。

目标（用户原话提炼）：
0. 补齐 L2/L3 暂停后留下的悬空前端（桌面 APP 要给最直观的 UI/UX）
1. 充分利用 gbrain，提升其在架构中的价值与利用率
2. 构建 AI Agent 的第二大脑：跨会话、跨主题的持久经验积累
3. 构建用户的第二大脑：可浏览/可编辑/可版本控制；完善万花筒的记忆星云 + 知识星云
4. 建立自我学习真系统：agent 自学习/自沉淀/自成长
5. 用户"喂"文档/音视频等资料 → gbrain 知识库扩展 + 静默吸收增强

---

## 1. 地基决策（已与用户确认）

### 决策 1 — 双层共生架构（模型 2）

uClaw 的长期记忆分为两个**互补、都活跃**的层，中间用引用桥连接：

| 层 | 存储 | 内容 | 来源 |
|---|---|---|---|
| 🧠 **知识层** | gbrain（外部 MCP · PGLite） | 实体/概念/对比/问题/综合/决策/空白 等"可编译综述页"——slug + compiled-truth + timeline + wikilink 图 | agent `put_page` + 用户喂的资料（子项目 B） |
| 🌌 **认知层** | memory_nodes（uClaw SQLite） | boot/identity/value/directive（agent 自我锚点）· episode（会话片段）· facet（openhuman 用户画像）· curated | QuickCapture · 学习管线 · proactive 场景 |

**引用桥**：两层互相 link（不复制内容）。例如一个 gbrain 知识页可以引用 memory_nodes 里的某个 episode 作为来源；一个 memory_nodes 的 UserProfile facet 可以指向 gbrain 里关于该主题的知识页。桥的具体表示（metadata key / 关联表）在子项目 C/D 设计时定。

**分工原则**："知识"进 gbrain，"自我与经验"进 memory_nodes。判断准则：这条信息是"关于世界的可综述知识"（→ gbrain）还是"关于 agent 自己/这次会话/这个用户的状态"（→ memory_nodes）。

### 决策 2 — gbrain 前端访问走 MCP 协议代理

前端不直接读 gbrain 的 PGLite（WASM/JS，Rust 读不了 + 强耦合内部 schema）。改为：新增 Tauri 命令（`gbrain_list_pages` / `gbrain_get_page` / `gbrain_search` 等），内部调用已有的持久 MCP 连接 `mcp_manager.call_tool("gbrain", ...)`。若 gbrain 现有 MCP 工具不足以支撑富浏览（如取整页 markdown / 反向链接 / 分页），则**扩展 gbrain 源码**（已 vendored 在 `src-tauri/gbrain-source/`，通过 `setup-gbrain-source.sh` 管理）。

### 决策 3 — 静默后台摄入 + 事后可查

用户喂资料（子项目 B）：拖放 → 后台静默解析（复用 `src-tauri/src/stt/` 处理音视频）→ 分块抽实体 → agent 静默写入 gbrain 页。只出一个轻提示。**事后**用户能在知识星云（子项目 C）看到新页、编辑、回滚。不强制逐页审核（除非未来加"重要资料才审核"的智能门控，列为 B 的可选增强）。

---

## 2. 程序级架构（数据流）

```
            👤 用户                      🤖 Agent
              │                            │
   ┌──────────┴────────────┐              │
   │ 【B】知识摄入管线        │              │
   │ 拖放 PDF/md/URL/音视频  │              │
   │ → 解析(STT) → 分块抽实体 │              │
   └──────────┬────────────┘              │
              ▼ (静默写)                    ▼
   ┌────────────────────┐   桥   ┌────────────────────┐
   │ 🧠 gbrain 知识层     │ ◀──▶ │ 🌌 memory_nodes 认知层│
   │ 综述页 + wikilink 图 │       │ 自我 + episode + facet│
   └─────────┬──────────┘       └──────────┬─────────┘
             │                              │
             │   【D】自学习闭环（夜间巩固）   │
             │   Decay·Drift·SpacedRep·Triangulation
             │   衰减低价值/复习核心/检测漂移/多源加固/跨层沉淀
             │                              │
   ┌─────────┴──────────────────────────────┴─────────┐
   │  Tauri 命令层                                      │
   │  【A】gbrain_* (MCP 代理)  +  已有 memory_graph_*    │
   └─────────┬──────────────────────────────┬─────────┘
             ▼                              ▼
   ┌────────────────────┐       ┌────────────────────┐
   │ 【A】gbrain 知识浏览器 │       │ 【C】万花筒 · 双星云  │
   │ 【E】WikiView/Health  │       │ 记忆星云 + 知识星云   │
   │ 页面/搜索/编辑/反链   │       │ 浏览·编辑·版本控制    │
   └────────────────────┘       └────────────────────┘
```

---

## 3. 五个子项目

| 子项目 | 名称 | 目标 | 依赖 | 关键产物 |
|---|---|---|---|---|
| **A** | gbrain 知识浏览器（前端通路） | 1, 3 | — | 新 `gbrain_*` Tauri 命令（MCP 代理）+ 前端页面浏览/搜索/编辑/反向链接组件 |
| **E** | 复活 L2/L3 悬空前端 | 0 | A | WikiView / MemoryHealthPanel 重新指向 gbrain 数据源 + 本会话新算法（drift/health）；最小代价，不重做 UI |
| **C** | 万花筒双星云 | 2, 3 | A, E | 记忆星云(memory_nodes 3D) + 知识星云(gbrain pages) 并排/融合；可编辑；版本控制视图 |
| **B** | 知识摄入管线 | 5 | A | 拖放摄入 UI + 后台解析（PDF/md/URL/音视频经 STT）+ 分块抽实体 + agent 静默 put_page |
| **D** | 自我学习闭环 | 4 | A, B | 把 Decay/Drift/SpacedRep/Triangulation 串成夜间巩固 cycle + 跨两层经验沉淀 + 自评 |

**实现顺序：A → E → C → B → D**。理由：A 是解锁器（用户首次"看见"主知识层）；E 复用 A 的通路最小补齐；C 在 A/E 之上做双星云；B 让知识层长大；D 把算法串成闭环让两层一起成长。

每个子项目独立 spec→plan→实现，可分别评审与回滚。

---

## 4. 已有基础（本设计充分复用，不重做）

- **gbrain 接入**（Sprint 2.0–2.4）：MCP server、PGLite、put_page/query/search、agent system-prompt 指令、chat 自动提取器、embedding endpoint、诊断 UI
- **memory_nodes 体系**（Foundation Phase 1–7）：9 种节点、auto-link、recall（图传播 + FTS trigram）、wiki_synth、brain_io markdown 同步
- **前端可视化**：MemoryNebulaView（Three.js 3D 星系）、MemoryGraphView（Canvas 2D 力导向）、WikiView（已建，待接数据）、MemoryHealthPanel（已建，待接数据）
- **L3 RETAINED 算法**（本会话 PR #271–#284）：Importance Decay（已激活）、Drift Detection（已激活）、Spaced Repetition（待接调度+LLM）、Triangulation（待接调度+LLM）、Timeline Engine（write API + SQL 聚合 + 时间分类器）
- **STT**（`src-tauri/src/stt/`）：音视频转文本，子项目 B 复用

---

## 5. 与既有 ADR 的关系

ADR 2026-05-20 当时定"gbrain 主、memory_nodes 进 maintenance mode（不再扩展应用层）"。本 v2 设计**部分修订**该决定：

- **保留**：gbrain 是知识层主源；L2 Cognitive（段落级 provenance/两步 compile/review queue）保持 paused —— 那些确实与 gbrain 重叠。
- **修订**：memory_nodes 不再是"冻结归档"，而是**重新激活为认知状态层**，有明确的、与 gbrain 不重叠的分工（自我锚点 + 会话片段 + 用户画像）。它的前端（3D 星云）继续作为"记忆星云"演进。
- **保留**：L3 的 Entity Graph Engine + Dream Cycle 1-6 阶段仍 paused（gbrain 覆盖）；但 RETAINED 的 4 项增强 + Timeline Engine 在子项目 D/C 中被接入闭环与可视化。

子项目落地时，会在各自 spec 里引用本节，必要时补一条新 ADR 记录"memory_nodes 重新激活"的正式决议。

---

## 6. 风险与开放问题（留给各子项目 spec 解决）

- **A**：gbrain 现有 MCP 工具是否够富（取整页 markdown / 反向链接 / 分页）？不够则需扩展 gbrain 源码 —— A 的 spec 要先探明 gbrain 工具表。
- **A/C**：gbrain 页面的"编辑"如何回写？经 `put_page` 覆盖，还是 gbrain 有 update 语义？版本控制依赖 gbrain 的 timeline 还是 uClaw 侧另存？
- **B**：音视频经 STT 后的分块/抽实体策略；大文件的后台任务调度 + 进度（即使静默也要能查状态）；token 预算。
- **C**：双星云是并排两个画布，还是一个融合画布（节点按层着色）？编辑 gbrain 页 vs 编辑 memory_node 的统一交互。
- **D**：夜间巩固 cycle 的触发时机（定时 / 空闲检测 / 手动）；跨两层"经验沉淀"的具体规则；LLM 预算。
- **桥**：两层互相引用的具体数据表示 + 在哪个子项目正式引入（倾向 C 或 D）。
