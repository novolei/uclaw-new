# Agent Memory OS — Engines Layer 设计(Entity Graph · Timeline · Dream Cycle)

> **🟡 STATUS: PARTIALLY PAUSED (2026-05-20)** — 见 [ADR 2026-05-20 §8](../../adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md)。
>
> 重叠审计发现 L3 跟 gbrain 不是一刀切重叠。决策(按 ADR §8):
>
> | L3 组件 | 状态 | 理由 |
> |---|---|---|
> | **Entity Graph Engine** (NER + alias + coreference) — Phase 15 | 🛑 PAUSED | gbrain `chat_extractor` + `maintain` skill 已覆盖 regex NER + alias dict + Haiku 消歧 + 周末 dedup。Coreference 是 anti-pattern。 |
> | **Timeline Engine** (全局 timeline + 日/周/月聚合) — Phase 16 | ✅ RETAINED | gbrain 只有 per-page timeline,没全局 timeline、没聚合循环。standalone-worthy。 |
> | **Dream Cycle 8 阶段** — Phase 17 | 🛑 PAUSED | 阶段①-⑥重叠 gbrain 6 阶段;⑦⑧ 折进具体消费方,不复用整套 pipeline。 |
> | **§4.12.1 Importance-Aware Decay** | ✅ RETAINED | gbrain 无 decay 算法;知识库卫生关键。 |
> | **§4.12.2 Hypothesis Generation** | 🛑 PAUSED | 边际价值低。 |
> | **§4.12.3 Spaced Repetition (Anki SM-2)** | ✅ RETAINED | 学习科学验证;gbrain 无等价。 |
> | **§4.12.4 Concept Drift Detection** | ✅ RETAINED | 跨版本矛盾发现。 |
> | **§4.12.5 Cross-Source Triangulation** | ✅ RETAINED | 多源置信度核对。 |
> | **§4.12.6 Predictive Boot Preparation** | 🛑 PAUSED | 优化级。 |
> | **§4.12.7 Synthetic Q&A** | 🛑 PAUSED | token 成本高,信号低。 |
>
> **执行规则:** 仅 RETAINED 项目开放实施。spec 中所有 V36 引用应改读为 V44(下一空闲号,V36 已被 Automation Phase 2b 占用)。

**Date:** 2026-05-18
**Status:** Partially Paused (was: Draft) — see ADR 2026-05-20 §8
**Layer position:** **Engines Layer(Phase 15-21)** —— 三层 Memory OS 设计的第三层。
- **Foundation Layer(Phase 1-7)**:[`2026-05-18-agent-memory-os-design.md`](2026-05-18-agent-memory-os-design.md)
- **Cognitive Layer(Phase 8-14)**:[`2026-05-18-agent-memory-os-cognitive-design.md`](2026-05-18-agent-memory-os-cognitive-design.md)
- **Engines Layer(本文,Phase 15-21)**:三个高阶引擎补完整体能力。
**Companion plan:** [`docs/superpowers/plans/agent-memory-os-engines.md`](../plans/agent-memory-os-engines.md)
**Inspired by:** [garrytan/gbrain](https://github.com/garrytan/gbrain)(BrainEngine API + dream cycle + entity-as-page + raw sidecar);Karpathy 的 LLM agent memory 论述;spaced repetition / Anki SM-2 算法;Anthropic 的 "memory architecture" 蓝图;有关 importance-weighted forgetting 的认知科学。

---

## 0. 本 Spec 与前两层的关系

| 层 | 关键能力 | 完工后系统能做什么 |
|---|---|---|
| **Foundation** | EntityPage 节点 / Auto-link / Wiki view / Health 扫描 | "我有一个会自动建图的实体级长期记忆" |
| **Cognitive** | 9 种 page type / 段落级 provenance / 两步 compile / Review brake / Adaptive RAG | "我的 wiki 知识可信、可审计、可分级、可路由" |
| **Engines** | NER + alias resolution / 全局 Timeline / Dream Cycle 8 阶段 + 7 项高级增强 | **"Agent 有时间观念、会自动夜间整理、知道自己不知道什么、会做记忆巩固"** |

**承诺继续:不破坏前两层。** Engines 层全部叠加在 Foundation+Cognitive 之上,通过新表(V44)和新 scenario 实现,**不修改任何已合并的 V1–V43 schema**。(原 spec 写的 V36 是过时假设 — 实际 Foundation 已用到 V42、Cognitive 用 V43、V36 是 skipped 空号(Phase 7 抢占 V37 时跳号);V44 是当前下一空闲。)

---

## 1. 设计哲学:从"记忆容器"到"认知有机体"

Foundation+Cognitive 让 uClaw 拥有了**结构化、可信、可路由**的长期记忆。但它还是被动的——只有用户/Agent 主动调用,它才工作。

**Engines Layer 让记忆系统**主动**起来:**

- **NER 主动扫描** —— Agent/用户写下"昨天跟 John 聊了 OpenAI",系统自己识别 John 和 OpenAI,自动建图,**不要求显式 `[[entity:...]]` 标记**
- **Timeline 主动聚合** —— 系统每天自己写"今天发生了什么"的摘要,周日写"本周回顾",月底写"月度回顾"
- **Dream Cycle 主动巩固** —— 每天凌晨 3 点,系统自己跑 8 阶段批处理:扫对话 → 抽实体 → 摘要会话 → 合并重复 → 沉淀长期 → 清除低价值 → 更新向量 → 刷新图边。**就像人睡觉时大脑做的事**

这是从"记忆容器"到"认知有机体"的本质跃迁:**系统不只是存储知识,系统**积极地**在维护和提升知识**。

---

## 2. Entity Graph Engine

Foundation Phase 2 的 auto-link 解决了**显式引用**的自动建图。Cognitive Phase 9 的双向 contradicted_by 解决了**矛盾追踪**。本层的 Entity Graph Engine 解决剩下的关键问题:**当文本里只有"John"、"那家公司"、"那个项目",怎么把它们链回正确的 EntityPage?**

### 2.1 三个子能力

| 子能力 | 输入 | 输出 | 实现位置 |
|---|---|---|---|
| **NER(命名实体识别)** | 自然语言文本(Episode / agent_messages) | `Vec<MentionCandidate>`(span + 候选 entity 列表 + 置信度) | `proactive/scenarios/entity_recognition.rs` |
| **Alias Resolution** | 一个 MentionCandidate | 命中已有 EntityPage 或 "create new" 决定 | `memory_graph/alias_resolver.rs` |
| **Coreference Resolution** | 同一文档内多个 mention("John … he … the engineer") | 把它们绑到同一个 entity | `memory_graph/coreference.rs` |

### 2.2 NER 实现策略

**两路融合:**

1. **Regex / Pattern 路径(零 LLM,先跑)**
   - 大写专有名词(`[A-Z][a-z]+(?:\s+[A-Z][a-z]+)*`)
   - 已知 alias 字典(从 `entity_aliases` 表加载,Aho-Corasick 多模式匹配)
   - URL / email / handle 模式(`@username`, `https://...`)
   - 引号包围的术语("RAG", "GraphRAG")
2. **LLM 路径(消除歧义时)**
   - 当 regex 抽出 candidate 但 alias 查表无命中或多义时
   - 调 Haiku 用 prompt:"判断以下 candidates 是 person/company/concept 还是普通名词,并给出 confidence"
   - 缓存 `(text, candidate) → result` 减少重复调用

**抽取后处理:**

```rust
pub struct MentionCandidate {
    pub text_span: (usize, usize),         // 文本起止偏移
    pub raw_text: String,                  // "John from OpenAI"
    pub entity_hint: Option<String>,       // 解析出的核心名 "John"
    pub modifier_hint: Option<String>,     // 解析出的修饰 "from OpenAI"
    pub kind_guess: EntityKindGuess,       // Person / Company / Concept / Unknown
    pub confidence: f32,                   // 0~1
    pub source_node_id: String,            // 这条 mention 来自哪个 node
}
```

### 2.3 Alias Resolution 算法

```rust
pub fn resolve_mention(mention: &MentionCandidate, ctx: &ResolveContext) -> Resolution {
    // 1. 字典精确命中(O(1) lookup on entity_aliases.alias)
    if let Some(node_id) = ctx.alias_index.exact_lookup(&mention.entity_hint?) {
        return Resolution::Existing { node_id, confidence: 1.0 };
    }
    
    // 2. 字典模糊命中(Levenshtein ≤ 2 或 Jaro-Winkler ≥ 0.92)
    let fuzzy = ctx.alias_index.fuzzy_lookup(&mention.entity_hint?, 0.92);
    if fuzzy.len() == 1 {
        return Resolution::Existing { 
            node_id: fuzzy[0].node_id.clone(), 
            confidence: fuzzy[0].similarity 
        };
    }
    if fuzzy.len() > 1 {
        // 多个相似 alias —— 用 modifier_hint 消歧
        // 比如 "John (from OpenAI)" + modifier="OpenAI" → 优先选 works_at=OpenAI 的 John
        let scored = disambiguate_with_modifier(&fuzzy, &mention.modifier_hint, ctx);
        if scored[0].score > scored[1].score * 1.5 {
            return Resolution::Existing { node_id: scored[0].node_id, confidence: 0.7 };
        }
        // 仍歧义 → review queue
        return Resolution::AmbiguousNeedsReview { candidates: scored };
    }
    
    // 3. 完全未命中 —— phantom slug 候选
    if mention.confidence > 0.7 {
        // 高置信度 → 创建 review_queue_items(kind='phantom_entity')
        return Resolution::PhantomCandidate { 
            suggested_slug: slugify(&mention.entity_hint?),
            evidence_node_ids: vec![mention.source_node_id.clone()],
        };
    }
    
    // 4. 低置信度 → 静默忽略
    Resolution::Skip { reason: "low-confidence + no-match" }
}
```

**phantom_entity review_queue 项**(Cognitive Phase 13 已建)的累积逻辑:
- 第 1 次出现 → 创 item with severity=low, evidence_node_ids=[a]
- 第 2 次出现 → update item, evidence_node_ids=[a, b], severity=med
- 第 3+ 次 → severity=high(明显应该建 page 了)
- 用户 Accept → 自动创建 EntityPage,把所有 evidence_node_ids 的 source 文本里的 mention 替换为 `[[entity:slug]]`,触发 auto-link

### 2.4 Coreference Resolution

同一文档内:
```
"John mentioned the new search infra. He thinks the engineer team underestimated complexity."
```
"John", "He" 都指同一人。

实现:**调 Haiku 一次**,prompt 模板:

```
Identify coreference clusters in the following text. Output JSON.

Text:
{full text}

Already-extracted mentions:
{JSON of MentionCandidate list}

Output:
{
  "clusters": [
    {
      "canonical": "John",
      "mentions": ["John (offset 0)", "He (offset 35)"]
    }
  ]
}
```

每个 cluster 内的所有 mention 共享同一个 resolved entity_id。**注意:跨文档不做 coreference**——那是 alias 表的职责,不要混淆。

### 2.5 数据模型(V44 部分)

```sql
-- 全局 alias 索引(快查 + 模糊匹配)
CREATE TABLE IF NOT EXISTS entity_aliases (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    node_id     TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    alias_lower TEXT NOT NULL,             -- alias 的 lowercased + normalized,加速查找
    weight      REAL NOT NULL DEFAULT 1.0, -- 主名 1.0,变体 < 1.0
    source      TEXT NOT NULL,             -- 'declared' (frontmatter) | 'inferred' (NER) | 'user'
    created_at  INTEGER NOT NULL,
    UNIQUE(space_id, alias_lower, node_id)
);
CREATE INDEX IF NOT EXISTS idx_entity_aliases_lookup ON entity_aliases(space_id, alias_lower);
CREATE INDEX IF NOT EXISTS idx_entity_aliases_node ON entity_aliases(node_id);

-- 模糊匹配辅助:trigram 索引 alias_lower(已有 memory_fts 的 trigram tokenizer pattern)
CREATE VIRTUAL TABLE IF NOT EXISTS entity_aliases_fts USING fts5(
    alias_id UNINDEXED,
    alias_lower,
    tokenize='trigram'
);

-- per-source 原始数据 sidecar(gbrain 的 .raw/ 翻版)
CREATE TABLE IF NOT EXISTS entity_raw_data (
    id          TEXT PRIMARY KEY,
    node_id     TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,             -- 'linkedin' | 'twitter' | 'github' | 'email' | 'custom'
    source_url  TEXT,
    raw_json    TEXT NOT NULL,             -- 原始 API 响应或解析前内容
    fetched_at  INTEGER NOT NULL,
    expires_at  INTEGER,                   -- 可选 TTL
    UNIQUE(node_id, source_kind)
);
CREATE INDEX IF NOT EXISTS idx_entity_raw_data_node ON entity_raw_data(node_id);

-- NER 决策审计(每次 NER scenario 跑的 candidate + 决策)
CREATE TABLE IF NOT EXISTS ner_decisions (
    id              TEXT PRIMARY KEY,
    source_node_id  TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    raw_text        TEXT NOT NULL,
    resolution_kind TEXT NOT NULL,         -- 'existing' | 'phantom_created' | 'ambiguous' | 'skipped'
    resolved_node_id TEXT,                  -- 命中的 EntityPage
    confidence      REAL,
    rationale       TEXT,                   -- 简短理由
    review_item_id  TEXT,                   -- 如果走了 review 流程
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ner_decisions_source ON ner_decisions(source_node_id);
```

### 2.6 Entity Graph 查询 API(`graph_query.rs`)

Foundation 的 `graph_propagation_search` 是单个函数,参数稀疏。本 Engines 层加一套**结构化查询 API**:

```rust
pub struct GraphQuery {
    pub seeds: Vec<SeedSpec>,                       // 起点
    pub edge_filter: Option<EdgeFilter>,            // 边类型/方向/属性
    pub node_filter: Option<NodeFilter>,            // 节点 kind/subkind/tag
    pub depth_limit: u8,                            // 最大跳数(default 3)
    pub max_nodes: usize,                           // 最多返回节点数(default 50)
    pub scoring: ScoringMode,                       // 'propagation' | 'shortest_path' | 'centrality'
    pub include_paths: bool,                        // 返回路径而非仅节点
}

pub enum SeedSpec {
    NodeId(String),
    EntitySlug(String),                              // 走 alias resolve
    QueryText(String),                               // 走 FTS 找 seed
}

pub enum EdgeFilter {
    KindIn(Vec<MemoryRelationKind>),
    KindNotIn(Vec<MemoryRelationKind>),
    DirectionOut, DirectionIn, DirectionBoth,
    MinConfidence(f32),                              // 走 memory_edge_audit.confidence
}

pub struct GraphQueryResult {
    pub nodes: Vec<ScoredNode>,
    pub edges: Vec<EdgeRef>,
    pub paths: Option<Vec<Path>>,                    // 如果 include_paths=true
    pub stats: QueryStats,                           // 命中数、查询耗时、剪枝原因
}
```

实现要点:
- 用递归 CTE(SQLite WITH RECURSIVE)+ visited[] 防环 + depth 限制(参考 gbrain `traverseGraph`)
- `centrality` 模式跑 weighted PageRank(简化版,k=10 迭代),用于"这个图里哪个 entity 最重要"
- 查询结果可缓存到内存 LRU(空间外不持久化,避免和 dream cycle 冲突)
- 暴露 tauri command `memory_graph_query(query) → result`

### 2.7 Backlinks API

简单但缺失:**给定一个 EntityPage,谁指向它?**

```rust
pub fn list_backlinks(node_id: &str, ctx: &Ctx) -> Vec<Backlink> {
    // SELECT memory_edges WHERE child_node_id = ?
    // LEFT JOIN memory_edge_audit ON edge_id
    // 返回 source page + relation_kind + audit.source + audit.confidence + context_text(从 source 节点 active version 抽 ±100 字符上下文)
}
```

前端在 EntityPage 渲染时,右侧栏新增 "References" 区,显示所有 backlinks。点击跳转。

### 2.8 NER scenario 触发节奏

- **on-create 事件驱动**:Episode / Reference 节点 create_version 后,emit 事件,NER scenario 立即处理(< 100ms)
- **批量回填**:`memory_ner_backfill` IPC 命令,把历史 N 天的节点重新扫一遍,用于刚装 NER 时一次性补图
- **节流**:同一节点 24h 内不重复跑 NER(用 `ner_decisions` 表的 `source_node_id + created_at` 查)

---

## 3. Timeline Engine

让 Agent 有**时间观念**。这是 uClaw 当前最大的认知缺口——`agent_messages` 有时间戳,但没有"过去两周做了什么"这种**叙事级时间能力**。

### 3.1 设计目标

回答以下 query 类型,且**召回质量 ≥ Adaptive RAG 在该类型上的水平**:

| Query 例子 | 期望行为 |
|---|---|
| "最近两周我一直在做什么?" | 列出过去 14 天的活动主题(按 frequency 聚类),引用关键 EntityPage 和 Episode |
| "上周三我跟 Garry 聊了什么?" | 定位到 2026-05-XX 跟 Garry 相关的 Episode/Session,引用具体内容 |
| "今年我学到了哪些新概念?" | 列出 status=verified 的 Concept-type EntityPage,created_at 在今年 |
| "把我五月份的工作总结一下" | 触发 May synthesis(月度回顾),如果未生成则即时生成 |

### 3.2 三个核心数据结构

#### 3.2.1 全局 `timeline_events` 表(V44 部分)

```sql
CREATE TABLE IF NOT EXISTS timeline_events (
    id            TEXT PRIMARY KEY,
    space_id      TEXT NOT NULL,
    event_kind    TEXT NOT NULL,           -- 'episode' | 'page_created' | 'page_promoted' | 'session_start' | 'review_resolved' | 'skill_learned' | 'custom'
    subject_id    TEXT,                    -- 涉及的 node_id / session_id 等
    title         TEXT NOT NULL,           -- 30 字内的事件描述
    payload_json  TEXT,                    -- 详情
    related_entity_ids TEXT,                -- JSON array,涉及到的 EntityPage UUIDs
    occurred_at   INTEGER NOT NULL,        -- 事件实际发生时间
    importance    REAL NOT NULL DEFAULT 0.5, -- 0~1,Dream Cycle 后会被更新
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_timeline_events_time 
    ON timeline_events(space_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_timeline_events_entity 
    ON timeline_events(related_entity_ids);
CREATE INDEX IF NOT EXISTS idx_timeline_events_kind 
    ON timeline_events(space_id, event_kind, occurred_at DESC);
```

**与现有 EntityPage.timeline 的关系:**

- `EntityPage.metadata.timeline[]` 是**该 entity 的本地时间线**,只关心"跟这个 entity 相关的事"
- `timeline_events` 是**全局时间线**,记录"系统在某时刻发生了什么"
- 两者同源:`timeline_events.related_entity_ids` 是反向索引,可派生出 entity 维度的视图

**与现有 `wiki_log_events`(Cognitive Phase 12)的区别:**

- `wiki_log_events` 是**审计日志**(每次 compile / lint / review 都记录),给系统内部用
- `timeline_events` 是**用户/Agent 关心的事件**(实际发生了什么,粒度更粗),给 query 用

#### 3.2.2 `temporal_aggregates` 表(每日/周/月预计算摘要)

```sql
CREATE TABLE IF NOT EXISTS temporal_aggregates (
    id            TEXT PRIMARY KEY,
    space_id      TEXT NOT NULL,
    grain         TEXT NOT NULL,           -- 'day' | 'week' | 'month' | 'quarter' | 'year'
    period_start  INTEGER NOT NULL,        -- 该 grain 的起始时间
    period_end    INTEGER NOT NULL,
    summary_md    TEXT NOT NULL,           -- LLM 写的 markdown 摘要
    event_count   INTEGER NOT NULL DEFAULT 0,
    top_themes    TEXT NOT NULL,           -- JSON array,主要主题
    top_entities  TEXT NOT NULL,           -- JSON array,涉及最多的 entity UUIDs
    llm_model     TEXT,
    token_cost    INTEGER,
    created_at    INTEGER NOT NULL,
    UNIQUE(space_id, grain, period_start)
);
CREATE INDEX IF NOT EXISTS idx_temporal_aggregates_lookup
    ON temporal_aggregates(space_id, grain, period_start DESC);
```

预计算时机(Dream Cycle 第 3 阶段触发):
- 每天凌晨跑前一天的 `grain=day` aggregate
- 每周日凌晨跑上周的 `grain=week` aggregate
- 每月 1 号凌晨跑上月的 `grain=month` aggregate
- 季度 / 年度同理

#### 3.2.3 `activity_clusters`(LLM 把同一窗口内的事件聚成主题)

```sql
CREATE TABLE IF NOT EXISTS activity_clusters (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    period_start    INTEGER NOT NULL,
    period_end      INTEGER NOT NULL,
    cluster_label   TEXT NOT NULL,        -- LLM 写的主题名,如 "uClaw memory OS 设计"
    description     TEXT NOT NULL,
    event_ids       TEXT NOT NULL,        -- JSON array,属于这个 cluster 的 timeline_event UUIDs
    related_entity_ids TEXT NOT NULL,
    score           REAL NOT NULL DEFAULT 0.5,  -- cluster 重要性
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_activity_clusters_period
    ON activity_clusters(space_id, period_start DESC);
```

### 3.3 Temporal Query Classifier(扩展 Cognitive Phase 14 的 Query Classifier)

Cognitive Phase 14 的 Query Classifier 把 query 分为 single-hop / multi-hop / topic-synthesis 三类。Engines 层**追加第四类:`temporal`**。

```rust
pub enum QueryClass {
    SingleHop { primary_entity_hint: Option<String> },
    MultiHop { seed_entity_hints: Vec<String>, max_depth: u8 },
    TopicSynthesis { topic_hint: Option<String>, scope: SynthesisScope },
    
    // NEW
    Temporal {
        time_range: TimeRange,                       // 解析出的时间区间
        focus: TemporalFocus,                        // What | Who | Where | How
        grain_hint: Option<TemporalGrain>,           // hint LLM 用什么粒度回答
        entity_filter: Option<Vec<String>>,          // 限定某个 entity 相关
    },
}

pub enum TimeRange {
    Absolute { start: i64, end: i64 },
    RelativeRecent { days: u32 },                    // "最近 N 天"
    RelativePast { unit: TimeUnit, count: u32 },     // "上 N 月"
    Calendar { year: i32, month: Option<u8> },       // "2026 年 5 月" / "今年"
}

pub enum TemporalFocus { What, Who, Where, How, Why }
pub enum TemporalGrain { Day, Week, Month, Quarter, Year }
```

**触发关键词**(中文 + 英文):
- 中文:`最近`、`过去`、`上(周|月|年)`、`这(周|月|年)`、`昨天`、`今天`、`X 天前`、`X 月`、`刚刚`
- 英文:`recent`、`past`、`last (week|month|year)`、`this (week|month|year)`、`yesterday`、`X days ago`、`In May`

LLM 解析模板(Haiku,~50 tokens):

```
Parse the temporal expression in the query and output JSON.

Query: {{user_query}}
Today: {{today_iso}}

Output:
{
  "is_temporal": true,
  "time_range": {
    "kind": "absolute" | "relative_recent" | "relative_past" | "calendar",
    ...
  },
  "focus": "what" | "who" | "where" | "how" | "why",
  "grain_hint": "day" | "week" | "month" | null,
  "entity_filter": ["..."] | null
}
```

### 3.4 Temporal Recall 管线

```rust
pub async fn temporal_recall(
    class: TemporalQueryClass,
    ctx: &RecallContext,
) -> RecallResult {
    let (start, end) = class.time_range.resolve(now())?;
    
    // 1. 拉时间窗口内的 timeline_events
    let events = ctx.store.list_events_in_range(start, end, class.entity_filter)?;
    
    // 2. 看是否有现成的 temporal_aggregate 命中
    let grain = pick_grain(start, end, class.grain_hint);
    if let Some(agg) = ctx.store.find_aggregate(grain, start, end)? {
        // 直接返回预计算摘要
        return RecallResult::TemporalDigest { 
            summary_md: agg.summary_md, 
            events_referenced: events,
            stats: AggregateStats::Cached,
        };
    }
    
    // 3. 没有现成 aggregate → 即时生成
    let clusters = compute_clusters(&events, ctx).await?;     // 可能 LLM,但小批量
    let summary = synthesize_period(&events, &clusters, class.focus, ctx).await?;
    
    // 4. 落库(下次同样的 query 可直接命中)
    ctx.store.upsert_aggregate(grain, start, end, &summary, &clusters)?;
    
    RecallResult::TemporalDigest { 
        summary_md: summary, 
        events_referenced: events,
        stats: AggregateStats::JustComputed,
    }
}
```

**"最近两周我一直在做什么?" 实际流程:**

1. Query Classifier 识别 `Temporal { time_range=RelativeRecent{14}, focus=What }`
2. `temporal_recall` 拉 14 天内 timeline_events(假设 200 条)
3. 检查 `grain=week, period=last-2-weeks` 是否有 aggregate → 命中(因为 dream cycle 每周日已经跑过)→ 返回缓存摘要
4. 如果没命中 → compute_clusters 把 200 条聚成 8 个 cluster("uClaw memory 设计"、"跟 Garry 沟通"、"Phase 14 实现" 等)→ LLM 写"过去两周你主要在:..."摘要

### 3.5 Timeline View 前端

新组件 `ui/src/components/memory/TimelineEngineView.tsx`:

```
┌────────────────────────────────────────────────────────────────┐
│ Timeline                                                        │
│ [Day] [Week] [Month] [Year]    [< 2026-05 >]    [Filter: ▾]    │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│ ▾ 2026-05-18 周一  · 12 events                                  │
│   ▾ 🧠 uClaw memory OS 设计(5 events,关联 EntityPage:Memory  │
│     OS / EntityPage 概念 / Tommy LLM Wiki)                      │
│     · 09:30 起草 cognitive spec ...                             │
│     · 11:15 添加 9 page type ...                                │
│     · 14:00 ...                                                 │
│   ▾ 💬 跟 Agent 对话(7 events, ...)                            │
│                                                                 │
│ ▾ 2026-05-17 周日  · 8 events                                   │
│   ▾ ...                                                         │
│                                                                 │
│ [Load more]                                                     │
└────────────────────────────────────────────────────────────────┘
```

- 时间粒度 toggle(Day / Week / Month / Year)
- 按 cluster 折叠分组
- 每个 cluster 展开看 events
- 每个 event 可点跳转到对应 node / session
- Filter 可按 event_kind / entity_id 过滤

### 3.6 Agent system prompt 加 "今日时间感知"

每个 agent session 起始,在 Boot context 注入:

```markdown
## Temporal Context
Today: 2026-05-18 (Monday)
This week's grain aggregate: see [[wiki:weekly-2026-W20]]
Recent active themes (last 7 days):
- uClaw memory OS 设计 (12 events)
- Agent self-evolution GEP (5 events)
- Plugin marketplace (3 events)
```

让 Agent 自带"现在是什么时候、最近在干嘛"的感知,无需 query 也能上下文相关。

---

## 4. Dream Cycle —— Daily Memory Consolidation

每日凌晨自动跑的批处理流水线。这是整个 Memory OS 最 ambitious 的组件——**让系统在用户睡觉时自己变聪明**。

### 4.1 整体架构

```
┌──────────────────────────────────────────────────────────────────┐
│                    DreamCycleOrchestrator                         │
│  (registered as a ProactiveService scenario, scheduled via cron)  │
└──────────────────────────────────────────────────────────────────┘
        │
        ▼
┌──────────────────────────────────────────────────────────────────┐
│  Run lifecycle:                                                   │
│  CREATE dream_cycle_runs row → status='running'                   │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  Stage Pipeline (each step in dream_cycle_stages table)  │    │
│  │   ① ScanConversations    ─→  events_scanned: 142         │    │
│  │   ② ExtractEntities      ─→  mentions: 87, new: 5        │    │
│  │   ③ SummarizeSessions    ─→  sessions: 8                 │    │
│  │   ④ MergeDuplicates      ─→  merges: 3                   │    │
│  │   ⑤ BuildLongTermMemory  ─→  promoted: 12                │    │
│  │   ⑥ RemoveLowValueMemory ─→  archived: 24                │    │
│  │   ⑦ UpdateEmbeddings     ─→  updated: 31                 │    │
│  │   ⑧ RefreshGraphEdges    ─→  reconciled: 18              │    │
│  └──────────────────────────────────────────────────────────┘    │
│  status='completed' OR 'failed' (with stage_failed_at)            │
└──────────────────────────────────────────────────────────────────┘
        │
        ▼
┌──────────────────────────────────────────────────────────────────┐
│  Advanced Enhancements (post-pipeline, 见 §4.10):                 │
│  · ImportanceDecay   · HypothesisGeneration                       │
│  · SpacedRepetition  · ConceptDriftDetection                      │
│  · Triangulation     · PredictiveBoot                             │
│  · SyntheticQA                                                    │
└──────────────────────────────────────────────────────────────────┘
```

### 4.2 数据模型(V44 部分)

```sql
-- 每次 Dream Cycle 运行的元数据
CREATE TABLE IF NOT EXISTS dream_cycle_runs (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    triggered_by    TEXT NOT NULL,         -- 'cron' | 'manual' | 'low-activity-detected'
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    status          TEXT NOT NULL,         -- 'running' | 'completed' | 'failed' | 'partial' | 'cancelled'
    stages_total    INTEGER NOT NULL,
    stages_completed INTEGER NOT NULL DEFAULT 0,
    error_message   TEXT,
    summary_json    TEXT,                  -- 整轮统计
    token_cost      INTEGER NOT NULL DEFAULT 0,
    duration_ms     INTEGER
);
CREATE INDEX IF NOT EXISTS idx_dream_cycle_runs_time 
    ON dream_cycle_runs(space_id, started_at DESC);

-- 每个 stage 的执行记录(细粒度,便于诊断 + 续跑)
CREATE TABLE IF NOT EXISTS dream_cycle_stages (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL REFERENCES dream_cycle_runs(id) ON DELETE CASCADE,
    stage_name      TEXT NOT NULL,         -- 'scan_conversations' | 'extract_entities' | ...
    stage_order     INTEGER NOT NULL,
    status          TEXT NOT NULL,         -- 'pending' | 'running' | 'completed' | 'failed' | 'skipped'
    started_at      INTEGER,
    finished_at     INTEGER,
    input_json      TEXT,                  -- 入参快照(便于重放)
    output_json     TEXT,                  -- 出参/统计
    checkpoint_json TEXT,                  -- 失败时的中间状态,用于续跑
    error_message   TEXT,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    token_cost      INTEGER NOT NULL DEFAULT 0,
    duration_ms     INTEGER
);
CREATE INDEX IF NOT EXISTS idx_dream_cycle_stages_run 
    ON dream_cycle_stages(run_id, stage_order);

-- 节点重要性分数(每日 Dream Cycle 更新)
CREATE TABLE IF NOT EXISTS memory_importance_scores (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    base_value       REAL NOT NULL,
    citation_factor  REAL NOT NULL,
    edge_factor      REAL NOT NULL,
    recency_factor   REAL NOT NULL,
    status_bonus     REAL NOT NULL,
    penalty          REAL NOT NULL,
    importance       REAL NOT NULL,        -- 综合分,0~1
    decay_half_life_days REAL NOT NULL,    -- 该节点的当前半衰期(可变)
    last_computed_at INTEGER NOT NULL,
    archive_pending_since INTEGER          -- 何时第一次低于 archive 阈值
);
CREATE INDEX IF NOT EXISTS idx_importance_scores_value 
    ON memory_importance_scores(importance DESC);
CREATE INDEX IF NOT EXISTS idx_importance_scores_archive 
    ON memory_importance_scores(archive_pending_since) 
    WHERE archive_pending_since IS NOT NULL;
```

### 4.3 调度机制

**默认时间:本地凌晨 3:00**(可配置 `memubot_config.dream_cycle_hour = 3`)。

**调度实现:**
- 不引入新 cron 库;复用 `ProactiveService::tick_inner`(每 30 秒)
- 每次 tick 检查当前本地时间是否进入了"应该运行 dream cycle 的窗口"(03:00 - 04:00)且今天还未运行
- 用 `dream_cycle_runs` 表的 `started_at` 字段做"今天是否跑过"判定:`SELECT COUNT(*) WHERE date(started_at, 'unixepoch', 'localtime') = date('now', 'localtime')`
- **额外触发条件:`low-activity-detected`** —— 如果连续 30 分钟没有 chat / agent 活动且最近 6 小时未运行,可触发"白天版"快速 dream cycle(只跑前 4 阶段)

**并发保护:**
- 用文件锁 `~/.uclaw/dream_cycle.lock`(参考 gbrain 的 `cycle.lock`)
- 同时 `dream_cycle_runs.status='running'` 行作为 DB 层的互斥
- 两道保险

### 4.4 Stage 1 - ScanConversations

**目的:** 扫描过去 24 小时(或上次成功 dream cycle 以来)的 conversation messages、agent_messages、agent_turns。

**输入:** `start_ts` = 上次 `dream_cycle_runs.finished_at`(或 `now - 24h`)。
**输出:** `MessagesBatch { conversation_msgs, agent_msgs, agent_turns, total_count }`。

**实现:**

```rust
async fn stage_scan_conversations(ctx: &DreamContext) -> Result<MessagesBatch> {
    let since = ctx.last_successful_run_at.unwrap_or(ctx.run_started_at - 86400_000);
    
    let conversation_msgs = ctx.store.list_messages_since(since)?;
    let agent_msgs = ctx.store.list_agent_messages_since(since)?;
    let agent_turns = ctx.store.list_agent_turns_since(since)?;
    
    let batch = MessagesBatch {
        conversation_msgs,
        agent_msgs,
        agent_turns,
        total_count: ... ,
    };
    
    save_checkpoint(ctx.stage_id, &batch)?;
    Ok(batch)
}
```

**错误处理:** SQLite 查询失败 → retry 3 次,失败则 stage_failed。

### 4.5 Stage 2 - ExtractEntities

**目的:** 对扫到的所有消息文本跑 NER scenario(§2.2),识别提及的 entity。

**输入:** Stage 1 的 `MessagesBatch`。
**输出:** `ExtractionResult { mentions: Vec<MentionCandidate>, new_phantoms: Vec<PhantomCandidate>, ambiguous: Vec<AmbiguousMention> }`。

**实现:** 批量调用 `alias_resolver::resolve_mention`(§2.3)。每 100 条文本一批,允许 stage 内部分失败(收集 errors,但不让 stage 失败)。

### 4.6 Stage 3 - SummarizeSessions

**目的:** 给每个昨天的 agent_session 生成简短摘要,写入 timeline_events 和 EntityPage timeline。

**输入:** Stage 1 的 `MessagesBatch` + Stage 2 的 `ExtractionResult`。
**输出:** `SummaryResult { sessions_summarized: u32, timeline_events_created: u32 }`。

**实现:**

```rust
async fn stage_summarize_sessions(ctx: &DreamContext, batch: &MessagesBatch, ext: &ExtractionResult) -> Result<SummaryResult> {
    let sessions = group_messages_by_session(&batch);
    
    for session in sessions {
        // 调 Haiku 写 ~150 字摘要
        let summary = ctx.llm.summarize_session(&session).await?;
        
        // 写一条 timeline_event
        let event = TimelineEvent {
            event_kind: "session_summary",
            subject_id: Some(session.id.clone()),
            title: summary.title,          // "Discussed uClaw memory OS design"
            payload_json: serde_json::to_string(&summary)?,
            related_entity_ids: extract_entities_from_session(&ext, &session),
            occurred_at: session.started_at,
            importance: estimate_importance(&summary, &ext),
            ..
        };
        ctx.store.create_timeline_event(&event)?;
        
        // 对每个 related entity,追加一条 timeline 条目到 EntityPage.metadata.timeline
        for entity_id in &event.related_entity_ids {
            ctx.store.append_timeline_entry(entity_id, &session.summary_short, &session.id)?;
        }
        
        // 同时也调用 Foundation 的 daily_summaries / fragment 体系(V30 表)
        ctx.store.upsert_daily_summary(session.day(), &summary.markdown)?;
    }
    
    Ok(SummaryResult { ... })
}
```

### 4.7 Stage 4 - MergeDuplicates

**目的:** 找到同一 entity 被错误创建为多个 EntityPage 的情况,提议合并。

**算法:** 三路并行:

1. **Title 相似度**:对所有 EntityPage,两两比对 title(Levenshtein + Jaro-Winkler);相似度 ≥ 0.9 → 候选
2. **Alias 重叠**:两个 page 的 alias 集合交集 ≥ 1 → 候选
3. **图邻居重叠**:两个 page 的入边/出边邻居集合 Jaccard 相似度 ≥ 0.7 → 候选

**输出:** 候选合并对 `(node_a, node_b, score, evidence)`。

**决策:**
- score ≥ 0.95 + edge-neighbor 重叠 ≥ 0.8 → **自动合并**(把 node_b 的 timeline 并到 node_a,node_b 标 archived,加 audit log)
- 0.7 ≤ score < 0.95 → 创建 `review_queue_items(kind='merge_candidate')`,人工决定
- < 0.7 → 忽略

### 4.8 Stage 5 - BuildLongTermMemory

**目的:** 把"碎片"提升为"长期记忆"。具体:

1. **Episode promotion**:某个 entity 在过去 7 天被提及 ≥ 3 次且无对应 EntityPage → 提议创建(走 review)
2. **Skill promotion**:某 Procedure 节点(`skill_extraction` 产出)的 `usage_count ≥ 5` 且 `cited_count ≥ 2` → 自动 promote(metadata.lifecycle='promoted'),并加入到 Boot 候选集
3. **Synthesis materialization**:某 EntityPage 的 timeline 增长 ≥ 10 条 → 触发 wiki_compile.synthesize 生成新 `synthesis` 类型 EntityPage 总结这一阶段的事

**输出:** `PromotionResult { episodes_promoted, skills_promoted, syntheses_created }`。

### 4.9 Stage 6 - RemoveLowValueMemory

**目的:** Importance-aware 清理(参考 §4.10.1)。

**算法:**

1. 重算所有 memory_nodes 的 importance(更新 `memory_importance_scores` 表)
2. importance < 0.2 且 last_cited > 30 天 → archive(`memory_versions.status='orphaned'`)
3. 已 archive 90 天 且仍 importance < 0.2 → physical delete(可选,默认关)
4. **不删除** Boot / Identity / Value / 任何 `status=verified` 的 EntityPage(白名单保护)

**输出:** `CleanupResult { archived, deleted }`。

### 4.10 Stage 7 - UpdateEmbeddings

**目的:** 给所有 version embedding_json 为 NULL 的节点回填 embedding。

**实现:** 复用 Foundation 已有的 `list_versions_without_embedding` + memU `retrieve` API(或 fastembed local)。批量 100 条/次,失败重试。

**节流:** 单次 Dream Cycle 最多更新 500 个 embedding(避免太长)。

### 4.11 Stage 8 - RefreshGraphEdges

**目的:** 重新跑 auto-link 对**昨日新增/修改的所有 version**,捕获之前漏掉的引用。

```rust
async fn stage_refresh_graph_edges(ctx: &DreamContext) -> Result<RefreshResult> {
    let modified_versions = ctx.store.list_versions_modified_since(ctx.last_run_at)?;
    let mut edges_added = 0;
    let mut edges_pruned = 0;
    
    for version in modified_versions {
        // 强制重跑 auto_link,即使之前跑过(因为这期间可能 alias 表有更新,
        // 之前没识别的实体现在能识别了)
        let result = ctx.auto_link.force_recompute(&version)?;
        edges_added += result.added;
        edges_pruned += result.pruned;
    }
    
    Ok(RefreshResult { edges_added, edges_pruned, versions_processed: modified_versions.len() })
}
```

### 4.12 Advanced Enhancements(高级增强,本 spec 的"灵魂")

**这是用户问的"更深层的更先进的 idea"。** 7 项,每项都是独立可选的 Stage 9+:

#### 4.12.1 Importance-Aware Decay(重要性感知衰减)

**思想:** Ebbinghaus 遗忘曲线,但**重要性高的记忆衰减慢**。

**公式**(写入 `memory_importance_scores`):

```
importance = clamp(
    base_value                                       # 0.5(中性起点)
  + log(1 + cited_count)        * 0.20              # 被引用越多越重要
  + log(1 + edge_count)         * 0.15              # 图邻居越多越重要
  + recency_factor(updated, h)  * 0.20              # 越新越重要(h=half_life)
  + status_bonus(status)        * 0.15              # verified > draft > inferred
  + tier_bonus(tier)            * 0.10              # Tier 1 > 2 > 3
  + boot_bonus                  * 0.20              # 是 Boot 节点直接 +0.20
  - low_value_penalty           * 0.30              # 短内容 + 无引用 + 无被引
  , 0, 1)

half_life_days = base_half_life * (0.5 + importance) 
               = 30 * 0.5 ~ 30 * 1.5 = 15~45 天
```

**结果:** 重要的 entity 半衰期可达 45 天,不重要的 15 天。所以"我半年前问的'GPT-5 什么时候出'"这种垃圾 query 早就该衰减没了,"David 是谁"这种核心实体应该一直留着。

#### 4.12.2 Hypothesis Generation(假设生成)

**思想:** Dream Cycle 用 LLM 主动生成"未被询问但可能有意义的问题",创建 Gap-type EntityPage。

**实现:**

```rust
async fn stage_generate_hypotheses(ctx: &DreamContext) -> Result<HypothesisResult> {
    // 1. 找候选信号
    let recently_active_entities = ctx.store.list_entities_with_recent_activity(7)?;
    let edge_density_clusters = ctx.store.find_dense_subgraphs(min_edges=5)?;
    
    // 2. 调 Sonnet(质量优先,Haiku 容易生成废话)
    let hypotheses = ctx.llm.generate_hypotheses(prompt: r#"
        Given these recently active entities and their relationships,
        what interesting QUESTIONS might be worth investigating?
        
        Entities: {entities}
        Dense subgraphs: {clusters}
        
        Output 1-5 hypotheses in JSON, each with:
        {
          "question": "...",
          "evidence_node_ids": [...],
          "expected_value": "high|medium|low",
          "rationale": "..."
        }
        
        DO NOT generate trivial questions. Focus on emergent patterns.
    "#).await?;
    
    // 3. 创建 Gap-type EntityPage(subkind="gap"),status="inferred", confidence=0.3
    // 4. 创建 review_queue_item(kind='hypothesis_review'),让用户决定是否值得追
    
    Ok(HypothesisResult { generated: hypotheses.len() })
}
```

**例子:**
- 输入数据:Garry Tan 提到了 gbrain,gbrain 跟 LLM Wiki 有边
- 假设:"gbrain 跟我们 uClaw 的 memory_graph 的设计差异是什么?会不会有可借鉴的 dream cycle?"(实际上已经在做了,但这恰好是个例子)

**预算:** 每天最多 5 个 hypothesis,token cost 上限 5k(`memubot_config.dream_hypothesis_daily_budget`)。

#### 4.12.3 Spaced Repetition(间隔复习)

**思想:** Anki SM-2 算法,对 `status=verified` 且 importance ≥ 0.6 的 EntityPage 安排复习。

**复习** = LLM 检查"在最近的 timeline 里这页的内容还成立吗?";如果发生变化 → 创建 review_queue_item。

**间隔:** 1, 3, 7, 14, 30, 90 天(每次复习通过则到下一档,不通过则回退)。

**实现表:**

```sql
CREATE TABLE IF NOT EXISTS spaced_repetition_state (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    interval_idx     INTEGER NOT NULL DEFAULT 0,  -- 0=1day, 1=3day, 2=7day, ...
    last_reviewed_at INTEGER NOT NULL,
    next_review_at   INTEGER NOT NULL,
    reviews_total    INTEGER NOT NULL DEFAULT 0,
    reviews_passed   INTEGER NOT NULL DEFAULT 0,
    enabled          INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_spaced_rep_due 
    ON spaced_repetition_state(next_review_at) 
    WHERE enabled=1;
```

#### 4.12.4 Concept Drift Detection(概念漂移检测)

**思想:** 追踪某 EntityPage 的 compiled_truth 在版本链上如何变化。如果最近 30 天内被重写 ≥ 3 次,且新旧版本之间 Levenshtein 距离 > 0.5 → 该 entity 处于"不稳定状态",可能是:
- (a) 真实演变(用户/世界在变)
- (b) LLM 输出不稳定(无足够 source 支撑)
- (c) 矛盾事实没被解决

不论哪种,都创建 review_queue_item 让用户看一眼。

**实现:** Dream Cycle 第 N 阶段对每个 EntityPage 做版本链分析,产 drift score:

```rust
fn compute_drift_score(versions: &[MemoryVersion]) -> f32 {
    if versions.len() < 2 { return 0.0; }
    let recent: Vec<_> = versions.iter().filter(|v| v.created_at > now - 30*86400_000).collect();
    if recent.len() < 3 { return 0.0; }
    
    // 平均两两 Levenshtein 相似度
    let avg_sim = average_pairwise_similarity(&recent);
    
    // 漂移分:1 - similarity(越不像越漂移)
    let drift = 1.0 - avg_sim;
    
    // 加权:重写频次越高,drift 越严重
    drift * (recent.len() as f32 / 30.0).min(1.0)
}
```

#### 4.12.5 Cross-Source Triangulation(三角验证)

**思想:** 对 status=draft 或 status=inferred 的 EntityPage,看是否有 ≥ 3 个独立 Source/Reference 节点支持其核心 claim。如果有 → 自动提升到 status=verified(并创建 review queue 项让用户最终拍板)。

**实现:**

```rust
async fn stage_triangulate(ctx: &DreamContext) -> Result<TriangulationResult> {
    let candidates = ctx.store.list_pages_by_status(&["draft", "inferred"], min_age_days: 1)?;
    
    for page in candidates {
        let sources = page.metadata.sources;  // UUID list of Reference nodes
        if sources.len() < 3 { continue; }
        
        // 用 Step 1 (Analyze) 结果(已缓存在 analysis_cache)看每个 claim 被多少 source 支持
        let analysis = ctx.store.get_analysis_cache(&page.node_id)?;
        let supported_claims = analysis.claims.iter()
            .filter(|c| c.source != "inferred" && c.source != "timeline")
            .collect::<Vec<_>>();
        
        let source_coverage = supported_claims.len() as f32 / analysis.claims.len() as f32;
        
        if source_coverage >= 0.7 && sources.len() >= 3 {
            // 创建 review item 提议 promote → verified
            ctx.review_queue.create(ReviewItem {
                kind: "triangulation_promote",
                title: format!("{} 候选 verified (3+ source 一致)", page.title),
                ...
            })?;
        }
    }
    Ok(...)
}
```

#### 4.12.6 Predictive Boot Preparation(预测性预热)

**思想:** 基于"昨天/最近问了什么"预测"今天可能会问什么",把对应 EntityPage 的 compiled_truth 预编译/预加载。

**实现:**

```rust
async fn stage_predictive_boot(ctx: &DreamContext) -> Result<PredictionResult> {
    // 1. 拿过去 7 天的 query 序列
    let recent_queries = ctx.store.list_query_classifications_since(7)?;
    
    // 2. 找最常出现的 entity_hints
    let top_entities = top_n_by_frequency(&recent_queries.flat_map(|q| q.entity_hints), 20);
    
    // 3. 对每个 entity 检查 compiled_truth 是否 stale
    for entity_id in top_entities {
        if needs_recompile(&entity_id, ctx)? {
            // 预编译(用 wiki_compile.compile,享 SHA-256 缓存)
            ctx.wiki_compiler.compile(&entity_id, CompileDecision::Auto).await?;
        }
    }
    
    // 4. 写"预热集" wiki_artifacts(kind="ready_set")
    ctx.store.upsert_wiki_artifact("ready_set", json!({"entity_ids": top_entities}))?;
    
    Ok(PredictionResult { ... })
}
```

**消费:** Agent 起 session 时,如果 query 命中 ready_set,优先返回缓存的 compiled_truth(0 LLM 成本)。

#### 4.12.7 Synthetic Q&A Materialization(合成问答物化)

**思想:** 高频反复出现的 query → 预生成 `EntityPage(subkind=question, status=verified)`,下次同样问题直接命中。

**实现:**

```rust
async fn stage_synthetic_qa(ctx: &DreamContext) -> Result<()> {
    let queries = ctx.store.list_query_classifications_since(30)?;
    
    // 把语义相似的 query 聚类(embedding similarity ≥ 0.9)
    let clusters = embedding_cluster(&queries, similarity=0.9)?;
    
    for cluster in clusters {
        if cluster.queries.len() < 5 { continue; }  // 阈值
        
        // 看是否已有匹配的 Question page
        let existing = ctx.store.search_questions(&cluster.canonical_text)?;
        if existing.is_some() { continue; }
        
        // 即时生成 answer(用 adaptive_recall 跑一次)
        let answer = ctx.recall.adaptive_recall(&cluster.canonical_text).await?;
        
        // 落库为 Question EntityPage
        ctx.store.create_entity_page(EntityPageCreate {
            subkind: WikiSubkind::Question,
            title: cluster.canonical_text,
            compiled_truth: answer.synthesize_markdown(),
            metadata: EntityPageMetadata {
                status: PageKnowledgeStatus::Draft,  // 让用户/lint review
                confidence: 0.6,
                sources: answer.cited_node_ids,
                ...
            },
            ...
        })?;
    }
    Ok(())
}
```

### 4.13 Tauri Commands

```rust
#[tauri::command]
pub async fn dream_cycle_run_now() -> Result<DreamRunDto, String> { ... }      // 手动触发

#[tauri::command]
pub async fn dream_cycle_list_runs(limit: usize) -> Result<Vec<DreamRunDto>, String> { ... }

#[tauri::command]
pub async fn dream_cycle_get_run(run_id: String) -> Result<DreamRunDetailDto, String> { ... }

#[tauri::command]
pub async fn dream_cycle_get_config() -> Result<DreamCycleConfigDto, String> { ... }

#[tauri::command]
pub async fn dream_cycle_set_config(config: DreamCycleConfigDto) -> Result<(), String> { ... }

#[tauri::command]
pub async fn dream_cycle_cancel_running() -> Result<bool, String> { ... }      // 强行停掉当前的
```

### 4.14 前端 — Dream Cycle Dashboard

新组件 `ui/src/components/memory/DreamCycleDashboard.tsx`:

```
┌────────────────────────────────────────────────────────────────┐
│ Dream Cycle                            [Run Now]  [Settings ▾]  │
├────────────────────────────────────────────────────────────────┤
│ Last run: 2026-05-18 03:00 (success, 14m 32s, $0.42)            │
│                                                                 │
│ ✓ ① ScanConversations    142 messages, 2s                       │
│ ✓ ② ExtractEntities      87 mentions, 5 new phantoms, 1m12s     │
│ ✓ ③ SummarizeSessions    8 sessions, 3m02s                      │
│ ✓ ④ MergeDuplicates      3 merges proposed (in review)          │
│ ✓ ⑤ BuildLongTermMemory  12 promoted, 1 synthesis created       │
│ ✓ ⑥ RemoveLowValueMemory 24 archived, 0 deleted                 │
│ ✓ ⑦ UpdateEmbeddings     31 updated                             │
│ ✓ ⑧ RefreshGraphEdges    18 reconciled                          │
│ ✓ Advanced:                                                     │
│   · ImportanceDecay      243 scores updated                     │
│   · HypothesisGeneration 2 hypotheses queued                    │
│   · SpacedRepetition     5 cards due tomorrow                   │
│   · ConceptDriftDetection 0 drift                                │
│   · Triangulation        1 promotion candidate                  │
│   · PredictiveBoot       18 entities pre-warmed                 │
│                                                                 │
│ Next run: 2026-05-19 03:00 (in 11h 12m)                         │
└────────────────────────────────────────────────────────────────┘
```

历史 run 在底部 paginated list,可点开看详情。

### 4.15 配置项

```rust
pub struct DreamCycleConfig {
    pub enabled: bool,                              // default true
    pub scheduled_hour: u8,                         // default 3 (03:00 local)
    pub timezone: String,                           // default user's local
    pub max_duration_minutes: u32,                  // default 30
    pub max_token_budget: u32,                      // default 100k
    pub stages_enabled: HashMap<String, bool>,      // 各阶段可单独关
    pub advanced_enhancements_enabled: HashMap<String, bool>,
    pub auto_merge_threshold: f32,                  // default 0.95
    pub auto_archive_importance_threshold: f32,     // default 0.2
    pub auto_delete_after_archive_days: Option<u32>, // default None(不物删)
}
```

---

## 5. 与前两层的集成

### 5.1 数据流总图

```
                  ┌─────────────────────────┐
                  │      User / Agent       │
                  └────────────┬────────────┘
                               │
                               ▼ writes Episode / Reference
                  ┌─────────────────────────┐
                  │  memory_graph (V1-V35)  │
                  └────────────┬────────────┘
                               │
        ┌──────────────────────┼──────────────────────┐
        ▼                      ▼                      ▼
   auto_link              entity_recognition       wiki_compile
   (sync hook)            (NEW · §2)               (Cognitive)
        │                      │                      │
        ▼                      ▼                      ▼
   memory_edges          entity_aliases /         memory_versions
                         ner_decisions            (with provenance)
                               │
                               ▼
                  ┌─────────────────────────┐
                  │   timeline_events       │ ◄── NEW · §3
                  │   (global timeline)     │
                  └────────────┬────────────┘
                               │
                               ▼
                  ┌─────────────────────────┐
                  │   Dream Cycle           │ ◄── NEW · §4
                  │   (nightly 8 stages +   │
                  │    7 advanced enhance)  │
                  └─────────────────────────┘
                               │
        ┌──────────────────────┼──────────────────────┐
        ▼                      ▼                      ▼
   importance_scores      temporal_aggregates   spaced_repetition_state
   memory_health_findings hypothesis pages      ready_set (predictive boot)
   review_queue_items     synthesis pages
```

### 5.2 与 Foundation Spec 边界

| Foundation 实体 | 本 spec 影响 |
|---|---|
| `MemoryNodeKind`(含 EntityPage) | **不动** |
| `MemoryRelationKind`(含 7 typed-edge) | **不动**——所有新边走已有类型 |
| `memory_nodes / versions / edges / routes / keywords / fts` | **只读 + 写入**,绝不 ALTER |
| `wiki_artifacts`(V34) | 复用,新 kinds: `ready_set`, `dream_summary` |
| `memory_health_findings` | 复用,新 check_kinds: `drift_warning`, `import_failure`, `ner_ambiguous` |
| `proactive/scenarios/` | 加 5 个新 scenarios:`entity_recognition`、`timeline_aggregator`、`dream_orchestrator`、`drift_detector`、`spaced_repetition_runner` |

### 5.3 与 Cognitive Spec 边界

| Cognitive 实体 | 本 spec 关联 |
|---|---|
| `wiki_page_templates`(V35) | Dream Cycle Stage 5 创建 `subkind=synthesis/question` 时**严格按模板** |
| `EntityPageMetadata.{confidence, status, provenance_state, ...}`(V35) | Triangulation 阶段会更新这些字段;Drift Detection 会读 |
| `wiki_compile.rs` 两步 compile(Phase 10) | Dream Cycle 各阶段触发 compile 时**都走它**,享 SHA-256 增量(Phase 11) |
| `review_queue_items`(V35) | Dream Cycle 各阶段是**写入大户**:hypothesis_review / merge_candidate / triangulation_promote / drift_warning / ner_ambiguous |
| `query_classifier`(Phase 14) | Engines 扩展加第四类 `Temporal` |
| `wiki_log_events`(V35) | Dream Cycle 写入大量 audit log,event_type: `dream_run_start`/`dream_run_end`/`dream_stage_start/end` |

---

## 6. V44 Migration 汇总(原 spec 写的 V36 已过时)

```sql
-- V44: Engines Layer

-- Entity Graph
CREATE TABLE IF NOT EXISTS entity_aliases (...);
CREATE VIRTUAL TABLE IF NOT EXISTS entity_aliases_fts USING fts5(...);
CREATE TABLE IF NOT EXISTS entity_raw_data (...);
CREATE TABLE IF NOT EXISTS ner_decisions (...);

-- Timeline Engine
CREATE TABLE IF NOT EXISTS timeline_events (...);
CREATE TABLE IF NOT EXISTS temporal_aggregates (...);
CREATE TABLE IF NOT EXISTS activity_clusters (...);

-- Dream Cycle
CREATE TABLE IF NOT EXISTS dream_cycle_runs (...);
CREATE TABLE IF NOT EXISTS dream_cycle_stages (...);
CREATE TABLE IF NOT EXISTS memory_importance_scores (...);
CREATE TABLE IF NOT EXISTS spaced_repetition_state (...);
```

**共 10 张新表**,全部 `IF NOT EXISTS`,纯 additive。

---

## 7. Risk Assessment

### 7.1 LLM 成本爆炸

**风险:** Dream Cycle 8 stages + 7 advanced 全跑,单次可能消耗 50-100k tokens。
**缓解:**
- `max_token_budget` 默认 100k,超额自动停在当前 stage 并标 `partial`
- 每个 stage 单独可关
- HypothesisGeneration / SyntheticQA 等贵的阶段默认关,用户主动开
- 跟 `cost_records`(V13)对接,每次 dream run 一行总计

### 7.2 长时间持有 DB 写锁

**风险:** Dream Cycle 跑 30 分钟,期间用户/Agent 写入 memory 卡顿。
**缓解:**
- 每个 stage 内部分 batch(每 100 行一个事务),不一次性长事务
- 在 stage 之间显式 `commit + sleep(50ms)` 让出锁
- 不在 03:00-04:00 之外跑(用户活跃时段保护)
- 添加 cancellation token,用户在 dashboard 点 Cancel 立即终止当前 stage

### 7.3 错误传播 / 重试地狱

**风险:** Stage 2 失败 → Stage 3 拿不到 input → 全部 abort,但 stage 1 的 expensive 结果浪费。
**缓解:**
- 每个 stage 写 checkpoint(input + output)到 `dream_cycle_stages.checkpoint_json`
- 失败 stage 重试 ≤ 3 次,exponential backoff
- 续跑:重启 dream cycle 时,先看上一次 status='running' 的 run,从失败 stage 续跑而不是从头
- 用户能在 dashboard 上单独"Re-run stage N"

### 7.4 NER 误识别污染图谱

**风险:** NER 把"Apple"识别成水果而不是公司,创建错 phantom slug,污染 alias 表。
**缓解:**
- 所有 NER 创建的 alias 都标 `source='inferred'`,weight < 1.0
- alias_resolver 优先级:`source='declared' > source='user' > source='inferred'`
- phantom 创建走 review queue,不是直接落 EntityPage
- 用户可在 EntityPage 编辑时删除"非我所愿"的 alias

### 7.5 Timeline 数据爆炸

**风险:** timeline_events 表每天写 100+ 条,一年 36500 行;temporal_aggregates 每天/每周/每月各一行。
**评估:** 10 万条 timeline_events + 适当索引,SQLite 单文件能扛(uClaw 库通常 < 1GB)。
**缓解:**
- timeline_events 上 `importance < 0.1` 且 `occurred_at > 90 days` 的可归档(Dream Cycle Stage 6 一并处理)
- 查询永远走索引(`space_id, occurred_at DESC`)

### 7.6 Predictive Boot 误预测

**风险:** ready_set 预热的 entity 跟今天用户实际问的不沾边,LLM 钱白花。
**评估:** 即便 100% 错,SHA-256 增量缓存让"预编译"成本接近 0(命中 cache → skip)。**所以预测错的代价远小于预测对的收益。**
**缓解:** 监控 `cost_records` 里的 dream_predictive_boot 行,如果命中率长期 < 30% 自动关闭这一项。

### 7.7 Spaced Repetition 过度打扰

**风险:** 100 个 verified entity,每隔 1-3 天就有几个 review item 涌入,用户被烦。
**缓解:**
- `memubot_config.spaced_rep_max_daily_reviews = 5`(default)
- 同一 entity 复习失败后,interval 回退而不是消失;但**优先调度 importance 高的**

### 7.8 Concept Drift 误报

**风险:** 用户主动改 EntityPage 三次,被 drift detector 误判为"不稳定"。
**缓解:**
- drift_score 计算时**排除 actor='user'** 的版本(只算 LLM 自动重写)
- drift review item severity = low,且 30 天 auto-dismiss

---

## 8. Future Work(Engines Layer 之后的可能方向)

1. **Multi-Agent Dream Cycle** —— 多个专家 sub-agent 并行跑各自专长 stage(memory architect / fact checker / synthesizer),协作产 output
2. **Causal Graph Mining** —— 在 memory_edges 上跑因果发现算法,识别"X 导致 Y" 模式,自动创建 Decision-type EntityPage
3. **Online Learning of Importance Function** —— 当前 importance 公式是手调权重;用户的 review 决定(accept/reject)作为标签,train logistic regression 校准权重
4. **Cross-Space Memory Federation** —— 通过 `MemoryVisibility::Shared` 让多个 space 共享一个"公共大脑"(技术深 + 业务复杂,放最远)
5. **Episodic-Semantic Memory 分离** —— 学术上区分两种 memory 类型,Episode 是按时间组织,Semantic 是按概念组织。当前 EntityPage = semantic, Episode = episodic, Dream Cycle 是两者之间的"桥"
6. **Knowledge Graph Embeddings** —— 在 memory_edges 上跑 TransE/RotatE,做 link prediction(预测缺失边)
7. **Counterfactual Reasoning** —— Agent 跟"如果 X 不是 Y 的话,Z 会怎样?"这种反事实问题——需要 hypothesis 系统的扩展

---

## Appendix A — 8 Stage 输入输出契约表

| Stage | 输入 | 输出 | 失败可重启 | 默认开关 |
|---|---|---|---|---|
| ① ScanConversations | `since: i64` | `MessagesBatch` | 是 | on |
| ② ExtractEntities | `MessagesBatch` | `ExtractionResult` | 是(从 checkpoint) | on |
| ③ SummarizeSessions | `MessagesBatch + ExtractionResult` | `SummaryResult` | 是 | on |
| ④ MergeDuplicates | `EntityPage list snapshot` | `MergeResult` | 是 | on |
| ⑤ BuildLongTermMemory | `ExtractionResult + SummaryResult` | `PromotionResult` | 是 | on |
| ⑥ RemoveLowValueMemory | `ImportanceScores updated` | `CleanupResult` | 是 | on(default soft archive) |
| ⑦ UpdateEmbeddings | `versions WHERE embedding_json IS NULL` | `EmbedResult` | 是 | on |
| ⑧ RefreshGraphEdges | `versions_modified_since` | `RefreshResult` | 是 | on |
| Advanced.ImportanceDecay | `all nodes` | importance update | 是 | on |
| Advanced.HypothesisGeneration | `recent activities` | hypothesis pages | 是 | **off**(高成本) |
| Advanced.SpacedRepetition | `due cards` | review items | 是 | on |
| Advanced.ConceptDriftDetection | `versions per node` | drift findings | 是 | on |
| Advanced.Triangulation | `draft/inferred pages` | promotion candidates | 是 | on |
| Advanced.PredictiveBoot | `recent query log` | ready_set + precompile | 是 | on |
| Advanced.SyntheticQA | `query clusters` | Question pages | 是 | **off**(高成本) |

---

## Appendix B — Tommy 8 张图 + Engines 层完整覆盖矩阵

| Tommy 要点 | Foundation | Cognitive | Engines |
|---|---|---|---|
| 8 控制面文件 | partial | ✅ | — |
| 9 page types | — | ✅ | — |
| 8 frontmatter keys | — | ✅ | — |
| 4 aggressive 字段(confidence/provenance/contradicted/inferredParagraphs) | — | ✅ | — |
| 两步 LLM compile | — | ✅ | — |
| SHA-256 增量编译 | — | ✅ | — |
| BM25+vector+graph → RRF | ✅ | enhanced | — |
| Adaptive RAG / Query Classifier | — | ✅ | **+ Temporal 第四类** |
| LLM=Compiler etc. 7 角色 | — | ✅ | — |
| **NER + Alias resolution** | — | — | ✅ |
| **Backlinks API** | — | — | ✅ |
| **Raw source sidecar** | — | — | ✅ |
| **Global timeline + 时间区间查询** | — | — | ✅ |
| **Temporal aggregates(日/周/月)** | — | — | ✅ |
| **Activity clustering** | — | — | ✅ |
| **Dream Cycle 8 stages** | — | — | ✅ |
| **Dream Cycle 7 advanced enhancements** | — | — | ✅ |
| **Importance-aware decay** | — | — | ✅ |
| **Hypothesis generation** | — | — | ✅ |
| **Spaced repetition (SM-2)** | — | — | ✅ |
| **Concept drift detection** | — | — | ✅ |
| **Cross-source triangulation** | — | — | ✅ |
| **Predictive Boot preparation** | — | — | ✅ |
| **Synthetic Q&A materialization** | — | — | ✅ |

✅ 完整覆盖。三层叠加 = Tommy 框架 100% + 7 项原创高级增强。
