# Agent Memory OS — Foundation Layer Implementation Plan(Phase 1-7)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Layer position:** **L1 Foundation Layer (Phase 1-7)** —— 三层 Memory OS 计划的第一层。
- **L1 Foundation(本文)**:Phase 1-7
- **L2 Cognitive**:[`agent-memory-os-cognitive.md`](agent-memory-os-cognitive.md) —— Phase 8-14
- **L3 Engines**:[`agent-memory-os-engines.md`](agent-memory-os-engines.md) —— Phase 15-21

**严格顺序合并:L1 → 跑 1-2 周 → L2 → 跑 1-2 周 → L3**。完成 L1 后,uClaw 已具备实体级长期记忆 + Auto-link + AI Wiki 基础视图。

**Goal:** 把 uClaw memory 子系统升级为 Runtime-Native Agent Memory OS——引入 EntityPage 双层抽象、Auto-link 写时副作用、AI Wiki 派生层、Health/Lint 维护循环,实现"用户的第二大脑 + Agent 的持续人生记忆"两个一等公民,**全过程 additive、向后兼容、可灰度回退**。

**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`(Foundation 设计依据)
**Next layer:** [`agent-memory-os-cognitive.md`](agent-memory-os-cognitive.md)(Cognitive Phase 8-14)

**Architecture summary:**

- 9 个现有 `MemoryNodeKind` 全部保留;新增 `EntityPage` 作为第 10 个变体。
- 4 个现有 `MemoryRelationKind` 全部保留;新增 7 种 typed-edge(`WorksAt / Founded / InvestedIn / Advises / Attended / Source / Mentions`)。
- 现有 `memory_nodes / memory_versions / memory_edges / memory_routes / memory_keywords / memory_fts` 全部不动;新增 3 张表(`memory_edge_audit / wiki_artifacts / memory_health_findings`)。
- V34 是本计划占用的下一个迁移号。**注意:CLAUDE.md 的 Active migration registry 落后**——表格停在 V26,但实际 `migrations.rs` 已用到 **V33**(V27=自定义 system prompts、V28=prompt 历史、V29=逻辑压缩、V30=fragment_reviews+daily_summaries、V31=memory_fts trigram 重建、V32=IM channel、V33=Symphony)。本 PR 顺带把 registry 补全到 V34。
- 每个 Phase = 一个独立可合并 PR + bisectable commits 表(参考 PR #29/#31/#33/#35/#36 风格)。
- 每个新能力都有 `memubot_config.rs` 开关,可灰度禁用回退。

**Reference baseline file map:**
- Spec: `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`(本 plan 的设计依据)
- Active migration registry: `src-tauri/src/db/migrations.rs` 实际代码(以代码为准——`grep -n "^pub const V\|^const V\|^/// V\|^// V" src-tauri/src/db/migrations.rs` 看真实占用)。CLAUDE.md 的 registry 表落后,本 PR 同时补全

---

## Pre-flight(每个 Phase 开始前都要跑一次)

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/<phase-name>   # 比如 claude/p1-entity-page
```

- [ ] **Step 0.2: Baseline pipeline**

```bash
echo "=== rust build ==="  && (cd src-tauri && cargo build 2>&1 | grep -E "^error" | head)
echo "=== rust tests ==="  && (cd src-tauri && cargo test --lib 2>&1 | tail -8)
echo "=== ts ==="          && (cd ui && npx tsc --noEmit 2>&1 | head -10)
echo "=== ui tests ==="    && (cd ui && npm test -- --run 2>&1 | tail -10)
```
Expected:cargo 干净、tests 全过、TS 0 errors。如果 baseline 已有红,**先解决再开始**。

- [ ] **Step 0.3: Active migration check**

```bash
grep -nE "^pub const V[0-9]+|^const V[0-9]+|^/// V[0-9]+ —|^// V[0-9]+ —" /Users/ryanliu/Documents/uclaw/src-tauri/src/db/migrations.rs
```
Expected:看到 V1 到 V33 完整列表,V34+ 未占用。如果有其它 PR 抢先占了 V34,**把本计划的迁移号往后顺延**(改 `V34_MEMORY_OS_PHASE_1` 为 `V35_…`,SQL 字符串名同步改),并更新 `CLAUDE.md` 注册表。

---

## Phase 1 — EntityPage 基础(MemoryNodeKind 第 10 变体 + V34 三张新表)

**Branch:** `claude/p1-entity-page-foundation`
**Bisectable commits:** 6
**Verification window:** ~30 分钟

### Task 1.1: 新增 `EntityPage` enum 变体

**Files:**
- Modify: `src-tauri/src/memory_graph/models.rs`(MemoryNodeKind enum + serde + to_str)

- [ ] **Step 1.1.1**

Edit `models.rs`,在 `MemoryNodeKind` 现有 9 个变体后追加:

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
    #[serde(rename = "entity_page")]
    EntityPage,
}
```

如果 `MemoryNodeKind` 有 `as_str` / `from_str` / `Display` impl,把 `EntityPage` 加进去(用 `"entity_page"` 字符串)。

**验证:**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```
Expected: 0 errors。如果有 missing match arm,补齐。

**Commit:** `feat(memory): add MemoryNodeKind::EntityPage variant`

### Task 1.2: V34 migration —— 三张新表

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`(add `V34_MEMORY_OS_PHASE_1`)

- [ ] **Step 1.2.1**

在 `migrations.rs` 末尾(`SQL_V33_SYMPHONY` 之后)新增:

```rust
/// V34: Memory OS Phase 1.
///
/// - memory_edge_audit: 记录每条 edge 是 auto-link 抽出的还是用户/Agent 显式建的,
///   便于后续 reconciliation 和审计。
/// - wiki_artifacts: AI Wiki 派生物(overview / index / synthesis 等的当前快照)。
/// - memory_health_findings: Health/Lint scenarios 发现的问题清单(可 dismiss)。
///
/// 所有新表 IF NOT EXISTS,纯 additive。
pub const V34_MEMORY_OS_PHASE_1: &str = "
CREATE TABLE IF NOT EXISTS memory_edge_audit (
    edge_id     TEXT PRIMARY KEY REFERENCES memory_edges(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,
    inferred_by TEXT,
    confidence  REAL,
    extracted_from_version_id TEXT REFERENCES memory_versions(id),
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_edge_audit_src ON memory_edge_audit(source);

CREATE TABLE IF NOT EXISTS wiki_artifacts (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    kind            TEXT NOT NULL,
    content         TEXT NOT NULL,
    generated_at    INTEGER NOT NULL,
    source_node_ids TEXT NOT NULL,
    llm_model       TEXT,
    token_cost      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_space_kind ON wiki_artifacts(space_id, kind);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_generated ON wiki_artifacts(generated_at);

CREATE TABLE IF NOT EXISTS memory_health_findings (
    id            TEXT PRIMARY KEY,
    space_id      TEXT NOT NULL,
    severity      TEXT NOT NULL,
    check_kind    TEXT NOT NULL,
    subject       TEXT NOT NULL,
    payload_json  TEXT,
    is_lint       INTEGER NOT NULL DEFAULT 0,
    dismissed     INTEGER NOT NULL DEFAULT 0,
    discovered_at INTEGER NOT NULL,
    dismissed_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_health_findings_active ON memory_health_findings(space_id, dismissed, discovered_at);
CREATE INDEX IF NOT EXISTS idx_health_findings_subject ON memory_health_findings(subject);
";
```

- [ ] **Step 1.2.2** 在 `run` 函数(V33 Symphony 应用之后)挂载:

```rust
tracing::debug!("Running migration V34: memory os phase 1");
for stmt in V34_MEMORY_OS_PHASE_1.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute(stmt, []) {
        tracing::warn!("V34 stmt skipped: {} :: {}", e, stmt);
    }
}
```

- [ ] **Step 1.2.3** 更新 `CLAUDE.md` Active migration registry:**先补全 V27–V33 的现状行**(否则下一个写 spec 的人又会踩坑),然后加 V34 行,status = "in progress (Memory OS Phase 1)"。

**验证:**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# 在 ~/.uclaw/uclaw.db 备份后跑应用,检查三张表创建
sqlite3 ~/.uclaw/uclaw.db.test ".schema memory_edge_audit"
sqlite3 ~/.uclaw/uclaw.db.test ".schema wiki_artifacts"
sqlite3 ~/.uclaw/uclaw.db.test ".schema memory_health_findings"
```

**Commit:** `feat(db): V34 — Memory OS Phase 1 schema (edge audit + wiki + health)`

### Task 1.3: EntityPage metadata schema 序列化结构

**Files:**
- Create: `src-tauri/src/memory_graph/entity_page.rs`(新模块)
- Modify: `src-tauri/src/memory_graph/mod.rs`(pub mod entity_page;)

- [ ] **Step 1.3.1** 在新模块定义:

```rust
//! EntityPage metadata schema — 双层 page(compiled_truth + timeline)的反序列化结构。
//! 注意:不是表,只是 memory_nodes.metadata_json 的约定 schema。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityPageMetadata {
    /// compiled_truth 放在 memory_versions.content,不放这里;这里只放 metadata
    pub timeline: Vec<TimelineEntry>,
    pub enrichment_tier: u8,                    // 1 / 2 / 3,默认 3 (stub)
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub last_synthesized_at: Option<String>,    // ISO 8601
    #[serde(default)]
    pub synthesis_source_count: u32,
    #[serde(default)]
    pub contradictions: Vec<Contradiction>,
    #[serde(default)]
    pub slug: Option<String>,                   // human-readable identifier
    #[serde(default)]
    pub subkind: Option<String>,                // 'person' | 'company' | 'concept' | 'project' | ...
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub date: String,                            // YYYY-MM-DD
    pub source_node_id: Option<String>,
    pub source_session_id: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub between_source_ids: Vec<String>,
    pub claim_a: String,
    pub claim_b: String,
    pub noticed_at: String,
}

impl EntityPageMetadata {
    pub fn from_json(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
```

- [ ] **Step 1.3.2** 单测(inline `#[cfg(test)]`):空 metadata → 默认值;含 timeline 的 JSON → 正确解析;未知 key 不报错。

**验证:**

```bash
cd src-tauri && cargo test --lib entity_page 2>&1 | tail -10
```

**Commit:** `feat(memory): EntityPage metadata schema (compiled_truth + timeline doctrine)`

### Task 1.4: `store.rs` 新增 EntityPage 专用查询

**Files:**
- Modify: `src-tauri/src/memory_graph/store.rs`

- [ ] **Step 1.4.1** 在 `MemoryGraphStore` impl 新增:

```rust
pub fn create_entity_page(
    &self,
    space_id: &str,
    slug: &str,
    title: &str,
    compiled_truth: &str,
    initial_metadata: EntityPageMetadata,
) -> Result<MemoryNodeDetail, Error> {
    // 1. find_by_slug:看是否已有 — 若有则返回错误(去重交给 caller)
    // 2. create_node (kind = EntityPage, metadata_json = initial_metadata.to_json())
    // 3. create_version (content = compiled_truth, embedding 留空)
    // 4. (可选) create_route (domain="entity", path=slug, is_primary=1)
    // 5. hydrate 返回 detail
}

pub fn list_entity_pages(
    &self,
    space_id: &str,
    subkind_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<MemoryNodeDetail>, Error> { ... }

pub fn find_entity_page_by_slug(
    &self,
    space_id: &str,
    slug: &str,
) -> Result<Option<MemoryNodeDetail>, Error> {
    // SELECT FROM memory_nodes WHERE kind='entity_page' AND space_id=? AND
    //   json_extract(metadata_json, '$.slug') = ?
}

pub fn append_timeline_entry(
    &self,
    node_id: &str,
    entry: TimelineEntry,
) -> Result<(), Error> {
    // 1. SELECT metadata_json
    // 2. parse → EntityPageMetadata
    // 3. push entry to timeline
    // 4. UPDATE memory_nodes SET metadata_json = ?
}
```

- [ ] **Step 1.4.2** 单测:create / find / list / append timeline。

**验证:**

```bash
cd src-tauri && cargo test --lib memory_graph::store 2>&1 | tail -15
```

**Commit:** `feat(memory): store.rs CRUD for EntityPage nodes`

### Task 1.5: Tauri commands —— EntityPage IPC

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`(新增 5 个命令)
- Modify: `src-tauri/src/main.rs`(`invoke_handler!` 注册)
- Modify: `ui/src/lib/tauri-bridge.ts`(typed wrapper)

- [ ] **Step 1.5.1** 新增命令(在现有 `memory_graph_*` 命令旁边):

```rust
#[tauri::command]
pub async fn memory_entity_page_create(
    state: State<'_, AppState>,
    input: EntityPageCreateInput,
) -> Result<EntityPageDto, String> { ... }

#[tauri::command]
pub async fn memory_entity_page_get(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Option<EntityPageDto>, String> { ... }

#[tauri::command]
pub async fn memory_entity_page_find_by_slug(
    state: State<'_, AppState>,
    space_id: String,
    slug: String,
) -> Result<Option<EntityPageDto>, String> { ... }

#[tauri::command]
pub async fn memory_entity_page_list(
    state: State<'_, AppState>,
    input: EntityPageListInput,
) -> Result<Vec<EntityPageDto>, String> { ... }

#[tauri::command]
pub async fn memory_entity_page_append_timeline(
    state: State<'_, AppState>,
    node_id: String,
    entry: TimelineEntryDto,
) -> Result<(), String> { ... }
```

- [ ] **Step 1.5.2** 在 `main.rs::invoke_handler!` 宏里**逐个注册**这 5 个命令(`CLAUDE.md` 警告:漏一个编译过但运行时 not found)。

- [ ] **Step 1.5.3** 在 `ui/src/lib/tauri-bridge.ts` 写对应 invoke 包装 + TS 类型定义。

**验证:**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
# 启动 dev:cargo tauri dev,在 DevTools console 跑:
#   await __TAURI__.core.invoke('memory_entity_page_list', { input: { spaceId: 'default', limit: 10 }})
# 应返回 []
```

**Commit:** `feat(ipc): tauri commands for EntityPage CRUD + invoke_handler registration`

### Task 1.6: Phase 1 feature flag + 文档

**Files:**
- Modify: `src-tauri/src/memubot_config.rs`(加 `entity_page_enabled: bool`,默认 true)
- Create: `docs/memory-os/phase-1-summary.md`(简短描述、灰度方法)

- [ ] **Step 1.6.1** `memubot_config.rs`:

```rust
pub struct MemubotConfig {
    // ... 现有字段 ...
    #[serde(default = "default_true")]
    pub entity_page_enabled: bool,
}

fn default_true() -> bool { true }
```

- [ ] **Step 1.6.2** 把 `entity_page_enabled` 在所有相关 commands 入口检查;如关闭则返回友好错误。

- [ ] **Step 1.6.3** 写 phase-1-summary.md(< 30 行):本 PR 做了什么、关掉怎么关、回滚什么。

**验证:**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib 2>&1 | tail -8
```

**Commit:** `feat(memory-os): Phase 1 feature flag (entity_page_enabled) + summary doc`

### Phase 1 PR shape

```
## Commits (bisectable)

| # | Subject |
|---|---|
| 1 | feat(memory): add MemoryNodeKind::EntityPage variant |
| 2 | feat(db): V34 — Memory OS Phase 1 schema (edge audit + wiki + health) |
| 3 | feat(memory): EntityPage metadata schema (compiled_truth + timeline doctrine) |
| 4 | feat(memory): store.rs CRUD for EntityPage nodes |
| 5 | feat(ipc): tauri commands for EntityPage CRUD + invoke_handler registration |
| 6 | feat(memory-os): Phase 1 feature flag + summary doc |
```

---

## Phase 2 — Auto-Link Post-Hook(zero-LLM 写时建图)

**Branch:** `claude/p2-memory-os-auto-link`
**Bisectable commits:** 5
**Depends on:** Phase 1 merged

### Task 2.1: `MemoryRelationKind` 新增 7 个 typed-edge

**Files:**
- Modify: `src-tauri/src/memory_graph/models.rs`

- [ ] **Step 2.1.1** 在 enum 现有 4 个变体后追加:

```rust
pub enum MemoryRelationKind {
    Contains,
    RelatesTo,
    Timeline,
    Trigger,
    #[serde(rename = "works_at")]
    WorksAt,
    Founded,
    #[serde(rename = "invested_in")]
    InvestedIn,
    Advises,
    Attended,
    Source,
    Mentions,
}
```

- [ ] **Step 2.1.2** `FromStr` / `as_str` / `Display` impl 加 7 个新分支。**用 snake_case** 跟 serde 一致。

- [ ] **Step 2.1.3** 单测:旧字符串 (`"contains"`)、新字符串 (`"works_at"`)、未知字符串(应 fallthrough 到 `Mentions`)。

**验证:**

```bash
cd src-tauri && cargo test --lib models 2>&1 | tail -10
```

**Commit:** `feat(memory): add 7 typed MemoryRelationKind variants (works_at/founded/...)`

### Task 2.2: Reference extractor + link-type inferrer

**Files:**
- Create: `src-tauri/src/memory_graph/auto_link.rs`(新模块)

- [ ] **Step 2.2.1** 实现:

```rust
//! Auto-link extraction inspired by gbrain/src/core/link-extraction.ts:
//! 把 LLM 自然写出来的引用文本零成本抽成 typed-edge。

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExtractedRef {
    EntitySlug(String),                  // [[entity:slug]]
    NodeUuid(String),                    // [[node:uuid]]
    MarkdownLink { display: String, path: String },  // [Text](entity/slug)
}

static RE_ENTITY: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[entity:([a-z0-9-]+)\]\]").unwrap());
static RE_NODE:   Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[node:([a-f0-9-]{36})\]\]").unwrap());
static RE_MD:     Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\(entity/([a-z0-9-]+)\)").unwrap());
static RE_FENCE:  Lazy<Regex> = Lazy::new(|| Regex::new(r"```[\s\S]*?```").unwrap());

/// 从 markdown content 抽 refs。先把 code fence 抠掉(避免代码里 slug 被误识别)。
pub fn extract_refs(content: &str) -> Vec<ExtractedRef> {
    let stripped = RE_FENCE.replace_all(content, "");
    let mut out = Vec::new();
    for cap in RE_ENTITY.captures_iter(&stripped) { out.push(ExtractedRef::EntitySlug(cap[1].to_string())); }
    for cap in RE_NODE.captures_iter(&stripped)   { out.push(ExtractedRef::NodeUuid(cap[1].to_string())); }
    for cap in RE_MD.captures_iter(&stripped)     { out.push(ExtractedRef::MarkdownLink {
        display: cap[1].to_string(), path: cap[2].to_string()
    }); }
    out
}

pub fn infer_link_type(
    src_kind: MemoryNodeKind,
    dst_kind: MemoryNodeKind,
    context_text: &str,
) -> MemoryRelationKind {
    use MemoryNodeKind::*;
    use MemoryRelationKind::*;
    let lower = context_text.to_lowercase();
    match (src_kind, dst_kind) {
        (UserProfile | EntityPage, EntityPage) => {
            if lower.contains("works at") || lower.contains("works as") || lower.contains("员工")
                || lower.contains("在职于")                                          { WorksAt }
            else if lower.contains("founded") || lower.contains("创立") || lower.contains("creator of") { Founded }
            else if lower.contains("invested in") || lower.contains("领投") || lower.contains("backed") { InvestedIn }
            else if lower.contains("advises") || lower.contains("advisor") || lower.contains("顾问")     { Advises }
            else if lower.contains("attended") || lower.contains("出席") || lower.contains("alumni")     { Attended }
            else                                                                                          { Mentions }
        }
        (_, Reference) => Source,
        _ => Mentions,
    }
}
```

- [ ] **Step 2.2.2** 单测:每条 regex 各 2 case;`infer_link_type` 每个 branch 1 case。

**验证:**

```bash
cd src-tauri && cargo test --lib auto_link 2>&1 | tail -10
```

**Commit:** `feat(memory): auto_link reference extractor + heuristic typer (zero-LLM)`

### Task 2.3: 把 hook 挂到 `store.rs::create_version`

**Files:**
- Modify: `src-tauri/src/memory_graph/store.rs`

- [ ] **Step 2.3.1** 改 `create_version` 在 `INSERT memory_fts` 之后加:

```rust
// Auto-link post-hook(失败不致命)。
if self.config.read().auto_link_enabled {
    if let Err(e) = self.run_auto_link_extraction(conn, version) {
        tracing::warn!("auto-link failed for version {}: {} (non-fatal)", version.id, e);
    }
}
```

- [ ] **Step 2.3.2** 实现 `run_auto_link_extraction`:

```rust
fn run_auto_link_extraction(
    &self,
    conn: &rusqlite::Connection,
    version: &MemoryVersion,
) -> Result<(), Error> {
    use crate::memory_graph::auto_link::*;

    // 1. 抽当前 version 的 refs
    let refs: HashSet<_> = extract_refs(&version.content).into_iter().collect();

    // 2. 抽前一个 active version 的 refs(用于 stale-reconciliation)
    let prev_refs: HashSet<_> = self.get_previous_version_refs(conn, &version.node_id, &version.id)?;

    // 3. added = current - prev — 写新 edge
    for r in refs.difference(&prev_refs) {
        if let Some(dst_id) = self.resolve_ref(conn, version.space_id.as_str(), r)? {
            let dst_kind = self.get_node_kind(conn, &dst_id)?;
            let src_kind = version.node_kind;
            let link_type = infer_link_type(src_kind, dst_kind, &version.content);
            let edge_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT OR IGNORE INTO memory_edges (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 'private', 50, ?, ?)",
                params![edge_id, version.space_id, version.node_id, dst_id, link_type.as_str(), now_ms(), now_ms()],
            )?;
            // audit
            conn.execute(
                "INSERT OR IGNORE INTO memory_edge_audit (edge_id, source, inferred_by, confidence, extracted_from_version_id, created_at)
                 VALUES (?, 'auto_link', 'heuristic', 0.6, ?, ?)",
                params![edge_id, version.id, now_ms()],
            )?;
        }
    }

    // 4. removed = prev - current — stale reconciliation(只删 source='auto_link' 的边,绝不动 explicit)
    for r in prev_refs.difference(&refs) {
        if let Some(dst_id) = self.resolve_ref(conn, version.space_id.as_str(), r)? {
            conn.execute(
                "DELETE FROM memory_edges
                 WHERE id IN (
                   SELECT e.id FROM memory_edges e
                   JOIN memory_edge_audit a ON a.edge_id = e.id
                   WHERE e.parent_node_id = ? AND e.child_node_id = ? AND a.source = 'auto_link'
                 )",
                params![version.node_id, dst_id],
            )?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2.3.3** 单测覆盖:
  - 写入含 `[[node:uuid]]` 的 version,自动产生 edge + audit
  - 改 version 文本去掉引用,stale edge 被删
  - 但若手动建过同对 edge(source='explicit'),stale reconciliation **不应该删它**

**验证:**

```bash
cd src-tauri && cargo test --lib store::tests::auto_link 2>&1 | tail -15
```

**Commit:** `feat(memory): wire auto_link post-hook into create_version (with stale reconciliation)`

### Task 2.4: 显式建边路径标记为 `source='explicit'`

**Files:**
- Modify: `src-tauri/src/memory_graph/store.rs::create_edge`

- [ ] **Step 2.4.1** 把现有 `create_edge` 改成在 INSERT memory_edges 后也 INSERT memory_edge_audit(source='explicit'),保证审计完整。

- [ ] **Step 2.4.2** 单测:`create_edge` 后查 `memory_edge_audit` 应有对应行,source='explicit'。

**验证:**

```bash
cd src-tauri && cargo test --lib store::tests 2>&1 | tail -10
```

**Commit:** `feat(memory): explicit create_edge also writes audit row (source='explicit')`

### Task 2.5: Phase 2 feature flag + 前端图谱节点着色

**Files:**
- Modify: `src-tauri/src/memubot_config.rs`(加 `auto_link_enabled: bool`,默认 true)
- Modify: `ui/src/components/memory/MemoryGraphView.tsx`(typed-edge 颜色 + 节点 kind 着色)

- [ ] **Step 2.5.1** Config 加 `auto_link_enabled`。

- [ ] **Step 2.5.2** Frontend:`MemoryGraphView.tsx` 用 theme tokens 区分:
  - EntityPage 节点:`bg-accent` border `text-foreground`
  - 7 种 typed-edge 用不同 stroke pattern(实线/虚线/点划线/虚点线)
  - **禁止** hardcoded `bg-zinc-*` `text-gray-*`(CLAUDE.md 主题约束)

- [ ] **Step 2.5.3** Vitest 单测(`MemoryGraphView.test.tsx`):mock memory_graph_search 返回带 EntityPage 节点 + typed edge,断言 DOM 节点带正确 className。

**验证:**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run MemoryGraphView 2>&1 | tail -10
```

**Commit:** `feat(memory-os): Phase 2 feature flag + frontend typed-edge visualization`

### Phase 2 PR shape

```
## Commits (bisectable)

| # | Subject |
|---|---|
| 1 | feat(memory): add 7 typed MemoryRelationKind variants |
| 2 | feat(memory): auto_link reference extractor + heuristic typer (zero-LLM) |
| 3 | feat(memory): wire auto_link post-hook into create_version (with stale reconciliation) |
| 4 | feat(memory): explicit create_edge also writes audit row |
| 5 | feat(memory-os): Phase 2 feature flag + frontend typed-edge visualization |
```

---

## Phase 3 — AI Wiki(overview / index + WikiView 前端)

**Branch:** `claude/p3-memory-os-wiki`
**Bisectable commits:** 6
**Depends on:** Phase 1 merged

### Task 3.1: `wiki_overview.rs` proactive scenario

**Files:**
- Create: `src-tauri/src/proactive/scenarios/wiki_overview.rs`
- Modify: `src-tauri/src/proactive/scenarios/mod.rs`(pub mod wiki_overview;)

- [ ] **Step 3.1.1** 实现 scenario:

```rust
pub struct WikiOverviewScenario {
    last_run_at: Mutex<Option<Instant>>,
    new_pages_since_last_run: AtomicU32,
    config: WikiOverviewConfig,  // 阈值、模型、prompt
}

impl WikiOverviewScenario {
    const REGEN_THRESHOLD: u32 = 10;  // 10 个新 EntityPage 之后才跑

    async fn maybe_regenerate(&self, ctx: &ScenarioContext) -> Result<()> {
        if self.new_pages_since_last_run.load(SeqCst) < Self::REGEN_THRESHOLD { return Ok(()); }
        // 1. SELECT 最近 50 个 EntityPage(按 updated_at)
        // 2. SELECT 最近 100 个 Episode(按 created_at)
        // 3. LLM prompt: "根据以下 EntityPage 和 Episode,写一份 overview.md..."
        // 4. INSERT wiki_artifacts (kind='overview', content=resp, generated_at=now, source_node_ids=json([uuids]), llm_model=..., token_cost=...)
        // 5. reset counter
    }
}
```

- [ ] **Step 3.1.2** Index.md 生成走类似流程,但**完全 SQL,无 LLM**——按 kind/subkind 分组列 EntityPage,写到 `wiki_artifacts (kind='index')`。

- [ ] **Step 3.1.3** 单测:mock LLM client,跑 maybe_regenerate,断言 wiki_artifacts 表有新行。

**验证:**

```bash
cd src-tauri && cargo test --lib proactive::scenarios::wiki_overview 2>&1 | tail -10
```

**Commit:** `feat(memory-os): wiki_overview scenario (overview.md + index.md regen)`

### Task 3.2: 注册 scenario + InfraService 订阅 EntityPage create

**Files:**
- Modify: `src-tauri/src/proactive/service.rs`(注册 WikiOverviewScenario)
- Modify: `src-tauri/src/memubot_config.rs`(`wiki_overview_enabled: bool`)
- Modify: `src-tauri/src/memory_graph/store.rs::create_entity_page`(emit InfraService 事件)

- [ ] **Step 3.2.1** `service.rs::tick_inner` 在循环里调 `wiki_overview.maybe_regenerate(ctx).await`。

- [ ] **Step 3.2.2** `create_entity_page` 在事务提交后 emit `entity_page_created` 事件,wiki_overview 订阅并 `fetch_add(1)`。

**验证:**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -10
```

**Commit:** `feat(memory-os): wire wiki_overview into ProactiveService tick loop`

### Task 3.3: Tauri commands —— wiki read/regenerate

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`(invoke_handler!)
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 3.3.1** 新增 3 个命令:`memory_wiki_get_overview` / `memory_wiki_get_index` / `memory_wiki_regenerate`(后者强制立即跑 scenario)。

- [ ] **Step 3.3.2** 在 `main.rs::invoke_handler!` 注册三个。

- [ ] **Step 3.3.3** TS 包装。

**验证:**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

**Commit:** `feat(ipc): wiki get/regenerate tauri commands`

### Task 3.4: 前端 `WikiView.tsx` 组件

**Files:**
- Create: `ui/src/components/memory/WikiView.tsx`
- Create: `ui/src/components/memory/EntityPageEditor.tsx`
- Modify: `ui/src/components/memory/MemoryPanel.tsx`(新增 "Wiki" tab)
- Create: `ui/src/atoms/wiki-atoms.ts`(Jotai atoms)

- [ ] **Step 3.4.1** `WikiView.tsx`:
  - 左侧:`index.md` 渲染的目录树,点击节点跳右侧
  - 右侧:选中 EntityPage 时显示 `compiled_truth`(markdown) + 折叠 timeline
  - 顶部:`overview.md` 的折叠展示(默认折叠,点击展开看全局)
  - 右上角"Regenerate" 按钮 → 调 `memory_wiki_regenerate`
  - **theme tokens only**(`bg-popover` `text-muted-foreground` `border-border`)

- [ ] **Step 3.4.2** `EntityPageEditor.tsx`:可编辑 compiled_truth(textarea 或简易 markdown editor),保存触发 `memory_graph_create_node`(新版本 via update_node + create_version)。

- [ ] **Step 3.4.3** `MemoryPanel.tsx` Tab 列表加 "Wiki" 项,渲染 WikiView。

- [ ] **Step 3.4.4** Vitest 单测:render WikiView with mocked atoms,断言 overview / index 渲染、点击 entity 渲染 timeline。

**验证:**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run WikiView 2>&1 | tail -10
```

**Commit:** `feat(ui): WikiView component with EntityPage editor`

### Task 3.5: `QuickCaptureDialog` 加 "Create as EntityPage" 选项

**Files:**
- Modify: `ui/src/components/memory/QuickCaptureDialog.tsx`

- [ ] **Step 3.5.1** 在 dialog 的 kind 下拉里加 "EntityPage";选中后表单切换为 EntityPage 模式(要求 slug + compiled_truth + 可选 subkind)。

**验证:**

```bash
cd ui && npm test -- --run QuickCaptureDialog 2>&1 | tail -10
```

**Commit:** `feat(ui): QuickCapture supports creating EntityPage`

### Task 3.6: 文档 + Phase 3 summary

**Files:**
- Create: `docs/memory-os/phase-3-summary.md`

- [ ] **Step 3.6.1** 写 phase-3-summary.md:演示 5 步 demo(create page → write timeline → trigger regen → 看 overview → 编辑 compiled_truth)。

**Commit:** `docs(memory-os): Phase 3 summary (Wiki view demo)`

### Phase 3 PR shape

```
## Commits (bisectable)

| # | Subject |
|---|---|
| 1 | feat(memory-os): wiki_overview scenario |
| 2 | feat(memory-os): wire wiki_overview into ProactiveService |
| 3 | feat(ipc): wiki get/regenerate tauri commands |
| 4 | feat(ui): WikiView component with EntityPage editor |
| 5 | feat(ui): QuickCapture supports creating EntityPage |
| 6 | docs(memory-os): Phase 3 summary |
```

---

## Phase 4 — Memory Health Scenario(0 LLM 维护循环)

**Branch:** `claude/p4-memory-os-health`
**Bisectable commits:** 5
**Depends on:** Phase 1 merged

### Task 4.1: `memory_health.rs` scenario

**Files:**
- Create: `src-tauri/src/proactive/scenarios/memory_health.rs`

- [ ] **Step 4.1.1** 实现 7 项检查(全 SQL,无 LLM,详见 spec §3.1 A3 表):
  - Orphan node、Stub node、Dangling FTS、Index drift、Phantom slug、Empty version chain、Missing primary route
- [ ] **Step 4.1.2** 每项产生 `HealthFinding { severity, check_kind, subject, payload_json }`,批量 INSERT 到 `memory_health_findings`(去重:`subject + check_kind` 已存在且未 dismissed 则跳过)。
- [ ] **Step 4.1.3** 节流:5 分钟跑一次(`Mutex<Instant>` last_run)。

**单测:** 7 项 fixture data,each emits exactly N findings,verify 表行。

**Commit:** `feat(memory-os): memory_health scenario (7 zero-LLM checks)`

### Task 4.2: Wire into ProactiveService

**Files:**
- Modify: `src-tauri/src/proactive/service.rs`
- Modify: `src-tauri/src/memubot_config.rs`(`memory_health_enabled: bool`)

**Commit:** `feat(memory-os): wire memory_health into tick loop`

### Task 4.3: Tauri commands —— health list/dismiss

- [ ] `memory_health_list_findings`、`memory_health_dismiss_finding`、`memory_health_run_now`
- [ ] `main.rs::invoke_handler!` 注册
- [ ] TS wrapper

**Commit:** `feat(ipc): health findings list/dismiss tauri commands`

### Task 4.4: 前端 `MemoryHealthPanel.tsx`

**Files:**
- Create: `ui/src/components/memory/MemoryHealthPanel.tsx`
- Modify: `ui/src/components/memory/MemoryPanel.tsx`(新 "Health" tab,badge 显示 active finding 数)

- [ ] List findings group by severity (error / warn / info),每项可"Dismiss"或"Go to node"。
- [ ] **Theme tokens only**。
- [ ] Vitest 测试。

**Commit:** `feat(ui): MemoryHealthPanel with dismiss + jump-to-node`

### Task 4.5: Phase 4 summary

**Commit:** `docs(memory-os): Phase 4 summary`

---

## Phase 5 — Hybrid Retrieval Boost + Memory Lint(LLM 维护)

**Branch:** `claude/p5-memory-os-lint-and-boost`
**Bisectable commits:** 6
**Depends on:** Phase 1+2+4 merged

### Task 5.1: `recall.rs` 加 compiled_truth boost + backlink boost

**Files:**
- Modify: `src-tauri/src/memory_graph/recall.rs`

- [ ] **Step 5.1.1** `compute_recall_score` 加 EntityPage × 1.5 boost(可配置,Phase 5 灰度 × 1.2,1 周后调到 × 1.5)。
- [ ] **Step 5.1.2** 查询时 LEFT JOIN `(SELECT child_node_id, COUNT(*) AS bc FROM memory_edges GROUP BY child_node_id)`,得 backlink_count;`log10(1 + bc) * 0.3` 加到 score。
- [ ] **Step 5.1.3** Bench:对 100 条历史 query 跑 before/after,断言 top-5 hit rate 上升 ≥ 5 pp(没指标就先记录绝对值)。

**Commit:** `feat(memory): compiled_truth + backlink boost in recall ranking`

### Task 5.2: RRF fusion(对接现有 FTS + memU vector)

- [ ] **Step 5.2.1** 把 `recall.rs` 原来"BFS 拼接 keyword"改成:
  - 跑 FTS(top-100)→ rank by FTS score
  - 跑 memU vector(top-100)→ rank by cosine
  - RRF fusion `score = Σ 1 / (60 + rank)`
  - 然后再叠 §5.1 的 boost
- [ ] **Step 5.2.2** 保留原算法作为 fallback,可在 config 切换:`recall_strategy: 'legacy' | 'rrf'`,默认 `legacy`,新功能灰度。

**Commit:** `feat(memory): RRF fusion for hybrid retrieval (opt-in, legacy default)`

### Task 5.3: `memory_lint.rs` scenario(LLM)

**Files:**
- Create: `src-tauri/src/proactive/scenarios/memory_lint.rs`

- [ ] **Step 5.3.1** 4 项 LLM 检查(spec §3.2 B2):Hub stub / Phantom hub / Stale summary / Contradictory facts。
- [ ] **Step 5.3.2** 触发节奏:**每 15 次 EntityPage 写入跑一次**,且每天最多 4 次。
- [ ] **Step 5.3.3** Lint finding 写 `memory_health_findings` 表,`is_lint=1`。
- [ ] **Step 5.3.4** 把 token cost 写 `cost_records`(V13)以便 cost dashboard 监控。

**Commit:** `feat(memory-os): memory_lint scenario (4 LLM checks with rate limit)`

### Task 5.4: 矛盾事实处理(Lint → Contradictions in metadata)

- [ ] **Step 5.4.1** 当 lint 发现 contradiction,把它写到对应 EntityPage 的 `metadata_json.contradictions[]`,而不只是 health_findings。
- [ ] **Step 5.4.2** 前端 EntityPageEditor 显示 `## Contradictions` 段,可手动 resolve(删除该项)。

**Commit:** `feat(memory-os): contradictions persist into EntityPage metadata + UI`

### Task 5.5: Cost guardrail

- [ ] **Step 5.5.1** `memubot_config.rs` 加 `memory_lint_daily_token_budget: u32 = 50000`。
- [ ] **Step 5.5.2** Lint scenario 起跑前从 `cost_records` 查今日 lint 消耗,超 budget 跳过本次。

**Commit:** `feat(memory-os): lint daily token budget enforcement`

### Task 5.6: Phase 5 summary

**Commit:** `docs(memory-os): Phase 5 summary (boost + lint with cost guard)`

---

## Phase 6 — Tier-Escalating Enrichment

**Branch:** `claude/p6-memory-os-tier-escalation`
**Bisectable commits:** 4
**Depends on:** Phase 1+3+5 merged

### Task 6.1: `tier_escalator.rs` scenario

- [ ] 每 EntityPage 计算 mention_count = `SELECT COUNT(*) FROM memory_edges WHERE child_node_id = ?`
- [ ] 阈值:1-2 → Tier 3,3-7 → Tier 2,≥8 → Tier 1
- [ ] 升级到 Tier 2 / Tier 1 触发 `WikiOverviewScenario::synthesize_entity(node_id)` LLM 重写 compiled_truth(限速:每天最多升级 10 个 entity)

**Commit:** `feat(memory-os): tier_escalator scenario (mention-count → enrichment tier)`

### Task 6.2: Entity synthesizer

- [ ] LLM prompt:输入 EntityPage 的 timeline(最近 50 条)+ 旧 compiled_truth,输出新 compiled_truth + 更新 aliases[]。
- [ ] 写新 version(走 `create_version`,享 auto-link 副作用)。

**Commit:** `feat(memory-os): EntityPage synthesizer (timeline → compiled_truth)`

### Task 6.3: 前端 Tier badge

- [ ] WikiView 渲染 Tier 1/2/3 badge + 最近合成时间。
- [ ] 可手动触发"Synthesize now"。

**Commit:** `feat(ui): EntityPage tier badge + manual synthesize`

### Task 6.4: Phase 6 summary

**Commit:** `docs(memory-os): Phase 6 summary`

---

## Phase 7 — Markdown 双向同步(Tier C1,可选)

**Branch:** `claude/p7-memory-os-markdown-sync`
**Bisectable commits:** 5
**Depends on:** Phase 1+3 merged
**Risk:** **较高**(用户编辑 + Agent 写入的冲突);建议 Phase 1-6 上线 1 个月稳定后再做

### Task 7.1: Export-to-markdown(单向)

- [ ] **Step 7.1.1** 新命令 `memory_wiki_export`,把所有 EntityPage 导出到 `~/Documents/workground/brain/<subkind>/<slug>.md`,frontmatter 携带 `node_uuid` 与 `last_synced_version_id`。
- [ ] **Step 7.1.2** overview.md / index.md 也同步导出到 `~/Documents/workground/brain/overview.md` / `index.md`。

**Commit:** `feat(memory-os): unidirectional export to ~/Documents/workground/brain/`

### Task 7.2: Sync-from-markdown(双向)

- [ ] **Step 7.2.1** 新命令 `memory_wiki_sync_from_disk`:扫描 brain/ 目录,对每个 md 文件比对 mtime 与 `last_synced_version_id` 对应的 version `created_at`。
- [ ] **Step 7.2.2** 改动检测到 → 创建新 version(content = 文件正文),frontmatter 解析为 metadata。
- [ ] **Step 7.2.3** auto-link 自动跑(已 wired)。

**Commit:** `feat(memory-os): bidirectional sync — disk changes flow back to memory_graph`

### Task 7.3: 冲突解决规则

- [ ] **Step 7.3.1** 如果用户编辑后,disk version 与 DB 中最新 version 都比 last_synced 新 → 报 `MemoryHealthFinding(severity=error, check_kind="sync_conflict")`,UI 显示 diff,要求用户选择哪边胜出。
- [ ] **Step 7.3.2** 灰度默认"disk wins"(用户优先,符合 gbrain 的 human-always-wins 原则)。

**Commit:** `feat(memory-os): conflict resolution rules + UI diff`

### Task 7.4: 文件观察(可选 - 实时同步)

- [ ] 用 `notify` crate 监听 brain/ 目录改动,自动调 sync。

**Commit:** `feat(memory-os): fs watcher for live sync`

### Task 7.5: Phase 7 summary + 用户教程

**Commit:** `docs(memory-os): Phase 7 summary + user guide`

---

## Phase 8(Future)— Pluggable Engine

不在本计划范围。占位:抽 `MemoryGraphEngine` trait,SQLite 默认,Postgres / Turso 作为可选后端,支持云端多端同步。

---

## 全局验证清单(每个 Phase 必跑)

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust build ==="  && (cd src-tauri && cargo build 2>&1 | grep -E "^error" | head)
echo "=== rust tests ==="  && (cd src-tauri && cargo test --lib 2>&1 | tail -10)
echo "=== ts ==="          && (cd ui && npx tsc --noEmit 2>&1 | head -10)
echo "=== ui tests ==="    && (cd ui && npm test -- --run 2>&1 | tail -10)

# 数据库 schema 检查(运行过应用之后,本地 db)
sqlite3 ~/.uclaw/uclaw.db "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'memory_%' OR name LIKE 'wiki_%'"
# Expected:memory_nodes / memory_versions / memory_edges / memory_routes / memory_keywords / memory_edge_audit / wiki_artifacts / memory_health_findings

# 迁移 idempotency 检查
# 在已有数据的 db 上重启应用,所有 V34+ statements 应当 IF NOT EXISTS 触发 skip,无 ERROR 日志
```

---

## 回退手册

每个 Phase 上线后,如果出严重问题:

1. **First step:** 关 feature flag。`memubot_config.rs` 把对应 `_enabled: false`,重启,新代码路径全部跳过。
2. **若 flag 关不掉(scenario 已写脏数据):** 用 `memory_health_list_findings` 看是否有大量错误 finding;如有,关 scenario + 手动跑清理 SQL(每个 phase 给清理 SQL,见 `docs/memory-os/phase-N-summary.md`)。
3. **Migrations 不可回退**(SQLite 不支持 DROP COLUMN 之前的 alter),但所有新表是 additive,**空表不影响现有功能**——不必回滚 migration,只关 flag。
4. **Auto-link 写脏边:** 删除 `memory_edge_audit` 里 `source='auto_link'` 的所有 audit,以及 `memory_edges` 里对应 edge_id 的行(SQL 给在 phase-2-summary.md)。
5. **EntityPage 误创:** `DELETE FROM memory_nodes WHERE kind='entity_page' AND created_at > <昨日 ms>`(级联删除已通过 FK 配置)。

---

## 与 Spec 的双向链接

- 设计依据:`docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`
- 本 plan 实现 Spec 的 §4 集成设计 + §6 Migration 总览
- Spec §7 Future Work 是 Phase 8+ 的展开

PR 描述模板(每个 Phase):

```markdown
## Memory OS Phase <N> — <name>

Implements Phase <N> of `docs/superpowers/plans/agent-memory-os.md`.
Spec: `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`.

### Commits (bisectable)
<insert commit table from Phase N PR shape above>

### Verification
- cargo build: clean
- cargo test --lib: all passing (added N new tests)
- npx tsc --noEmit: 0 errors
- npm test -- --run: all passing

### Feature flag
- `memubot_config.<phaseN>_enabled` (default: true)
- Disable to fully bypass new code path

### Rollback
See "回退手册" in plan doc.

### Adjacent edits (called out per CLAUDE.md)
- Registered N new tauri commands in `main.rs::invoke_handler!`
- Registered N new scenarios in `service.rs::tick_inner`
- N new files; no deletions; no modifications to V1-V33 migrations
```
