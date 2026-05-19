# Agent Memory OS — Engines Layer Implementation Plan(Phase 15-21)

> **🟡 STATUS: PARTIALLY PAUSED (2026-05-20)** — See [ADR 2026-05-20 — gbrain primary, freeze L2 Cognitive](../../adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md) §8.
>
> An overlap audit found L3 Engines splits cleanly into "gbrain-redundant" and "gbrain-additive" pieces. Decision per the ADR:
>
> | L3 component | Status | Why |
> |---|---|---|
> | **Entity Graph Engine** (NER + alias resolution + coreference) — Phase 15 | 🛑 PAUSED | gbrain's `chat_extractor` + `maintain` skill already cover regex NER + Aho-Corasick + Haiku-disambiguation + weekly alias dedup. Coreference is anti-pattern. |
> | **Timeline Engine** (global `timeline_events` + daily/weekly/monthly summaries) — Phase 16 | ✅ RETAINED | gbrain has per-page timeline but no global timeline, no aggregation loop. Standalone-worthy. ~400 LOC. |
> | **Dream Cycle stages ①–⑧** — Phase 17 | 🛑 PAUSED | Stages ①–⑥ overlap gbrain's 6-stage cycle. ⑦ UpdateEmbeddings + ⑧ RefreshGraphEdges to be folded into specific consumers, not re-imported as a pipeline. |
> | **Enhancement 4.12.1 Importance-Aware Decay** | ✅ RETAINED | gbrain has no decay algorithm; knowledge-hygiene gap. ~250 LOC. |
> | **Enhancement 4.12.2 Hypothesis Generation** | 🛑 PAUSED | Low marginal value; user queries cover it. |
> | **Enhancement 4.12.3 Spaced Repetition (Anki SM-2)** | ✅ RETAINED | Proven learning-science; gbrain has nothing equivalent. ~300 LOC. |
> | **Enhancement 4.12.4 Concept Drift Detection** | ✅ RETAINED | Catches contradictions across versions. ~200 LOC. |
> | **Enhancement 4.12.5 Cross-Source Triangulation** | ✅ RETAINED | Key when external data sources land. ~250 LOC. |
> | **Enhancement 4.12.6 Predictive Boot Preparation** | 🛑 PAUSED | Optimization-level, ~10% UX win. |
> | **Enhancement 4.12.7 Synthetic Q&A Materialization** | 🛑 PAUSED | High token cost, low signal. |
>
> **Execution rule:** Only the RETAINED items are open for implementation. Anything PAUSED should not be picked up without revisiting the ADR. Schema additions for retained items target **V44** (the V36 number this plan originally claimed is reserved/skipped per CLAUDE.md registry).

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Layer position:** **Engines Layer (Phase 15-21)** —— 三层 Memory OS 计划的第三层。
- **Foundation Plan(Phase 1-7)**:[`agent-memory-os.md`](agent-memory-os.md)
- **Cognitive Plan(Phase 8-14)**:[`agent-memory-os-cognitive.md`](agent-memory-os-cognitive.md)
- **Engines Plan(本文,Phase 15-21)**

**Goal:** 给 uClaw 装上三个高阶引擎——Entity Graph(NER + alias + 查询 API)、Timeline Engine(全局时间线 + 时间区间查询 + 聚合摘要)、Dream Cycle(8 阶段夜间巩固 + 7 项高级增强)。完成后,uClaw 真正成为"会自动夜间整理 + 有时间观念 + 能识别非显式引用"的认知有机体。

**Spec:** [`docs/superpowers/specs/2026-05-18-agent-memory-os-engines-design.md`](../specs/2026-05-18-agent-memory-os-engines-design.md)

**Migration claim:** Engines layer 占用 **V44**(原 plan 写的 V36 已过时 — 实际 V35-V42 是 Foundation + browser-task + MCP audit,V43 是 Cognitive,V36 在 CLAUDE.md 注册表里标为 skipped(Phase 7 抢占 V37 时跳号产生的空号))。Engines 层 RETAINED 项的新表都进 V44(单一迁移,目前估计 4-5 张新表 — 仅 Timeline + 4 enhancements 需要的)。

**合并节奏建议:** Foundation Phase 1-7 完工 + 跑 1-2 周 → Cognitive Phase 8-14 完工 + 跑 1-2 周 → 再开 Engines Phase 15。**不要并行**。

---

## Pre-flight(每个 Phase 开始前都要跑一次)

- [ ] **Step 0.1: 确认 Foundation + Cognitive 全部合并**

```bash
cd /Users/ryanliu/Documents/uclaw
git log --oneline main | grep -E "feat\(memory-os\)" | head -40
```
Expected:能看到 V34 / V35 migration 提交 + 14 个 phase summary 文档。

- [ ] **Step 0.2: Branch + baseline**(同 Foundation/Cognitive plan)

- [ ] **Step 0.3: Active migration check**

```bash
grep -nE "^pub const V[0-9]+|^const V[0-9]+" src-tauri/src/db/migrations.rs | tail -10
```
Expected:看到 V35-V43 已被 Foundation Phase 1-7 + Automation + Cognitive Phase 8.1 占用(详见 CLAUDE.md Active migration registry)。**V44 应该未占用**。如有其它 PR 抢先把 V44 也占了,顺延到下一空闲号,所有引用同步改。

---

## Phase 15 — Entity Recognition + Alias Resolution(Entity Graph · 上半部)

**Branch:** `claude/p15-entity-recognition`
**Bisectable commits:** 7
**Depends on:** Cognitive Phase 13 merged(review_queue 已存在)
**Spec ref:** Engines §2

### Task 15.1: V44 migration —— Engines RETAINED 子集新表(原 plan 是 V36 + 10 张表)

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`(add `V44_ENGINES_LAYER`)
- **NOTE**: Phase 15 (Entity Graph Engine) is PAUSED per ADR §8 — the SQL snippets for `entity_aliases` etc. should NOT land. Only Timeline Engine (Phase 16) + RETAINED enhancements (4.12.1/3/4/5) tables should make it into V44.

- [ ] **Step 15.1.1** 在 V43 之后追加 V44 常量。完整 SQL 见 Engines Spec §6 — **跳过 Entity Graph 相关表**(`entity_aliases`, `entity_aliases_fts`, `entity_raw_data`)按 ADR §8 paused;仅保留 Timeline + 4 enhancements 所需:

```rust
pub const V44_ENGINES_LAYER: &str = "
-- Entity Graph
CREATE TABLE IF NOT EXISTS entity_aliases (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    node_id     TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    alias_lower TEXT NOT NULL,
    weight      REAL NOT NULL DEFAULT 1.0,
    source      TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    UNIQUE(space_id, alias_lower, node_id)
);
CREATE INDEX IF NOT EXISTS idx_entity_aliases_lookup 
    ON entity_aliases(space_id, alias_lower);
CREATE INDEX IF NOT EXISTS idx_entity_aliases_node ON entity_aliases(node_id);

CREATE VIRTUAL TABLE IF NOT EXISTS entity_aliases_fts USING fts5(
    alias_id UNINDEXED,
    alias_lower,
    tokenize='trigram'
);

CREATE TABLE IF NOT EXISTS entity_raw_data (
    id          TEXT PRIMARY KEY,
    node_id     TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_url  TEXT,
    raw_json    TEXT NOT NULL,
    fetched_at  INTEGER NOT NULL,
    expires_at  INTEGER,
    UNIQUE(node_id, source_kind)
);
CREATE INDEX IF NOT EXISTS idx_entity_raw_data_node ON entity_raw_data(node_id);

CREATE TABLE IF NOT EXISTS ner_decisions (
    id              TEXT PRIMARY KEY,
    source_node_id  TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    raw_text        TEXT NOT NULL,
    resolution_kind TEXT NOT NULL,
    resolved_node_id TEXT,
    confidence      REAL,
    rationale       TEXT,
    review_item_id  TEXT,
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ner_decisions_source ON ner_decisions(source_node_id);

-- Timeline Engine
CREATE TABLE IF NOT EXISTS timeline_events (...);          -- 详见 Spec §3.2.1
CREATE TABLE IF NOT EXISTS temporal_aggregates (...);      -- 详见 Spec §3.2.2
CREATE TABLE IF NOT EXISTS activity_clusters (...);        -- 详见 Spec §3.2.3

-- Dream Cycle
CREATE TABLE IF NOT EXISTS dream_cycle_runs (...);
CREATE TABLE IF NOT EXISTS dream_cycle_stages (...);
CREATE TABLE IF NOT EXISTS memory_importance_scores (...);
CREATE TABLE IF NOT EXISTS spaced_repetition_state (...);
";
```

(把 spec §6 的全部 10 张表 SQL 都填进去。这一次性建好,后续 phase 不再加表。)

- [ ] **Step 15.1.2** 挂载到 `run()`,在 V43_COGNITIVE_LAYER 之后追加 V44 runner,**再加 V44 注册表行到 CLAUDE.md**(V35-V42 + V43 行 PR #262/#264 已完成)。

- [ ] **Step 15.1.3** 验证 10 张表都被创建:

```bash
sqlite3 ~/.uclaw/uclaw.db ".tables" | tr ' ' '\n' | grep -E "entity_aliases|entity_raw_data|ner_decisions|timeline_events|temporal_aggregates|activity_clusters|dream_cycle_runs|dream_cycle_stages|memory_importance_scores|spaced_repetition_state"
# 应输出 10 行(entity_aliases_fts 是虚拟表)
```

**Commit:** `feat(db): V44 — engines layer schema (RETAINED subset, ~5 new tables)`

### Task 15.2: `alias_resolver.rs` 模块

**Files:**
- Create: `src-tauri/src/memory_graph/alias_resolver.rs`

- [ ] **Step 15.2.1** 类型定义(`MentionCandidate / Resolution / EntityKindGuess`,完整签名见 Spec §2.2)。

- [ ] **Step 15.2.2** Alias index 加载:

```rust
pub struct AliasIndex {
    by_lower: HashMap<String, Vec<AliasEntry>>,      // 精确查找
    aho: aho_corasick::AhoCorasick,                  // 多模式匹配
    trigram_fts: ...,                                // 通过 entity_aliases_fts 查
}

impl AliasIndex {
    pub fn load(conn: &Connection, space_id: &str) -> Result<Self> { ... }
    pub fn exact_lookup(&self, key: &str) -> Option<NodeId> { ... }
    pub fn fuzzy_lookup(&self, key: &str, threshold: f32) -> Vec<AliasMatch> { ... }
    pub fn aho_scan(&self, text: &str) -> Vec<(usize, usize, NodeId)> { ... }
}
```

- [ ] **Step 15.2.3** `resolve_mention(mention, ctx) → Resolution`(spec §2.3 完整算法)。

- [ ] **Step 15.2.4** 单测:
  - 精确命中
  - Levenshtein 模糊命中(1 个候选)
  - 多候选歧义 → ambiguous
  - 完全未命中且 confidence > 0.7 → phantom
  - confidence ≤ 0.7 → skip

**Commit:** `feat(memory): alias_resolver with exact + fuzzy + aho lookup`

### Task 15.3: NER scenario(基于 regex + Aho-Corasick)

**Files:**
- Create: `src-tauri/src/proactive/scenarios/entity_recognition.rs`
- Modify: `src-tauri/src/proactive/scenarios/mod.rs`

- [ ] **Step 15.3.1** Scenario 框架 + 触发条件(on-create event):

```rust
pub struct EntityRecognitionScenario {
    alias_index: Arc<RwLock<AliasIndex>>,
    last_node_processed: Mutex<HashMap<String, Instant>>,   // 24h 去重
    config: NerConfig,
}

#[async_trait]
impl Scenario for EntityRecognitionScenario {
    async fn handle_event(&self, evt: InfraEvent) -> Result<()> {
        match evt {
            InfraEvent::VersionCreated { node_id, version_id } => {
                self.process_node(node_id, version_id).await
            }
            _ => Ok(())
        }
    }
    async fn tick(&self, ctx: &ScenarioContext) -> Result<()> { Ok(()) }  // 不主动 tick
}
```

- [ ] **Step 15.3.2** `process_node`:
  - 加载 version content
  - regex 抽 candidates(大写专有 + email + URL + @handle + 引号术语)
  - Aho-Corasick 扫已知 alias
  - 合并 candidates,去重重叠
  - 对每个 candidate 调 `alias_resolver::resolve_mention`
  - 落 `ner_decisions` audit
  - 命中现有 → 在 version content 文本中插 `[[node:uuid]]` 标记 → 触发现有 auto_link → typed-edge
  - phantom 候选 → 写 review_queue_items

- [ ] **Step 15.3.3** 单测(用 fixture text):
  - "Yesterday I met John Smith from Acme" → 2 mentions
  - "Discussed RAG with [[entity:john-smith]]" → 1 显式 + 0 NER(显式优先)
  - "I want to learn about LangChain" → 1 mention

**Commit:** `feat(memory-os): entity_recognition scenario with regex + Aho-Corasick`

### Task 15.4: LLM disambiguation(走 Haiku)

**Files:**
- Modify: `src-tauri/src/memory_graph/alias_resolver.rs`
- Modify: `src-tauri/src/proactive/scenarios/entity_recognition.rs`

- [ ] **Step 15.4.1** 当 `resolve_mention` 返回 `AmbiguousNeedsReview` 且 candidates ≤ 5 时,**自动调 Haiku 消歧**(只在 candidates < 5 时跑,避免噪音):

```rust
async fn llm_disambiguate(
    mention: &MentionCandidate,
    candidates: &[AliasMatch],
    context_window: &str,        // mention 周围 ±200 字符
    ctx: &Ctx,
) -> Result<Option<NodeId>> {
    let prompt = format!(r#"
        Given this text context:
        {context_window}
        
        The mention "{}" could refer to:
        {{candidates with their compiled_truth preview}}
        
        Which is most likely? Output JSON:
        {{ "node_id": "...", "confidence": 0.0-1.0, "rationale": "..." }}
        If unsure, return {{ "node_id": null }}.
    "#, mention.raw_text);
    let resp = ctx.llm.haiku(prompt).await?;
    parse_json(resp).map(|r| if r.confidence > 0.7 { Some(r.node_id) } else { None })
}
```

- [ ] **Step 15.4.2** 在 entity_recognition scenario 流程里集成:ambiguous → 跑 LLM → 若仍未决 → 走 review。

- [ ] **Step 15.4.3** 单测:mock LLM 返回 confidence 0.8 → 命中;返回 null → 仍走 review。

**Commit:** `feat(memory-os): LLM disambiguation for ambiguous mentions`

### Task 15.5: Coreference resolution(同文档内)

**Files:**
- Create: `src-tauri/src/memory_graph/coreference.rs`

- [ ] **Step 15.5.1** `resolve_coreference(text, mentions) → Vec<CoreferenceCluster>` 用 Haiku 一次解决(spec §2.4)。

- [ ] **Step 15.5.2** 在 entity_recognition scenario 内调用:同 cluster 内的所有 mention 共享 resolved node_id。

**Commit:** `feat(memory-os): intra-document coreference resolution`

### Task 15.6: Backlinks API + Raw data sidecar

**Files:**
- Modify: `src-tauri/src/memory_graph/store.rs`(加 `list_backlinks` + `upsert_raw_data` + `get_raw_data`)
- Modify: `src-tauri/src/tauri_commands.rs` + `main.rs::invoke_handler!`
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 15.6.1** Store API:
```rust
pub fn list_backlinks(&self, node_id: &str) -> Result<Vec<Backlink>, Error> { ... }
pub fn upsert_raw_data(&self, node_id: &str, source_kind: &str, raw: &Value) -> Result<()> { ... }
pub fn get_raw_data(&self, node_id: &str, source_kind: Option<&str>) -> Result<Vec<RawDataEntry>> { ... }
```

- [ ] **Step 15.6.2** Tauri commands:`memory_entity_page_backlinks` / `memory_entity_page_raw_data` / `memory_entity_page_set_raw_data`。

- [ ] **Step 15.6.3** WikiView 右侧栏新增"References" 区,显示 backlinks list。

**Commit:** `feat(memory): backlinks API + raw_data sidecar storage`

### Task 15.7: feature flag + Phase 15 summary

**Files:**
- Modify: `src-tauri/src/memubot_config.rs`(`ner_enabled: bool` default true)
- Create: `docs/memory-os/phase-15-summary.md`

- [ ] **Step 15.7.1** 灰度 config:
```rust
pub struct MemubotConfig {
    // ... 现有 ...
    pub ner_enabled: bool,
    pub ner_llm_disambiguation: bool,           // 是否调 LLM 消歧
    pub ner_coreference_enabled: bool,
    pub backlinks_panel_enabled: bool,
}
```

- [ ] **Step 15.7.2** Summary 文档(< 30 行)。

**Commit:** `feat(memory-os): Phase 15 flags + summary`

### Phase 15 PR shape

```
## Commits (bisectable)

| # | Subject |
|---|---|
| 1 | feat(db): V44 — engines layer schema (RETAINED subset, ~5 new tables) |
| 2 | feat(memory): alias_resolver with exact + fuzzy + aho lookup |
| 3 | feat(memory-os): entity_recognition scenario |
| 4 | feat(memory-os): LLM disambiguation for ambiguous mentions |
| 5 | feat(memory-os): intra-document coreference resolution |
| 6 | feat(memory): backlinks API + raw_data sidecar storage |
| 7 | feat(memory-os): Phase 15 flags + summary |
```

---

## Phase 16 — Entity Graph Query API(Entity Graph · 下半部)

**Branch:** `claude/p16-entity-graph-query`
**Bisectable commits:** 5
**Depends on:** Phase 15 merged
**Spec ref:** Engines §2.6

### Task 16.1: `graph_query.rs` 结构化查询

**Files:**
- Create: `src-tauri/src/memory_graph/graph_query.rs`

- [ ] **Step 16.1.1** 完整类型(`GraphQuery / SeedSpec / EdgeFilter / NodeFilter / ScoringMode / GraphQueryResult`,见 Spec §2.6)。

- [ ] **Step 16.1.2** 执行器:
```rust
pub fn execute(&self, query: GraphQuery, ctx: &Ctx) -> Result<GraphQueryResult> {
    let seeds = resolve_seeds(&query.seeds, ctx)?;
    
    // 用递归 CTE 实现 BFS,加 edge/node filter,加 depth/max_nodes 限制
    let sql = build_recursive_cte(&query, &seeds);
    let rows = ctx.conn.execute_query(sql)?;
    
    let scored = match query.scoring {
        ScoringMode::Propagation => score_propagation(&rows, &query),
        ScoringMode::ShortestPath => score_shortest_path(&rows, &seeds),
        ScoringMode::Centrality => score_pagerank(&rows, 10),
    };
    
    Ok(GraphQueryResult { nodes: scored, edges: ..., paths: ..., stats: ... })
}
```

- [ ] **Step 16.1.3** 三种 ScoringMode 实现:
  - **Propagation**:复用现有 `graph_propagation_search` 算法(decay 0.6)
  - **ShortestPath**:Dijkstra 简化版(因为边权重 ≈ 1)
  - **Centrality**:简化 PageRank,k=10 迭代,转移矩阵从 memory_edges 加权派生

- [ ] **Step 16.1.4** 单测:每个 ScoringMode 各 1 个 fixture graph。

**Commit:** `feat(memory): graph_query module with 3 scoring modes`

### Task 16.2: 内存 LRU 查询缓存

**Files:**
- Modify: `src-tauri/src/memory_graph/graph_query.rs`

- [ ] **Step 16.2.1** 加 `lru::LruCache<QueryHash, GraphQueryResult>`(容量 200,TTL 5 分钟)。

- [ ] **Step 16.2.2** 缓存命中后立即返回,miss 则执行 + 写缓存。

- [ ] **Step 16.2.3** memory_edges 表写入时(`create_edge` / `delete_edge` / auto-link)**清空相关 cache entry**——粗暴策略:每次写都全清(因为缓存 5 分钟内不会太多)。

**Commit:** `feat(memory): graph_query LRU cache with TTL invalidation`

### Task 16.3: Tauri command —— `memory_graph_query`

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` + `main.rs::invoke_handler!`
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 16.3.1** 接受结构化 query JSON,返回 result。

- [ ] **Step 16.3.2** TS 类型:`GraphQueryInput` / `GraphQueryResult`,完整暴露三种 ScoringMode。

**Commit:** `feat(ipc): memory_graph_query structured API`

### Task 16.4: 前端 — Graph Explorer 升级

**Files:**
- Modify: `ui/src/components/memory/MemoryGraphView.tsx`
- Create: `ui/src/components/memory/GraphQueryPanel.tsx`

- [ ] **Step 16.4.1** 在 MemoryGraphView 加一个 collapsed query panel:
  - 起点选择(下拉:从 search / 当前选中节点)
  - Edge filter(checkbox 7 个 typed-edge)
  - Depth slider(1-5)
  - Scoring mode toggle
  - "Run Query" 按钮

- [ ] **Step 16.4.2** 查询结果用力导向布局,scoring 高的节点变大。

- [ ] **Step 16.4.3** Vitest 测试:mock query result,断言节点 + 边渲染正确。

**Commit:** `feat(ui): graph explorer with structured query panel`

### Task 16.5: 多跳 recall 接入(替换 Cognitive Phase 14 的占位实现)

**Files:**
- Modify: `src-tauri/src/memory_graph/multi_hop_recall.rs`(Cognitive Phase 14 创建)

- [ ] **Step 16.5.1** 把 Cognitive 14 里那个简单的 BFS 替换为通过 `graph_query::execute` 跑(享 LRU 缓存 + 三种 scoring 可选)。

- [ ] **Step 16.5.2** 默认 ScoringMode::Propagation,可通过 query class hint 切换。

**Commit:** `refactor(memory): multi_hop_recall routes through graph_query API`

---

## Phase 17 — Timeline Engine Foundation(Timeline · 基础)

**Branch:** `claude/p17-timeline-engine`
**Bisectable commits:** 6
**Depends on:** Phase 15 merged
**Spec ref:** Engines §3.1-3.5

### Task 17.1: `timeline_events` 写入埋点

**Files:**
- Create: `src-tauri/src/memory_graph/timeline_event.rs`(helper)
- Modify:多个文件—— `agent/dispatcher.rs` / `auto_link.rs` / `wiki_compile.rs` / `tier_escalator.rs` / `memory_lint.rs` / `proactive scenarios`

- [ ] **Step 17.1.1** Helper:

```rust
pub fn emit_timeline_event(
    conn: &Connection,
    space_id: &str,
    kind: TimelineEventKind,
    subject_id: Option<&str>,
    title: &str,
    payload: Option<&Value>,
    related_entity_ids: &[String],
    occurred_at: i64,
) -> Result<String /* id */> {
    let id = uuid::Uuid::new_v4().to_string();
    let importance = estimate_importance(kind, payload);
    conn.execute(
        "INSERT INTO timeline_events (id, space_id, event_kind, subject_id, title, payload_json, related_entity_ids, occurred_at, importance, created_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
        params![id, space_id, kind.as_str(), subject_id, title, serde_json::to_string(&payload)?, serde_json::to_string(related_entity_ids)?, occurred_at, importance, now_ms()],
    )?;
    Ok(id)
}
```

- [ ] **Step 17.1.2** 关键写入点埋点:
  - agent_session 开始/结束 → `session_start` / `session_end` event
  - EntityPage create → `page_created`
  - EntityPage tier promote → `page_promoted`
  - skill_extraction 产出 Procedure → `skill_learned`
  - review_queue 解决 → `review_resolved`

- [ ] **Step 17.1.3** 单测:每种 event_kind 各产 1 条 fixture,检查 timeline_events 行。

**Commit:** `feat(memory): timeline_events emission at 5 key write points`

### Task 17.2: 全局 Timeline 查询 API

**Files:**
- Create: `src-tauri/src/memory_graph/timeline_query.rs`
- Modify: `src-tauri/src/tauri_commands.rs` + `main.rs::invoke_handler!`

- [ ] **Step 17.2.1** Query API:
```rust
pub struct TimelineQuery {
    pub time_range: (i64, i64),
    pub event_kind_filter: Option<Vec<String>>,
    pub entity_id_filter: Option<Vec<String>>,
    pub min_importance: f32,
    pub limit: usize,
    pub offset: usize,
}
pub fn list_events(query: TimelineQuery, ctx: &Ctx) -> Result<Vec<TimelineEvent>> { ... }
```

- [ ] **Step 17.2.2** Tauri commands:`memory_timeline_list` + `memory_timeline_get_event`。

- [ ] **Step 17.2.3** TS wrapper。

**Commit:** `feat(ipc): timeline_query API + tauri commands`

### Task 17.3: `temporal_aggregator` scenario(日/周/月聚合)

**Files:**
- Create: `src-tauri/src/proactive/scenarios/temporal_aggregator.rs`

- [ ] **Step 17.3.1** Scenario 每天凌晨跑(在 dream cycle 之前,~02:50):
  - 跑 `grain="day"` for 昨天
  - 周一跑 `grain="week"` for 上周
  - 1 号跑 `grain="month"` for 上月

- [ ] **Step 17.3.2** Aggregation 逻辑:
  - 拉 timeline_events 在区间内
  - LLM(Haiku)写 summary_md 200-500 字
  - 提取 top_themes / top_entities
  - 写 `temporal_aggregates`(unique key 防重)

- [ ] **Step 17.3.3** 单测:mock 50 个 events,断言 aggregate 行被创建。

**Commit:** `feat(memory-os): temporal_aggregator scenario (day/week/month)`

### Task 17.4: Activity Clustering

**Files:**
- Create: `src-tauri/src/memory_graph/activity_clustering.rs`

- [ ] **Step 17.4.1** 给一组 events,用两种方式聚类:
  - **快速版(Aggregator 内部用)**:related_entity_ids 重叠 → 同一 cluster
  - **质量版(Temporal Recall on-demand 用)**:调 Haiku 用 prompt"把这些 events 按主题聚成 3-8 个 cluster,每个 cluster 起一个名"

- [ ] **Step 17.4.2** 落 `activity_clusters` 表。

- [ ] **Step 17.4.3** 单测:mock 30 events,断言 cluster 数在 3-8 之间。

**Commit:** `feat(memory-os): activity clustering (fast + quality modes)`

### Task 17.5: 前端 — TimelineEngineView

**Files:**
- Create: `ui/src/components/memory/TimelineEngineView.tsx`
- Modify: `ui/src/components/memory/MemoryPanel.tsx`(替换或增强现有 MemoryTimeline tab)

- [ ] **Step 17.5.1** 完整 UI(spec §3.5 ASCII mock):
  - 时间粒度 toggle(Day / Week / Month / Year)
  - 日期 navigator(`< 2026-05 >`)
  - Filter dropdown(event_kind / entity)
  - 按 cluster 折叠分组

- [ ] **Step 17.5.2** 数据流:atom 缓存当前 view 的 events + aggregates。

- [ ] **Step 17.5.3** Vitest:渲染 mock data,断言 cluster 分组正确,toggle 切换日期范围。

**Commit:** `feat(ui): TimelineEngineView with grain toggle + clustering`

### Task 17.6: feature flag + Phase 17 summary

- [ ] `memubot_config.timeline_engine_enabled = true`
- [ ] `phase-17-summary.md`

**Commit:** `feat(memory-os): Phase 17 flag + summary`

---

## Phase 18 — Temporal Query Classification(Timeline · 查询路由)

**Branch:** `claude/p18-temporal-query`
**Bisectable commits:** 5
**Depends on:** Phase 14 + Phase 17 merged
**Spec ref:** Engines §3.3-3.4

### Task 18.1: 扩 Query Classifier 加 Temporal 类

**Files:**
- Modify: `src-tauri/src/memory_graph/query_classifier.rs`(Cognitive Phase 14 创建)

- [ ] **Step 18.1.1** `QueryClass` enum 加 `Temporal` 变体(完整定义见 spec §3.3)。

- [ ] **Step 18.1.2** classifier prompt 模板加 temporal 判定 + time_range 解析。

- [ ] **Step 18.1.3** 单测:
  - "最近两周我在做什么" → Temporal { RelativeRecent { days: 14 }, focus: What }
  - "昨天" → Temporal { Absolute { yesterday-start, yesterday-end } }
  - "上个月" → Temporal { Calendar { current_year, current_month - 1 } }
  - "VeRL 作者是谁" → SingleHop(不误判)

**Commit:** `feat(memory): query_classifier adds Temporal class`

### Task 18.2: `temporal_recall` 管线

**Files:**
- Create: `src-tauri/src/memory_graph/temporal_recall.rs`

- [ ] **Step 18.2.1** 完整流程(spec §3.4):
  1. 解析 TimeRange → 具体 start/end
  2. 拉 timeline_events
  3. 查 temporal_aggregates 是否命中
  4. 命中 → 返回缓存
  5. 未命中 → 即时 cluster + synthesize + 落库

- [ ] **Step 18.2.2** 接入 `adaptive_recall` 顶层 dispatcher(Cognitive Phase 14):

```rust
match class {
    // ... existing 3 cases ...
    QueryClass::Temporal { ... } => temporal_recall::run(class, ctx).await,
}
```

- [ ] **Step 18.2.3** 单测:命中缓存 + 未命中两种情况。

**Commit:** `feat(memory): temporal_recall pipeline with aggregate cache`

### Task 18.3: Time-aware Boot context

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs`

- [ ] **Step 18.3.1** 每个 agent session 开始时,Boot system prompt 注入(spec §3.6):

```markdown
## Temporal Context
Today: {{today_iso}} ({{weekday}})
Recent active themes (last 7 days):
- {{theme 1}} ({{event count}} events)
- {{theme 2}} ...
```

数据来源:`temporal_aggregates(grain=week, period=current_week)` 的 top_themes。

- [ ] **Step 18.3.2** `memubot_config.temporal_boot_injection_enabled = true`(默认开)。

- [ ] **Step 18.3.3** 单测:mock aggregate 存在/不存在两种,验证 prompt 内容。

**Commit:** `feat(memory-os): inject temporal context into agent Boot`

### Task 18.4: 前端 — Slash hint 支持 temporal

**Files:**
- Modify: `ui/src/components/chat/ChatInput.tsx` 和 `ui/src/components/agent/AgentView.tsx`(**两个 composer 都改,CLAUDE.md 警告**)

- [ ] **Step 18.4.1** 识别 `/recap` `/timeline` `/this-week` 前缀,转成 class_hint=Temporal 发给后端。

**Commit:** `feat(ui): /recap /timeline /this-week slash hints (both composers)`

### Task 18.5: feature flag + summary

- [ ] `memubot_config.temporal_query_enabled = false`(灰度,稳定后开)
- [ ] `phase-18-summary.md` 附演示视频/截图链接

**Commit:** `feat(memory-os): Phase 18 flag + summary`

---

## Phase 19 — Dream Cycle Pipeline Basics(8 阶段基础版)

**Branch:** `claude/p19-dream-cycle-basics`
**Bisectable commits:** 8
**Depends on:** Phase 15 + 17 merged
**Spec ref:** Engines §4.1-4.11

### Task 19.1: `DreamCycleOrchestrator` 框架

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/mod.rs`
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/orchestrator.rs`
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/types.rs`

- [ ] **Step 19.1.1** Types(`DreamRun / DreamStage / StageStatus / DreamContext`)。

- [ ] **Step 19.1.2** Orchestrator 核心 loop:
  ```rust
  pub async fn run(&self, trigger: TriggerKind) -> Result<DreamRun> {
      let run = self.create_run(trigger)?;
      self.acquire_lock()?;
      
      for stage in self.stages.iter() {
          let stage_row = self.create_stage_row(&run, &stage)?;
          match stage.execute(&run.context).await {
              Ok(out) => self.complete_stage(&stage_row, &out)?,
              Err(e) if stage.retryable() && stage_row.retry_count < 3 => {
                  // retry with backoff
              }
              Err(e) => {
                  self.fail_stage(&stage_row, &e)?;
                  return Ok(self.mark_run_partial(&run));
              }
          }
      }
      Ok(self.mark_run_completed(&run))
  }
  ```

- [ ] **Step 19.1.3** 文件锁:`~/.uclaw/dream_cycle.lock`,RAII guard,Drop 时释放。

- [ ] **Step 19.1.4** 单测:mock 3 个 stages,模拟成功/失败/重试。

**Commit:** `feat(memory-os): dream cycle orchestrator framework`

### Task 19.2: Scheduler —— 接入 ProactiveService tick

**Files:**
- Modify: `src-tauri/src/proactive/service.rs`

- [ ] **Step 19.2.1** 在 `tick_inner` 加判定:
  - 本地时间是否在配置的 scheduled_hour 内?
  - 今天是否已经成功跑过?
  - 是否有正在运行的 run?
  - 是否手动触发?

- [ ] **Step 19.2.2** 满足条件 → spawn orchestrator,不阻塞 tick。

- [ ] **Step 19.2.3** 单测:mock 时间,验证调度逻辑。

**Commit:** `feat(memory-os): dream cycle scheduler in ProactiveService`

### Task 19.3: Stage 1+2 — ScanConversations + ExtractEntities

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/scan.rs`
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/extract.rs`

- [ ] **Step 19.3.1** `scan_conversations`:
  - 查 conversations / agent_sessions / agent_messages / agent_turns since 上次成功 run
  - 输出 `MessagesBatch`
  - checkpoint 写入

- [ ] **Step 19.3.2** `extract_entities`:
  - 把 batch 喂给 entity_recognition scenario(Phase 15)的批处理 API
  - 输出 `ExtractionResult`
  - 100 条一批,允许分片失败

- [ ] **Step 19.3.3** 各自单测。

**Commit:** `feat(memory-os): dream cycle Stages 1+2 (scan + extract)`

### Task 19.4: Stage 3 — SummarizeSessions

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/summarize.rs`

- [ ] **Step 19.4.1** 逻辑(spec §4.6):
  - 按 session_id 分组 messages
  - 每 session 调 Haiku 写 150 字摘要
  - 写 timeline_events(event_kind='session_summary')
  - 给每个 related_entity 追加 timeline 条目到 EntityPage.metadata.timeline

- [ ] **Step 19.4.2** Token 控制:每个 session 输入 ≤ 4000 tokens(截断 oldest messages)。

**Commit:** `feat(memory-os): dream cycle Stage 3 (summarize sessions)`

### Task 19.5: Stage 4 — MergeDuplicates

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/merge.rs`

- [ ] **Step 19.5.1** 三路并行候选(spec §4.7):title 相似 + alias 重叠 + 图邻居重叠。

- [ ] **Step 19.5.2** Score ≥ 0.95 + edge 重叠 ≥ 0.8 → 自动合并(timeline 合并 + node_b → archived)。

- [ ] **Step 19.5.3** 0.7 ≤ score < 0.95 → 创 `review_queue_items(kind='merge_candidate')`。

- [ ] **Step 19.5.4** 单测:三种 path 各 1 个 fixture pair。

**Commit:** `feat(memory-os): dream cycle Stage 4 (merge duplicates)`

### Task 19.6: Stage 5+6 — BuildLongTerm + RemoveLowValue

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/promote.rs`
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/cleanup.rs`

- [ ] **Step 19.6.1** Promotion 三路(spec §4.8):
  - episode → entity 候选
  - procedure usage_count ≥ 5 + cited_count ≥ 2 → auto-promote
  - timeline 增 ≥ 10 → 触发 synthesis 创建

- [ ] **Step 19.6.2** Cleanup(spec §4.9):
  - 算 importance(基础版,Phase 20 才接 advanced 公式)
  - importance < 0.2 且 last_cited > 30 天 → archive
  - 不删除 Boot/Identity/Value/verified

**Commit:** `feat(memory-os): dream cycle Stages 5+6 (promote + cleanup)`

### Task 19.7: Stage 7+8 — UpdateEmbeddings + RefreshGraphEdges

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/embed.rs`
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/stages/refresh_edges.rs`

- [ ] **Step 19.7.1** Embed:批 100 条/次调 memU retrieve / fastembed,单次 dream 上限 500 条。

- [ ] **Step 19.7.2** Refresh:对昨日修改的 version 强制 force_recompute auto_link。

**Commit:** `feat(memory-os): dream cycle Stages 7+8 (embed + refresh)`

### Task 19.8: 前端 — DreamCycleDashboard + Tauri commands

**Files:**
- Create: `ui/src/components/memory/DreamCycleDashboard.tsx`
- Modify: `src-tauri/src/tauri_commands.rs` + `main.rs::invoke_handler!`(6 个新命令,spec §4.13)
- Modify: `ui/src/components/memory/MemoryPanel.tsx`(新 "Dreams" tab)

- [ ] **Step 19.8.1** Tauri commands:
  - `dream_cycle_run_now`
  - `dream_cycle_list_runs`
  - `dream_cycle_get_run`
  - `dream_cycle_get_config` / `set_config`
  - `dream_cycle_cancel_running`

- [ ] **Step 19.8.2** Dashboard UI(spec §4.14):
  - 上一次 run 状态 + duration + cost
  - 8 stages 进度 + 各阶段统计
  - 历史 runs paginated list
  - Run Now 按钮 + Settings 抽屉

- [ ] **Step 19.8.3** Vitest:渲染 3 种状态(running / completed / failed)的 fixture run。

**Commit:** `feat(ui): DreamCycleDashboard + 6 tauri commands`

### Phase 19 PR shape

```
| # | Subject |
|---|---|
| 1 | feat(memory-os): dream cycle orchestrator framework |
| 2 | feat(memory-os): dream cycle scheduler |
| 3 | feat(memory-os): Stages 1+2 (scan + extract) |
| 4 | feat(memory-os): Stage 3 (summarize) |
| 5 | feat(memory-os): Stage 4 (merge) |
| 6 | feat(memory-os): Stages 5+6 (promote + cleanup) |
| 7 | feat(memory-os): Stages 7+8 (embed + refresh) |
| 8 | feat(ui): DreamCycleDashboard + 6 tauri commands |
```

---

## Phase 20 — Dream Cycle Advanced Enhancements(7 项高级增强)

**Branch:** `claude/p20-dream-advanced`
**Bisectable commits:** 7
**Depends on:** Phase 19 merged
**Spec ref:** Engines §4.12

### Task 20.1: Importance-Aware Decay

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/advanced/importance_decay.rs`

- [ ] **Step 20.1.1** 完整公式(spec §4.12.1):

```rust
pub fn compute_importance(node: &MemoryNode, edges: &EdgeStats, ...) -> ImportanceScore {
    let citation = (1.0 + node.cited_count as f64).log10() * 0.20;
    let edge_factor = (1.0 + edges.in_count as f64 + edges.out_count as f64).log10() * 0.15;
    let recency = compute_recency_factor(node.updated_at, half_life_days) * 0.20;
    let status_bonus = match status { 
        Verified => 0.15, Draft => 0.05, Inferred => -0.05, Disputed => -0.10 
    };
    let tier_bonus = match tier { 1 => 0.10, 2 => 0.05, 3 => 0.0 };
    let boot_bonus = if node.kind == Boot { 0.20 } else { 0.0 };
    let low_value_penalty = compute_low_value_penalty(node, edges) * 0.30;
    
    let importance = (0.5 + citation + edge_factor + recency + status_bonus + tier_bonus + boot_bonus - low_value_penalty)
        .clamp(0.0, 1.0);
    
    let half_life = 30.0 * (0.5 + importance);  // 15-45 天
    
    ImportanceScore { importance, half_life, ... }
}
```

- [ ] **Step 20.1.2** 批量更新 `memory_importance_scores`(每 dream cycle 跑一次,全表)。

- [ ] **Step 20.1.3** Stage 6 (RemoveLowValueMemory) 改成读这张表而不是即时算。

- [ ] **Step 20.1.4** 单测:5 类 fixture(boot / verified / draft / orphan / disputed),验证分数合理。

**Commit:** `feat(memory-os): importance-aware decay (advanced enhancement)`

### Task 20.2: Hypothesis Generation

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/advanced/hypothesis.rs`

- [ ] **Step 20.2.1** Find candidate signals(spec §4.12.2):
  - recently_active_entities(7d activity)
  - dense_subgraphs(degree ≥ 5 + 平均 edge weight ≥ 0.7)

- [ ] **Step 20.2.2** Sonnet LLM 调用(质量优先,Haiku 易出废话):
  - Prompt 模板(spec §4.12.2)
  - 严格 JSON 输出
  - 单次最多 5 个 hypothesis

- [ ] **Step 20.2.3** 落库:
  - 创建 `EntityPage(subkind=gap, status=inferred, confidence=0.3)`
  - 创建 `review_queue_items(kind='hypothesis_review')`,severity=low(由用户判断价值)

- [ ] **Step 20.2.4** 预算守卫:`memubot_config.dream_hypothesis_daily_budget = 5000` tokens。

- [ ] **Step 20.2.5** 单测:mock Sonnet 返回 3 个 hypothesis,验证 page + review 都创建。

**Commit:** `feat(memory-os): hypothesis generation (advanced enhancement)`

### Task 20.3: Spaced Repetition(SM-2)

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/advanced/spaced_rep.rs`
- Create: `src-tauri/src/proactive/scenarios/spaced_repetition_runner.rs`(独立 scenario,因为复习时机不仅在 dream cycle)

- [ ] **Step 20.3.1** Dream Cycle 阶段:
  - 找新增的 `status=verified + importance ≥ 0.6` EntityPage,加入 `spaced_repetition_state`(interval_idx=0, next_review_at=now + 1 day)
  - 已存在的不重复加

- [ ] **Step 20.3.2** Runner scenario(每天 tick):
  - 查 `next_review_at <= now AND enabled=1`
  - 每天 ≤ `memubot_config.spaced_rep_max_daily_reviews`(default 5)
  - 对每个 due:LLM 检查 compiled_truth 是否还成立(给最近 timeline + compiled_truth,问"还正确吗?")
  - Pass → interval_idx++,update next_review_at
  - Fail → interval_idx--,create review_queue item

- [ ] **Step 20.3.3** 间隔表:`[1, 3, 7, 14, 30, 90]` 天(写死,后续可 user-config)。

- [ ] **Step 20.3.4** 单测覆盖 enroll + due + pass + fail 四种 path。

**Commit:** `feat(memory-os): spaced repetition (SM-2 advanced enhancement)`

### Task 20.4: Concept Drift Detection

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/advanced/drift.rs`

- [ ] **Step 20.4.1** 算法(spec §4.12.4):
  - 取每个 EntityPage 最近 30 天的 version chain(排除 actor='user' 的版本)
  - 平均两两 Levenshtein 相似度
  - drift = (1 - similarity) × (recent_versions / 30).min(1.0)
  - drift > 0.4 → 创 review item(severity=low,30 天 auto-dismiss)

- [ ] **Step 20.4.2** 单测:3 类 fixture(稳定 / 微变 / 剧变),验证 drift 分数。

**Commit:** `feat(memory-os): concept drift detection (advanced enhancement)`

### Task 20.5: Cross-Source Triangulation

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/advanced/triangulation.rs`

- [ ] **Step 20.5.1** 完整算法(spec §4.12.5):
  - 找 status=draft/inferred + 至少 1 天龄的 page
  - 拉 `analysis_cache`(Cognitive Phase 10 缓存的 Step 1 分析)
  - 算 source_coverage = supported_claims / total_claims
  - coverage ≥ 0.7 + sources.len() ≥ 3 → 创 review item 提议 verified

- [ ] **Step 20.5.2** 单测:覆盖三种 fixture(高 coverage 多 source / 低 coverage / source 不足)。

**Commit:** `feat(memory-os): cross-source triangulation (advanced enhancement)`

### Task 20.6: Predictive Boot Preparation

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/advanced/predictive_boot.rs`

- [ ] **Step 20.6.1** 实现(spec §4.12.6):
  - 查过去 7 天 query 分类记录(可以加日志埋点在 query_classifier 出口)
  - top_N entity_hints
  - 对每个 entity:`wiki_compiler.compile(node_id, Decision::Auto)` —— 享 SHA-256 缓存
  - 落 `wiki_artifacts(kind="ready_set")` 存预热 list

- [ ] **Step 20.6.2** Agent agentic_loop 在 query 时优先查 ready_set。

- [ ] **Step 20.6.3** 单测:mock query log + ready_set 命中。

**Commit:** `feat(memory-os): predictive boot preparation (advanced enhancement)`

### Task 20.7: Synthetic Q&A Materialization

**Files:**
- Create: `src-tauri/src/proactive/scenarios/dream_cycle/advanced/synthetic_qa.rs`

- [ ] **Step 20.7.1** 实现(spec §4.12.7):
  - 拉 30 天 query 记录
  - 用 embedding 聚类(similarity ≥ 0.9)
  - cluster.len() ≥ 5 + 未有对应 Question page → 即时跑 `adaptive_recall` 生成答案 → 落 EntityPage(subkind=question, status=draft, confidence=0.6)

- [ ] **Step 20.7.2** Default off(高成本,稳定后再开)。

- [ ] **Step 20.7.3** 单测。

**Commit:** `feat(memory-os): synthetic Q&A materialization (advanced enhancement)`

### Phase 20 PR shape

```
| # | Subject |
|---|---|
| 1 | feat(memory-os): importance-aware decay |
| 2 | feat(memory-os): hypothesis generation |
| 3 | feat(memory-os): spaced repetition |
| 4 | feat(memory-os): concept drift detection |
| 5 | feat(memory-os): cross-source triangulation |
| 6 | feat(memory-os): predictive boot preparation |
| 7 | feat(memory-os): synthetic Q&A materialization |
```

---

## Phase 21 — Integration & Cost Dashboard(收尾)

**Branch:** `claude/p21-engines-integration`
**Bisectable commits:** 5
**Depends on:** Phase 15-20 merged
**Spec ref:** Engines §5 + §7

### Task 21.1: Cost dashboard 完整化

**Files:**
- Modify: `ui/src/components/settings/UsageSettings.tsx`(P5 已有)

- [ ] **Step 21.1.1** 新增 tab "Memory OS Cost":
  - 三层 stacked bar(Foundation / Cognitive / Engines)
  - 按 module 分(wiki_compile / dream_cycle.* / lint / ner_llm / temporal_aggregator / 等)
  - 时间 picker(7d / 30d / 90d)

- [ ] **Step 21.1.2** 新 SQL 视图查询:
```sql
SELECT model, SUM(cost_usd), SUM(input_tokens + output_tokens) AS tokens 
FROM cost_records 
WHERE created_at > ? 
GROUP BY model
```

**Commit:** `feat(ui): Memory OS cost dashboard with module breakdown`

### Task 21.2: End-to-end smoke test 脚本

**Files:**
- Create: `scripts/memory-os-smoke.sh`

- [ ] **Step 21.2.1** 脚本:
  1. 创建一个 EntityPage(测 entity_page_create)
  2. 写入一条 Episode 提到这个 entity(测 NER + auto-link)
  3. 触发 dream_cycle_run_now(测 8 阶段)
  4. 查 timeline(测 timeline_events 是否被填)
  5. 查 review queue(确保没有意外项)
  6. 跑 temporal recall "最近 1 天发生了什么"
  7. Cleanup test artifacts

- [ ] **Step 21.2.2** 跑一次确保 green。

**Commit:** `test(memory-os): end-to-end smoke test`

### Task 21.3: 性能基准

**Files:**
- Create: `src-tauri/benches/memory_os_bench.rs`(可选)
- Modify: `docs/memory-os/perf-baseline.md`

- [ ] **Step 21.3.1** 准备一个含 1000 EntityPage + 5000 edge + 10000 timeline_event 的 fixture DB。

- [ ] **Step 21.3.2** Benchmark:
  - `memory_graph_search` p50 / p99 延迟
  - `temporal_recall(grain=week)` 命中缓存 vs 即时计算
  - `dream_cycle` 全跑耗时 + token cost
  - `graph_query(depth=3)` LRU 命中率

- [ ] **Step 21.3.3** 落到 `perf-baseline.md` 表格。

**Commit:** `bench(memory-os): performance baseline doc`

### Task 21.4: 系统总览文档

**Files:**
- Create: `docs/memory-os/README.md`

- [ ] **Step 21.4.1** 一份用户向 README:
  - 三层架构总览图
  - 每层能力 + 配置开关清单
  - "如何开/关某项功能"指南
  - 常见问题(为什么 Wiki 没自动创建 / dream cycle 没跑 / review queue 太多 等)
  - 关联到三份 spec + 三份 plan

**Commit:** `docs(memory-os): top-level user-facing README`

### Task 21.5: CLAUDE.md 更新

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 21.5.1** Memory subsystem 段落补一段"三层架构概述",链到 README。

- [ ] **Step 21.5.2** Active migration registry 补 V44 行(V32-V42 在 PR #262 之前的工作已补全;V43 在 PR #264 已加)。

- [ ] **Step 21.5.3** 列出新的 feature flags(共 ~15 个,合并到现有 memubot_config 描述)。

**Commit:** `docs: CLAUDE.md reflects Memory OS three-layer architecture`

### Phase 21 PR shape

```
| # | Subject |
|---|---|
| 1 | feat(ui): Memory OS cost dashboard |
| 2 | test(memory-os): end-to-end smoke |
| 3 | bench(memory-os): perf baseline |
| 4 | docs(memory-os): README |
| 5 | docs: CLAUDE.md updated |
```

---

## 全局验证清单(每个 Engines Phase 必跑)

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust build ==="  && (cd src-tauri && cargo build 2>&1 | grep -E "^error" | head)
echo "=== rust tests ==="  && (cd src-tauri && cargo test --lib 2>&1 | tail -10)
echo "=== ts ==="          && (cd ui && npx tsc --noEmit 2>&1 | head -10)
echo "=== ui tests ==="    && (cd ui && npm test -- --run 2>&1 | tail -10)

# V44 表完整性(RETAINED 子集 ~5 张表;原 plan 是 V36 + 10 张)
sqlite3 ~/.uclaw/uclaw.db "SELECT name FROM sqlite_master WHERE type='table' AND name IN (
  'entity_aliases','entity_raw_data','ner_decisions',
  'timeline_events','temporal_aggregates','activity_clusters',
  'dream_cycle_runs','dream_cycle_stages',
  'memory_importance_scores','spaced_repetition_state'
)"
# Expected: 10 行

# 跑一次 dream cycle smoke(本地 dev)
sqlite3 ~/.uclaw/uclaw.db "SELECT id, status, stages_completed, token_cost FROM dream_cycle_runs ORDER BY started_at DESC LIMIT 5"
```

---

## 回退手册

每个 Phase 都有 feature flag:

```rust
// memubot_config.rs 累积新增
pub struct MemubotConfig {
    // ... Foundation + Cognitive flags ...
    
    // Engines · Entity Graph
    pub ner_enabled: bool,                          // P15 (default true)
    pub ner_llm_disambiguation: bool,               // P15 (default true)
    pub ner_coreference_enabled: bool,              // P15 (default true)
    pub backlinks_panel_enabled: bool,              // P15 (default true)
    pub graph_query_cache_ttl_sec: u32,             // P16 (default 300)
    
    // Engines · Timeline
    pub timeline_engine_enabled: bool,              // P17 (default true)
    pub temporal_query_enabled: bool,               // P18 (default false, 灰度)
    pub temporal_boot_injection_enabled: bool,      // P18 (default true)
    
    // Engines · Dream Cycle
    pub dream_cycle_enabled: bool,                  // P19 (default true)
    pub dream_cycle_scheduled_hour: u8,             // P19 (default 3)
    pub dream_cycle_max_duration_minutes: u32,      // P19 (default 30)
    pub dream_cycle_max_token_budget: u32,          // P19 (default 100_000)
    
    // Engines · Advanced
    pub dream_importance_decay_enabled: bool,       // P20.1 (default true)
    pub dream_hypothesis_enabled: bool,             // P20.2 (default false, 贵)
    pub dream_hypothesis_daily_budget: u32,         // P20.2 (default 5000 tokens)
    pub dream_spaced_rep_enabled: bool,             // P20.3 (default true)
    pub spaced_rep_max_daily_reviews: u32,          // P20.3 (default 5)
    pub dream_drift_detection_enabled: bool,        // P20.4 (default true)
    pub dream_triangulation_enabled: bool,          // P20.5 (default true)
    pub dream_predictive_boot_enabled: bool,        // P20.6 (default true)
    pub dream_synthetic_qa_enabled: bool,           // P20.7 (default false, 贵)
}
```

**最坏情况:** 把所有 engines flags 关掉,uClaw 回退到 gbrain-primary 行为(Foundation Phase 1-7 maintenance mode + gbrain MCP)。V44 表是 additive,不删除,空表不影响任何其他功能。

---

## 与三层架构合并节奏的最终建议

```
Phase 1-7 (Foundation)                  ━━━━━━━━━━━━━━━━━━━ Layer 1 完工
                                        ↓ 跑 1-2 周看真实数据
Phase 8-14 (Cognitive)                  ━━━━━━━━━━━━━━━━━━━ Layer 2 完工
                                        ↓ 跑 1-2 周看真实数据
Phase 15-21 (Engines)                   ━━━━━━━━━━━━━━━━━━━ Layer 3 完工

                                        = Tommy LLM Wiki 框架 100% 实现
                                          + 7 项原创高级 idea
                                          + 完整 Memory OS 闭环
```

**强烈不建议并行**——Cognitive 严重依赖 Foundation 的 EntityPage 抽象;Engines 严重依赖 Cognitive 的 provenance + wiki_compile + review_queue。并行会导致 schema 冲突 + 重复重构,得不偿失。

---

## PR 描述模板(每个 Engines Phase)

```markdown
## Memory OS Engines Phase <N> — <name>

Implements Phase <N> of `docs/superpowers/plans/agent-memory-os-engines.md`.
Engines Spec: `docs/superpowers/specs/2026-05-18-agent-memory-os-engines-design.md` §<section>.

### Commits (bisectable)
<insert commit table from Phase N PR shape above>

### Verification
- cargo build: clean
- cargo test --lib: all passing (added N new tests)
- npx tsc --noEmit: 0 errors
- npm test -- --run: all passing
- DB:V44 表创建无错;若是 Phase 19+(已 paused — 仅 4 项 enhancements RETAINED)别跑 dream_cycle_run_now

### Feature flag
- `memubot_config.<flag>` (default: <value>)
- Disable to fully bypass new code path

### Adjacent edits (called out per CLAUDE.md)
- Phase 18 改了**两个** composer(ChatInput + AgentView)
- 注册 N 个新 tauri commands 到 `main.rs::invoke_handler!`
- 注册 N 个新 scenarios 到 ProactiveService::tick_inner

### Performance impact
- 主路径开销:< 5ms per write hop(NER 钩子)
- Dream Cycle 单次 token 预算:< 100k(配置硬封顶)
- 召回延迟:无变化(三层缓存)

### Rollback
关 `memubot_config.<flag>`。V44 表是 additive,空表无影响。
```
