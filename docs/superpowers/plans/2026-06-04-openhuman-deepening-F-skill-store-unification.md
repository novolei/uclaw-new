# openhuman Slice F — Skill Store Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make memory_graph Procedure nodes the single source of truth for skills; keep the bucket_seal `skills` namespace as a **continuous projection** (the read/search face). Reads stay on the projection; writes (counter bumps, lifecycle promotion) go to Procedure + re-project.

**Architecture:** `project_skill` refreshes the projection on every authoritative Procedure write (extraction + bump). A marker-gated backfill projects existing Procedure skills (replacing the one-time `migrate_skills`). Write call sites route through `bump_skill_and_reproject` / `promote_skill_and_reproject`.

**Spec:** `docs/superpowers/specs/2026-06-04-openhuman-deepening-F-skill-store-unification-design.md`

---

## Pinned facts (from recon — verbatim, do not re-derive)

- **slug = `crate::proactive::skill_parser::normalize_title_for_dedup(title)`** everywhere (migration `node_to_skill`, `resolve_slash_skill`, `record_skill_cited`). The projection's slug MUST be `normalize_title_for_dedup(node.title)` so existing read paths resolve.
- **`Skill`** (`memory_adapter/skills.rs:8`): `{ slug, space, name, body, usage_count: u64, cited_count: u64, keywords: Vec<String>, status: String }`. `put_skill(adapter: &Arc<dyn MemoryAdapter>, skill: &Skill)`; key = `space\x01slug`, latest-wins.
- **`store.find_learned_skill_by_normalized_title(space_id, normalized) -> Result<Option<MemoryNode>>`** (store.rs:507) — SQL `lower(trim(title)) = ?3` + `kind=Procedure` + `skill_type='learned'`.
- **`store.get_active_version(node_id) -> Result<Option<MemoryVersion>>`** (store.rs:677) — `.content` is the body.
- **`store.bump_skill_usage(node_ids: &[&str]) -> Result<()>`** (store.rs:432) — `json_set $.usage_count = COALESCE(...,0)+1`. **No `bump_skill_cited` yet** — add it mirroring this.
- **`store.update_node(id, title: Option<&str>, kind: Option<MemoryNodeKind>, metadata: Option<&serde_json::Value>) -> Result<()>`** (store.rs:148) — generic metadata writer (calls `enforce_freeze`).
- **`store.get_keywords_for_node(node_id)`** exists (used in upgrade_existing_skill).
- **Node metadata fields**: `usage_count`, `cited_count`, `lifecycle` ('draft'|'promoted'), `skill_type='learned'`, `enabled`. Read via `node.metadata.as_ref().and_then(|m| m.get("...")...)`.
- **Insertion points**: `store_skill_as_procedure` ends `Ok(node)` (skill_parser.rs:480); `upgrade_existing_skill` ends `Ok(existing)` (skill_parser.rs:1238). Both currently take `store: &MemoryGraphStore` only.
- **Prod caller of `store_skill_as_procedure`** is in `proactive/service.rs` (~2380, the skill-extraction block) — it has `refs.bucket_seal_adapter: Option<Arc<BucketSealAdapter>>`.
- **`project_fact`** (recall_projection.rs:38): `pub async fn project_fact(adapter: &Arc<BucketSealAdapter>, node_id, text)` → `adapter.store_kept(ns, key, content, Core, None)`. **But `put_skill` needs `&Arc<dyn MemoryAdapter>`** (trait), so `project_skill` takes `&Arc<dyn MemoryAdapter>`.
- **Backfill template**: `memory_adapter/recall_projection_backfill.rs` (marker `__recall_projection_backfill_v1__`, `find_entity_page_by_slug`/`entity_page_put` for the marker, all-ok gate, enumerate `list_nodes_by_kind` → `get_active_version` → project). Spawned in `app.rs:1181`.
- **`migrate_skills`** boot spawn: `app.rs:1207-1218`; marker `__skills_migrated_v1__` in space `__migration__`. Only caller. Remove both.
- **Write call sites to repoint** (all currently hit `skills::bump_cited`/`bump_usage`/`put_skill`):
  - `resolve_slash_skill` (tauri_commands.rs:7358) — bump cited+usage, auto-promote at cited≥3, then read body.
  - `record_skill_cited` (tauri_commands.rs:7674) — bump cited+usage, auto-promote at cited≥3.
  - `load_skill` (agent/tools/builtin/load_skill.rs) — `skills::get_skill` + `skills::bump_usage`.
  - `skill_search` (agent/tools/builtin/skill_search.rs:~166) — `skills::search` + `skills::bump_usage` on hits.
  - `service.rs` reinforcement (~2343, 2357) — `skills::get_skill` + `skills::put_skill`.
- **`skills::search`** takes concrete `&Arc<BucketSealAdapter>` (calls `recall_hybrid`) — KEEP unchanged.
- Test fixtures: `crate::db::migrations::run(&conn)`.
- **NEW FILES need explicit `git add <path>`** (a prior slice lost a new file via `git commit -am`). Verify `git show <commit> --stat` lists every new module.

---

## Task 1: `project_skill` + wire into extraction

**Files:**
- Modify: `src-tauri/src/proactive/skill_parser.rs`

- [ ] **Step 1: Add `project_skill`**

Add to `skill_parser.rs` (near `store_skill_as_procedure`):
```rust
/// openhuman-F — project a Procedure skill node into the bucket_seal `skills`
/// namespace (the read/search face). Best-effort; the Procedure write is
/// authoritative. Slug = normalize_title_for_dedup(title) so existing read
/// paths (slash / load_skill / skill_search) resolve unchanged.
pub async fn project_skill(
    adapter: &std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
    node: &crate::memory_graph::models::MemoryNode,
    body: &str,
) {
    let meta = node.metadata.as_ref();
    let get_u64 = |k: &str| -> u64 {
        meta.and_then(|m| m.get(k)).and_then(|v| v.as_u64()).unwrap_or(0)
    };
    let status = meta
        .and_then(|m| m.get("lifecycle"))
        .and_then(|v| v.as_str())
        .unwrap_or("draft")
        .to_string();
    let skill = crate::memory_adapter::skills::Skill {
        slug: normalize_title_for_dedup(&node.title),
        space: node.space_id.clone(),
        name: node.title.clone(),
        body: body.to_string(),
        usage_count: get_u64("usage_count"),
        cited_count: get_u64("cited_count"),
        keywords: Vec::new(), // keyword index lives in memory_keywords; projection body carries text
        status,
    };
    if let Err(e) = crate::memory_adapter::skills::put_skill(adapter, &skill).await {
        tracing::warn!(node_id = %node.id, error = %format!("{e:#}"), "project_skill failed (Procedure authoritative ok)");
    }
}
```

- [ ] **Step 2: Thread the adapter into `store_skill_as_procedure` + `upgrade_existing_skill`**

Both fns are SYNC and return the node. Adding an async projection inside a sync fn isn't possible directly. **Approach:** keep the two fns sync (they do the authoritative Procedure write), and project from the CALLER (which is async). So do NOT modify the two fns' bodies; instead expose what the caller needs:
- `store_skill_as_procedure` already returns the created `MemoryNode`.
- `upgrade_existing_skill` returns the existing `MemoryNode`.
The caller (service.rs, Task… here) will, after calling them, fetch the active version body and `project_skill`. To make that ergonomic, add a helper:
```rust
/// openhuman-F — after an authoritative Procedure skill write, refresh the
/// projection. Fetches the node's active version body and projects. Best-effort.
pub async fn project_skill_node(
    store: &MemoryGraphStore,
    adapter: &std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
    node: &crate::memory_graph::models::MemoryNode,
) {
    let body = match store.get_active_version(&node.id) {
        Ok(Some(v)) => v.content,
        _ => String::new(),
    };
    project_skill(adapter, node, &body).await;
}
```

- [ ] **Step 3: Write a failing test for `project_skill`**

In `skill_parser.rs` tests (use a real store + a real BucketSealAdapter coerced to `Arc<dyn MemoryAdapter>` — check how existing skill_parser/skills tests build an adapter; if there's a test helper, reuse it; else build an in-memory `BucketSealAdapter`). Test: create a Procedure skill node + active version, `project_skill_node`, then `skills::get_skill(adapter, space, normalize_title_for_dedup(title))` returns a Skill with matching name/body/counters.

(If building a BucketSealAdapter in a unit test is heavy, place this test where other `skills::` tests build the adapter — match that pattern. Report the approach.)

- [ ] **Step 4: Run → FAIL, implement (Steps 1-2 already are the impl), Run → PASS**

`cd src-tauri && cargo test --lib proactive::skill_parser 2>&1 | tail -20`.

- [ ] **Step 5: Call the projection from the extraction caller**

In `proactive/service.rs` skill-extraction block (~2380, after `store_skill_as_procedure` returns `node`): if `refs.bucket_seal_adapter` is `Some`, coerce to `Arc<dyn MemoryAdapter>` and `project_skill_node(&refs.memory_graph_store, &adapter, &node).await`. (The block already has `space_id` + the node.) Mirror for the merge path if the extraction distinguishes create vs upgrade (store_skill_as_procedure internally calls upgrade on dedup hit and returns the node either way — so one projection call after it covers both).

- [ ] **Step 6: Build + commit**

`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
```bash
git add src-tauri/src/proactive/skill_parser.rs src-tauri/src/proactive/service.rs
git commit -m "feat(memory): project_skill — refresh bucket_seal skills projection on Procedure skill write (Slice F)"
```

---

## Task 2: Skill projection backfill + remove `migrate_skills`

**Files:**
- Create: `src-tauri/src/memory_adapter/skill_projection_backfill.rs`
- Modify: `src-tauri/src/memory_adapter/mod.rs` (add `pub mod`), `src-tauri/src/app.rs`
- Delete usage: `src-tauri/src/proactive/skill_migration.rs` (`migrate_skills`) + its boot spawn

- [ ] **Step 1: Create the backfill (mirror `recall_projection_backfill.rs`)**

`skill_projection_backfill.rs`: marker `__skill_projection_backfill_v1__` (use the same `find_entity_page_by_slug`/`entity_page_put` marker mechanism as recall_projection_backfill, space `"default"`). Enumerate Procedure skill nodes per space (use `store.list_top_learned_skills(space, 100000)` or `list_nodes_by_kind` filtered to skill_type='learned' — match what's available; `list_top_learned_skills` returns `MemoryNodeDetail` which carries the node + body, convenient). For each: build the body (active version) + `project_skill`. all-ok gate → set marker. Signature:
```rust
pub async fn backfill_skill_projections(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<dyn crate::memory_adapter::MemoryAdapter>,
) -> anyhow::Result<usize>
```
Add `pub mod skill_projection_backfill;` to `memory_adapter/mod.rs`.

- [ ] **Step 2: Spawn it in app.rs; remove the migrate_skills spawn**

In `app.rs`: replace the `migrate_skills` spawn block (lines ~1207-1218) with a `backfill_skill_projections` spawn (mirror the recall backfill spawn at ~1181-1205, coercing `bucket_seal_adapter` to `Arc<dyn MemoryAdapter>`). Remove the `migrate_skills` call entirely.

- [ ] **Step 3: Remove `migrate_skills`**

Delete `migrate_skills` + `node_to_skill` from `skill_migration.rs` (confirm no other caller: `grep -rn "migrate_skills\|node_to_skill" src-tauri/src`). If the whole file becomes empty/dead, delete the file + its `mod` declaration. Keep `normalize_skill_title` if it lives elsewhere and is still used (it's in `crate::skills`).

- [ ] **Step 4: Test the backfill**

Test: seed N Procedure skill nodes (+ active versions) + absent marker → `backfill_skill_projections` → N projection rows present (via `skills::get_skill`) + marker set; re-run → 0 (marker gate).

- [ ] **Step 5: Build + verify + commit**

`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty. `cargo test --lib memory_adapter 2>&1 | tail -5` → green.
```bash
git add src-tauri/src/memory_adapter/skill_projection_backfill.rs src-tauri/src/memory_adapter/mod.rs src-tauri/src/app.rs src-tauri/src/proactive/skill_migration.rs
git commit -m "feat(memory): skill projection backfill (marker-gated) replacing one-time migrate_skills (Slice F)"
```
**Verify `git show HEAD --stat` lists `skill_projection_backfill.rs` (new file).**

---

## Task 3: Counter bumps + lifecycle → Procedure + re-project; repoint write sites

**Files:**
- Modify: `src-tauri/src/memory_graph/store.rs` (add `bump_skill_cited`)
- Modify: `src-tauri/src/proactive/skill_parser.rs` (add `bump_skill_and_reproject` + `promote_skill_and_reproject`)
- Modify: `src-tauri/src/tauri_commands.rs`, `agent/tools/builtin/load_skill.rs`, `agent/tools/builtin/skill_search.rs`, `proactive/service.rs`
- Modify: `src-tauri/src/memory_adapter/skills.rs` (remove `bump_cited`/`bump_usage`)

- [ ] **Step 1: Add `bump_skill_cited` to store (mirror `bump_skill_usage`)**

In `store.rs`, mirror `bump_skill_usage` (line 432) but for `$.cited_count`:
```rust
pub fn bump_skill_cited(&self, node_ids: &[&str]) -> Result<u64, crate::error::Error> {
    // bump cited_count on each; return the new cited_count of the FIRST id
    // (callers bump one at a time for the promotion-threshold check).
    let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut last: u64 = 0;
    for id in node_ids {
        conn.execute(
            "UPDATE memory_nodes SET metadata_json = json_set(COALESCE(metadata_json,'{}'), '$.cited_count', COALESCE(json_extract(metadata_json,'$.cited_count'),0)+1), updated_at=?1 WHERE id=?2",
            params![now, id],
        ).map_err(crate::error::Error::Database)?;
        last = conn.query_row(
            "SELECT COALESCE(json_extract(metadata_json,'$.cited_count'),0) FROM memory_nodes WHERE id=?1",
            params![id], |r| r.get::<_, i64>(0),
        ).unwrap_or(0).max(0) as u64;
    }
    Ok(last)
}
```

- [ ] **Step 2: Add the repoint helpers in `skill_parser.rs`**

```rust
/// openhuman-F — bump a learned skill's counters on the AUTHORITATIVE Procedure
/// node (by slug) + re-project. Returns the new cited_count (for promotion gate),
/// or None if the skill doesn't exist. Best-effort projection.
pub async fn bump_skill_and_reproject(
    store: &MemoryGraphStore,
    adapter: &std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
    space_id: &str,
    slug: &str,
    bump_cited: bool,
    bump_usage: bool,
) -> Option<u64> {
    let node = store.find_learned_skill_by_normalized_title(space_id, slug).ok().flatten()?;
    let mut new_cited = node.metadata.as_ref().and_then(|m| m.get("cited_count")).and_then(|v| v.as_u64()).unwrap_or(0);
    if bump_usage { let _ = store.bump_skill_usage(&[node.id.as_str()]); }
    if bump_cited { new_cited = store.bump_skill_cited(&[node.id.as_str()]).unwrap_or(new_cited); }
    // re-project with fresh counters
    if let Ok(Some(fresh)) = store.find_learned_skill_by_normalized_title(space_id, slug) {
        project_skill_node(store, adapter, &fresh).await;
    }
    Some(new_cited)
}

/// openhuman-F — set lifecycle='promoted' on the Procedure node + re-project.
pub async fn promote_skill_and_reproject(
    store: &MemoryGraphStore,
    adapter: &std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
    space_id: &str,
    slug: &str,
) {
    if let Ok(Some(node)) = store.find_learned_skill_by_normalized_title(space_id, slug) {
        let mut meta = node.metadata.clone().unwrap_or_else(|| serde_json::json!({}));
        meta["lifecycle"] = serde_json::json!("promoted");
        let _ = store.update_node(&node.id, None, None, Some(&meta));
        if let Ok(Some(fresh)) = store.find_learned_skill_by_normalized_title(space_id, slug) {
            project_skill_node(store, adapter, &fresh).await;
        }
    }
}
```

- [ ] **Step 3: Repoint the write call sites**

For EACH site, replace `skills::bump_cited`/`bump_usage`/`put_skill(promotion)` with the Procedure helpers (`state.memory_graph_store` + `state.bucket_seal_adapter as Arc<dyn MemoryAdapter>`):
- **`resolve_slash_skill`** (tauri_commands.rs): replace the adapter bump+promote block with `bump_skill_and_reproject(store, adapter, space, normalized, true, true)`; if returned cited ≥ 3 → `promote_skill_and_reproject(...)`. Keep the body read via `skills::get_skill` (projection now current).
- **`record_skill_cited`** (tauri_commands.rs): same — `bump_skill_and_reproject(.., true, true)` + promote at ≥3. Return `Some(normalized)` as before.
- **`load_skill`** (agent/tools/builtin/load_skill.rs): replace `skills::bump_usage` with `bump_skill_and_reproject(.., false, true)`. Keep `skills::get_skill` body read.
- **`skill_search`** (agent/tools/builtin/skill_search.rs): replace the per-hit `skills::bump_usage` with `bump_skill_and_reproject(.., false, true)`. Keep `skills::search`.
- **`service.rs` reinforcement** (~2343/2357): if it bumps usage/status via get+put, replace with the appropriate helper (`bump_skill_and_reproject` for usage; `promote_skill_and_reproject` for lifecycle). Quote what it does + repoint equivalently; if it does something the helpers don't cover, report before improvising.

These call sites need `Arc<dyn MemoryAdapter>` + the store. tauri sites have `state.memory_graph_store` + `state.bucket_seal_adapter`. Agent tools: check what the tool's ctx exposes (the tools already get the adapter for `skills::*`; they likely have access to a store too — if not, report). The agent tools currently only have the adapter; if they lack the memory_graph store, the bump can't go to Procedure there → report this blocker; fallback: leave load_skill/skill_search usage bumps as projection-only writes that the next extraction re-projection will reconcile (note the tradeoff) OR thread the store into those tools.

- [ ] **Step 4: Remove adapter `bump_cited`/`bump_usage`**

In `memory_adapter/skills.rs`, delete `bump_cited` + `bump_usage`. `grep -rn "skills::bump_cited\|skills::bump_usage" src-tauri/src` → empty.

- [ ] **Step 5: Build + tests + commit**

`cargo build 2>&1 | grep -E "^error"` → empty. `cargo test --lib proactive::skill_parser memory_graph::store tauri_commands 2>&1` (run separately) → green.
```bash
git add -A  # multiple files; verify with git status first
git commit -m "feat(memory): route skill counter/lifecycle writes to Procedure + re-project; drop adapter bump_* (Slice F)"
```

---

## Task 4: Whole-slice verification + ship

- [ ] **Step 1**: `cargo build` + `cargo clippy --lib` clean (no new warnings in skill_parser/skills/skill_projection_backfill/store).
- [ ] **Step 2**: tests — `proactive::skill_parser`, `memory_adapter`, `memory_graph::store`, broad dependent run. All green.
- [ ] **Step 3**: grep gates — `migrate_skills` gone; `skills::bump_cited`/`bump_usage` gone; `project_skill` called from extraction + backfill + bump helpers; `skills::search` unchanged.
- [ ] **Step 4**: `npx gitnexus analyze`.
- [ ] **Step 5**: PR with `## Commits (bisectable)` table. Note: reads unchanged (projection), writes→Procedure, migrate_skills replaced by backfill. **Verify `git show <each commit> --stat`** includes the new backfill file.
- [ ] **Step 6**: rebase onto latest origin/main, rebase-merge, sync main, cleanup, reindex, update memory (project-openhuman-deepening → F SHIPPED + next G).

---

## Self-Review

**Spec coverage:** §1 project_skill → T1; §2 backfill → T2; §3 reads-unchanged (no task needed — that's the point); §4 writes→Procedure + remove migrate_skills + remove adapter bumps → T2 (migration) + T3 (bumps). ✓
**Placeholder scan:** the agent-tool store-access question (T3 Step 3) is a flagged blocker-or-fallback with a concrete fallback, not a TODO. ✓
**Type consistency:** `project_skill(&Arc<dyn MemoryAdapter>, &MemoryNode, &str)`, `project_skill_node`, `bump_skill_and_reproject`/`promote_skill_and_reproject`, `bump_skill_cited` — names consistent across def + call sites. `Skill` reused. ✓
**New-file safety:** T2 + T4 explicitly verify `git show --stat` lists `skill_projection_backfill.rs`. ✓
