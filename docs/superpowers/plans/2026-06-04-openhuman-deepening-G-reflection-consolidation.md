# openhuman Slice G — Reflection Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Stop reflection from minting a brand-new isolated node per extracted fact. Add dedup → merge → light link, reusing skill_parser's dedup machinery (extracted to a shared module). G2 (multi-hop recall) is already implemented — out of scope.

**Spec:** `docs/superpowers/specs/2026-06-04-openhuman-deepening-G-reflection-consolidation-design.md`

---

## Pinned facts (from recon — verbatim, do not re-derive)

- **`persist_items_to_graph`** (`memory_graph/reflection.rs:148-308`): loops `&[ExtractedItem]`. `ExtractedItem { memory_type: String, content: String }` (extractor.rs:20). Node `title` = first 50 chars (char-boundary-safe) of `content` (reflection.rs:164-175). Version `content` = full `summary`. Returns `Vec<PersistedFact { node_id, memu_type, content }>` (reflection.rs:129).
- **`map_memu_type_to_kind`** (reflection.rs:11): profile→UserProfile("user_profile"), event→Episode("episode"), knowledge→Reference("reference"), behavior→Directive("directive"), skill/tool→Procedure. **Reflection (non-Procedure) kinds = `["user_profile","episode","reference","directive"]`.**
- **Dedup helpers to EXTRACT** (`proactive/skill_parser.rs`): `normalize_title_for_dedup`(1148, pub), `title_bigrams`(1023, pub), `word_bigrams`(1097, pub), `jaccard_similarity`(1037, pub), `cjk_char_ratio`(1062, pub), `tokenize_mixed`(1115, **private**), `is_cjk_char`(1072, **private**), `FUZZY_DEDUP_THRESHOLD`(1012, `pub const = 0.75`). Call sites in skill_parser.rs that must keep working: 267, 293, 294, 300, 304, 305, 319, 320, 321, 505, 295.
- **`find_learned_skill_by_normalized_title`** (store.rs:529) — template; SQL matches `lower(trim(title)) = ?3` + `kind=Procedure` + `skill_type='learned'`.
- **`list_recent_nodes(space, limit)`** (store.rs:556) — template for a kinds-filtered variant.
- **`upgrade_existing_skill`** merge mechanics (skill_parser.rs:1171): `store.get_active_version(id)` → `store.deprecate_version(active.id)` (store.rs:725, `UPDATE ... status='deprecated'`) → `store.create_version(&MemoryVersion{...status:Active...})` (store.rs:636) → bump counter.
- **`MemoryVersion`** (models.rs:206): `{ id, node_id, supersedes_version_id: Option<String>, status: MemoryVersionStatus, content, metadata: Option<Value>, embedding_json: Option<String>, created_at }`. `MemoryVersionStatus::Active`.
- **`store.create_edge(&MemoryEdge)`** (store.rs:737). `MemoryEdge` (models.rs:221): `{ id, space_id, parent_node_id: Option<String>, child_node_id, relation_kind: MemoryRelationKind, visibility: MemoryVisibility, priority: i32, trigger_text: Option<String>, created_at, updated_at }`. Use `MemoryRelationKind::RelatesTo` (graph_propagation_search weights "related_to" at 0.7 → traversable) + `trigger_text = Some("co_extracted")` for provenance (zero enum change). `MemoryVisibility::Private`.
- **reflect() seam** (reflection.rs:595-619): `let facts = persist_items_to_graph(...)?;` then a `for f in &facts { if is_recallable_memu_type { project_fact(...) } }` loop. Add the pairwise linking AFTER that loop, using `facts[].node_id`.
- **Reflection test fixture** (reflection.rs:701): `fresh_store()` builds in-memory conn via `execute_batch(V4_MEMORY_GRAPH)` + `execute_batch(V35_MEMORY_OS_PHASE_1)`. `ExtractedItem { memory_type, content }` constructed inline. Match this fixture for G's reflection tests.
- **No migration.** **No new file beyond `text_dedup.rs`** (NEW — use explicit `git add`).

---

## Task 1: Extract dedup helpers to shared `memory_graph/text_dedup.rs`

**Files:**
- Create: `src-tauri/src/memory_graph/text_dedup.rs`
- Modify: `src-tauri/src/memory_graph/mod.rs` (add `pub mod text_dedup;`), `src-tauri/src/proactive/skill_parser.rs`

- [ ] **Step 1: Create `text_dedup.rs` with all 8 items**

Move VERBATIM from skill_parser.rs into `text_dedup.rs`: `normalize_title_for_dedup`, `title_bigrams`, `word_bigrams`, `jaccard_similarity`, `cjk_char_ratio`, `tokenize_mixed`, `is_cjk_char`, `FUZZY_DEDUP_THRESHOLD`. Make ALL of them `pub` (including the previously-private `tokenize_mixed`/`is_cjk_char` — they're now cross-module). Module doc:
```rust
//! openhuman-G — shared text-dedup primitives (normalize, bigram-Jaccard,
//! CJK-aware tokenization). Used by skill_parser (skill dedup) AND reflection
//! (fact dedup). Pure functions, no DB.
```
Add `pub mod text_dedup;` to `memory_graph/mod.rs`.

- [ ] **Step 2: Repoint skill_parser.rs to the shared module**

Remove the 8 moved definitions from skill_parser.rs. Add `use crate::memory_graph::text_dedup::{normalize_title_for_dedup, title_bigrams, word_bigrams, jaccard_similarity, cjk_char_ratio, FUZZY_DEDUP_THRESHOLD};` (the two now-pub helpers `tokenize_mixed`/`is_cjk_char` are only used internally by `word_bigrams`/`cjk_char_ratio` which moved too — so skill_parser doesn't need to import them). If any skill_parser code or OTHER module referenced these via `crate::proactive::skill_parser::<name>` (e.g. `resolve_slash_skill` uses `normalize_title_for_dedup`), either keep a `pub use crate::memory_graph::text_dedup::normalize_title_for_dedup;` re-export in skill_parser.rs OR repoint those external callers. **Grep `skill_parser::normalize_title_for_dedup` + the other names across src-tauri** and handle every external caller (re-export is the lowest-churn option — add `pub use` for any name with external callers).

- [ ] **Step 3: Move/port the helpers' unit tests**

If skill_parser's test module has tests for these helpers, move them to `text_dedup.rs` tests. Add at least: normalize (case/whitespace/trailing-punct), title_bigrams, jaccard (identical=1.0, disjoint=0.0), cjk_char_ratio.

- [ ] **Step 4: Build + verify skill_parser unchanged**

`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
`cd src-tauri && cargo test --lib proactive::skill_parser memory_graph::text_dedup 2>&1` (run each separately) → green. skill_parser's dedup behavior must be unchanged (its existing dedup tests pass).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_graph/text_dedup.rs src-tauri/src/memory_graph/mod.rs src-tauri/src/proactive/skill_parser.rs
git commit -m "refactor(memory): extract text-dedup helpers to shared memory_graph/text_dedup.rs (Slice G)"
```
**Verify `git show HEAD --stat` lists `text_dedup.rs` (new file).**

---

## Task 2: Store helpers for fact dedup + idempotent linking

**Files:**
- Modify: `src-tauri/src/memory_graph/store.rs`

- [ ] **Step 1: Write failing tests** (in store.rs tests; use the store test fixture — confirm it uses `db::migrations::run` or V4 schema; match existing store tests)

Tests for:
- `find_fact_by_normalized_title(space, kinds, normalized)`: seed a Reference node titled "User likes fish" → found by normalized "user likes fish" with kinds=["reference",...]; a Procedure (skill) node with same title → NOT found (excluded); wrong-kind excluded.
- `list_recent_nodes_by_kinds(space, kinds, limit)`: returns only nodes of the given kinds, recent first, capped.
- `find_edge_between(space, a, b, relation)`: false when none; true after `create_edge`; true regardless of direction (a→b or b→a).

- [ ] **Step 2: Implement the three helpers**

```rust
/// openhuman-G — find a reflection FACT node by normalized title within the
/// given kinds (EXCLUDES Procedure/skills — facts dedup separately from skills).
pub fn find_fact_by_normalized_title(
    &self,
    space_id: &str,
    kinds: &[&str],
    normalized_title: &str,
) -> Result<Option<MemoryNode>, crate::error::Error> {
    if normalized_title.trim().is_empty() || kinds.is_empty() {
        return Ok(None);
    }
    let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let placeholders = std::iter::repeat("?").take(kinds.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
         FROM memory_nodes
         WHERE space_id = ?1 AND kind IN ({placeholders})
           AND COALESCE(json_extract(metadata_json, '$.skill_type'), '') <> 'learned'
           AND lower(trim(title)) = ?{}
         ORDER BY updated_at DESC LIMIT 1",
        kinds.len() + 2
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(kinds.len() + 2);
    params.push(Box::new(space_id.to_string()));
    for k in kinds { params.push(Box::new(k.to_string())); }
    params.push(Box::new(normalized_title.to_string()));
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(crate::error::Error::Database)?;
    let result = stmt.query_row(refs.as_slice(), |row| Self::row_to_node(row)).ok();
    Ok(result)
}

/// openhuman-G — list recent nodes of the given kinds (D2 fuzzy candidate scan).
pub fn list_recent_nodes_by_kinds(
    &self,
    space_id: &str,
    kinds: &[&str],
    limit: usize,
) -> Result<Vec<MemoryNode>, crate::error::Error> {
    if kinds.is_empty() || limit == 0 {
        return Ok(vec![]);
    }
    let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let placeholders = std::iter::repeat("?").take(kinds.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
         FROM memory_nodes
         WHERE space_id = ?1 AND kind IN ({placeholders})
           AND COALESCE(json_extract(metadata_json, '$.skill_type'), '') <> 'learned'
         ORDER BY updated_at DESC LIMIT ?{}",
        kinds.len() + 2
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(kinds.len() + 2);
    params.push(Box::new(space_id.to_string()));
    for k in kinds { params.push(Box::new(k.to_string())); }
    params.push(Box::new(limit as i64));
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(crate::error::Error::Database)?;
    let rows = stmt.query_map(refs.as_slice(), |row| Self::row_to_node(row))
        .map_err(crate::error::Error::Database)?.flatten().collect();
    Ok(rows)
}

/// openhuman-G — does an edge of `relation` exist between a and b (either
/// direction)? For idempotent co_extracted linking.
pub fn find_edge_between(
    &self,
    space_id: &str,
    a: &str,
    b: &str,
    relation: &str,
) -> Result<bool, crate::error::Error> {
    let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_edges
         WHERE space_id = ?1 AND relation_kind = ?4
           AND ((parent_node_id = ?2 AND child_node_id = ?3)
             OR (parent_node_id = ?3 AND child_node_id = ?2))",
        rusqlite::params![space_id, a, b, relation],
        |r| r.get(0),
    ).map_err(crate::error::Error::Database)?;
    Ok(n > 0)
}
```
(Confirm `MemoryRelationKind::RelatesTo.as_str()` value for the `relation` arg passed by callers — likely `"relates_to"`. Quote it in your report.)

- [ ] **Step 3: Run → PASS; Commit**

`cargo test --lib memory_graph::store 2>&1 | tail -5` → green.
```bash
git add src-tauri/src/memory_graph/store.rs
git commit -m "feat(memory): find_fact_by_normalized_title + list_recent_nodes_by_kinds + find_edge_between (Slice G)"
```

---

## Task 3: Dedup-before-create + `upgrade_existing_fact` in reflection

**Files:**
- Modify: `src-tauri/src/memory_graph/reflection.rs`

- [ ] **Step 1: Add the reflection-kinds const + `upgrade_existing_fact`**

```rust
/// openhuman-G — reflection fact kinds (non-Procedure; skills dedup separately).
const FACT_KINDS: &[&str] = &["user_profile", "episode", "reference", "directive"];

/// openhuman-G — merge a re-stated fact into an existing node: supersede the
/// active version with the new content, bump reinforced_count. Mirrors
/// skill_parser::upgrade_existing_skill. Returns the existing node id.
fn upgrade_existing_fact(
    store: &MemoryGraphStore,
    existing: &crate::memory_graph::models::MemoryNode,
    new_content: &str,
    now: &str,
) -> anyhow::Result<String> {
    if let Ok(Some(active)) = store.get_active_version(&existing.id) {
        if let Err(e) = store.deprecate_version(&active.id) {
            tracing::warn!(node_id = %existing.id, err = %e, "reflection: deprecate_version failed");
        }
    }
    let new_version = crate::memory_graph::models::MemoryVersion {
        id: uuid::Uuid::new_v4().to_string(),
        node_id: existing.id.clone(),
        supersedes_version_id: None,
        status: crate::memory_graph::models::MemoryVersionStatus::Active,
        content: new_content.to_string(),
        metadata: None,
        embedding_json: None,
        created_at: now.to_string(),
    };
    store.create_version(&new_version)?;
    // bump reinforced_count (best-effort)
    let mut meta = existing.metadata.clone().unwrap_or_else(|| serde_json::json!({}));
    let rc = meta.get("reinforced_count").and_then(|v| v.as_u64()).unwrap_or(0) + 1;
    meta["reinforced_count"] = serde_json::json!(rc);
    let _ = store.update_node(&existing.id, None, None, Some(&meta));
    Ok(existing.id.clone())
}
```

- [ ] **Step 2: Write failing tests** (mirror reflection.rs `fresh_store()` fixture)

- `dedup_exact`: persist `[{knowledge, "User likes fish."}]`; persist `[{knowledge, "user likes fish"}]` (case/punct variant, same first-50 title) → ONE Reference node; its `reinforced_count==1`; active version content = the 2nd; old version deprecated.
- `dedup_fuzzy`: persist `[{knowledge, "User prefers Rust for backend services"}]`; persist a high-similarity variant → merged (one node).
- `no_false_merge`: two clearly-different knowledge facts → two nodes.
- `kind_isolation`: a `knowledge` (Reference) fact and a `skill` (Procedure) with identical content/title → NOT merged (skill goes to Procedure via its own path; fact dedup excludes Procedure). (Note: skill items in persist_items_to_graph still create Procedure nodes as today; just confirm the fact-dedup doesn't fold a Reference into a Procedure or vice versa.)

- [ ] **Step 3: Wire dedup into `persist_items_to_graph`**

In the item loop, BEFORE the `let node_id = uuid::Uuid::new_v4()` create path, add dedup for non-Procedure kinds:
```rust
let kind = map_memu_type_to_kind(memu_type);
// openhuman-G — dedup reflection facts (not skills/Procedure) into existing nodes.
if kind != MemoryNodeKind::Procedure {
    let normalized = crate::memory_graph::text_dedup::normalize_title_for_dedup(title);
    // D1 exact
    let mut hit = store.find_fact_by_normalized_title(space_id, FACT_KINDS, &normalized).ok().flatten();
    // D2 fuzzy (only if D1 missed + title long enough)
    if hit.is_none() && normalized.chars().count() >= 4 {
        if let Ok(cands) = store.list_recent_nodes_by_kinds(space_id, FACT_KINDS, 500) {
            let new_grams = crate::memory_graph::text_dedup::title_bigrams(&normalized);
            let cjk = crate::memory_graph::text_dedup::cjk_char_ratio(&normalized);
            let threshold = if cjk >= 0.5 { 0.65_f32 } else { crate::memory_graph::text_dedup::FUZZY_DEDUP_THRESHOLD };
            let mut best: Option<(f32, crate::memory_graph::models::MemoryNode)> = None;
            for cand in cands {
                let cn = crate::memory_graph::text_dedup::normalize_title_for_dedup(&cand.title);
                if cn == normalized { continue; }
                let sim = crate::memory_graph::text_dedup::jaccard_similarity(&new_grams, &crate::memory_graph::text_dedup::title_bigrams(&cn));
                if sim >= threshold && best.as_ref().map(|(b,_)| sim > *b).unwrap_or(true) {
                    best = Some((sim, cand));
                }
            }
            hit = best.map(|(_, n)| n);
        }
    }
    if let Some(existing) = hit {
        match upgrade_existing_fact(store, &existing, summary, &now) {
            Ok(id) => {
                facts.push(PersistedFact { node_id: id, memu_type: memu_type.to_string(), content: summary.to_string() });
                continue; // skip the create path
            }
            Err(e) => tracing::warn!(err = %e, "reflection: upgrade_existing_fact failed; creating new"),
        }
    }
}
// ...existing create path (uuid::new_v4 etc.)...
```
Match the EXACT in-scope variable names (`memu_type`, `title`, `summary`, `now`, `facts`) from the real loop — adapt as needed. `MemoryNodeKind` import is already in reflection.rs.

- [ ] **Step 4: Run → PASS; Build; Commit**

`cargo test --lib memory_graph::reflection 2>&1 | tail -10` → green. `cargo build 2>&1 | grep -E "^error"` → empty.
```bash
git add src-tauri/src/memory_graph/reflection.rs
git commit -m "feat(memory): reflection fact dedup (D1 exact + D2 fuzzy) + upgrade_existing_fact merge (Slice G)"
```

---

## Task 4: Pairwise `co_extracted` linking in reflect()

**Files:**
- Modify: `src-tauri/src/memory_graph/reflection.rs`

- [ ] **Step 1: Write a failing test**

In reflection.rs tests: call `reflect()` (or directly persist + a new `link_co_extracted` helper) with a turn yielding 3 distinct facts → assert 3 `relates_to`/`co_extracted` edges exist pairwise between the fact node ids (via `store.find_edge_between(space, a, b, "relates_to")`); re-running the same turn does NOT duplicate edges (idempotent).
(If `reflect()` is hard to call in a unit test — it needs an extractor + bucket_seal + app_handle — extract the linking into a sync helper `link_co_extracted(store, space, node_ids: &[String], now)` and test THAT directly. Prefer this: it's cleanly testable.)

- [ ] **Step 2: Implement `link_co_extracted` + call it in reflect()**

```rust
/// openhuman-G — relate the facts co-extracted in one turn so the graph
/// connects and multi-hop recall can traverse them. Idempotent (skips existing
/// edges). Capped at the first LINK_CAP nodes to bound O(n²). Best-effort.
const LINK_CAP: usize = 5;
fn link_co_extracted(store: &MemoryGraphStore, space_id: &str, node_ids: &[String], now: &str) {
    let ids: Vec<&String> = node_ids.iter().take(LINK_CAP).collect();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let (a, b) = (ids[i], ids[j]);
            match store.find_edge_between(space_id, a, b, crate::memory_graph::models::MemoryRelationKind::RelatesTo.as_str()) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => { tracing::warn!(err = %e, "reflection: find_edge_between failed"); continue; }
            }
            let edge = crate::memory_graph::models::MemoryEdge {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: space_id.to_string(),
                parent_node_id: Some(a.clone()),
                child_node_id: b.clone(),
                relation_kind: crate::memory_graph::models::MemoryRelationKind::RelatesTo,
                visibility: crate::memory_graph::models::MemoryVisibility::Private,
                priority: 0,
                trigger_text: Some("co_extracted".to_string()),
                created_at: now.to_string(),
                updated_at: now.to_string(),
            };
            if let Err(e) = store.create_edge(&edge) {
                tracing::warn!(err = %e, "reflection: co_extracted create_edge failed");
            }
        }
    }
}
```
In `reflect()`, AFTER the `project_fact` loop (reflection.rs:~618):
```rust
let now_link = chrono::Utc::now().to_rfc3339();
let fact_ids: Vec<String> = facts.iter().map(|f| f.node_id.clone()).collect();
link_co_extracted(&self.store, space_id, &fact_ids, &now_link);
```
(Dedup in T3 means a re-stated fact reuses its node id, so co_extracted edges naturally re-point to the merged node; `find_edge_between` keeps it idempotent.)

- [ ] **Step 3: Run → PASS; Build; Commit**

`cargo test --lib memory_graph::reflection 2>&1 | tail -10` → green. `cargo build 2>&1 | grep -E "^error"` → empty.
```bash
git add src-tauri/src/memory_graph/reflection.rs
git commit -m "feat(memory): pairwise co_extracted linking of reflection facts (Slice G)"
```

---

## Task 5: Whole-slice verification + ship

- [ ] **Step 1**: `cargo build` + `cargo clippy --lib` clean (no new warnings in text_dedup/reflection/store/skill_parser).
- [ ] **Step 2**: tests — `memory_graph::text_dedup`, `memory_graph::reflection`, `memory_graph::store`, `proactive::skill_parser` (the refactor), broad dependent run. All green.
- [ ] **Step 3**: grep gates — the 8 helpers exist ONLY in text_dedup.rs (not duplicated in skill_parser); skill_parser uses the shared module; reflection uses text_dedup + the new store helpers; `co_extracted` linking present.
- [ ] **Step 4**: `npx gitnexus analyze`.
- [ ] **Step 5**: PR with `## Commits (bisectable)` table. Note: G2 already implemented (out of scope); co_extracted uses RelatesTo + trigger_text; dedup reuses skill_parser pattern via shared module. **Verify `git show <commit> --stat` includes `text_dedup.rs`.**
- [ ] **Step 6**: rebase onto latest origin/main, rebase-merge, sync main, cleanup, reindex, update memory (project-openhuman-deepening → G SHIPPED → **A–G program COMPLETE**; MEMORY.md).

---

## Self-Review

**Spec coverage:** §1 shared module → T1; §2 dedup-before-create + upgrade → T3 (+ store helpers T2); §3 upgrade_existing_fact → T3; §4 co_extracted linking → T4. Testing items 1-8 → T1/T2/T3/T4 tests + T5 broad run. ✓
**Placeholder scan:** the `MemoryRelationKind::RelatesTo.as_str()` value + the reflect()-vs-helper test decision (T4 Step 1) are flagged confirmations with concrete fallbacks, not TODOs. ✓
**Type consistency:** `find_fact_by_normalized_title`/`list_recent_nodes_by_kinds`/`find_edge_between` (T2) used in T3/T4; `upgrade_existing_fact`/`link_co_extracted`/`FACT_KINDS`/`LINK_CAP` consistent; `MemoryEdge`/`MemoryVersion` fields per recon. ✓
**New-file safety:** T1 + T5 verify `git show --stat` lists `text_dedup.rs`. ✓
**Kind isolation:** dedup gated on `kind != Procedure` + `find_fact_by_normalized_title` excludes skill_type='learned' → skills & facts never cross-merge. ✓
