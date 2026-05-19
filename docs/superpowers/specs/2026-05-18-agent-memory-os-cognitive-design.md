# Agent Memory OS — Cognitive Layer Design(第二大脑完全形态)

**Date:** 2026-05-18
**Status:** Draft
**Layer position:** **L2 Cognitive Layer (Phase 8-14)** —— 三层 Memory OS 设计的第二层。
- **L1 Foundation**:[`2026-05-18-agent-memory-os-design.md`](2026-05-18-agent-memory-os-design.md) + [`agent-memory-os.md`](../plans/agent-memory-os.md) —— 实体级长期记忆 + Auto-link + AI Wiki view
- **L2 Cognitive(本文)**:段落级 provenance + 9 page type + 两步 compile + Adaptive RAG
- **L3 Engines**:[`2026-05-18-agent-memory-os-engines-design.md`](2026-05-18-agent-memory-os-engines-design.md) + [`agent-memory-os-engines.md`](../plans/agent-memory-os-engines.md) —— Entity Graph(NER) + Timeline Engine + Dream Cycle

**Companion plan:** `docs/superpowers/plans/agent-memory-os-cognitive.md`(Phase 8-14)
**Inspired by:** [唐国梁 Tommy / LLM Wiki 完整运行控制面](https://space.bilibili.com/) 直播分享(2026 年 5 月),Karpathy "LLM as wiki" 论述,llm-wiki-compiler 与 atomicmemory 两个工程实例

---

## 0. 这份 spec 跟 Foundation Spec 是什么关系

Foundation Spec(`2026-05-18-agent-memory-os-design.md`)已经覆盖:

- 实体级长期记忆抽象 `EntityPage`(MemoryNodeKind 第 10 变体)
- compiled-truth + timeline 双层 page
- Auto-link post-hook(zero-LLM 写时建图)
- Memory Health scenario(0 LLM 维护)
- Wiki overview / index 派生层
- Hybrid recall + compiled-truth boost + backlink boost
- Tier-Escalating Enrichment
- Markdown 双向同步(Phase 7)

**它能做到的是:实体级长期记忆 + 自动维护的图谱视图。**

但对照 Tommy 完整框架(8 张图全景),仍有 **11 个红色缺口**:

| Tommy 框架要点 | Foundation Spec 状态 |
|---|---|
| 9 种页面类型(Comparison / Question / Decision / Gap / Meta 缺失) | ❌ |
| 段落级 provenance(`inferredParagraphs`) | ❌ |
| Page-level `confidence` (0~1) | ❌ |
| `status: verified/draft/inferred/disputed`(知识效力,非生命周期) | ❌ |
| `provenanceState: full/partial/none` | ❌ |
| 双向 `contradictedBy` 边 | ⚠ 单向 |
| `hot.md` 最近上下文热区 | ❌ |
| `purpose.md` 目的约束 | ❌ |
| `log.md` wiki 级事件流 | ❌ |
| `review/queue` 人工判断入口 | ⚠ 弱 |
| `state/` SHA-256 增量编译 | ❌ |
| 两步 LLM compile(Analyze → Generate) | ⚠ 单步 |
| Adaptive RAG + Query Classifier | ❌ |
| 7-role 角色框架显式化 | ⚠ 隐式 |

**本 spec 的目标:把上述 11 项全部补齐,让 uClaw 真正成为"用户和 Agent 的第二大脑"——而不是"一个有 wiki tab 的 memory graph"。**

设计承诺继续:**对 Foundation Spec 全部向后兼容,不替换、不破坏**。Cognitive layer 通过两条途径叠加:
1. **元数据约定**(零 schema 变化):大部分新字段是 `metadata_json` 的新 key,旧解析器忽略
2. **新表(V43 migration)**:仅添加 5 张新表,绝不修改 V1-V42 现有任何表(V35-V42 是 Foundation Phase 1-7 + browser-task + MCP audit;V43 是当前下一个空闲号)

---

## 1. 7-Role Conceptual Framing(角色定位)

Tommy 第 8 张图的核心论点是:**LLM Wiki 不只是"加个 wiki tab",而是让 LLM 从"每次重来的回答器"变成"对知识资产的增量投资工具"**。这是个心智模型的根本转变。

在 uClaw 上下文里,7 个角色明确映射到现有/新增模块:

| Role | 概念定位 | uClaw 实现锚点 |
|---|---|---|
| **① LLM = Compiler** | 把原始材料"编译"成结构化知识页面;query 时拼接而非生成 | `providers/` + `llm/` + 新 `wiki_compile.rs`(两步 Analyze→Generate) |
| **② Chat = Entry Point** | 用户和 Agent 进入知识系统的入口,但**不是知识本身** | `ui/src/components/chat/ChatInput.tsx` + `agent/AgentView.tsx` |
| **③ Wiki = Product** | 用户每天**翻阅**的成品,不是 ingest 的副产物 | `ui/src/components/memory/WikiView.tsx`(Foundation Phase 3 已建) |
| **④ Graph = Navigation Layer** | 在 wiki 上提供拓扑导航,**不是主存储** | `memory_edges` + `MemoryGraphView.tsx` |
| **⑤ Schema = Operating System** | frontmatter + node kind + provenance 字段构成"知识 OS" | `models.rs` + 本 spec §3 / §4 |
| **⑥ Review = Brake** | 人在回路:当 confidence 低、矛盾出现、新实体未确认时,**用户/Agent 必须能踩刹车** | 新表 `review_queue_items` + `ReviewQueuePanel.tsx`(本 spec §5) |
| **⑦ Log = Audit Trail** | 每一次 ingest / query / lint / promotion 都留痕,可回放 | 新表 `wiki_log_events` + 现有 `agent_messages` |

**实施含义:** 本 spec 的 Phase 8-14 会按这 7 个角色**逐角色补完**,而不是按工程模块切分——这样确保每个 PR 都让一个角色"活起来",而不是改一堆零散基础设施。

---

## 2. Knowledge Object Taxonomy(9 种页面类型)

Tommy 图 2 列出的 9 种 page type,在 uClaw 的实现策略是:**复用现有 `MemoryNodeKind`,通过 `metadata.subkind` 区分细类**(避免 enum 爆炸 + 保持向后兼容)。

### 2.1 9 种页面在 uClaw 的归位

| Tommy 类型 | uClaw NodeKind | metadata.subkind | 例子 |
|---|---|---|---|
| **Source** | `Reference`(现有) | `"source"` | karpathy-gist.md, anthropic-blog-2025-04.md |
| **Entity** | `EntityPage`(Foundation 新增) | `"entity"`(默认) | OpenAI, LangChain, Zhang San |
| **Concept** | `EntityPage` | `"concept"` | RAG, GraphRAG, Memory OS, RRF Fusion |
| **Comparison** | `EntityPage` | `"comparison"` | RAG vs LLM Wiki, SQLite vs Postgres |
| **Question** | `EntityPage` | `"question"` | Why frontmatter? · How does memU degrade? |
| **Synthesis** | `EntityPage` | `"synthesis"` | 2026 knowledge architecture, current RLHF landscape |
| **Decision** | `EntityPage` | `"decision"` | 为何选 trigram FTS, 为何用 V34 而非 V27 |
| **Gap** | `EntityPage` | `"gap"` | forgetting 最佳实践?· memU vs local vector ROI? |
| **Meta** | `wiki_artifacts`(Foundation 已建) | `kind="index"/"overview"/"hot"/"purpose"/"log"` | index.md, overview.md, hot.md, purpose.md, log.md |

**关键设计选择:**
- **Source 复用 `Reference` 而非新建**——`Reference` 节点天然就是"外部资料引用",加 `subkind="source"` 表示"已被消化成 wiki 页面的来源"
- **6 种知识页(Entity/Concept/Comparison/Question/Synthesis/Decision/Gap)共享 `EntityPage` kind**——它们都是"用户/Agent 当前最佳理解的可重写综述",存储模式完全一致,只在**展示模板**和**提示词**上差异化
- **Meta(index/overview/hot/purpose/log)走 `wiki_artifacts`**——这些是派生物或控制面文件,生命周期跟内容页根本不同(频繁重写或 append-only)

### 2.2 6 种知识页的差异化提示词模板

每种 subkind 对应一份**模板提示词**,用于:
- 创建 / 重写 compiled_truth 时给 LLM 用
- WikiView 渲染该页时的 UI 卡片布局

| Subkind | 必填段落 | LLM 模板要点 |
|---|---|---|
| **entity** | Summary / Background / Current Status / Relationships / Open Questions | 实体的"是什么 + 当前在做什么 + 跟谁关联" |
| **concept** | Definition / Key Properties / Examples / Confusables / References | 概念的"严格定义 + 易混淆点 + 实例" |
| **comparison** | Dimensions / Side-by-Side Table / When-to-Use / Trade-offs | **强制 markdown 表格**;每个对比维度一行 |
| **question** | Question Statement / Why It Matters / Current Hypotheses / Known Answers / Status: `open / answered / disputed` | 问题驱动的页面;answered 时附 Answer + 引用 |
| **synthesis** | Topic / Scope / Key Findings / Open Issues / Source List | 跨多个 source/entity/concept 的综合结论;**必须有引用列表** |
| **decision** | Context / Options Considered / Decision / Rationale / Pitfalls Avoided | 一次决策的完整记录;Pitfalls 段是"踩坑教训"的核心载体 |
| **gap** | Question / What We Know / What We Don't Know / Possible Paths / Priority: `urgent / important / curious` | "已知的不知道",显式留白让 Agent 主动调研 |

### 2.3 提示词模板的存储位置

不放代码里,放在新表 `wiki_page_templates`:

```sql
CREATE TABLE IF NOT EXISTS wiki_page_templates (
    subkind         TEXT PRIMARY KEY,           -- 'entity' | 'concept' | 'comparison' | ...
    display_name    TEXT NOT NULL,
    compile_prompt  TEXT NOT NULL,              -- LLM prompt for create/regenerate
    sections_json   TEXT NOT NULL,              -- 必填段落清单
    ui_card_layout  TEXT,                       -- 前端渲染 hint
    updated_at      INTEGER NOT NULL
);
```

预置 7 行(6 种知识页 + 兜底 default),用户可手动调 prompt(放在 V43 一次性 seed)。**这给用户/Agent 改变 wiki 形态的自由度——后续要加新 subkind 不需要改 Rust 代码。**

---

## 3. Page-Level Provenance System(可信度灵魂)

Tommy 图 3、图 4 的核心是:**让 LLM 生成的每一段话都能追溯到"是从原文抽出来的 vs 是 LLM 推断的 vs 是被反驳的"**。这是第二大脑能否被信任的关键。

### 3.1 8 keys 硬约束 frontmatter

Foundation Spec 的 EntityPage `metadata_json` 已经有 `aliases / timeline / contradictions / slug / subkind` 等。本 spec **正式化** 8 个 keys 为硬约束:

```json
{
  "type": "entity_page",
  "title": "LLM Wiki",
  "summary": "A knowledge engineering paradigm where LLM compiles raw sources into reusable structured pages, replacing per-query re-derivation.",
  "sources": ["uuid-of-karpathy-gist", "uuid-of-atomicmemory-paper"],
  "tags": ["rag", "wiki", "knowledge"],
  "status": "verified",
  "confidence": 0.92,
  "updated": "2026-04-15T10:30:00Z"
}
```

| Key | 类型 | 约束 |
|---|---|---|
| `type` | string | 等于 node kind(`entity_page`/`reference`/`procedure`/...) |
| `title` | string | 必填,non-empty |
| `summary` | string | 必填,≤ 200 字符;空时显示 ⚠ |
| `sources` | string[] | 该页结论所依据的 Source/Reference 节点的 UUID 列表;空数组允许但触发 `provenanceState=none` |
| `tags` | string[] | 多对一映射 `memory_keywords` 表(后端层做同步) |
| `status` | enum | `verified` / `draft` / `inferred` / `disputed` —— **知识效力**,见 §3.2 |
| `confidence` | float [0,1] | 该页结论的置信度;LLM compile 时给初值,人工 review 可调 |
| `updated` | ISO 8601 | UTC 时间戳;每次 create_version 自动更新 |

### 3.2 `status` 跟 `version_status` 的关键区别

uClaw 现有 `MemoryVersionStatus` 是 **生命周期** 维度(active/deprecated/orphaned),管的是"这个版本现在还有效吗"。

本 spec 新增的 page-level `status` 是 **知识效力** 维度,管的是"这条知识本身的真伪/确定性":

| 值 | 语义 | UI 显示 |
|---|---|---|
| `verified` | 人工 review 通过,所有 inferredParagraphs 也已确认 | 🟢 绿色边框 |
| `draft` | LLM 刚 compile 完,等待 review | 🟡 黄色 |
| `inferred` | 大部分内容是 LLM 推断,缺少明确 source 支持 | 🟠 橙色 |
| `disputed` | 至少有一条 contradictedBy 边激活 | 🔴 红色 + 警告 |

**两者**正交**共存**:一个 version 可以是 `version_status=active` + `status=disputed`,意味着"这是当前最新版本,但内容有争议"。

### 3.3 段落级 provenance(`inferredParagraphs`)

Tommy 图 4 最有冲击力的字段。compiled_truth markdown 正文里,有些段落是从 source 抽出的真材实料,有些是 LLM 自己想的——**用户必须能区分**。

实现方式:`inferredParagraphs: [3, 7]` 是段落索引数组(1-indexed,按 markdown 顶层段落计数)。前端渲染时,这些段落被加上**虚线左侧 border + 灰底 + 鼠标悬停显示"This paragraph is inferred by LLM, not directly from sources"**。

LLM 生成 compiled_truth 时,**强制要求**在两步 compile 的 Generate 步骤里:
1. 把每段标"来源段"(指向 sources 数组里某个 source 的 chunk)或"推断段"
2. 输出结构化的 paragraph map:`{ "1": "source:uuid-a:chunk-3", "2": "inferred", "3": "source:uuid-b:chunk-1", "4": "inferred" }`
3. `inferredParagraphs` 字段从这个 map 自动派生(`paragraphs where value == "inferred"`)

这个 paragraph map 完整存到新字段 `paragraphSourceMap` 里:

```json
{
  "inferredParagraphs": [2, 4],
  "paragraphSourceMap": {
    "1": "source:uuid-karpathy-gist:chunk-3",
    "2": "inferred",
    "3": "source:uuid-atomicmemory:chunk-1",
    "4": "inferred",
    "5": "source:uuid-atomicmemory:chunk-7"
  }
}
```

后续点开任何一段都能直接跳到原 source 的对应 chunk。

### 3.4 `provenanceState` —— 整页的来源覆盖状态

| 值 | 触发条件 |
|---|---|
| `full` | 所有非空段落都有 paragraphSourceMap 指向某个 source chunk |
| `partial` | 部分段落 inferred,部分有 source |
| `none` | sources 数组为空,或所有段落都是 inferred |

`provenanceState=none` 在 UI 上**特别醒目**——意味着这是个纯 LLM 推断的"猜测页",用户/Agent 应该把它当假设而非事实。

### 3.5 `contradictedBy` —— 双向矛盾边

Foundation Spec 已有 `metadata.contradictions[]`(单向,在矛盾被发现的页面内列对方)。本 spec 要求**双向**:

- 页面 A 的 `contradictions[].against` 列出 page B 的 UUID
- 页面 B 的 `contradictedBy[]` 自动包含 page A 的 UUID(由 `memory_lint` scenario 同步)

**为什么必须双向:** 你打开 page B,需要立刻看到"有别的页面在反对我",而不是要去搜索哪个其他页面提到了 B。Tommy 强调这点是因为"矛盾必须被两边的读者都看到"。

实现:`memory_lint` scenario 在标矛盾时同时写两侧。`memory_health` 加一项检查:`contradictedBy` 引用的 page 是否真的在它的 `contradictions[].against` 里(防漂移)。

### 3.6 召回时如何用这些信号

`recall.rs` 排序更新:

```rust
fn compute_recall_score(...) -> f32 {
    let mut score = fts_rank * 1.0 + vector_score * 1.2;
    if node.kind == MemoryNodeKind::EntityPage { score *= 1.5; }
    score += (edge_count + 1).log10() * 0.3;

    // NEW: knowledge-validity weighting
    let status_mult = match status {
        "verified" => 1.3,
        "draft"    => 0.9,
        "inferred" => 0.7,
        "disputed" => 0.5,
        _          => 1.0,
    };
    score *= status_mult;
    score *= confidence;  // 0~1 直接乘,confidence=0.5 等于罚分一半

    // NEW: provenanceState penalty
    if provenance_state == "none" { score *= 0.6; }

    score
}
```

**效果:** verified + confidence 0.95 的页面比 inferred + 0.3 的页面在召回排序上有 ~7x 权重差。Agent 自然倾向于引用可信内容。

---

## 4. Control Plane —— 8 个文件的完整运行控制面

Tommy 图 1 是整个框架最具结构感的部分。8 个文件 / 目录每一个都对应一个"控制开关",而不是装饰品。在 uClaw 的实现策略是**全部用 `wiki_artifacts` 表 + 几张新辅助表存,通过 `kind` 字段区分**,不引入文件系统依赖(Phase 7 的 markdown 同步可以**导出**它们,但 source of truth 是 SQLite)。

### 4.1 8 个文件的逐项实现

| Tommy 文件 | uClaw 实现 | 写入方 | 读取方 |
|---|---|---|---|
| `index.md` | `wiki_artifacts(kind="index")` | `wiki_overview` scenario(Foundation Phase 3) | WikiView 左侧目录 |
| `log.md` | 新表 `wiki_log_events` 的 markdown 渲染(append-only) | 所有 scenario / tauri command 写入 | `WikiLogView.tsx`(新组件) |
| `overview.md` | `wiki_artifacts(kind="overview")` | `wiki_overview` scenario(Foundation Phase 3) | WikiView 顶部 |
| **`hot.md`** | `wiki_artifacts(kind="hot")`,每 N 分钟重写 | 新 `wiki_hot` scenario | WikiView 侧边栏 + Boot 注入(可选) |
| **`purpose.md`** | `wiki_artifacts(kind="purpose")`,用户手动编辑 | 用户(很少改) | Boot 注入到 system prompt + WikiView 顶部 |
| `state/` | 新表 `page_content_hashes` | `wiki_compile.rs` 在每次 compile 后写 | `wiki_compile.rs` 增量编译时读 |
| **`review/queue`** | 新表 `review_queue_items` | `memory_lint` / `auto_link` / 用户手动 | `ReviewQueuePanel.tsx` |
| `graph.json` | 派生 `memory_edges`(已有),新 tauri command 导出 | — | `MemoryGraphView.tsx`(现有) |

### 4.2 `hot.md` —— 最近上下文热区

**目的:** 给 Agent 和用户提供"快速回到现场"的能力。Tommy 框架里这是为了"翻 wiki 时第一眼看到最近在搞什么"。

**生成逻辑:** `wiki_hot` scenario 每 N 分钟(默认 15 分钟)跑一次:
1. SELECT 最近 24 小时内 updated 的 EntityPage(LIMIT 20)
2. SELECT 最近 24 小时内 created 的 Episode(LIMIT 30)
3. SELECT 最近的 5 条 `wiki_log_events`(type=`ingest`/`promote`)
4. LLM 用 Haiku 写一段 ~200 字 markdown:"过去 24 小时,我们的认知有这些变动..."
5. 写到 `wiki_artifacts(kind="hot")`,**覆盖式**(只保留当前版本)

**消费:** 
- WikiView 顶部把 `hot.md` 跟 `overview.md` 一起渲染,默认折叠展开"热区"是 Tab 切换
- **可选** `boot_inject_hot=true` 时,Agent 每个 session 的 Boot 上下文里自动注入 hot.md(让 Agent 知道"用户最近在搞什么")

### 4.3 `purpose.md` —— 目的约束

**目的:** Tommy 反复强调"Wiki 到底为什么存在"这件事必须显式写下来。否则 wiki 会被无限填充噪音。

**形态:** 用户手动编辑的 markdown(WikiView 上有"编辑 Purpose"按钮),典型内容:

```markdown
# uClaw Wiki Purpose

This wiki captures:
- People, companies, and projects relevant to building uClaw
- Architectural decisions and their rationales
- Open research questions in agent memory / wiki / knowledge OS

This wiki does NOT capture:
- Code-level documentation (lives in src-tauri/src/)
- Day-to-day task tracking (lives in agent_sessions)
- Generic Wikipedia-style facts about unrelated topics
```

**消费:**
- **强制注入 Boot 上下文**——每个 agent session 的 system prompt 都会带 purpose.md,Agent 看到任何不相关的内容时主动决定"这个不该 ingest 到 wiki,放 episode 就好"
- `memory_lint` scenario 在 LLM 检查 hub stub 时,也会读 purpose.md 作为 "what should be in this wiki" 的判定基准

### 4.4 `log.md` —— wiki 级事件流

新表设计:

```sql
CREATE TABLE IF NOT EXISTS wiki_log_events (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    event_type  TEXT NOT NULL,        -- 'ingest' | 'promote' | 'lint' | 'review_open' | 'review_close' | 'compile_skip' | 'compile_run'
    subject_id  TEXT,                 -- node_id or wiki_artifact_id or review_queue_id
    actor       TEXT NOT NULL,        -- 'user' | 'agent' | 'scenario:<name>'
    payload_json TEXT,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_time ON wiki_log_events(space_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_subject ON wiki_log_events(subject_id);
```

**渲染:** `WikiLogView.tsx` 按时间倒序显示,每条事件可点开看 payload。也可以按 event_type 过滤(看 Agent 一周内做了多少次 promote)。

**为什么必须有 log:** Tommy 的角色 ⑦ Log = Audit Trail——没有 log 你**没法回放为什么 wiki 变成今天这样**,也没法做"我两周前把这个 entity 改成 verified 是基于什么"这种回溯。

### 4.5 `state/` —— SHA-256 增量编译缓存

**目的:** Tommy 框架里这是"最被低估的部分"——把 LLM compile 工程化成可缓存、可增量、可审计的流水线。

**实现:**

```sql
CREATE TABLE IF NOT EXISTS page_content_hashes (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    sources_hash     TEXT NOT NULL,           -- sha256(concat(sources[].active_version.content))
    timeline_hash    TEXT NOT NULL,           -- sha256(concat(timeline[].text))
    compiled_hash    TEXT NOT NULL,           -- sha256(current compiled_truth)
    last_compiled_at INTEGER NOT NULL,
    last_skip_count  INTEGER NOT NULL DEFAULT 0,
    skip_reason      TEXT
);
```

**增量逻辑**(`wiki_compile.rs::should_recompile`):

```rust
fn should_recompile(node_id: &str) -> CompileDecision {
    let cached = page_content_hashes::get(node_id)?;
    let current_sources_hash = sha256(concat(load_sources(node_id)));
    let current_timeline_hash = sha256(concat(load_timeline(node_id)));

    if cached.sources_hash == current_sources_hash 
       && cached.timeline_hash == current_timeline_hash {
        // 上游源和 timeline 都没变 → skip recompile
        page_content_hashes::bump_skip(node_id);
        return CompileDecision::Skip { reason: "all-inputs-unchanged" };
    }
    if cached.timeline_hash != current_timeline_hash 
       && cached.sources_hash == current_sources_hash {
        // 只有 timeline 增量 → partial recompile(只重写 Current Status 段)
        return CompileDecision::Partial { sections: vec!["Current Status"] };
    }
    // sources 变了 → full recompile
    return CompileDecision::Full;
}
```

**实际效益:** 对一个 1000 entity 的 wiki,如果一周只有 50 个 entity 的 source 变了,full recompile 是 50 次 LLM 调用而不是 1000 次——**LLM 成本 95% 节省**。Tommy 演讲里的 atomicmemory 实例就是验证这个思路。

### 4.6 `review/queue` —— 人在回路

**目的:** Agent 不应该独自决定矛盾解决、新 entity 创建、低 confidence 内容是否信任——这些必须有用户介入。

**新表:**

```sql
CREATE TABLE IF NOT EXISTS review_queue_items (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    item_kind       TEXT NOT NULL,
        -- 'phantom_entity'  → 多次提及但不存在的 slug,问"要不要建?"
        -- 'low_confidence'  → confidence < 0.5 的新 EntityPage
        -- 'contradiction'   → 两个 page 有矛盾,问"哪个对?"
        -- 'inferred_review' → inferredParagraphs 超过 50%,问"这页可信吗?"
        -- 'promotion'       → tier_escalator 升级到 Tier 1 前的确认
        -- 'manual'          → 用户主动加的 review 项
    severity        TEXT NOT NULL,           -- 'high' | 'med' | 'low'
    subject_ids     TEXT NOT NULL,           -- JSON array of node_ids / edge_ids 涉及的对象
    title           TEXT NOT NULL,           -- 一句话描述
    context_json    TEXT,                    -- 上下文(diff、source 引用等)
    status          TEXT NOT NULL DEFAULT 'open',  -- 'open' | 'resolved' | 'dismissed' | 'snoozed'
    resolution      TEXT,                    -- 'accept' | 'reject' | 'merge' | 'split' | 'ignore'
    resolution_note TEXT,
    assignee        TEXT,                    -- 'user' | 'agent' (Agent 也能 propose 解决方案)
    created_at      INTEGER NOT NULL,
    resolved_at     INTEGER,
    snooze_until    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_review_queue_active ON review_queue_items(space_id, status, severity, created_at);
CREATE INDEX IF NOT EXISTS idx_review_queue_subject ON review_queue_items(subject_ids);
```

**前端:** `ReviewQueuePanel.tsx`(新组件)显示在 MemoryPanel 的 "Review" tab。每条带:
- 标题 + 上下文 diff
- "Accept" / "Reject" / "Merge"(双 entity 合并) / "Split"(单 entity 拆分) / "Snooze 7d" / "Ask Agent" 6 个按钮
- "Ask Agent" 调 LLM 给出 proposed resolution + 用户最终拍板

**写入触发(谁创建 review 项):**

| Scenario / Hook | 创建什么 review 项 |
|---|---|
| `memory_health::phantom_slug` | `phantom_entity` 项 |
| `memory_lint::contradiction` | `contradiction` 项 |
| `memory_lint::hub_stub` | `low_confidence`(prompt 用户 enrich) |
| `wiki_compile` 检测到 inferredParagraphs > 50% | `inferred_review` 项 |
| `tier_escalator` 准备升级 Tier 1 | `promotion` 项 |
| 用户在 WikiView 上点 "Flag for Review" | `manual` 项 |

**brake 语义的关键:** 当一个 review_queue_item 的 severity = high 且 status = open 时,**相关页面的召回会被打折(× 0.5)**——这样 Agent 不会在"待 review 的有问题内容"上下次答错。这是真正的"刹车"——把不确定的内容**先冻起来**,等用户判断。

---

## 5. Two-Stage Compile Pipeline(LLM = Compiler)

Tommy 图 5 左边的 llm_wiki 实例,把 compile 拆成两步 LLM 调用——这跟一次性塞所有 source 给 LLM 让它"想到啥写啥"的暴力路径有质的区别。

### 5.1 为什么要拆两步

| 单步 compile | 两步 compile |
|---|---|
| 输入 = 所有 source 全文 + 现有 compiled_truth | 输入 = 同上 |
| 输出 = 新 compiled_truth | Step 1 输出 = 结构化分析(JSON):key claims, entities mentioned, contradictions, gaps, confidence per claim |
| LLM 决策不可见 | Step 2 输入 = 上述 JSON + 模板 prompt → 输出新 compiled_truth + paragraphSourceMap |
| 难做段落级 provenance | **天然能做段落级 provenance**(Step 1 已经给每条 claim 标了 source) |
| token 一次性消耗 | 总 token 略多(~30%),但可缓存 Step 1 结果 |

### 5.2 实现位置

新文件 `src-tauri/src/memory_graph/wiki_compile.rs`:

```rust
pub struct WikiCompiler {
    llm_client: Arc<dyn LlmClient>,
}

impl WikiCompiler {
    pub async fn compile(
        &self,
        node_id: &str,
        decision: CompileDecision,
    ) -> Result<CompileResult, Error> {
        // 增量检查(§4.5)
        match decision {
            CompileDecision::Skip { reason } => {
                emit_wiki_log("compile_skip", node_id, &reason)?;
                return Ok(CompileResult::Skipped);
            }
            _ => {}
        }

        // Step 1: Analyze
        let sources = load_sources(node_id)?;
        let timeline = load_timeline(node_id)?;
        let previous = load_compiled_truth(node_id)?;
        let analysis: StructuredAnalysis = self.run_analyze(
            &sources, &timeline, &previous, decision
        ).await?;

        // 缓存 Step 1 结果
        analysis_cache::put(node_id, &analysis, sha256_of_inputs)?;

        // Step 2: Generate
        let template = load_template_for_subkind(node_id)?;
        let GenerateOutput { compiled_truth, paragraph_source_map, confidence, status_proposal } =
            self.run_generate(&analysis, &template).await?;

        // 落库:create_version(content=compiled_truth),metadata 更新 confidence/provenance/inferredParagraphs
        create_version_with_provenance(
            node_id, compiled_truth, paragraph_source_map, confidence
        )?;

        // 写 log + 触发可能的 review item
        emit_wiki_log("compile_run", node_id, ...)?;
        if has_high_inferred_ratio(&paragraph_source_map) {
            review_queue::create_inferred_review(node_id, ...)?;
        }

        Ok(CompileResult::Compiled { /* stats */ })
    }
}
```

### 5.3 Step 1 / Step 2 Prompt 长什么样

**Step 1 (Analyze) prompt:**

```
You are analyzing source documents to extract a structured representation
for downstream compilation. Output ONLY valid JSON.

Sources:
{{for each source}}
[SOURCE id={uuid} title="{title}"]
{full content of source's compiled_truth}
[/SOURCE]
{{end}}

Existing compiled_truth (may be empty):
{{previous}}

Recent timeline events:
{{timeline last 20 entries}}

Output schema:
{
  "claims": [
    {
      "text": "Atomic claim sentence",
      "source": "uuid-of-source" | "inferred" | "timeline",
      "confidence": 0.0~1.0,
      "section": "summary" | "current_status" | "relationships" | "open_questions" | ...
    }
  ],
  "entities_mentioned": [{"name": "...", "type": "person|company|concept"}],
  "contradictions": [
    {
      "between_sources": ["uuid-a", "uuid-b"],
      "claim_a": "...",
      "claim_b": "..."
    }
  ],
  "gaps": ["List of things mentioned but not explained"],
  "overall_confidence": 0.0~1.0
}
```

**Step 2 (Generate) prompt:**

```
You are compiling a {{subkind}} wiki page. Write Markdown following the template.

Structured analysis (DO NOT exceed information in this JSON):
{{analysis JSON}}

Template (required sections):
{{template sections}}

Output format: TWO parts separated by ===PARAGRAPH-MAP===:

Part 1: The Markdown body (top-level paragraphs/headings only)

Part 2: paragraphSourceMap as JSON:
{
  "1": "source:uuid:claim-index" | "timeline:entry-index" | "inferred",
  "2": "...",
  ...
}

Rules:
- Every Markdown paragraph index (1-based) MUST have an entry
- If a paragraph blends multiple sources, list comma-separated
- Mark "inferred" ONLY when you have to bridge claims without direct support
```

### 5.4 Step 1 结果可独立消费

`analysis_cache::put(node_id, analysis, ...)` 把 Step 1 结果存下来 —— 这个 JSON 本身是有价值的:
- `memory_lint` 可以拿 `analysis.contradictions` 直接生成 review item
- `gap` 类型 EntityPage 可以从 `analysis.gaps` 自动派生候选
- Adaptive RAG 的 Query Classifier(§6)用 Step 1 缓存命中率作为"这个 entity 是否已被深入分析过"的信号

---

## 6. Adaptive RAG —— Query Classifier 路由

Tommy 图 7 的核心:**不同问题类型应该走不同管线**。简单事实问题(single-hop)用 vanilla RAG;多跳推理用 GraphRAG;主题综述用 LLM Wiki(直接读综合页)。一刀切的检索策略是浪费——简单问题过慢,复杂问题过浅。

### 6.1 分类信号

`query_classifier.rs` 输入用户 query,输出 `QueryClass`:

```rust
pub enum QueryClass {
    SingleHop {              // "VeRL 作者是谁?" "memU bridge 用什么协议?"
        primary_entity_hint: Option<String>,
    },
    MultiHop {               // "Zhang San 现在做的项目跟 RAG 有什么关系?"
        seed_entity_hints: Vec<String>,
        max_depth: u8,
    },
    TopicSynthesis {         // "综述当前 uClaw memory 设计现状" "对比 V31 跟 V11 trigram"
        topic_hint: Option<String>,
        scope: SynthesisScope, // Local / Global
    },
}
```

分类逻辑用 Haiku(便宜),提示词模板:

```
Classify the user query into ONE of:
- "single_hop":需要 1-2 个事实回答
- "multi_hop":需要沿实体关系链推理
- "topic_synthesis":需要跨多个 entity/source 综合,问"综述/对比/演变"

Output JSON:
{
  "class": "single_hop" | "multi_hop" | "topic_synthesis",
  "hop_count": 1~5,
  "entity_hints": ["..."],
  "scope": "local" | "global",
  "rationale": "one-line why"
}

Query: {{user_query}}
```

### 6.2 三条管线对应实现

| QueryClass | uClaw 路径 |
|---|---|
| **SingleHop** | 现有 `recall.rs::hybrid_search`(FTS + vector + RRF + boost)→ Top-3 直接返回 |
| **MultiHop** | 先用 entity_hints 解析 seed node → `graph_propagation_search`(BFS depth=max_depth,边类型过滤为 `WorksAt/Founded/InvestedIn/Source/Mentions`)→ 沿路径收集 EntityPage 的 compiled_truth → 拼为 multi-hop context |
| **TopicSynthesis** | 优先返回 `wiki_artifacts(kind="synthesis")` 里跟 topic 匹配的页面;若无现成 synthesis → 触发 `wiki_compile.rs` 即时创建 `EntityPage(subkind=synthesis)` 并落库 |

### 6.3 路由代码骨架

```rust
pub async fn adaptive_recall(query: &str, ctx: &RecallContext) -> RecallResult {
    let class = query_classifier::classify(query).await?;
    
    emit_wiki_log("query_classified", query, &class)?;

    match class {
        QueryClass::SingleHop { primary_entity_hint } => {
            recall::hybrid_search(query, ctx).await
        }
        QueryClass::MultiHop { seed_entity_hints, max_depth } => {
            let seeds = resolve_entity_hints(&seed_entity_hints, ctx)?;
            recall::graph_propagation_recall(seeds, max_depth, query, ctx).await
        }
        QueryClass::TopicSynthesis { topic_hint, scope } => {
            if let Some(synth) = find_existing_synthesis(&topic_hint, scope, ctx)? {
                return RecallResult::Synthesis(synth);
            }
            let new_synth = wiki_compile::synthesize_topic(&topic_hint, scope, ctx).await?;
            RecallResult::Synthesis(new_synth)
        }
    }
}
```

### 6.4 与现有 `recall.rs` 的关系

`adaptive_recall` 是 `recall.rs::hybrid_search` 的**上层包装**,不替换它。可通过 `memubot_config.adaptive_recall_enabled = false` 关闭,退化为现状(全部走 SingleHop 路径)。

---

## 7. 集成设计 —— 跟 Foundation Spec 的边界

| 字段 / 表 / 模块 | 来源 | 状态 |
|---|---|---|
| `MemoryNodeKind::EntityPage` | Foundation Phase 1 | **不动**,本 spec 通过 `metadata.subkind` 扩展语义 |
| EntityPage `metadata_json` 的 `compiled_truth` + `timeline` + `aliases` + `contradictions` | Foundation Phase 1 | **不动**,本 spec 新增 `confidence` / `status` / `provenanceState` / `contradictedBy` / `inferredParagraphs` / `paragraphSourceMap` / `subkind` / `tags` / `summary` 字段 |
| `MemoryRelationKind` 7 个新 typed-edge | Foundation Phase 2 | **不动** |
| `memory_edge_audit` 表 | Foundation Phase 1(V35) | **不动**,Cognitive 不引入新 audit 类型 |
| `wiki_artifacts` 表 | Foundation Phase 1(V35) | **复用**:新 kinds (`hot`, `purpose`, `log_index`) 加进去,无 schema 变化 |
| `memory_health_findings` 表 | Foundation Phase 1(V35) | **不动**,但 review_queue_items 是它的更高一级抽象(health findings 是问题清单,review queue 是带工作流的待办) |
| **`wiki_log_events`** | 本 spec 新表 | V43 |
| **`page_content_hashes`** | 本 spec 新表 | V43 |
| **`review_queue_items`** | 本 spec 新表 | V43 |
| **`wiki_page_templates`** | 本 spec 新表 + seed 数据 | V43 |
| **`analysis_cache`** | 本 spec 新表(Step 1 缓存) | V43 |
| `recall.rs` | Foundation Phase 5 | **扩展**:在现有 score 公式上叠加 `status_mult` + `confidence` + `provenanceState` penalty |
| `wiki_overview` scenario | Foundation Phase 3 | **不动**,但本 spec 加 `wiki_hot` 兄弟 scenario |
| `wiki_compile.rs` | 本 spec 新模块 | 替换 Foundation Phase 3 的"直接调 LLM 重写 compiled_truth"路径,改走两步 compile + 增量缓存 |
| `query_classifier.rs` | 本 spec 新模块 | 包在 `recall.rs` 之上,可灰度 |

### 7.1 数据模型变更总览(V43)

```sql
-- V43: Cognitive Layer

-- 5.1 Wiki-level event ledger
CREATE TABLE IF NOT EXISTS wiki_log_events (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    subject_id  TEXT,
    actor       TEXT NOT NULL,
    payload_json TEXT,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_time ON wiki_log_events(space_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_subject ON wiki_log_events(subject_id);

-- 5.2 SHA-256 incremental compile cache
CREATE TABLE IF NOT EXISTS page_content_hashes (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    sources_hash     TEXT NOT NULL,
    timeline_hash    TEXT NOT NULL,
    compiled_hash    TEXT NOT NULL,
    last_compiled_at INTEGER NOT NULL,
    last_skip_count  INTEGER NOT NULL DEFAULT 0,
    skip_reason      TEXT
);

-- 5.3 Review queue (human-in-the-loop)
CREATE TABLE IF NOT EXISTS review_queue_items (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    item_kind       TEXT NOT NULL,
    severity        TEXT NOT NULL,
    subject_ids     TEXT NOT NULL,
    title           TEXT NOT NULL,
    context_json    TEXT,
    status          TEXT NOT NULL DEFAULT 'open',
    resolution      TEXT,
    resolution_note TEXT,
    assignee        TEXT,
    created_at      INTEGER NOT NULL,
    resolved_at     INTEGER,
    snooze_until    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_review_queue_active 
    ON review_queue_items(space_id, status, severity, created_at);
CREATE INDEX IF NOT EXISTS idx_review_queue_subject ON review_queue_items(subject_ids);

-- 5.4 Per-subkind page templates (seed 7 rows)
CREATE TABLE IF NOT EXISTS wiki_page_templates (
    subkind         TEXT PRIMARY KEY,
    display_name    TEXT NOT NULL,
    compile_prompt  TEXT NOT NULL,
    sections_json   TEXT NOT NULL,
    ui_card_layout  TEXT,
    updated_at      INTEGER NOT NULL
);

-- 5.5 Two-stage compile: Step 1 cache
CREATE TABLE IF NOT EXISTS analysis_cache (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    inputs_hash      TEXT NOT NULL,            -- sha256(sources + timeline + previous_compiled)
    analysis_json    TEXT NOT NULL,
    llm_model        TEXT,
    token_cost       INTEGER,
    created_at       INTEGER NOT NULL
);
```

### 7.2 与现有 V1-V42 的兼容性

完全 additive,无任何 ALTER TABLE。所有 V43 新表用 `IF NOT EXISTS`。Foundation Spec 的 V35 表(memory_edge_audit / wiki_artifacts / memory_health_findings)被 Cognitive 复用,不修改其 schema(只是写入更多 kinds)。

---

## 8. 7 个 Phase 总览(详见 plan 文档)

| Phase | 角色 | 内容 | 涉及迁移 |
|---|---|---|---|
| Phase 8 | Schema(OS) | 9 种 subkind + frontmatter 8 keys 硬约束 + wiki_page_templates seed | V43(部分) |
| Phase 9 | Schema(OS) | Page-level provenance(confidence / status / provenanceState / contradictedBy / inferredParagraphs / paragraphSourceMap)+ 召回排序权重接入 | — |
| Phase 10 | LLM(Compiler) | wiki_compile.rs 两步 compile pipeline + analysis_cache + paragraph map | V43(部分) |
| Phase 11 | LLM(Compiler) | SHA-256 incremental compile + page_content_hashes | V43(部分) |
| Phase 12 | Wiki(Product) | hot.md + purpose.md + log.md 三个控制面文件 + WikiLogView | V43(部分) |
| Phase 13 | Review(Brake) | review_queue_items + ReviewQueuePanel + 召回打折 + Agent propose resolution | V43(部分) |
| Phase 14 | Chat(Entry) / Graph(Nav) | Adaptive RAG + Query Classifier + 多管线路由 | — |

每个 Phase 是一个独立可合并 PR,bisectable commits,匹配 `superpowers:subagent-driven-development` 范式。

---

## 9. Risk Assessment

### 9.1 LLM 成本风险

**风险:** 两步 compile + 段落 source map 让单页 compile token 翻倍(~30% 实测,论文数据)。
**缓解:**
- SHA-256 增量编译(Phase 11)在稳态下命中率应 ≥ 90%,**实际 LLM 调用 ≪ 一倍**
- analysis_cache(Step 1 缓存)被 lint scenario 复用,不重复跑分析
- `memubot_config.cognitive_compile_daily_token_budget`(默认 200k tokens/day)硬封顶,接入 `cost_records`

### 9.2 数据迁移风险

**风险:** Phase 9 引入 provenance 字段后,Foundation 阶段创建的 EntityPage 没有这些字段,UI 渲染要兼顾"老页 + 新页"。
**缓解:**
- 所有新字段 default 给 `Option` / 空数组,旧页解析不报错
- WikiView 在显示老页时,confidence/status 显示 "—"(未评估),提供"Run analysis" 按钮触发一次 backfill compile

### 9.3 Review Queue 过载风险

**风险:** 早期上线,memory_lint + auto_link + tier_escalator 可能一次产生 100+ review items,用户被淹。
**缓解:**
- Phase 13 默认 severity 阈值偏严(只产 `high` severity 的 phantom_entity / contradiction)
- `auto_dismiss_after_days = 30`,低 severity 旧 item 自动消失
- ReviewQueuePanel 默认按 severity 排,且 paginated

### 9.4 Adaptive RAG 路由错误

**风险:** Query Classifier 可能把"综述类问题"误判为 SingleHop,导致检索不全。
**缓解:**
- 每个 QueryClass 在 wiki_log_events 留痕,后续可以离线评估准确率
- 用户可在 chat 加 `/multi-hop` `/synthesis` 强制 hint
- 灰度 flag `adaptive_recall_enabled` 默认 false,稳定后开

### 9.5 跟现有 Memorization Service / Proactive Service 的冲突

**评估:** Phase 8-14 新增 4 个 scenarios(wiki_hot / wiki_compile_runner / review_queue_processor / query_classifier_metrics),都是**只读 + 写新表**,不竞争现有 4 个 scenario 的资源。具体:
- wiki_hot 只读 memory_nodes/timeline/log,写 wiki_artifacts(kind=hot)
- wiki_compile_runner 只读 sources/timeline,写 memory_versions + analysis_cache + page_content_hashes
- review_queue_processor 只读 memory_health_findings/contradictions,写 review_queue_items
- query_classifier 只在 query 时同步调用,不在后台跑

### 9.6 前端复杂度

**风险:** WikiView 渲染要支持 9 种 subkind,加 confidence/status badge,加 inferredParagraphs 视觉标注,加 review queue 入口——容易过载。
**缓解:**
- Phase 12 引入"View Mode Switch":Simple(只显示 compiled_truth)/ Rich(显示所有 provenance 信号)/ Edit(可编辑模式),默认 Simple
- 主题 token only(`bg-popover` / `text-muted-foreground` / `border-border`),确保 11 个主题下都正常

---

## 10. Future Work

1. **可视化 paragraph map 编辑器** — 用户可以直接拖段落,改 `paragraphSourceMap` 标注(对发现的 LLM 错误标注做反馈)
2. **跨 wiki 的 cross-link** — 多个 space 间的 EntityPage 可共享同一 entity(`MemoryVisibility::Shared` 现有 enum 终于派上用场)
3. **Knowledge Graph Embedding** — 在 memory_edges 上跑 TransE / RotatE,做 link prediction(补 phantom edges)
4. **Episodic compression** — 旧 Episode 在 wiki_compile 把 timeline 编进 compiled_truth 后,定期 archive 到冷存储
5. **Adaptive RAG 自我训练** — 用户对召回结果的 feedback(点 like/dislike)反过来训 query_classifier 的分类边界
6. **Wiki diff & merge UI** — 跨 device 的 markdown sync(Phase 7)冲突可以在 review queue 里以 diff 视图解决
7. **Confidence calibration** — 用历史 review 结果做 confidence 校准(LLM 给 0.9 但实际人工拒绝率 50% → 校准函数)

---

## Appendix A —— 与 Foundation Spec 的字段索引

| 字段 | 定义位置 | 当前 spec 引用 |
|---|---|---|
| `MemoryNodeKind::EntityPage` | Foundation §4.2.1 | 本 spec §2 复用,加 subkind |
| `metadata_json.compiled_truth` | Foundation §4.2.2 | 本 spec §3.3 加 paragraphSourceMap 关联 |
| `metadata_json.timeline` | Foundation §4.2.2 | 本 spec §5 wiki_compile 输入 |
| `metadata_json.contradictions` | Foundation §4.2.2 | 本 spec §3.5 扩展为双向 |
| `MemoryRelationKind 7 typed` | Foundation §4.2.3 | 本 spec §6.2 multi-hop 路径过滤 |
| `wiki_artifacts.kind` | Foundation §4.2.4 | 本 spec §4.1 加 hot/purpose/log_index |
| `memory_health_findings` | Foundation §4.2.4 | 本 spec §4.6 review_queue 是其更高抽象 |
| `recall.rs::compute_recall_score` | Foundation §4.3.2 | 本 spec §3.6 扩展公式 |
| `wiki_overview` scenario | Foundation §4.3.x | 本 spec §4.2 加 wiki_hot 兄弟 |

## Appendix B —— Tommy 8 张图覆盖度终检

| Tommy 要点 | 实现位置 |
|---|---|
| 8 文件控制面 | §4 全节(全部 8 个落到 wiki_artifacts 或新表) |
| 9 种页面类型 | §2.1 全表 + §2.2 模板 |
| 8 keys frontmatter 硬约束 | §3.1 |
| confidence(0~1) | §3.1 + §3.6 召回权重 |
| status: verified/draft/inferred/disputed | §3.2 |
| provenanceState: full/partial/none | §3.4 |
| contradictedBy 双向 | §3.5 |
| inferredParagraphs 段落级 provenance | §3.3 + §5 两步 compile 产出 |
| paragraphSourceMap | §3.3 + §5 |
| 两步 LLM compile(Analyze→Generate) | §5 全节 |
| SHA-256 增量编译 | §4.5 + §7.1 page_content_hashes |
| atomicmemory 模式 | §4.5 should_recompile 决策树 |
| BM25 + vector + graph → RRF | Foundation Phase 5 |
| Adaptive RAG / Query Classifier | §6 全节 |
| LLM=Compiler 心智模型 | §1 + §5 |
| Chat=Entry / Wiki=Product / Graph=Nav / Schema=OS / Review=Brake / Log=Audit | §1 全节 + Phase 编排按 7 角色组织 |

✅ 8 张图覆盖率:**100%(11 个红色项 + 6 个黄色项全部纳入设计)**
