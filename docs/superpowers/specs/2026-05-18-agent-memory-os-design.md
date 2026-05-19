# Agent Memory OS — Foundation Layer 设计(实体级长期记忆 + Auto-Link + AI Wiki)

> **🔧 STATUS: MAINTENANCE MODE (2026-05-20)** — 见 [ADR 2026-05-20 — gbrain primary, freeze L2 Cognitive](../../adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md)。
>
> Phase 1–7 已经全部 ship 到 main(2026-04 ~ 2026-05-18)。后续工作:**gbrain (Sprint 2.0–2.4) 是 uClaw 的主长期知识层**;本 spec 描述的 EntityPage 路径进入 maintenance mode,只维护已有数据。新知识不再通过此路径写入,改走 `mcp__gbrain__put_page`。

**Date:** 2026-05-18
**Status:** Maintenance mode (was: Draft) — see ADR 2026-05-20
**Layer position:** **L1 Foundation Layer (Phase 1-7)** ——本 spec 是**三层** Memory OS 设计的第一层。
- **L1 Foundation(本文)**:实体级长期记忆 + Auto-link + AI Wiki view
- **L2 Cognitive**:[`2026-05-18-agent-memory-os-cognitive-design.md`](2026-05-18-agent-memory-os-cognitive-design.md) + [`agent-memory-os-cognitive.md`](../plans/agent-memory-os-cognitive.md) —— 段落级 provenance + 9 page type + 两步 compile + Adaptive RAG
- **L3 Engines**:[`2026-05-18-agent-memory-os-engines-design.md`](2026-05-18-agent-memory-os-engines-design.md) + [`agent-memory-os-engines.md`](../plans/agent-memory-os-engines.md) —— Entity Graph(NER) + Timeline Engine + Dream Cycle 8 阶段 + 7 项高级增强

**先完成 L1,再做 L2,最后 L3。**
**Inspired by:** [garrytan/gbrain](https://github.com/garrytan/gbrain)(BrainEngine + put_page auto-link + 双层 page)、[SamurAIGPT/llm-wiki-agent](https://github.com/SamurAIGPT/llm-wiki-agent)(LLM-as-wiki + health/lint 分层)、Karpathy "LLM Wiki" 论述
**Builds on:** `src-tauri/src/memory_graph/{store,models,recall,reflection,auto_classify}.rs`、`memory.rs`、`memu/{bridge,client}.rs`、`proactive/service.rs`、`memorization/service.rs`
**Companion plan:** `docs/superpowers/plans/agent-memory-os.md`(Phase 1-7)

---

## 1. Background and Problem Statement

### 1.1 uClaw 当前 memory 架构总览

uClaw 已经具备一个**分层多模式 memory 框架**(详见调研笔记 §1):

| 层 | 模块 | 主表 | 角色 |
|---|---|---|---|
| L0 KV | `memory.rs` | `memories` | 会话级临时缓存(偏好、配置、context) |
| L1 Graph | `memory_graph/store.rs` | `memory_nodes` / `memory_versions` / `memory_edges` / `memory_routes` / `memory_keywords` / `memory_fts`(trigram) | 长期结构化知识 |
| L2 LLM Bridge | `memu/{bridge,client}.rs` | memU 自管 `~/.uclaw/memory/memu.db` | 向量召回 + LLM 分类 |
| L3 Proactive | `proactive/service.rs` + 4 个 scenarios | 写回 L1 + `fragment_reviews` 等辅助表 | 24/7 主动学习(skill / failure / multimodal / conversation) |
| L3' Memorization | `memorization/service.rs` | 写回 L1 | 响应式记忆抽取(与 Proactive 并行,有重叠) |

数据模型(`memory_graph/models.rs`):
- **9 种节点类型**:`Boot / Identity / Value / UserProfile / Directive / Curated / Episode / Procedure / Reference`
- **4 种边类型**:`Contains / RelatesTo / Timeline / Trigger`
- **3 种可见性**:`Private / Session / Shared`
- 版本链:`memory_versions.supersedes_version_id`
- FTS5 trigram tokenizer 对 CJK 友好:`memory_fts` 在 `migrations.rs::V31_MEMORY_FTS_TRIGRAM`(L1265-1286)重建(早期用 unicode61,V31 改成 trigram + 从 memory_nodes ∪ active memory_versions 回填);`messages_fts` 用同样 pattern 在 V11 重建
- 图传播:`store.rs::graph_propagation_search` BFS + 衰减(`decay_factor = 0.6`)

**迁移注册表当前状态(以代码为准,CLAUDE.md 的表格已落后):** `migrations.rs` 已使用到 V33(V33 = Symphony runtime,L1356–L1690)。CLAUDE.md 的 Active migration registry 只列到 V26,**V27–V33 实际全部已合并**(V27=自定义 system prompts、V28=prompt version 历史、V29=逻辑压缩、V30=fragment_reviews+daily_summaries、V31=memory_fts trigram 重建、V32=IM channel infra、V33=Symphony)。本设计的下一个可用号是 **V34**。Plan 文档已据此调整;同时建议在本 PR 顺手补全 CLAUDE.md 的 registry 表。

### 1.2 三个差距:为什么仅靠现有架构无法成为 "Memory OS"

调研笔记 §10 列出 11 项瓶颈,本 spec 把它们归并到三类核心差距:

**差距 A:节点是"碎片",不是"实体页"。**

现有 `memory_nodes` 表一条记录 = 一条事实碎片,即便是 `UserProfile` 节点,也只表达"某次更新捕到的一个事实"。要回答"关于张三这个人,我们当前知道的完整画像是什么",必须遍历所有提到他的 episode、抽取关键句、自己拼装——LLM 每次 query 都重做一遍这件事。

gbrain 把这件事**预先编译**成一个 page:每个 entity 一份 markdown,上半页是 "compiled truth"(可重写的当前最佳理解),下半页是 timeline(append-only 证据流)。query 时直接读这页。

uClaw 现状:`memory_versions` 是有 supersedes 链,但**链的语义是"覆盖"而非"演变"**——旧版本被标 deprecated 后基本不再被访问。没有"per-entity 长期视图"这个一等公民概念。

**差距 B:写图谱靠"显式 tool call",不靠"写时副作用"。**

`memory_graph_create_edge` 是 Agent 必须主动调用的 tauri command。LLM 写完一段 episode 文本后,要不要建边、建什么类型、连到谁,**全靠 LLM 自己记得调用 tool**——经常忘、经常拼错。

gbrain 的做法是 **put_page 自带 auto-link post-hook**(`src/core/link-extraction.ts`):每次写 page 都跑 regex 抽 `[Name](dir/slug)` 和 `[[dir/slug|Name]]` 两种引用,启发式推断 7 种边类型(`attended / works_at / invested_in / founded / advises / source / mentions`),零 LLM 调用、stale-link reconciliation 一并完成。

llm-wiki 类似:每次 ingest 都强制 LLM 在 source 页里写 `## Connections` 段,wikilink 由后续的 `build_graph.py` 静默成图。

uClaw 现状:写边和写节点是两个独立动作,**写边的召唤路径非常长**——一边写完正文要再调一个 tool,LLM 经常省略,导致 `memory_edges` 表稀疏。

**差距 C:没有"维护循环",没有"成本预算意识"。**

现有 4 个 proactive scenarios 都在做**抽取**(skill / failure / multimodal / conversation→memory),没有任何 scenario 在做**整理**——孤儿节点扫描、stub 节点告警、index 与 FTS 不同步检测、空闲版本清理、矛盾事实标注。

更进一步,所有 scenarios 都用同等 LLM 预算运行,**没有 cheap-first 路由**。gbrain 的 dream cycle 6 阶段(lint / backlinks / sync / extract / embed / orphans)和 llm-wiki 的 **health(0 LLM)/ lint(LLM)** 二分,都把"维护"做成一等公民,并且**先跑免费检查、再跑收费检查**。

uClaw 现状:`db/migrations.rs::V12` 之后,FTS 回填和 trigram 索引是冷启动一次性跑;运行时没有任何定期 reconciliation。

### 1.3 目标:Runtime-Native Agent Memory OS

把 uClaw 的 memory 子系统提升为一个具备以下性质的"操作系统":

1. **双脑**:既是用户的第二大脑(用户能 grep、编辑、版本回滚自己的记忆),也是 Agent 的持续人生记忆(Agent 跨会话、跨主题持续 accumulate 经验)。
2. **AI Wiki**:在现有图谱之上,额外维护一份 **derived-but-canonical** 的人类可读 wiki(per-entity page + 全局 overview + index),作为"成 KB 视角"而非"成图视角"的查询面。
3. **维护内生**:周期性自整理 + 矛盾标注 + 空白填充 + 成本预算分层,作为 proactive 的第 5 个 scenario,而非用户必须显式触发。
4. **不破坏现状**:V1–V26 迁移、`models.rs` 已有 enum、所有 `tauri_commands.rs` IPC、`ui/src/components/memory/` 组件**全部保留向后兼容**。新能力以**新增**(新 enum 变体、新表、新 hook、新 scenario)的方式叠加,不替换。

### 1.4 OKRs

| Objective | Key Results |
|---|---|
| **O1: 引入 EntityPage 抽象——每实体一份双层视图** | KR1: 新增 `MemoryNodeKind::EntityPage` 变体(`models.rs` 第 10 个 kind,纯新增不替换) |
| | KR2: `memory_nodes.metadata_json` 约定双 schema 段 `compiled_truth` + `timeline[]`,作为 EntityPage 的 canonical content |
| | KR3: 新表 `entity_page_links`(自动/显式两种来源)+ V34 migration |
| **O2: 写时自动建图——put_page 风格的 zero-LLM post-hook** | KR4: `store.rs::create_version` 后插入 `auto_link_extraction` 钩子,regex + 启发式抽 typed-edge |
| | KR5: `MemoryRelationKind` enum 新增 `WorksAt / Founded / InvestedIn / Advises / Attended / Source / Mentions` 七个变体(向后兼容,现有 `Contains/RelatesTo/Timeline/Trigger` 不动) |
| | KR6: 写时 stale-link reconciliation(删掉文本里已经不存在的引用对应的 edge) |
| **O3: AI Wiki 派生层** | KR7: 新增 `wiki_artifacts` 表存 `overview.md` + `index.md` 的当前快照(LLM 周期性重写) |
| | KR8: 新增 `WikiOverviewScenario`(proactive 第 5 个 scenario,every-N-ingests 触发) |
| | KR9: 新增 `memory_wiki_*` tauri commands(read/regenerate)+ 前端 `WikiView.tsx` tab |
| **O4: 维护循环——Health vs Lint 分层** | KR10: 新增 `MemoryHealthScenario`(0 LLM,每 session 跑,检查 orphan / dangling FTS / stub node / index drift) |
| | KR11: 新增 `MemoryLintScenario`(LLM,every 10-15 次写入跑,检查 contradiction / stale summary / hub stub / phantom hub) |
| | KR12: 检查结果落 `memory_health_findings` 表,前端 `MemoryHealthPanel.tsx` 显示 |
| **O5: 用户的第二大脑——可选的 markdown 双向同步** | KR13: 新增 `gbrain_sync` 风格的 export-to-markdown 命令,落 `~/Documents/workground/brain/`(opt-in) |
| | KR14: 用户编辑 markdown 文件后,`memory_wiki_sync` 检测改动并增量回灌到 `memory_nodes`(以文件为 source of truth,延续用户优先原则) |

---

## 2. 三方架构对比

### 2.1 数据模型对比

| 维度 | uClaw `memory_graph` | gbrain | llm-wiki-agent |
|---|---|---|---|
| **最小单元** | `memory_node`(SQLite row, UUID) | `page`(markdown 文件, slug=filename) | `page`(markdown 文件, slug=filename) |
| **节点种类** | 9 种 enum(`Boot / Identity / Value / UserProfile / Directive / Curated / Episode / Procedure / Reference`)(`models.rs:5-49`) | 17+ MECE 目录(people / companies / deals / meetings / projects / ideas / concepts / writing / programs / org / civic / media / personal / household / hiring / sources / prompts / inbox / archive),由 RESOLVER.md 决策树归位 | 4 种 frontmatter type(`source / entity / concept / synthesis`) |
| **节点 schema** | `memory_nodes` 表 + `metadata_json` JSON | frontmatter(`role/company/aliases/tags`) + markdown body 双层(compiled truth + timeline) | frontmatter(`title/type/tags/sources/last_updated`) + markdown body 5 段(`Summary/Key Claims/Key Quotes/Connections/Contradictions`) |
| **版本机制** | `memory_versions.supersedes_version_id` 链 + status (Active/Deprecated/Orphaned) | `page_version` 表自动版本化 + git 跟踪 markdown 文件 | 完全靠 git,无应用层版本 |
| **边表达** | `memory_edges` 表(parent_node_id, child_node_id, relation_kind, priority) | `link` 表 + 7 种 link_type enum + 同对 entity 可有多 type(works_at AND advises) | 隐式 `[[wikilink]]` 内嵌 markdown body + 离线 `build_graph.py` 解析为 EXTRACTED / INFERRED / AMBIGUOUS edge |
| **边类型** | 4 种 enum(`Contains / RelatesTo / Timeline / Trigger`)——通用,语义稀薄 | 7 种 typed(`attended / works_at / invested_in / founded / advises / source / mentions`)——领域感强,VC/创业语境 | 2 大类:EXTRACTED(确定性,从 wikilink 解析)/ INFERRED(LLM 推断,带 confidence) |
| **可见性** | 3 种 enum(`Private / Session / Shared`) | 无可见性概念(整个 brain 是单一 namespace,subagent 用 `wiki/agents/<id>/` 路径前缀隔离) | 无可见性概念(用户拥有整个 wiki) |
| **Slug / Identity** | UUID,与显示标题无关 | slug = filename = primary key(`first-last.md`、`company-name.md`),冲突加限定符 | slug = filename,kebab-case for source / TitleCase for entity, concept |
| **别名处理** | 无显式 aliases 字段 | `aliases: [..]` frontmatter 字段 + `.raw/` sidecar 存外部 source 的 handle 映射 | 无 |

**关键差距**(对应 §1.2 差距 A):uClaw 节点 = 碎片粒度,gbrain/llm-wiki 节点 = 实体粒度。一个 gbrain page ≈ uClaw 一个"人"的所有 episode + 一次 LLM 综合的预计算结果。

### 2.2 检索算法对比

| 维度 | uClaw | gbrain | llm-wiki |
|---|---|---|---|
| **检索时机** | 显式 tauri command(`memory_graph_search` / `recall_for_agent`)+ Boot 自动注入 | always-on `brain-ops` skill,人物/公司/决策类问题必须 brain-first(`skills/conventions/brain-first.md` 5 步) | `/wiki-query`(显式 slash command 触发) |
| **关键词** | FTS5 trigram tokenizer,LIKE-style 回退,`memory_fts` virtual table(`migrations.rs::V12`) | Postgres tsvector + GIN + `websearch_to_tsquery` + `ts_rank` | 不存在(LLM 直接读 `index.md` 选页面) |
| **向量** | `memory_versions.embedding_json` 可选,`memU` 提供 fastembed/cloud 向量;`memU` 不在线时降级 | pgvector HNSW cosine + OpenAI text-embedding-3-large + chunks 表分块 | 不存在 |
| **图遍历** | `graph_propagation_search` BFS + decay 0.6 + relation_weight 表(`store.rs:636-735`) | 递归 CTE + 边类型过滤 + 深度上限 10 + visited[] 防环 + `gbrain graph-query <slug> --type works_at --depth 2 --direction in/out/both` | 离线 `build_graph.py` 跑 Louvain community detection,community 关系不参与 query |
| **融合策略** | 召回侧靠 `recall.rs` 用 token_budget 5000 做 BFS+keyword 拼接,无显式 RRF | **RRF**(reciprocal rank fusion,`score = Σ 1/(60+rank)`)+ cosine re-score + **compiled-truth boost** + **backlink boost** + 4 层 source-aware dedup | 无算法,LLM 看 `index.md` 自由选页面 |
| **意图分类** | 无显式 intent classifier | Haiku 路由:entity / temporal / event / general 自动选 detail level | 无 |
| **预算管理** | 固定 `token_budget = 5000`(`recall.rs:82-90`),不透明截断 | dedup + intent + Top-K 联合控制 | LLM 自适应 |
| **召回评估** | 无内置 benchmark | `gbrain eval --qrels queries.json` 跑 P@k / R@k / MRR / nDCG@k,BrainBench 数据 P@5=49.1% R@5=97.9%,比 ripgrep-BM25 高 +31.4pt | 无 |

**关键差距**(对应 §1.2 差距 C 的成本部分):gbrain 用 RRF + boost 把多路召回融合得很优雅;uClaw 的 `recall.rs` 是"先 BFS、再 keyword、塞满 token budget",**boost 信号没用上**(节点的 cited_count、usage_count、updated_at 都是已有数据,可以接入)。

### 2.3 写入流程对比

| 阶段 | uClaw | gbrain | llm-wiki |
|---|---|---|---|
| **触发** | Agent 主动调 `memorize` tool 或 ProactiveService 后台抓取 | always-on `signal-detector` skill 每条消息并行 spawn 便宜模型抽 idea/entity;ingestion skills 各自从源拉数据 | 用户显式 `/wiki-ingest raw/xxx.md` |
| **去重** | `find_learned_skill_by_normalized_title`(title 小写+trim+空格折叠)on Procedure 节点 | dedup protocol(exact 搜 + fuzzy 搜 + alias 跨页 grep + .raw/ 邮箱/handle 匹配)→ 命中 UPDATE,未命中 CREATE;`aliases:` frontmatter 维护别名;周度 `maintain` skill 跑 deduplication scan | 完全靠 LLM 在 ingest step 2 读 `index.md` + step 6/7 read-modify-write existing entity/concept page |
| **建边** | 显式 `memory_graph_create_edge` tool call | put_page 自带 auto-link post-hook(`link-extraction.ts`):regex 抽 ref → `inferLinkType` 启发式(page role prior + 文本模式)→ 7 种 typed-edge → stale-reconciliation | ingest workflow step 6/7 强制 LLM 在 source 页 `## Connections` 段写 `[[PageName]]`;离线 `build_graph.py` 解析成 EXTRACTED edge |
| **矛盾** | 不处理(新版本覆盖旧版本) | enrichment 阶段给每条 claim 标 `observed/self-described/inferred` + confidence | ingest step 8 强制 LLM 在 source 页 `## Contradictions` 段写 `Contradicts [[OtherPage]] on: ...`,**flag-don't-resolve** |
| **LLM 角色** | Proactive scenarios 内 LLM 抽取 + memU 分类;reflection.rs 用 memU retrieve 做覆盖判断 | 主路径**零 LLM**(regex + 启发式);tier-escalating enrichment 才上 LLM(Tier 3 stub → Tier 2 web+social → Tier 1 full Crustdata+Exa);query 阶段 Haiku 跑 multi-query expansion | LLM 是主路径,几乎每一步都用(读源 → 写 source 页 → 重写 overview → 创建/更新 entity/concept) |

**关键差距**(对应 §1.2 差距 B):gbrain 的 zero-LLM auto-link 是**单点高 ROI 设计**——把 LLM 已经写出的引用文本(自然语言里 `[[张三]]` 或 markdown link)免费转成结构化边。uClaw 写 episode 时也产生大量带引用的自然语言,但没人把它们结构化。

### 2.4 长期记忆 vs 短期记忆 vs 维护循环

| 维度 | uClaw | gbrain | llm-wiki |
|---|---|---|---|
| **working memory** | session 滑动窗口(20 条,`proactive/service.rs:155-170`) | 无显式概念 | 无显式概念 |
| **episodic memory** | `Episode` 节点 + `agent_messages` + `agent_turns` 表 | timeline 段(append-only,每行 `YYYY-MM-DD \| source — 发生了什么`),嵌在 page 下半 | 隐式存在于 source 页 + log.md |
| **semantic memory** | `UserProfile / Curated / Reference / Procedure` 节点 | compiled-truth 段(page 上半,可重写) | entity / concept / synthesis 页面 |
| **遗忘机制** | Gaussian decay(`store.rs::compute_skill_score`,半衰期 30 天,`SKILL_DECAY_HALF_LIFE_DAYS`) | 无衰减算法;有 page lifecycle(Active/Dormant 6+ 月/Archived/Graduated);`maintain` skill 周度 flag dormant page | 无;靠用户/LLM 在 ingest 时改写 |
| **促进机制** | usage_count + cited_count 双计数,加权进 score | tier-escalating enrichment 等价 promotion(stub → full profile by 提及频次) | 无 |
| **维护任务** | 无运行时维护(只有冷启动 FTS 回填) | `gbrain dream` 6 阶段(lint → backlinks → sync → extract → embed → orphans),DB lock 表 + 文件锁防并发;`gbrain doctor` 健康检查 | `tools/health.py`(0 LLM,每 session)+ `tools/lint.py`(LLM,每 10-15 ingests);PR #40 显式表格化 health vs lint 边界 |
| **成本预算意识** | 无 | dream cycle 选择性跑哪几个阶段 + multi-query expansion 用最便宜 Haiku | health-first 原则:**"Run health first — linting an empty file wastes tokens"** |

---

## 3. 可借鉴的设计策略(分级)

按"对 uClaw 收益 / 集成成本"打分,分三级:

### 3.1 Tier A — 必须采纳(高 ROI,低集成成本)

**A1. Compiled-Truth + Timeline 双层 schema(来自 gbrain)。**

每个 EntityPage 节点的 `metadata_json` 约定两段:
- `compiled_truth: string`(markdown,可重写,LLM 周期性 regenerate)
- `timeline: TimelineEntry[]`(append-only,每条 `{date, source_node_id, source_session_id?, text}`)

效益:把"关于这个实体我们当前知道什么"从 query 时拼装变成预编译查询。对应差距 A。
成本:**零 schema 变化**——`metadata_json` 已经是 JSON 字段,只是约定两个 key。

**A2. Auto-Link Post-Hook(来自 gbrain `link-extraction.ts`)。**

在 `store.rs::create_version` 末尾插入钩子:
1. regex 扫文本里的 `[[entity:slug]]` 和 `[[node:uuid]]` 两种引用
2. `inferLinkType` 用节点 kind 配对查表(see §4.2.3)+ 文本模式(`work at` `founded by`)启发式判型
3. 在 `memory_edges` 写新边(`ON CONFLICT DO NOTHING`)
4. stale-reconciliation:本次新版本里没出现、但上一版本里出现过的引用,对应的 edge 删除

效益:对应差距 B。LLM 已经在文本里写引用,免费抽。
成本:新增 ~150 行 Rust + 7 个新 `MemoryRelationKind` enum 变体(纯 additive)+ `entity_page_links` view-style 表(可选)。

**A3. Health-Scenario(来自 llm-wiki + gbrain dream)。**

在 `proactive/scenarios/` 增加 `memory_health.rs`,**零 LLM 调用**,跑以下检查并把 finding 写到新表 `memory_health_findings`:

| 检查 | 触发条件 | 是否致命 |
|---|---|---|
| Orphan node | 无 incoming/outgoing edge 的非 Boot 节点 | warn |
| Stub node | `active_version.content` < 100 字符 | warn |
| Dangling FTS | `memory_fts` 行存在但对应 `memory_nodes` 已删 | error |
| Index drift | `memory_routes` 指向不存在的 node_id | error |
| Phantom slug | edge 的 `child_node_id` 不存在 | error |
| Empty version chain | node 存在但所有 version 都被 Deprecated | warn |
| Missing primary route | EntityPage 节点无 primary route | warn |

触发节奏:每次 `ProactiveService::tick_inner` 跑一次(30s 一次,可降到每 5 分钟),整轮检查应在 100ms 内完成(纯 SQL,无 IPC)。

效益:对应差距 C,极低成本。
成本:新 scenario + 新表(V34)+ 前端 `MemoryHealthPanel.tsx`(可选)。

**A4. Compiled-Truth Boost & Backlink Boost(来自 gbrain hybrid search)。**

修改 `recall.rs` 召回排序:
- compiled-truth boost:命中 EntityPage 节点的 `compiled_truth` 段比命中其它内容的得分 × 1.5
- backlink boost:节点被多少 edge 指向(`COUNT(*) FROM memory_edges WHERE child_node_id = ?`)进入分数,`log(1+backlinks)` 加权

效益:用现有字段提升召回质量。`cited_count`、edge 计数都是已有数据。
成本:~50 行 Rust 改动,无 schema 变化。

### 3.2 Tier B — 强烈建议采纳(中 ROI,中集成成本)

**B1. Wiki Overview & Index Scenario(来自 llm-wiki overview.md + index.md)。**

新 scenario `wiki_overview.rs`,触发条件:有 N 个新 EntityPage 节点 / N 次 EntityPage 更新(N=10 默认)。每次跑:
1. SELECT 最近活跃的 EntityPage(按 updated_at 取 50)
2. SELECT 最近 Episode(按 created_at 取 100)
3. LLM 用模板 prompt 重写 `overview.md`(全局"我们当前知道什么"的导读)
4. 生成 `index.md`(EntityPage 列表 + 分组)
5. 落库到新表 `wiki_artifacts(id, kind, content, generated_at, source_node_ids)`

效益:对应差距 A 的全局视图维度。
成本:`wiki_artifacts` 表已在 V34 一并建好,无新迁移;新 scenario + LLM 调用(可控,每 10 次 ingest 一次)。

**B2. Lint-Scenario(来自 llm-wiki + PR #27 graph-aware checks)。**

新 scenario `memory_lint.rs`,**有 LLM 调用**,触发节奏:**每 10-15 次 EntityPage 写入**才跑一次(成本控制)。检查项:

| 检查 | 是否要 LLM | 说明 |
|---|---|---|
| Hub stub | LLM | 节点度数 > μ+2σ 但 `compiled_truth` < 500 字符 → "highly referenced 但内容空,应该被 enrich" |
| Phantom hub | LLM | 在 ≥3 个 EntityPage 的 timeline 里被引用、但本身节点不存在的 slug → "应该创建的 page 候选" |
| Stale summary | LLM | `compiled_truth` 上次更新已超 7 天 + 这期间 timeline 新增 ≥5 条 → "应该 regenerate" |
| Contradictory facts | LLM | 同一 EntityPage 的不同 timeline entry 在用 LLM 判定矛盾 → 标到 `compiled_truth` 的 `## Contradictions` 段 |

效益:对应差距 C 的 LLM 维护维度。
成本:复用 V34 已建的 `memory_health_findings` 表(`is_lint=1` 区分)+ LLM 调用(成本上限可配)。

**B3. Tier-Escalating Enrichment(来自 gbrain)。**

EntityPage 节点 metadata 增加 `enrichment_tier: 1 | 2 | 3`,根据提及频次自动升级:
- Tier 3(stub):被提及 1-2 次,`compiled_truth` 只有一句话
- Tier 2(rich):提及 3-7 次,LLM 写 200-500 字 compiled_truth
- Tier 1(full):提及 ≥8 次,LLM 写完整 profile + 跑跨 source 综合

效益:把昂贵的 LLM regenerate 留给真正重要的 entity,冷门的省成本。
成本:metadata schema 约定 + lint scenario 集成。

### 3.3 Tier C — 可选采纳(高价值但有破坏性,放到 Phase 7+)

**C1. Markdown 双向同步(来自 gbrain `~/brain/` + `gbrain sync`)。**

新增 `memory_wiki_export` / `memory_wiki_sync` tauri command:
- export:把所有 EntityPage 导出为 `~/Documents/workground/brain/<kind>/<slug>.md`,frontmatter 携带 node UUID
- sync:扫描目录,detect 改动文件(mtime 比对 `memory_versions.created_at`),创建新 version,重跑 auto-link

效益:用户的第二大脑 — 可以用 Obsidian / VSCode / git 直接管理记忆;断网/离线照样能改。
风险:**source of truth 二义性**——如果 SQLite 和 markdown 同时被修改,合并语义需要明确规则(本 spec §6.5 给出)。
成本:新模块 + 新命令 + 用户教育成本。**Phase 7+ 才做。**

**C2. Pluggable Engine(来自 gbrain `BrainEngine` interface)。**

抽象 `MemoryGraphEngine trait`,SQLite 实现作为默认。未来可加 Postgres / Turso / SQLD 后端供云同步使用。

效益:为未来云端多端同步铺路。
风险:**侵入性高**,要重构所有 `store.rs` 调用点。
**本 spec 不做。**列入 Future Work。

**C3. 文件型 entity 路径(来自 llm-wiki / gbrain `wiki/entities/TitleCase.md`)。**

把 EntityPage 的 `routes` 表里加一个 primary `domain="brain", path="<kind>/<slug>"` 路由,作为 markdown 同步的目录寻址依据。

效益:让 C1 实现起来更干净。
**与 C1 绑定,放 Phase 7+。**

### 3.4 不采纳清单(对应 §1.3 "不破坏现状")

| 项 | 来源 | 不采纳原因 |
|---|---|---|
| 把整个 source of truth 搬到 markdown 文件系统 | gbrain + llm-wiki | uClaw 已有 SQLite + V1-V26 迁移基础;切换会破坏现有所有 tauri commands |
| 用 PGLite/Postgres 替换 SQLite | gbrain | 同上 |
| 完全交由 LLM read-modify-write reconciliation | llm-wiki | 已有 `find_learned_skill_by_normalized_title` 程序去重,LLM only 风险高(llm-wiki 自承认 620 节点规模就开始 61.6% orphan) |
| 用 Louvain community detection 改组节点 | llm-wiki `build_graph.py` | 算法成本高,价值未验证,可作为 future evaluation 项 |
| 取消 `MemoryNodeKind` enum,改用 free-form directory | gbrain MECE 目录 | 与现有数据强冲突,且 enum 给 Rust 编译期检查带来的安全收益不小 |

---

## 4. 集成设计

### 4.1 总览图

```
                       ┌────────────────────────────────────────────┐
                       │           Agent / User Interaction          │
                       └────────────────────────────────────────────┘
                                  │             │             │
              memorize/retrieve   │  ┌──────────┴───────────┐ │ wiki
                                  ▼  ▼                      ▼ ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       memory_graph (Rust, SQLite)                    │
│                                                                      │
│  ┌────────────┐    ┌────────────┐    ┌────────────┐  ┌────────────┐ │
│  │ MemoryNode │    │MemoryVersion│   │MemoryEdge  │  │MemoryRoute │ │
│  │ (9+1 kinds)│◄───┤  + content  │   │(4+7 kinds) │  │  +primary  │ │
│  └────────────┘    └─────┬──────┘    └─────▲──────┘  └────────────┘ │
│         ▲                │                  │                        │
│         │                │ create_version   │ auto_link_post_hook    │
│         │                │ ┌────────────────┴──┐                     │
│         │                └►│  AutoLinkExtractor │◄── NEW (Tier A2)   │
│         │                  └────────────────────┘                    │
│         │                                                            │
│  ┌──────┴──────────────────────────────────┐                         │
│  │   EntityPage(metadata_json schema):     │ ◄── NEW (Tier A1)       │
│  │   { compiled_truth, timeline[],         │                         │
│  │     enrichment_tier, last_synthesized } │                         │
│  └─────────────────────────────────────────┘                         │
└──────────┬───────────────────────────────────────────┬──────────────┘
           │                                            │
           ▼                                            ▼
┌────────────────────────────┐         ┌──────────────────────────────┐
│  recall.rs (hybrid search) │         │  ProactiveService scenarios  │
│  ─ FTS5 trigram            │         │  ─ skill_extraction (现有)    │
│  ─ memU vector (when up)   │         │  ─ conversation_learning     │
│  ─ graph_propagation       │         │  ─ multimodal_context        │
│  ─ compiled_truth boost ◄──┼── NEW   │  ─ failure_memory            │
│  ─ backlink boost     ◄────┼── NEW   │  ─ memory_health ◄── NEW (A3)│
│  ─ RRF fusion         ◄────┼── NEW   │  ─ memory_lint   ◄── NEW (B2)│
└────────────────────────────┘         │  ─ wiki_overview ◄── NEW (B1)│
                                       │  ─ tier_escalator◄── NEW (B3)│
                                       └──────────────────────────────┘
                                                       │
                                                       ▼
                                       ┌──────────────────────────────┐
                                       │  wiki_artifacts table        │
                                       │  ─ overview.md content       │
                                       │  ─ index.md content          │
                                       │  ─ generated_at, source_ids  │
                                       └──────────────────────────────┘
                                                       │
                                                       ▼
                                       ┌──────────────────────────────┐
                                       │  Frontend (ui/src/components/│
                                       │   memory/):                  │
                                       │  ─ MemoryPanel (现有)         │
                                       │  ─ WikiView ◄── NEW          │
                                       │  ─ MemoryHealthPanel◄── NEW  │
                                       └──────────────────────────────┘
```

### 4.2 数据模型扩展

#### 4.2.1 `MemoryNodeKind` 新增第 10 个变体(纯 additive)

`src-tauri/src/memory_graph/models.rs`:

```rust
pub enum MemoryNodeKind {
    Boot,
    Identity,
    Value,
    UserProfile,
    Directive,
    Curated,
    Episode,
    Procedure,
    Reference,
    EntityPage,  // NEW — per-entity compiled-truth + timeline view
}
```

向后兼容性:**所有现有数据库行都不变**。`EntityPage` 是新增枚举值,旧数据没有该 kind,不影响任何现有查询。

#### 4.2.2 `metadata_json` 在 `EntityPage` kind 下的约定 schema

```json
{
  "compiled_truth": "Markdown 字符串,LLM 周期性 regenerate 的当前最佳综合",
  "timeline": [
    {
      "date": "2026-05-18",
      "source_node_id": "uuid-...",
      "source_session_id": "session-...",
      "text": "在某次会话里发生了什么"
    }
  ],
  "enrichment_tier": 2,
  "last_synthesized_at": "2026-05-18T10:30:00Z",
  "synthesis_source_count": 7,
  "aliases": ["张三", "Zhang San", "ZS"],
  "contradictions": [
    {
      "between_source_ids": ["uuid-a", "uuid-b"],
      "claim_a": "他在 Google 工作",
      "claim_b": "他在 Meta 工作",
      "noticed_at": "2026-05-15"
    }
  ]
}
```

向后兼容:**纯约定,无 schema 变化**。`metadata_json` 已是 JSON 字段,任何节点 kind 都可以塞额外 key,旧解析器忽略未知 key。

#### 4.2.3 `MemoryRelationKind` 新增 7 种 typed edge(纯 additive)

```rust
pub enum MemoryRelationKind {
    // 现有 (V1-V26 全程使用,绝不动)
    Contains,
    RelatesTo,
    Timeline,
    Trigger,
    // 新增 (V27+,auto-link 钩子写入)
    WorksAt,
    Founded,
    InvestedIn,
    Advises,
    Attended,
    Source,      // page-A 引用 page-B 作为信息来源
    Mentions,    // 兜底类型,推断失败时落到这里
}
```

`inferLinkType` 启发式(参考 gbrain `src/core/link-extraction.ts`)伪代码:

```rust
fn infer_link_type(
    src_kind: MemoryNodeKind,
    dst_kind: MemoryNodeKind,
    context_text: &str,
) -> MemoryRelationKind {
    use MemoryNodeKind::*;
    use MemoryRelationKind::*;
    let lower = context_text.to_lowercase();
    match (src_kind, dst_kind) {
        // person → company
        (UserProfile | EntityPage, EntityPage) => {
            if lower.contains("works at") || lower.contains("works as") || lower.contains("员工")  { WorksAt }
            else if lower.contains("founded") || lower.contains("创立")              { Founded }
            else if lower.contains("invested in") || lower.contains("领投")          { InvestedIn }
            else if lower.contains("advises") || lower.contains("顾问")              { Advises }
            else if lower.contains("attended") || lower.contains("出席")             { Attended }
            else                                                                     { Mentions }
        }
        // 任何节点 → Reference,默认 Source
        (_, Reference) => Source,
        // fallback
        _ => Mentions,
    }
}
```

#### 4.2.4 新表(V34 migration)

```sql
-- V34: Memory OS Phase 1 — Auto-link audit + Wiki artifacts + Health findings
-- (V27-V33 已被既有功能占用,本设计避让到 V34;详见 §1.4 状态说明)

-- auto-link 写边时记录"是 LLM 文本里抽出来的"vs"用户/Agent 显式建的"
CREATE TABLE IF NOT EXISTS memory_edge_audit (
    edge_id     TEXT PRIMARY KEY REFERENCES memory_edges(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,                  -- 'auto_link' | 'explicit' | 'reflection'
    inferred_by TEXT,                            -- 'regex' | 'heuristic' | 'llm' | NULL
    confidence  REAL,                            -- 0.0-1.0,启发式给 0.6,LLM 给 0.8+
    extracted_from_version_id TEXT REFERENCES memory_versions(id),
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_edge_audit_src ON memory_edge_audit(source);

-- AI Wiki 派生物存储(overview / index 的最新快照)
CREATE TABLE IF NOT EXISTS wiki_artifacts (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    kind            TEXT NOT NULL,               -- 'overview' | 'index' | 'tag_cluster'
    content         TEXT NOT NULL,               -- markdown
    generated_at    INTEGER NOT NULL,
    source_node_ids TEXT NOT NULL,               -- JSON array of UUIDs
    llm_model       TEXT,
    token_cost      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_space_kind ON wiki_artifacts(space_id, kind);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_generated ON wiki_artifacts(generated_at);

-- Health / Lint findings(可 ack 可 dismiss)
CREATE TABLE IF NOT EXISTS memory_health_findings (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    severity    TEXT NOT NULL,                   -- 'error' | 'warn' | 'info'
    check_kind  TEXT NOT NULL,                   -- 'orphan' | 'stub' | 'dangling_fts' | 'index_drift' | 'phantom_slug' | 'stale_summary' | 'hub_stub' | 'phantom_hub' | 'contradiction'
    subject     TEXT NOT NULL,                   -- 涉及的 node_id 或 edge_id
    payload_json TEXT,                            -- 检查细节
    is_lint     INTEGER NOT NULL DEFAULT 0,      -- 0=health(免费),1=lint(LLM)
    dismissed   INTEGER NOT NULL DEFAULT 0,
    discovered_at INTEGER NOT NULL,
    dismissed_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_health_findings_active ON memory_health_findings(space_id, dismissed, discovered_at);
CREATE INDEX IF NOT EXISTS idx_health_findings_subject ON memory_health_findings(subject);
```

#### 4.2.5 与现有 V1-V33 迁移的边界声明

(基于实际 `migrations.rs` 代码,非 CLAUDE.md 注册表——后者已落后到 V26。)

| 现有迁移 | 内容 | 本 spec 影响 |
|---|---|---|
| V1-V3 | initial、artifact_cache、memories | **零影响** |
| V4 | memory_graph(memory_nodes/versions/edges/routes/keywords/memory_fts unicode61) | **复用,不改 schema**;只往 `metadata_json` 加约定 key |
| V5-V10 | agent_sessions、agent_teams、automations、message_process、messages_fts | **零影响** |
| V11 | messages_fts trigram 重建 | **零影响** |
| V12 | agent_messages_fts | **零影响** |
| V13 | cost_records | **复用**——Lint scenario 把 token 消耗写这张表用于成本面板 |
| V14-V21 | permission/metrics/workspace/skill-tags/automation | **零影响** |
| V22-V25 | automation_installed_skills、marketplace、standalone_installs | **零影响** |
| V26 | conversations.archived | **零影响** |
| V27-V29 | 自定义 system prompts、prompt 历史、逻辑压缩 | **零影响** |
| V30 | fragment_reviews + daily_summaries(memory fragments) | **零影响**(本 spec 的 EntityPage 与 fragment 是平行抽象,不冲突;未来可融合) |
| V31 | memory_fts 重建为 trigram | **零影响,且依赖**——auto-link 钩子和 Health scenario 都假定 memory_fts 是 trigram |
| V32 | IM channel infra | **零影响** |
| V33 | Symphony runtime | **零影响** |
| **V34**(本 spec 新增) | memory_edge_audit + wiki_artifacts + memory_health_findings | 全部纯 additive |

承诺:`migrations.rs::run` 的 idempotent 语义被严格保留——所有新 SQL 用 `IF NOT EXISTS`。**本 PR 同时补全 CLAUDE.md 的 Active migration registry**(把 V27–V33 的现状写进去,避免下一个 PR 又踩坑)。

### 4.3 关键代码改动点

#### 4.3.1 `store.rs::create_version` 新增 auto-link 钩子

```rust
// src-tauri/src/memory_graph/store.rs

impl MemoryGraphStore {
    pub fn create_version(&self, version: &MemoryVersion) -> Result<(), Error> {
        // ... 现有逻辑 (INSERT memory_versions + INSERT memory_fts) 不变 ...

        // NEW: auto-link post-hook(默认开启,可通过 memubot_config 关闭)
        if self.config.read().auto_link_enabled {
            if let Err(e) = self.run_auto_link_extraction(version) {
                tracing::warn!("auto-link extraction failed: {} (non-fatal)", e);
                // 决不让 auto-link 失败导致 create_version 失败
            }
        }
        Ok(())
    }

    fn run_auto_link_extraction(&self, version: &MemoryVersion) -> Result<(), Error> {
        // 1. 扫文本里的 [[entity:slug]] 和 [[node:uuid]] 引用
        let refs = extract_refs(&version.content);

        // 2. 比对上一版本的 refs,得出 added / removed 集合
        let prev_refs: HashSet<_> = self
            .get_previous_version_refs(&version.node_id, &version.id)?
            .into_iter().collect();
        let current: HashSet<_> = refs.iter().collect();
        let added = current.difference(&prev_refs);
        let removed = prev_refs.difference(&current);

        // 3. added:解析 slug → node_id,推断 link_type,插 memory_edges + memory_edge_audit
        for r in added {
            let dst_node_id = resolve_ref(&self.conn, r)?;
            let link_type = infer_link_type(version.node_kind, dst_kind, &version.content);
            self.upsert_edge_with_audit(version.node_id, dst_node_id, link_type, version.id)?;
        }

        // 4. removed:stale-reconciliation,删 edge + audit
        for r in removed {
            self.remove_auto_link_edge(version.node_id, &r)?;
        }
        Ok(())
    }
}
```

引用语法约定(参考 gbrain 的两种 markdown 形式 + 一种 uuid 形式):
- `[[entity:zhang-san]]`(human-readable slug,基于 EntityPage 节点的 `aliases` 字段或 title)
- `[[node:uuid-...]]`(精确 UUID,LLM/Agent 写入时优先用此)
- `[Display Text](entity/zhang-san)`(兼容 gbrain markdown 风格,可选)

#### 4.3.2 `recall.rs` 排序加 boost 信号

```rust
// src-tauri/src/memory_graph/recall.rs

fn compute_recall_score(
    node: &MemoryNodeDetail,
    fts_rank: f32,
    vector_score: f32,
    edge_count: i64,
) -> f32 {
    let mut score = fts_rank * 1.0 + vector_score * 1.2;

    // NEW: Compiled-truth boost — EntityPage 节点的 compiled_truth 命中权重高
    if node.node.kind == MemoryNodeKind::EntityPage {
        score *= 1.5;
    }

    // NEW: Backlink boost — 越多边指向越重要
    score += (edge_count as f32 + 1.0).log10() * 0.3;

    score
}
```

#### 4.3.3 `proactive/scenarios/memory_health.rs`(新文件)

```rust
// src-tauri/src/proactive/scenarios/memory_health.rs

pub struct MemoryHealthScenario {
    last_run: Mutex<Instant>,
    interval: Duration,  // 默认 5 分钟
}

#[async_trait]
impl Scenario for MemoryHealthScenario {
    async fn run(&self, ctx: &ScenarioContext) -> Result<()> {
        // 节流:5 分钟跑一次
        if self.last_run.lock().elapsed() < self.interval { return Ok(()); }

        let conn = ctx.memory_store.conn.lock().await;
        let mut findings = Vec::new();

        // Orphan
        findings.extend(self.find_orphans(&conn)?);
        // Stub
        findings.extend(self.find_stubs(&conn)?);
        // Dangling FTS
        findings.extend(self.find_dangling_fts(&conn)?);
        // Index drift
        findings.extend(self.find_index_drift(&conn)?);
        // Phantom slug
        findings.extend(self.find_phantom_slugs(&conn)?);

        self.persist_findings(&conn, findings)?;
        *self.last_run.lock() = Instant::now();
        Ok(())
    }
}
```

#### 4.3.4 `tauri_commands.rs` 新增 IPC 命令

```rust
#[tauri::command]
pub async fn memory_wiki_get_overview(state: State<'_, AppState>, input: WikiGetInput)
    -> Result<Option<WikiArtifactDto>, Error> { ... }

#[tauri::command]
pub async fn memory_wiki_regenerate(state: State<'_, AppState>, input: WikiRegenInput)
    -> Result<WikiArtifactDto, Error> { ... }

#[tauri::command]
pub async fn memory_health_list_findings(state: State<'_, AppState>, input: HealthListInput)
    -> Result<Vec<HealthFindingDto>, Error> { ... }

#[tauri::command]
pub async fn memory_health_dismiss_finding(state: State<'_, AppState>, finding_id: String)
    -> Result<(), Error> { ... }

#[tauri::command]
pub async fn memory_entity_page_get(state: State<'_, AppState>, node_id: String)
    -> Result<Option<EntityPageDto>, Error> { ... }

#[tauri::command]
pub async fn memory_entity_page_synthesize(state: State<'_, AppState>, node_id: String)
    -> Result<EntityPageDto, Error> { ... }
```

**别忘了**(`CLAUDE.md` 警告):新命令必须在 `main.rs::invoke_handler!` 宏里同时注册,否则运行时缺失。

### 4.4 双脑模型

**用户的第二大脑(User's 2nd Brain)** — 用户视角:

| 能力 | 实现 | 阶段 |
|---|---|---|
| 浏览所有 EntityPage | `WikiView.tsx`,左侧目录 + 右侧 page | Phase 3 |
| 编辑 compiled_truth | `WikiView.tsx` 内联编辑器,保存触发 create_version | Phase 3 |
| 看时间线 | EntityPage 下方 collapsible timeline,事件按日期分组 | Phase 3 |
| 搜索 | 现有 `MemorySearchPanel` 加 EntityPage 过滤选项 | Phase 3 |
| 矛盾标注 | `WikiView.tsx` 内 `## Contradictions` 段渲染,可手工解决 | Phase 5 |
| Markdown 导出/同步 | `~/Documents/workground/brain/<kind>/<slug>.md` 双向 | Phase 7+ |
| Git 版本 | 同步出 markdown 后自动 git init / commit / log | Phase 7+ |

**Agent 的第二大脑(Agent's 2nd Brain / Persistent Life Memory)** — Agent 视角:

| 能力 | 实现 | 阶段 |
|---|---|---|
| 跨会话延续记忆 | 现有 `Boot` 注入 + 新 `EntityPage` 自动 recall(via compiled_truth boost) | Phase 1+4 |
| 自然语言写引用,自动建图 | Auto-link post-hook(Tier A2) | Phase 2 |
| 看到自己的失败历史 | 现有 `failure_memory.rs` + 新 EntityPage 关联 | Phase 4 |
| 自动学新技能(已有) | 现有 `skill_extraction.rs`,不动 | — |
| 自我介绍 + 当前任务上下文 | `wiki_overview` 自动生成 + Boot 注入摘要 | Phase 3 |
| 看到关于"自己"的 EntityPage | 创建 `Self` slug 的 EntityPage,所有 self-eval 记录归并到这里 | Phase 5 |

### 4.5 AI Wiki 构建策略

知识组织(参考 llm-wiki 的 4 类 page,映射到 uClaw):

| llm-wiki 类型 | 对应 uClaw kind | 说明 |
|---|---|---|
| source | `Reference` + EntityPage | Reference 是原始资料,可再衍生 EntityPage |
| entity | `EntityPage` | 一对一映射 |
| concept | `EntityPage` 或 `Curated` | concept 作为 EntityPage 的特殊 kind 区分(metadata.subkind="concept") |
| synthesis | `wiki_artifacts.kind="synthesis"` | query 答案"--save" 后落库 |

检索优化(参考 gbrain 5 层 hybrid):

```
Query
  → uClaw intent classifier (新建,Tier C 可选,先用 LLM)
  → multi-query expansion (Haiku rewrite to 3 variants,Phase 5)
  → vector search (memU when up) + FTS5 (trigram)
  → graph_propagation BFS (现有)
  → RRF fusion: score = Σ 1/(60 + rank)
  → compiled_truth boost × 1.5
  → backlink boost × log(1 + edge_count) * 0.3
  → dedup by node_id
  → Top-K (token_budget 5000 截断,现有)
```

自动更新策略:

| 触发 | 动作 | scenario |
|---|---|---|
| 新 Episode 节点写入 | auto-link 抽边 + 如果引用了未存在的 entity → phantom_slug finding | `auto_link` + `memory_health` |
| EntityPage timeline 增长 ≥5 条 | 标记 `compiled_truth` 为 stale | `memory_health`(0 LLM 检查) |
| 10+ 次 EntityPage 写入累计 | regenerate overview.md | `wiki_overview` |
| 每天凌晨 / 用户主动触发 | 跑 lint(LLM) | `memory_lint` |
| 同一 EntityPage 内出现 contradictory facts | 标到 `metadata.contradictions[]` | `memory_lint` |

---

## 5. 风险与回退

### 5.1 性能风险

| 风险 | 影响范围 | 评估 | 缓解 |
|---|---|---|---|
| auto-link 钩子在 `create_version` 同步执行,影响写延迟 | 所有写记忆路径 | 每次 hook ≤ 5ms(纯 regex + 索引查找) | 异步化选项 + 灰度配置 |
| `compiled_truth_boost` 让 EntityPage 永远排前,挤压其它 kind | 召回侧 | 灰度上 boost,初期 × 1.2,跑 1 周看 cited_count 分布再调到 × 1.5 | 配置化 boost 因子 |
| `memory_lint` 烧 token | LLM 成本 | 默认 every 15 ingests 才跑,且只对 hub stub 跑 LLM(其它纯 SQL) | `cost_records`(V13)监控 |
| `wiki_overview` 重写整页 markdown 占 token | LLM 成本 | 用 Haiku(便宜)+ 限制 top 50 EntityPage 作为输入 | 同上 |
| 健康检查 SQL 在大库下变慢 | proactive tick | 已加 index,本地实测 10k 节点 < 50ms | 节流到 5 分钟一次 |

### 5.2 架构冲突风险

| 风险 | 评估 | 防护 |
|---|---|---|
| `MemoryNodeKind::EntityPage` 不在历史数据里,所有 list_nodes_by_kind 默认查询不会扫到 | **零影响** | 测试覆盖现有 kind 的查询 |
| 新 7 个 `MemoryRelationKind` 变体破坏现有 edge 解析(从 string 解析) | 中等 | `FromStr` 用 fallthrough + `serde(rename_all = "snake_case")`,旧字符串照样能 deserialize |
| 新表(V27)与 `tauri_commands.rs` 现有 memory_* 命令冲突 | **零** — 新表新命令,完全正交 | grep `memory_wiki_` `memory_health_` `memory_entity_page_` 确认无重名 |
| 现有 `proactive` 4 个 scenarios 与新 3 个 scenarios 抢 InfraService 消息 | 中等 | 新 scenarios 走"被动消费"模型,不订阅 conversation event,只读 SQL |
| `memory_graph/reflection.rs` 的 memU type → MemoryNodeKind 硬编码映射(`reflection.rs:10-21`) | **零** — 不动 reflection.rs | 该映射保持 9 种 kind,EntityPage 由新流程创建,不走 reflection |
| `find_learned_skill_by_normalized_title` 现有去重逻辑与 EntityPage 的 slug 寻找冲突 | **零** — 一个查 Procedure kind,一个查 EntityPage kind,WHERE 子句不重叠 | 在 PR 描述里明确指出 |
| `memorization/service.rs` 与新 wiki_overview 写同一节点 | 中等 | wiki_artifacts 是新表,不和 memory_nodes 冲突;EntityPage 写入走 store.rs API,有 ON CONFLICT 保护 |

### 5.3 前端兼容性

`ui/src/components/memory/` 现有组件:

| 组件 | 是否需要改 | 改动 |
|---|---|---|
| `MemoryPanel.tsx` | 是 — tab 列表加 "Wiki" 和 "Health" 选项 | Phase 3 / Phase 4 |
| `MemoryGraphView.tsx` | 是 — 节点颜色加 EntityPage 区分;新 typed-edge 颜色 | Phase 2 |
| `MemorySearchPanel.tsx` | 是 — 加 EntityPage 过滤;结果卡片显示 compiled_truth 摘要 | Phase 3 |
| `MemoryTimeline.tsx` | 否(timeline 是按节点 updated_at,不变) | — |
| `MemoryBootList.tsx` | 否 | — |
| `MemoryNodeCard.tsx` | 是 — EntityPage 卡片渲染 compiled_truth + 折叠 timeline | Phase 3 |
| `FragmentCard.tsx` | 否 | — |
| `QuickCaptureDialog.tsx` | 是 — 加 "Create as EntityPage" 选项 | Phase 3 |
| `DailySummaryView.tsx` | 否 | — |

新增组件:
- `WikiView.tsx`(Phase 3)
- `MemoryHealthPanel.tsx`(Phase 4)
- `EntityPageEditor.tsx`(Phase 3)
- `MarkdownSyncDialog.tsx`(Phase 7+)

主题约束(`CLAUDE.md` 警告):新组件全部用 theme tokens(`bg-popover`、`text-muted-foreground`),禁止 `bg-zinc-900` `text-gray-500` 等硬编码,否则在 warm-paper / qingye / forest-* 主题下颜色错乱。

### 5.4 回退策略

每个 Phase 提供 feature flag(在 `memubot_config.rs` 加 bool),禁用相应 scenario / hook。最坏情况:

- 关 `auto_link_enabled` → `create_version` 不再写新 edge,但已写的 edge 留着不被破坏
- 关 `wiki_overview_enabled` → 不再 regenerate,但 `wiki_artifacts` 表已有记录仍可读
- 关 `entity_page_boost_enabled` → 召回排序回到 V26 的行为

**Migration 不可回退**(idempotent 但 DDL 单向),但所有新表是 additive,空表不影响现有功能。

### 5.5 与 Proactive / Memorization 的兼容性

调研 §6/§7 揭示:Proactive 与 Memorization 当前职责重叠(都会写 memory_graph)。本 spec **不引入新的重叠**:

- `memory_health` / `memory_lint` / `wiki_overview` / `tier_escalator` 4 个新 scenario **只读** memory_graph + **只写** 3 个新表 + 调 `create_version` API(已经是 store.rs 暴露的写入路径)
- Memorization 现有路径不动
- Auto-link 钩子在 `create_version` 内部,Memorization 调它时也会得到 auto-link 副作用——这是预期行为,等价于"所有进入图的内容都享受自动建边"

---

## 6. Migration 总览(详见 plan 文档)

| Phase | 内容 | PR # | 涉及迁移 |
|---|---|---|---|
| Phase 1 | EntityPage kind + metadata schema + V34 三张新表 | #N+1 | V34 |
| Phase 2 | Auto-link post-hook + 7 种 typed-edge enum | #N+2 | — |
| Phase 3 | Wiki overview/index + 前端 WikiView | #N+3 | — |
| Phase 4 | Memory Health scenario + 前端 Health panel | #N+4 | — |
| Phase 5 | Memory Lint scenario + compiled_truth boost + backlink boost | #N+5 | — |
| Phase 6 | Tier-Escalating Enrichment | #N+6 | — |
| Phase 7 | Markdown 双向同步(Tier C1) | #N+7 | V35(可选,新增 brain_sync_state 表) |
| Phase 8 | Pluggable engine trait + Postgres POC(Tier C2) | Future | Future |

Plan 文档展开每个 Phase 的可 bisect commit、文件清单、验证命令。

---

## 7. Future Work

1. **Pluggable engine trait**(C2)— 为云端多端同步铺路。
2. **Louvain community detection** 跑在 `memory_edges`,辅助 lint 的 "fragile bridge" 检查。
3. **Cross-space shared memory pool** — `MemoryVisibility::Shared` 已有 enum,缺一套"全局公共知识库"语义。
4. **Subagent namespace isolation**(参考 gbrain `wiki/agents/<id>/`)— 让 sub-agent 写 EntityPage 时强制前缀 namespace,防 hallucinated entity 污染主图。
5. **Episode → EntityPage 自动 graduation** — 当某个 entity 在 ≥5 个 Episode 里被提及,自动创建 EntityPage 并迁移 timeline。
6. **Confidence-aware contradiction resolution** — 矛盾事实自动给 each side 打 confidence(LLM + source-recency + edge-count),不只是 flag。
7. **Memory benchmarks** — 借鉴 gbrain BrainBench,把 P@k / R@k / MRR / nDCG@k 跑在 uClaw 内部数据上,作为 `harness/` 的一组评估任务。

---

## Appendix A — 三方架构关键源码引用

### A.1 uClaw

- 数据模型:`src-tauri/src/memory_graph/models.rs:5-49`(MemoryNodeKind)、`models.rs:84-108`(MemoryRelationKind)、`models.rs:142-184`(MemoryNode / MemoryVersion / MemoryEdge structs)
- 存储 API:`src-tauri/src/memory_graph/store.rs:36-53`(create_node)、`store.rs:469`(create_version + FTS upsert)、`store.rs:187-223`(list_top_learned_skills + Gaussian decay)、`store.rs:636-735`(graph_propagation_search,decay_factor=0.6,relation_weight table)、`store.rs:979-995`(with_transaction)、`store.rs:1000-1077`(batch_hydrate_details N+1 消除)
- Migrations:`src-tauri/src/db/migrations.rs:100-160`(V4_MEMORY_GRAPH schema)、`migrations.rs:142`(FTS5 trigram tokenizer)
- memU bridge:`src-tauri/src/memu/bridge.rs:21,24`(timeouts)、`bridge.rs:125-180`(spawn_subprocess)、`memu/client.rs:76-129`(memorize / retrieve)
- Proactive:`src-tauri/src/proactive/service.rs:155-170`(CONTEXT_WINDOW_SIZE=20)、`proactive/scenarios/skill_parser.rs`(84KB,skill 提取)、`proactive/scenarios/failure_memory.rs:1-50`
- Reflection:`src-tauri/src/memory_graph/reflection.rs:10-21`(memU type → kind 硬编码映射)
- Recall:`src-tauri/src/memory_graph/recall.rs:82-90`(token_budget=5000)
- Tauri commands:`src-tauri/src/tauri_commands.rs:4366-4389`(memory_graph_search / get_node)、`tauri_commands.rs:2442-2551`(KV memory_* commands)、`tauri_commands.rs:138-189`(memory_recall_config)
- Frontend:`ui/src/lib/tauri-bridge.ts:180-210`、`ui/src/atoms/memory-voice-atoms.ts`、`ui/src/components/memory/MemoryPanel.tsx` / `MemoryGraphView.tsx`

### A.2 gbrain(基于 README / CLAUDE.md / ENGINES.md / llms-full.txt)

- BrainEngine interface:`docs/ENGINES.md:58-110`(完整 TS interface,见 §2.1 调研笔记 §9.1)
- Link extraction:`src/core/link-extraction.ts`(extractEntityRefs / extractPageLinks / inferLinkType / parseTimelineEntries),`llms-full.txt:2489` 行
- Batch insert pattern:`src/core/postgres-engine.ts::addLinksBatch` / `addTimelineEntriesBatch`,`llms-full.txt:2423` 行
- Compiled-truth + timeline 双层 page schema:`README.md` 示例
- Subagent allowlist:`src/core/minions/tools/brain-allowlist.ts`,`llms-full.txt:2521` 行
- Plugin manifest:`openclaw.plugin.json`

### A.3 llm-wiki-agent(基于 README / CLAUDE.md / AGENTS.md / GEMINI.md)

- Ingest workflow:`CLAUDE.md` 9 步流程(见 §2.3 调研笔记 §9.1)
- Page frontmatter:`CLAUDE.md` 5 字段 schema(`title / type / tags / sources / last_updated`)
- Source page body schema:`## Summary / Key Claims / Key Quotes / Connections / Contradictions`
- Health vs Lint table:`AGENTS.md`(PR #40 引入,7 列对照)
- Slash commands:`.claude/commands/wiki-health.md`(13 行,完整给出 6 项检查)
- Graph build:`tools/build_graph.py`(NetworkX + Louvain;基于 README + issue #25 描述)
- Graph-aware lint(PR #27 增):hub stubs / fragile bridges / isolated communities / sparse pages
- Dependencies:`requirements.txt` = `litellm>=1.0.0` + `networkx>=3.2`

---

## Appendix B — 决策日志

| 决策 | 选择 | 备选 | 理由 |
|---|---|---|---|
| 新 kind 还是新表存 EntityPage | 新 kind,沿用 `memory_nodes` | 新 `entity_pages` 表 | 复用现有 versioning / FTS / edge 基础设施 |
| compiled_truth 放 `memory_versions.content` 还是 `metadata_json` | `content` 存正文(享 FTS),`metadata_json` 存 timeline + 结构化字段 | 都放 `content` JSON | 让 FTS 直接搜 compiled_truth 文本 |
| 新边类型放 enum 还是 free-form string | enum + serde rename_all | string | Rust 编译期检查,gbrain 7 种已是稳定集合 |
| auto-link 同步还是异步 | 同步,但 try-catch swallow | 异步 channel | 同步保证版本和边强一致;失败不阻塞写 |
| 用 markdown 还是 JSON 存 wiki_artifacts | markdown | JSON | 用户/Agent 直接读,无渲染步骤 |
| 健康检查触发节奏 | proactive tick 节流 5 分钟 | 定时 cron / 事件驱动 | 复用现有 ProactiveService,不引入新调度器 |
| EntityPage 寻址用 UUID 还是 slug | 既支持也支持 — slug 作为 `aliases[0]` 或 metadata.slug,UUID 仍是 PK | 改 PK 为 slug | 不破坏现有 UUID 主键合约 |
| Markdown 同步是 Phase 3 还是 Phase 7 | Phase 7 | Phase 3 | 用户优先级未知,先把内部图谱跑起来 |
