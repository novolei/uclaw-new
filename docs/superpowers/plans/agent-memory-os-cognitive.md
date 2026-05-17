# Agent Memory OS — Cognitive Layer Implementation Plan(Phase 8-14)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Layer position:** **L2 Cognitive Layer (Phase 8-14)** —— 三层 Memory OS 计划的第二层。
- **L1 Foundation**:[`agent-memory-os.md`](agent-memory-os.md) —— Phase 1-7,**必须先完成**
- **L2 Cognitive(本文)**:Phase 8-14
- **L3 Engines**:[`agent-memory-os-engines.md`](agent-memory-os-engines.md) —— Phase 15-21

**Goal:** 把 uClaw Memory OS 从 Foundation Layer(实体级长期记忆 + auto-link + Wiki view)推进到 Cognitive Layer(段落级 provenance + 9 种页面类型 + 两步 compile + 增量编译 + 控制面 + Review brake + Adaptive RAG)。完成后 uClaw 真正成为"用户和 Agent 的第二大脑"——Tommy LLM Wiki 框架的完全实现。

**Companion docs:**
- Cognitive Spec(本 plan 的设计依据):`docs/superpowers/specs/2026-05-18-agent-memory-os-cognitive-design.md`
- Foundation Spec(底层依赖):`docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`
- Foundation Plan(Phase 1-7,**必须先完成才能开 Phase 8**):`docs/superpowers/plans/agent-memory-os.md`
- Engines Spec / Plan(下一层):`agent-memory-os-engines-design.md` / `agent-memory-os-engines.md`

**Architecture summary:**

- Foundation Phase 1-7 完成后,uClaw 已有 EntityPage 节点 + Auto-link + WikiView + Health/Lint + Tier escalator + Markdown sync。
- Cognitive Phase 8-14 在此基础上**纯叠加**:扩 metadata schema、新增 5 张表(V35)、新建 4 个 scenarios、上 2 个新模块(wiki_compile.rs / query_classifier.rs)。**零 schema 破坏,零现有功能影响**。
- 每个 Phase 对应 Tommy 7-role framing 中的一个或两个角色(LLM=Compiler / Schema=OS / Wiki=Product / Review=Brake / Chat=Entry / Graph=Nav / Log=Audit)。

**Migration claim:** Cognitive layer 占用 **V35**(单一迁移,内含 5 张新表的 CREATE)。Foundation plan 占 V34,Cognitive 占 V35,无冲突。

---

## Pre-flight(每个 Phase 开始前都要跑一次)

- [ ] **Step 0.1: 确认 Foundation Phase 1-7 已经合并**

```bash
cd /Users/ryanliu/Documents/uclaw
git log --oneline main | grep -E "feat\((memory|memory-os)\)" | head -30
```
Expected:至少能看到 `feat(memory): add MemoryNodeKind::EntityPage` 和 `feat(db): V34` 的提交。如果没有,**先回去把 Foundation plan 跑完**,本 plan 强依赖那些基础。

- [ ] **Step 0.2: Branch off latest main**

```bash
git checkout main && git pull
git checkout -b claude/<cognitive-phase-name>   # 比如 claude/p8-knowledge-taxonomy
```

- [ ] **Step 0.3: Baseline pipeline**

```bash
echo "=== rust build ==="  && (cd src-tauri && cargo build 2>&1 | grep -E "^error" | head)
echo "=== rust tests ==="  && (cd src-tauri && cargo test --lib 2>&1 | tail -8)
echo "=== ts ==="          && (cd ui && npx tsc --noEmit 2>&1 | head -10)
echo "=== ui tests ==="    && (cd ui && npm test -- --run 2>&1 | tail -10)
```

- [ ] **Step 0.4: Active migration check**

```bash
grep -nE "^pub const V[0-9]+|^const V[0-9]+|^/// V[0-9]+ —|^// V[0-9]+ —" src-tauri/src/db/migrations.rs | tail -10
```
Expected:看到 V33 和 V34(本 cognitive layer 之前 Foundation Phase 1 加的)。**V35 应该未占用**。如有其它 PR 抢先,顺延到 V36,所有引用同步改。

---

## Phase 8 — Knowledge Object Taxonomy(Schema = OS · 角色 ⑤)

**Branch:** `claude/p8-knowledge-taxonomy`
**Bisectable commits:** 5
**Depends on:** Foundation Phase 1 merged
**Spec ref:** Cognitive §2

### Task 8.1: V35 migration —— 5 张新表 + wiki_page_templates seed

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`(add `V35_COGNITIVE_LAYER`)

- [ ] **Step 8.1.1** 新增常量(完整 SQL 见 Cognitive Spec §7.1)。在 `SQL_V33_SYMPHONY` 和已有 V34 之后:

```rust
/// V35: Cognitive Layer.
///
/// - wiki_log_events: wiki-level audit trail
/// - page_content_hashes: SHA-256 incremental compile cache
/// - review_queue_items: human-in-the-loop brake queue
/// - wiki_page_templates: per-subkind compile prompt + section schema
/// - analysis_cache: Step 1 (Analyze) LLM result cache
///
/// 全部纯 additive,无任何 ALTER TABLE。
pub const V35_COGNITIVE_LAYER: &str = "
CREATE TABLE IF NOT EXISTS wiki_log_events (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    subject_id  TEXT,
    actor       TEXT NOT NULL,
    payload_json TEXT,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_time 
    ON wiki_log_events(space_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_subject 
    ON wiki_log_events(subject_id);

CREATE TABLE IF NOT EXISTS page_content_hashes (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    sources_hash     TEXT NOT NULL,
    timeline_hash    TEXT NOT NULL,
    compiled_hash    TEXT NOT NULL,
    last_compiled_at INTEGER NOT NULL,
    last_skip_count  INTEGER NOT NULL DEFAULT 0,
    skip_reason      TEXT
);

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
CREATE INDEX IF NOT EXISTS idx_review_queue_subject 
    ON review_queue_items(subject_ids);

CREATE TABLE IF NOT EXISTS wiki_page_templates (
    subkind         TEXT PRIMARY KEY,
    display_name    TEXT NOT NULL,
    compile_prompt  TEXT NOT NULL,
    sections_json   TEXT NOT NULL,
    ui_card_layout  TEXT,
    updated_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS analysis_cache (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    inputs_hash      TEXT NOT NULL,
    analysis_json    TEXT NOT NULL,
    llm_model        TEXT,
    token_cost       INTEGER,
    created_at       INTEGER NOT NULL
);
";
```

- [ ] **Step 8.1.2** 在 `run()` 函数挂载(V34 应用之后):

```rust
tracing::debug!("Running migration V35: cognitive layer");
for stmt in V35_COGNITIVE_LAYER.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute(stmt, []) {
        tracing::warn!("V35 stmt skipped: {} :: {}", e, stmt);
    }
}
```

- [ ] **Step 8.1.3** 更新 `CLAUDE.md` Active migration registry 加 V35 行,status = "in progress (Memory OS Cognitive Layer)"。

**验证:**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# 在已有数据库的 dev 跑应用,检查 5 张表创建
sqlite3 ~/.uclaw/uclaw.db ".tables" | tr ' ' '\n' | grep -E "wiki_log_events|page_content_hashes|review_queue_items|wiki_page_templates|analysis_cache"
# 应输出 5 行
```

**Commit:** `feat(db): V35 — cognitive layer schema (5 new tables)`

### Task 8.2: subkind 概念 + 7 个内置模板 seed

**Files:**
- Create: `src-tauri/src/memory_graph/subkind.rs`
- Modify: `src-tauri/src/db/migrations.rs`(在 V35 后增加 seed)

- [ ] **Step 8.2.1** 定义 subkind enum:

```rust
// src-tauri/src/memory_graph/subkind.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WikiSubkind {
    Entity,
    Concept,
    Comparison,
    Question,
    Synthesis,
    Decision,
    Gap,
    Default,        // 兜底,未指定
}

impl WikiSubkind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "entity"     => Self::Entity,
            "concept"    => Self::Concept,
            "comparison" => Self::Comparison,
            "question"   => Self::Question,
            "synthesis"  => Self::Synthesis,
            "decision"   => Self::Decision,
            "gap"        => Self::Gap,
            _            => Self::Default,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Entity     => "entity",
            Self::Concept    => "concept",
            Self::Comparison => "comparison",
            Self::Question   => "question",
            Self::Synthesis  => "synthesis",
            Self::Decision   => "decision",
            Self::Gap        => "gap",
            Self::Default    => "default",
        }
    }
    pub fn display_name(&self) -> &'static str { /* 中文名映射 */ }
}
```

- [ ] **Step 8.2.2** Seed 7 行 `wiki_page_templates`,内含每种 subkind 的 compile prompt + sections。在 V35 之后增加常量:

```rust
pub const V35_SEED_TEMPLATES: &str = r#"
INSERT OR IGNORE INTO wiki_page_templates 
    (subkind, display_name, compile_prompt, sections_json, ui_card_layout, updated_at)
VALUES
    ('entity', '实体', '<see source file for full prompt>', 
     '["Summary","Background","Current Status","Relationships","Open Questions"]', 
     'card_layout_entity', strftime('%s','now')*1000),
    ('concept', '概念', '...', 
     '["Definition","Key Properties","Examples","Confusables","References"]', 
     'card_layout_concept', strftime('%s','now')*1000),
    -- ... 共 7 行
;
"#;
```

(完整 prompt 模板见 Cognitive Spec §2.2,实际写入时把 prompt 字符串 escape 后填入)

- [ ] **Step 8.2.3** 单测:
  - `WikiSubkind::from_str` round-trip
  - V35 + seed 跑完后 `SELECT COUNT(*) FROM wiki_page_templates` == 7

**验证:**

```bash
cd src-tauri && cargo test --lib subkind 2>&1 | tail -10
sqlite3 ~/.uclaw/uclaw.db "SELECT subkind, display_name FROM wiki_page_templates"
# 应输出 7 行
```

**Commit:** `feat(memory): WikiSubkind enum + 7 builtin page templates`

### Task 8.3: EntityPage metadata 扩展加 subkind

**Files:**
- Modify: `src-tauri/src/memory_graph/entity_page.rs`(Foundation Phase 1 已创建)

- [ ] **Step 8.3.1** `EntityPageMetadata` 增加 `subkind` 字段(注意:Foundation 已有 `subkind: Option<String>`,本 Phase 把它从 free-form string 提升为 enum):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityPageMetadata {
    // ... 现有字段 ...
    
    #[serde(default, with = "crate::memory_graph::subkind::serde_optional")]
    pub subkind: Option<WikiSubkind>,
    
    // 新增 8 keys frontmatter 硬约束(Phase 9 才完整接入,这里先占位)
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,           // UUID list of Source/Reference nodes
    #[serde(default)]
    pub tags: Vec<String>,
}
```

- [ ] **Step 8.3.2** 单测:
  - JSON 反序列化:有 subkind 字段 → enum 正确;无 → None
  - 旧格式(Foundation Phase 1 创建的 EntityPage)反序列化不报错

**Commit:** `feat(memory): EntityPage metadata adopts WikiSubkind + 8-keys frontmatter`

### Task 8.4: Tauri commands 暴露 subkind 信息

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`(`invoke_handler!`)
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 8.4.1** 新增命令:

```rust
#[tauri::command]
pub async fn wiki_list_templates(...) -> Result<Vec<TemplateDto>, String> { ... }

#[tauri::command]
pub async fn wiki_get_template(subkind: String) -> Result<Option<TemplateDto>, String> { ... }
```

- [ ] **Step 8.4.2** `memory_entity_page_create` 入参加 `subkind: Option<String>`,默认 `entity`。

- [ ] **Step 8.4.3** `main.rs::invoke_handler!` 注册 2 个新命令。

**Commit:** `feat(ipc): wiki_list_templates + wiki_get_template + subkind in create`

### Task 8.5: 前端 — QuickCapture 加 subkind 选择 + WikiView subkind 渲染

**Files:**
- Modify: `ui/src/components/memory/QuickCaptureDialog.tsx`(Foundation Phase 3 已有 "Create as EntityPage")
- Modify: `ui/src/components/memory/WikiView.tsx`
- Create: `ui/src/components/memory/wiki/SubkindCard.tsx`(分发渲染)

- [ ] **Step 8.5.1** QuickCapture 把"Create as EntityPage" 改成下拉:Entity / Concept / Comparison / Question / Synthesis / Decision / Gap。

- [ ] **Step 8.5.2** WikiView 渲染时根据 subkind 切换卡片布局:
  - **comparison**: 渲染 `## Side-by-Side` 段为表格
  - **question**: 顶部高亮 question statement,底部 "Status: open/answered/disputed" badge
  - **decision**: 顶部时间戳 + 决策人,Options Considered 列表 + Decision 高亮 + Pitfalls Avoided 红色提示
  - **gap**: 顶部"Priority: urgent/important/curious" badge,"What We Don't Know" 段加问号图标
  - 其它 subkind 用默认布局

- [ ] **Step 8.5.3** Vitest 测试每种 subkind 的渲染 fixture。

**Commit:** `feat(ui): WikiView renders 7 subkind layouts + QuickCapture subkind picker`

### Phase 8 PR shape

```
## Commits (bisectable)

| # | Subject |
|---|---|
| 1 | feat(db): V35 — cognitive layer schema (5 new tables) |
| 2 | feat(memory): WikiSubkind enum + 7 builtin page templates |
| 3 | feat(memory): EntityPage metadata adopts WikiSubkind + 8-keys frontmatter |
| 4 | feat(ipc): wiki_list_templates + wiki_get_template + subkind in create |
| 5 | feat(ui): WikiView renders 7 subkind layouts + QuickCapture subkind picker |
```

---

## Phase 9 — Page-Level Provenance(Schema = OS · 角色 ⑤)

**Branch:** `claude/p9-page-provenance`
**Bisectable commits:** 6
**Depends on:** Phase 8 merged
**Spec ref:** Cognitive §3

### Task 9.1: 5 个新 metadata 字段

**Files:**
- Modify: `src-tauri/src/memory_graph/entity_page.rs`

- [ ] **Step 9.1.1** `EntityPageMetadata` 加:

```rust
pub struct EntityPageMetadata {
    // ... existing ...

    #[serde(default = "default_confidence")]
    pub confidence: f32,                // 0.0~1.0,默认 0.5

    #[serde(default = "default_status")]
    pub status: PageKnowledgeStatus,     // 枚举,默认 Draft

    #[serde(default)]
    pub provenance_state: ProvenanceState,  // 枚举,默认 None

    #[serde(default)]
    pub contradicted_by: Vec<String>,    // node UUIDs,Phase 5 lint 同步双向

    #[serde(default)]
    pub inferred_paragraphs: Vec<usize>, // 1-indexed paragraph numbers

    #[serde(default)]
    pub paragraph_source_map: BTreeMap<String, String>, // "1" → "source:uuid:chunk-3" | "inferred"
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PageKnowledgeStatus {
    Verified,
    Draft,
    Inferred,
    Disputed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceState {
    Full,
    Partial,
    #[default]
    None,
}
```

- [ ] **Step 9.1.2** 单测覆盖默认值 + round-trip。

**Commit:** `feat(memory): EntityPage provenance schema (confidence/status/contradicted_by/inferred_paragraphs)`

### Task 9.2: `recall.rs` 接入 status_mult + confidence + provenance penalty

**Files:**
- Modify: `src-tauri/src/memory_graph/recall.rs`

- [ ] **Step 9.2.1** `compute_recall_score` 扩展(完整公式见 Cognitive Spec §3.6):

```rust
let status_mult = match status {
    PageKnowledgeStatus::Verified => 1.3,
    PageKnowledgeStatus::Draft    => 0.9,
    PageKnowledgeStatus::Inferred => 0.7,
    PageKnowledgeStatus::Disputed => 0.5,
};
score *= status_mult;
score *= metadata.confidence;
if metadata.provenance_state == ProvenanceState::None { score *= 0.6; }
```

- [ ] **Step 9.2.2** Bench:200 条历史 query,before/after 看 verified 内容召回排序提升。

**Commit:** `feat(memory): recall weights confidence + status + provenance penalty`

### Task 9.3: 双向 contradicted_by 同步逻辑

**Files:**
- Modify: `src-tauri/src/proactive/scenarios/memory_lint.rs`(Foundation Phase 5 创建)

- [ ] **Step 9.3.1** 在 lint 发现矛盾后,**同时更新两侧** metadata:
  - page_a.contradictions[].against = [page_b]
  - page_b.contradicted_by = page_b.contradicted_by ∪ [page_a]

- [ ] **Step 9.3.2** `memory_health` scenario 加新检查 `contradiction_drift`:`contradicted_by` 引用的 page 是否真在它的 `contradictions[].against` 里。漂移则修复或建 review_queue 项。

- [ ] **Step 9.3.3** 单测:模拟矛盾发现,两侧 metadata 都被更新。

**Commit:** `feat(memory-os): bidirectional contradicted_by sync in lint scenario`

### Task 9.4: 前端 — provenance 可视化

**Files:**
- Modify: `ui/src/components/memory/WikiView.tsx`
- Create: `ui/src/components/memory/wiki/ProvenanceMarkdown.tsx`(段落级渲染)

- [ ] **Step 9.4.1** `ProvenanceMarkdown` 组件:解析 markdown,按段落索引检查 `inferredParagraphs`,匹配的段落加:
  - 左侧虚线 border(`border-l-2 border-dashed border-muted-foreground/50`)
  - 灰底 (`bg-muted/30`)
  - hover 显示 tooltip "This paragraph is inferred by LLM, not directly from sources"
  - 段落首加 ⓘ 图标
- [ ] **Step 9.4.2** 页面顶部 badge 栏:
  - status badge(verified 绿/draft 黄/inferred 橙/disputed 红)
  - confidence(进度条形式,0~1)
  - provenanceState 图标(full=✓ / partial=◐ / none=○)
- [ ] **Step 9.4.3** contradicted_by 段:如果非空,顶部显示警告 banner + 跳转链接到对方 page。
- [ ] **Step 9.4.4** Theme tokens only。Vitest 测试 4 种 status 各渲染正确 badge。

**Commit:** `feat(ui): paragraph-level provenance visualization + status/confidence/contradiction badges`

### Task 9.5: View Mode Switch(Simple / Rich / Edit)

**Files:**
- Modify: `ui/src/components/memory/WikiView.tsx`
- Create: `ui/src/atoms/wiki-view-mode.ts`

- [ ] **Step 9.5.1** 顶栏 toggle:
  - **Simple**:只渲染 compiled_truth 不显示 provenance 标记(给读者用)
  - **Rich**:全部 provenance 信号(给 review 用)
  - **Edit**:可编辑 compiled_truth + paragraph_source_map(给 maintainer 用)

- [ ] **Step 9.5.2** 用户偏好持久化到 atom + localStorage(jsdom shim 已有,见 ui/src/test-utils/setup.ts)。

**Commit:** `feat(ui): WikiView mode switch (Simple/Rich/Edit)`

### Task 9.6: feature flag + Phase 9 summary

- [ ] `memubot_config.provenance_recall_weighting = true`(默认开)
- [ ] `docs/memory-os/phase-9-summary.md`

**Commit:** `feat(memory-os): Phase 9 flag + summary doc`

---

## Phase 10 — Two-Stage LLM Compile Pipeline(LLM = Compiler · 角色 ①)

**Branch:** `claude/p10-wiki-compile-pipeline`
**Bisectable commits:** 6
**Depends on:** Phase 8+9 merged
**Spec ref:** Cognitive §5

### Task 10.1: `wiki_compile.rs` 模块骨架 + LLM 调用抽象

**Files:**
- Create: `src-tauri/src/memory_graph/wiki_compile.rs`
- Modify: `src-tauri/src/memory_graph/mod.rs`

- [ ] **Step 10.1.1** 定义 `CompileDecision` / `CompileResult` / `StructuredAnalysis` / `GenerateOutput` 全部类型。

- [ ] **Step 10.1.2** `WikiCompiler::new(llm_client, store)` + `compile(node_id, decision)` 顶层 dispatcher(实际 step 在 10.2/10.3 实现)。

- [ ] **Step 10.1.3** 单测:CompileDecision::Skip 直接返回,不调 LLM。

**Commit:** `feat(memory): wiki_compile module skeleton + types`

### Task 10.2: Step 1 — Analyze

**Files:**
- Modify: `src-tauri/src/memory_graph/wiki_compile.rs`

- [ ] **Step 10.2.1** `run_analyze`:
  - 加载 sources(从 EntityPage.metadata.sources UUID 列表 → 对应 Reference 节点 active version content)
  - 加载 timeline(EntityPage.metadata.timeline)
  - 加载 previous compiled_truth(memory_versions content)
  - 构造 prompt(Cognitive Spec §5.3 Step 1)
  - 调 LLM,要求 strict JSON 输出
  - 用 serde 反序列化为 `StructuredAnalysis`,失败重试一次(retry 模板上加"You must output valid JSON only.")

- [ ] **Step 10.2.2** 写 analysis_cache 表:

```rust
analysis_cache::put(node_id, &analysis, inputs_hash, model, tokens)?;
```

- [ ] **Step 10.2.3** 单测:mock LLM 返回固定 JSON,validate 解析。

**Commit:** `feat(memory): wiki_compile Step 1 Analyze + analysis_cache`

### Task 10.3: Step 2 — Generate

**Files:**
- Modify: `src-tauri/src/memory_graph/wiki_compile.rs`

- [ ] **Step 10.3.1** `run_generate`:
  - 加载 template(`wiki_page_templates.compile_prompt`)
  - 构造 prompt(Cognitive Spec §5.3 Step 2)
  - 调 LLM,要求 markdown + paragraph map JSON 用 `===PARAGRAPH-MAP===` 分隔
  - 解析两部分,验证段落数对得上

- [ ] **Step 10.3.2** 写回 EntityPage:
  - `create_version(content=compiled_truth)`
  - 更新 metadata:`paragraph_source_map` / `inferred_paragraphs`(从 map 派生)/ `provenance_state`(根据 inferred 比例自动计算)/ `confidence`(GenerateOutput 提供)
  - emit `wiki_log_events(event_type='compile_run')`

- [ ] **Step 10.3.3** 单测:end-to-end mock,验证 metadata 字段写正确。

**Commit:** `feat(memory): wiki_compile Step 2 Generate + paragraph map persistence`

### Task 10.4: 替换 Foundation Phase 3 的 wiki_overview 直接 LLM 调用

**Files:**
- Modify: `src-tauri/src/proactive/scenarios/wiki_overview.rs`(Foundation Phase 3 创建)

- [ ] **Step 10.4.1** `WikiOverviewScenario` 内部的"直接 LLM 调用"重写为 `WikiCompiler::compile(overview_node_id, ...)`。
- [ ] **Step 10.4.2** Foundation Phase 3 的现有测试要全过(行为等价,只是路径变了)。

**Commit:** `refactor(memory-os): wiki_overview routes through wiki_compile pipeline`

### Task 10.5: Tauri commands —— 手动触发 compile

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` + `main.rs::invoke_handler!`
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] 新命令 `memory_entity_page_compile(node_id, force)` —— `force=true` 跳过 SHA-256 检查直接重编(Phase 11 才接入 hash,这里先骨架)。
- [ ] WikiView 加 "Recompile" 按钮调它。

**Commit:** `feat(ipc): manual entity_page_compile command`

### Task 10.6: feature flag + summary

- [ ] `memubot_config.two_stage_compile_enabled = true`
- [ ] `phase-10-summary.md`

**Commit:** `feat(memory-os): Phase 10 flag + summary`

---

## Phase 11 — SHA-256 Incremental Compile(LLM = Compiler · 角色 ①)

**Branch:** `claude/p11-incremental-compile`
**Bisectable commits:** 4
**Depends on:** Phase 10 merged
**Spec ref:** Cognitive §4.5

### Task 11.1: `should_recompile` 决策树

**Files:**
- Modify: `src-tauri/src/memory_graph/wiki_compile.rs`

- [ ] **Step 11.1.1** 实现:

```rust
pub fn should_recompile(&self, node_id: &str) -> Result<CompileDecision> {
    let cached = self.store.get_content_hashes(node_id)?;
    let sources_hash = sha256_concat_sources(self.store.load_sources(node_id)?);
    let timeline_hash = sha256_concat_timeline(self.store.load_timeline(node_id)?);

    match cached {
        None => Ok(CompileDecision::Full),  // 没缓存,全量编译
        Some(c) if c.sources_hash == sources_hash 
                && c.timeline_hash == timeline_hash => {
            self.store.bump_skip_count(node_id)?;
            Ok(CompileDecision::Skip { reason: "all-inputs-unchanged".into() })
        }
        Some(c) if c.sources_hash == sources_hash => {
            Ok(CompileDecision::Partial { sections: vec!["Current Status".into()] })
        }
        _ => Ok(CompileDecision::Full),
    }
}
```

- [ ] **Step 11.1.2** sha256 函数用 `sha2` crate(现在依赖里应该有 — 如果没有加进 Cargo.toml)。

**Commit:** `feat(memory): wiki_compile should_recompile decision tree`

### Task 11.2: compile 完成后写 page_content_hashes

**Files:**
- Modify: `src-tauri/src/memory_graph/wiki_compile.rs`
- Modify: `src-tauri/src/memory_graph/store.rs`(加 `upsert_content_hashes` API)

- [ ] **Step 11.2.1** Compile 成功后 `INSERT OR REPLACE INTO page_content_hashes`。

- [ ] **Step 11.2.2** 单测:
  - 第一次 compile → Full decision
  - 立即第二次 compile(无 source/timeline 变化)→ Skip,LLM 未被调用
  - 修改 timeline 后第三次 → Partial(或回退 Full,如果 Phase 11.3 partial 还没做)

**Commit:** `feat(memory): persist content hashes after compile + skip-on-cache-hit`

### Task 11.3: Partial recompile —— 只重编一段

**Files:**
- Modify: `src-tauri/src/memory_graph/wiki_compile.rs`

- [ ] **Step 11.3.1** 当 decision == Partial 时,Step 2 prompt 模板替换为"只重写指定 section,保留其它段落原样":

```
The previous compiled_truth was:
{previous_full_markdown}

Re-generate ONLY the following section(s):
{sections}

Output the FULL markdown (unchanged sections copied verbatim, target sections regenerated).
Update paragraphSourceMap for ONLY the regenerated paragraphs.
```

- [ ] **Step 11.3.2** 单测:partial path 单独 fixture。

**Commit:** `feat(memory): partial recompile preserves untouched sections`

### Task 11.4: cost dashboard 集成 + summary

- [ ] **Step 11.4.1** wiki_compile 每次 skip / partial / full 都把节省/消耗的 token 写 `cost_records`(V13),用 model 字段标 "compile-skip" / "compile-partial" / "compile-full"。
- [ ] **Step 11.4.2** UsageSettings.tsx 加 "Compile cost breakdown" 段。
- [ ] **Step 11.4.3** `phase-11-summary.md` 给出预期节省比例。

**Commit:** `feat(memory-os): Phase 11 cost telemetry + summary`

---

## Phase 12 — Control Plane Files(Wiki = Product · 角色 ③)

**Branch:** `claude/p12-control-plane`
**Bisectable commits:** 6
**Depends on:** Phase 8 merged
**Spec ref:** Cognitive §4.2-4.4

### Task 12.1: `wiki_hot` scenario —— 最近上下文热区

**Files:**
- Create: `src-tauri/src/proactive/scenarios/wiki_hot.rs`

- [ ] **Step 12.1.1** Scenario 实现:每 15 分钟跑一次,生成 `wiki_artifacts(kind="hot")`(覆盖式,只保留 1 行)。

- [ ] **Step 12.1.2** 单测:mock 数据 + LLM,断言 wiki_artifacts 表 kind="hot" 行被 upsert。

**Commit:** `feat(memory-os): wiki_hot scenario (24h rolling context cache)`

### Task 12.2: purpose.md 用户编辑流程

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`(`memory_wiki_get_purpose` / `memory_wiki_set_purpose`)
- Modify: `src-tauri/src/main.rs::invoke_handler!`
- Modify: `ui/src/components/memory/WikiView.tsx`(顶部加 Purpose 编辑入口)

- [ ] **Step 12.2.1** 两个 IPC 命令读/写 `wiki_artifacts(kind="purpose")`(只 1 行)。

- [ ] **Step 12.2.2** WikiView 顶部新增"Wiki Purpose" 折叠区,默认折叠;展开后显示当前 purpose,有"Edit"按钮进入编辑模式。

**Commit:** `feat(memory-os): purpose.md user-editable Wiki intent`

### Task 12.3: purpose.md 注入 Boot 上下文

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs`(Boot context 拼接处)

- [ ] **Step 12.3.1** Boot context 拼接时,如果 wiki_artifacts(kind="purpose") 存在,自动加到 system prompt 前部:
  ```
  ## Wiki Purpose (do not violate this scope)
  {purpose content}
  ```

- [ ] **Step 12.3.2** 单测:mock purpose 存在/不存在两种情况,验证 system prompt 是否带入。

**Commit:** `feat(memory-os): inject purpose.md into agent system prompt`

### Task 12.4: `wiki_log_events` 写入埋点

**Files:**
- Modify:多处—— `wiki_compile.rs`, `auto_link.rs`, `memory_health.rs`, `memory_lint.rs`, `tier_escalator.rs`, `tauri_commands.rs::memory_entity_page_*`
- Create: `src-tauri/src/memory_graph/wiki_log.rs`(helper)

- [ ] **Step 12.4.1** `wiki_log::emit(event_type, subject_id, actor, payload)` helper。

- [ ] **Step 12.4.2** 在 Foundation + Cognitive 所有关键写入路径调用 emit。

- [ ] **Step 12.4.3** 单测:跑一个 EntityPage create + compile,验证产生至少 3 条 log event(create / compile_run / fts_indexed)。

**Commit:** `feat(memory-os): wiki_log_events emit at all key write paths`

### Task 12.5: 前端 — WikiLogView 组件

**Files:**
- Create: `ui/src/components/memory/WikiLogView.tsx`
- Modify: `ui/src/components/memory/MemoryPanel.tsx`(新 "Log" tab)
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 12.5.1** 新命令 `memory_wiki_log_list(space_id, filter, limit, offset)`。

- [ ] **Step 12.5.2** WikiLogView:按时间倒序,可按 event_type filter,每行可点开看 payload。

- [ ] **Step 12.5.3** Vitest:渲染 30 条 mock event,断言 filter 工作。

**Commit:** `feat(ui): WikiLogView with event filtering`

### Task 12.6: hot.md 注入 + summary

- [ ] **Step 12.6.1** `memubot_config.boot_inject_hot = false`(默认关,稳定后可开)。开启时 agentic_loop 在 Boot context 后追加 hot.md。

- [ ] **Step 12.6.2** `phase-12-summary.md`。

**Commit:** `feat(memory-os): Phase 12 hot.md boot injection + summary`

---

## Phase 13 — Review Queue(Review = Brake · 角色 ⑥)

**Branch:** `claude/p13-review-queue`
**Bisectable commits:** 6
**Depends on:** Phase 8+9 merged
**Spec ref:** Cognitive §4.6

### Task 13.1: `review_queue` 写入入口

**Files:**
- Create: `src-tauri/src/memory_graph/review_queue.rs`
- Modify: `src-tauri/src/proactive/scenarios/memory_health.rs`(phantom_slug → 创 review item)
- Modify: `src-tauri/src/proactive/scenarios/memory_lint.rs`(contradiction / hub_stub → 创 review item)
- Modify: `src-tauri/src/memory_graph/wiki_compile.rs`(inferred ratio > 0.5 → 创 review item)

- [ ] **Step 13.1.1** `review_queue::create(item_kind, severity, subject_ids, title, context)` helper。

- [ ] **Step 13.1.2** 改造 4 个触发场景接入。**注意去重**:同一 `subject_id + item_kind` 已经在 open 状态时,不重复创建。

- [ ] **Step 13.1.3** 单测:mock 触发条件,验证 review_queue_items 表新行。

**Commit:** `feat(memory-os): review_queue creation from health/lint/compile triggers`

### Task 13.2: 召回打折(brake 语义)

**Files:**
- Modify: `src-tauri/src/memory_graph/recall.rs`

- [ ] **Step 13.2.1** 召回时 LEFT JOIN 一个子查询:`SELECT subject_ids FROM review_queue_items WHERE status='open' AND severity='high'`,展开 subject_ids。

- [ ] **Step 13.2.2** 命中的 node 得分 × 0.5(配置化 `memubot_config.review_brake_multiplier`)。

- [ ] **Step 13.2.3** 单测:制造一个 high severity open review item,验证对应 node 召回排序下降。

**Commit:** `feat(memory): high-severity review items brake recall ranking`

### Task 13.3: Resolution actions

**Files:**
- Create: `src-tauri/src/memory_graph/review_resolution.rs`
- Modify: `src-tauri/src/tauri_commands.rs` + invoke_handler!

- [ ] **Step 13.3.1** 7 个 resolution action:
  - `accept`(phantom_entity → 创 EntityPage)
  - `reject`(关闭,subject 不变)
  - `merge`(双 EntityPage → 一个 keep,一个 archive,timeline 合并)
  - `split`(单 EntityPage → 拆成两个)
  - `snooze`(set status='snoozed' + snooze_until)
  - `dismiss`(set status='dismissed')
  - `ask_agent`(调 LLM 给 proposed resolution,但**不自动执行**)

- [ ] **Step 13.3.2** 每个 action 都写 `wiki_log_events(event_type='review_close')`。

- [ ] **Step 13.3.3** 单测覆盖每种 action。

**Commit:** `feat(memory-os): 7 review resolution actions`

### Task 13.4: 前端 — ReviewQueuePanel

**Files:**
- Create: `ui/src/components/memory/ReviewQueuePanel.tsx`
- Modify: `ui/src/components/memory/MemoryPanel.tsx`(新 "Review" tab,badge 显示 open count)

- [ ] **Step 13.4.1** 列表按 severity (high → low) + created_at 排,每条:
  - Item kind 图标 + title
  - 上下文 diff(2 个 page 的 contradiction 对比,or phantom slug 出现次数)
  - 7 个 action 按钮(按 item_kind 显示哪些可用)

- [ ] **Step 13.4.2** "Ask Agent" 按钮:loading 状态 → 显示 LLM 建议 + "Apply" / "Override" 二选。

- [ ] **Step 13.4.3** Vitest:渲染 5 种 item_kind 各 1 个 fixture,断言对应 action 按钮显示。

**Commit:** `feat(ui): ReviewQueuePanel with 7 actions + Ask Agent`

### Task 13.5: Agent propose resolution

**Files:**
- Modify: `src-tauri/src/memory_graph/review_resolution.rs`
- Modify: `src-tauri/src/tauri_commands.rs::memory_review_ask_agent`

- [ ] **Step 13.5.1** LLM prompt 模板:把 item context + 涉及 page 的 compiled_truth 喂进去,要求输出:
  ```
  {
    "proposed_action": "accept" | "merge" | ...,
    "rationale": "...",
    "params": { /* 比如 merge 的 keeper_id */ }
  }
  ```

- [ ] **Step 13.5.2** 前端把 proposed_action 高亮,但用户拍板才生效。

**Commit:** `feat(memory-os): Agent propose review resolution via LLM`

### Task 13.6: auto-dismiss 旧 item + summary

- [ ] `memubot_config.review_auto_dismiss_after_days = 30`(default)
- [ ] `memory_health` scenario 加扫描:status=open AND severity=low AND created_at < now-30d → set status='dismissed' + log。
- [ ] `phase-13-summary.md`

**Commit:** `feat(memory-os): Phase 13 auto-dismiss + summary`

---

## Phase 14 — Adaptive RAG · Query Classifier(Chat = Entry / Graph = Nav · 角色 ②④)

**Branch:** `claude/p14-adaptive-rag`
**Bisectable commits:** 5
**Depends on:** Phase 8-13 merged
**Spec ref:** Cognitive §6

### Task 14.1: `query_classifier.rs` 模块

**Files:**
- Create: `src-tauri/src/memory_graph/query_classifier.rs`

- [ ] **Step 14.1.1** 定义 `QueryClass` enum + `classify(query) → QueryClass`。LLM 用 Haiku。

- [ ] **Step 14.1.2** 缓存(query 短哈希)避免重复分类。

- [ ] **Step 14.1.3** 单测:8 条 fixture query,断言分类正确。

**Commit:** `feat(memory): query_classifier with Haiku-based 3-way routing`

### Task 14.2: Three pipelines

**Files:**
- Modify: `src-tauri/src/memory_graph/recall.rs`
- Create: `src-tauri/src/memory_graph/multi_hop_recall.rs`
- Create: `src-tauri/src/memory_graph/synthesis_recall.rs`

- [ ] **Step 14.2.1** `adaptive_recall(query, ctx)` 顶层 dispatcher,按 QueryClass 路由。

- [ ] **Step 14.2.2** `multi_hop_recall`:解析 entity_hints → seed nodes → `graph_propagation_search`(限制边类型 + depth)→ 收集 EntityPage 的 compiled_truth → 拼 context。

- [ ] **Step 14.2.3** `synthesis_recall`:
  - 先查现有 `EntityPage(subkind=synthesis)` 匹配 topic_hint
  - 没有 → 触发 `wiki_compile::synthesize_topic` 即时创建并落库
  - 返回 synthesis page

- [ ] **Step 14.2.4** 各自单测。

**Commit:** `feat(memory): three recall pipelines (single-hop, multi-hop, synthesis)`

### Task 14.3: 接入现有 agent recall 入口

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs`(`before_llm` hook 里的 recall 调用)
- Modify: `src-tauri/src/tauri_commands.rs::memory_graph_search`

- [ ] **Step 14.3.1** `before_llm` 里的 `recall::hybrid_search` → `adaptive_recall`(用 flag 切换)。

- [ ] **Step 14.3.2** `memory_graph_search` IPC 加可选 `class_hint`,前端可强制路由(用户输入 `/synthesis ...` 走 synthesis pipeline)。

**Commit:** `feat(memory): wire adaptive_recall into agent loop + IPC override`

### Task 14.4: 前端 — query class hint UI

**Files:**
- Modify: `ui/src/components/memory/MemorySearchPanel.tsx`
- Modify: `ui/src/components/chat/ChatInput.tsx` 和 `ui/src/components/agent/AgentView.tsx`(**两个 composer 都要改,Foundation Spec §1.2 警告**)

- [ ] **Step 14.4.1** MemorySearchPanel 加 toggle:Auto / Single-hop / Multi-hop / Synthesis。

- [ ] **Step 14.4.2** Chat/Agent composer 识别 `/multi-hop` `/synthesis` slash prefix,把 hint 传给后端。
  - **CLAUDE.md 警告**:Chat 和 Agent 两个 composer 是平行实现,改一边漏另一边是常见 bug。**两个文件都改,commit 信息里 call out**。

**Commit:** `feat(ui): query class hint via slash prefix (both composers)`

### Task 14.5: feature flag + summary

- [ ] `memubot_config.adaptive_recall_enabled = false`(默认关,稳定后开)
- [ ] `phase-14-summary.md` + 简单 benchmark 表

**Commit:** `feat(memory-os): Phase 14 flag + summary`

---

## 全局验证清单(每个 Cognitive Phase 必跑)

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust build ==="  && (cd src-tauri && cargo build 2>&1 | grep -E "^error" | head)
echo "=== rust tests ==="  && (cd src-tauri && cargo test --lib 2>&1 | tail -10)
echo "=== ts ==="          && (cd ui && npx tsc --noEmit 2>&1 | head -10)
echo "=== ui tests ==="    && (cd ui && npm test -- --run 2>&1 | tail -10)

# V35 表存在性
sqlite3 ~/.uclaw/uclaw.db "SELECT name FROM sqlite_master WHERE type='table' AND name IN (
  'wiki_log_events','page_content_hashes','review_queue_items','wiki_page_templates','analysis_cache'
)"
# Expected: 5 行

# wiki_page_templates seed
sqlite3 ~/.uclaw/uclaw.db "SELECT COUNT(*) FROM wiki_page_templates"
# Expected: 7
```

---

## 回退手册

每个 Phase 都有 feature flag:

```rust
// memubot_config.rs(累积新增)
pub struct MemubotConfig {
    // ... Foundation flags ...
    
    // Cognitive
    pub provenance_recall_weighting: bool,        // Phase 9
    pub two_stage_compile_enabled: bool,          // Phase 10
    pub incremental_compile_enabled: bool,        // Phase 11
    pub boot_inject_hot: bool,                    // Phase 12 (default false)
    pub review_brake_multiplier: f32,             // Phase 13 (default 0.5,设 1.0 = 不打折)
    pub review_auto_dismiss_after_days: u32,      // Phase 13 (default 30)
    pub adaptive_recall_enabled: bool,            // Phase 14 (default false)
}
```

最坏情况:把所有 cognitive flags 关掉,uClaw 回退到 Foundation Phase 1-7 的行为(EntityPage + Wiki view + Auto-link + Health,无段落 provenance / 无两步 compile / 无 review queue / 无 adaptive RAG)。**V35 表是 additive,**不删除**——空的 review_queue_items 不影响 Foundation 跑。

---

## 与 Foundation Plan 的合并节奏建议

```
Phase 1 (Foundation, EntityPage 基础)         ←先合
Phase 2 (Foundation, Auto-link)
Phase 3 (Foundation, Wiki overview/index)
Phase 4 (Foundation, Health scenario)
Phase 5 (Foundation, Lint + boost)
Phase 6 (Foundation, Tier escalator)
Phase 7 (Foundation, Markdown sync) [可选]
———————— Foundation Layer 完工,uClaw 已升级为 entity-level 长期记忆 ————————

Phase 8 (Cognitive, 9 种 subkind)            ←Cognitive 起点
Phase 9 (Cognitive, page provenance)
Phase 10 (Cognitive, two-stage compile)
Phase 11 (Cognitive, incremental compile)
Phase 12 (Cognitive, control plane files)
Phase 13 (Cognitive, review queue)
Phase 14 (Cognitive, adaptive RAG)
———————— Cognitive Layer 完工,uClaw 成为 Tommy LLM Wiki 完整实现 ————————
```

**建议:Foundation Phase 1-6 全部合并并跑 1-2 周(收集真实 wiki 使用反馈)后再开 Cognitive Phase 8**。这样 Phase 8+ 的设计选择(比如要不要 9 种 subkind 全做、Adaptive RAG 的分类边界放哪)能基于真实数据校准,而不是空想。

---

## PR 描述模板(每个 Cognitive Phase)

```markdown
## Memory OS Cognitive Phase <N> — <name>

Implements Phase <N> of `docs/superpowers/plans/agent-memory-os-cognitive.md`.
Cognitive Spec: `docs/superpowers/specs/2026-05-18-agent-memory-os-cognitive-design.md` §<section>.

### Commits (bisectable)
<insert commit table from Phase N PR shape above>

### Verification
- cargo build: clean
- cargo test --lib: all passing (added N new tests)
- npx tsc --noEmit: 0 errors
- npm test -- --run: all passing
- DB:SELECT COUNT FROM <new table> 行数符合预期

### Feature flag
- `memubot_config.<feature>_enabled` (default: <true/false>)
- Disable to fully bypass new code path

### Tommy framework coverage
- Covered Tommy 要点 X / Y / Z(对照 cognitive spec §<section>)

### Adjacent edits (called out per CLAUDE.md)
- Phase 14 改了**两个** composer(ChatInput + AgentView)
- 注册 N 个新 tauri commands 到 `main.rs::invoke_handler!`
- 注册 N 个新 scenarios 到 ProactiveService

### Rollback
关 `memubot_config.<flag>` flag。V35 表是 additive,空表无影响。
```
