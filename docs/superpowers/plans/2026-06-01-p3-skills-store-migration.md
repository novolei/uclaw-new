# P3-skills skill_parser Store Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate `skill_parser`'s learned-skill store off the frozen `memory_graph` onto an extended `memory_adapter::skills` facade — faithfully (space scoping + `usage_count` ranking) — so memory_graph's skill writer retires and P4 can drop the freeze hook.

**Architecture:** Extend `skills.rs` to parity (space + usage_count), one-time-migrate existing `Procedure` nodes into the `"skills"` namespace at boot, then repoint the four touch points (write / rank-read / get / cite+usage) onto the facade behind `skill_store_repoint_enabled` (default on; rollback restores memory_graph).

**Tech Stack:** Rust, Tauri, `memory_adapter::skills` (P1c) + `BucketSealAdapter`, `MemoryGraphStore` (read-only source for migration + rollback).

---

## Recon findings (complete — ground truth)

- **`skills.rs` (P1c, no live callers yet):** `const SKILLS_NAMESPACE = "skills"`; `Skill { slug, name, body, cited_count, keywords, status }` (NO `space`, NO `usage_count`); `put_skill(&Arc<dyn MemoryAdapter>, &Skill)`; `get_skill(adapter, slug) -> Option<Skill>`; `top_skills(adapter, limit)` sorts by `cited_count` desc; `bump_cited(adapter, slug) -> bool`. Tests use an in-memory `InMemoryAdapter` double + `Skill{…}` literals (these literals must be updated when fields are added).
- **`MemoryGraphStore` (read source):** `list_nodes_by_kind(...)` (store.rs:170) — yields nodes by kind (use `Procedure`); `get_active_version(node_id) -> Option<MemoryVersion>` (609) → `body`; `list_top_skills_by_usage(space_id, limit)` (275, the rank read; `ORDER BY usage_count DESC, cited_count DESC, updated_at DESC`); `bump_skill_usage(node_ids: &[&str])` (364). `MemoryNode` has `id, kind, title, space_id, metadata_json` (cited_count/usage_count/status live in `metadata_json`); `MemoryVersion` has `content`, `status: MemoryVersionStatus`.
- **Touch points:** **W** `proactive/service.rs:~2095` → `skill_parser::store_skill_as_procedure(store, skill: &ParsedSkill, space_id)` (skill_parser.rs:259). **R** `agent/tools/memu_tools.rs:168` `store.list_top_skills_by_usage(&self.space_id, limit)`. **G** `tauri_commands.rs:8532` `get_learned_skill`. **C+usage** `proactive/feedback.rs` (SQL bumps `metadata_json.$.cited_count` and `$.usage_count`; the citation path bumps both, the "unhelpful" path bumps usage only) — this is the cite/promotion + usage-bump site; `record_skill_cited` routes here. Recon `PROMOTION_THRESHOLD` (the draft→promoted cutoff) in feedback.rs/tauri_commands.
- **Migration pattern:** `proactive/memory_migration.rs::migrate_episodes` (marker-gated, idempotent, boot-spawn) + P2b `gbrain_page_migration` (completion-marker). `app.rs` has the proactive-episode-migration spawn site + `bucket_seal_adapter` + the `memory_graph_store` handle.
- **Config:** `MemoryOsConfig` (`memubot_config.rs`) — `default_*` + manual `impl Default` (~679) + `#[cfg(test)]` default tests, mirror `gbrain_read_repoint_enabled` (P2c-1).
- `state.bucket_seal_adapter: Arc<BucketSealAdapter>` → `as Arc<dyn MemoryAdapter>`.

## Worktree setup

Worktree under `/Users/ryanliu/Documents/uclaw-worktrees/` on `claude/p3-skills-store-migration` off `origin/main`. Fresh-build placeholders:
```bash
WT=/Users/ryanliu/Documents/uclaw-worktrees/p3-skills-store-migration
mkdir -p "$WT/src-tauri/bunembed" "$WT/src-tauri/pyembed" "$WT/src-tauri/gbrain-source"
touch "$WT/src-tauri/bunembed/bun" "$WT/src-tauri/pyembed/python"
echo x > "$WT/src-tauri/gbrain-source/placeholder.txt"
```
Baseline `cargo build` clean before Task 1.

## File structure

| File | Responsibility |
|---|---|
| `memory_adapter/skills.rs` | extend `Skill` (+space, +usage_count); space-scope fns; `bump_usage`; `bump_cited`→count; usage-rank `top_skills` |
| `proactive/skill_migration.rs` (new) | Procedure→Skill migration + marker + tests |
| `app.rs` | boot spawn for the migration |
| `memubot_config.rs` | `skill_store_repoint_enabled` flag |
| `proactive/service.rs` + `skill_parser.rs` | **W** write repoint |
| `agent/tools/memu_tools.rs` | **R** rank repoint |
| `tauri_commands.rs` | **G** get + part of **C** repoint |
| `proactive/feedback.rs` | **C** cite + usage bump repoint |

---

### Task 1: Extend the `skills.rs` facade (space + usage_count)

**Files:** Modify `src-tauri/src/memory_adapter/skills.rs` (struct, fns, tests).

- [ ] **Step 1: Extend `Skill` + space-qualified key helper**

```rust
pub struct Skill {
    pub slug: String,
    /// P3-skills — space_id scope (was implicit single-namespace).
    pub space: String,
    pub name: String,
    pub body: String,
    /// P3-skills — primary ranking signal (skill usage frequency).
    pub usage_count: u64,
    pub cited_count: u64,
    pub keywords: Vec<String>,
    pub status: String,
}

/// Space-qualified storage key so identical slugs in different spaces don't collide.
fn skill_key(space: &str, slug: &str) -> String {
    format!("{space}\u{1}{slug}")
}
```

- [ ] **Step 2: Space-scope the fns + add `bump_usage` + `bump_cited`→count + usage-rank**

```rust
pub async fn put_skill(adapter: &Arc<dyn MemoryAdapter>, skill: &Skill) -> anyhow::Result<()> {
    let content = serde_json::to_string(skill)?;
    adapter.store(SKILLS_NAMESPACE, &skill_key(&skill.space, &skill.slug), &content, MemoryCategory::Core, None).await
}

pub async fn get_skill(adapter: &Arc<dyn MemoryAdapter>, space_id: &str, slug: &str) -> anyhow::Result<Option<Skill>> {
    match adapter.get(SKILLS_NAMESPACE, &skill_key(space_id, slug)).await? {
        Some(e) => Ok(serde_json::from_str::<Skill>(&e.content).ok()),
        None => Ok(None),
    }
}

/// Top-N skills in `space_id` by usage_count DESC, then cited_count DESC, then recency.
pub async fn top_skills(adapter: &Arc<dyn MemoryAdapter>, space_id: &str, limit: usize) -> anyhow::Result<Vec<Skill>> {
    let entries = adapter.list(Some(SKILLS_NAMESPACE), None, None).await?;
    let mut skills: Vec<(Skill, String)> = entries.into_iter()
        .filter_map(|e| serde_json::from_str::<Skill>(&e.content).ok().map(|s| (s, e.timestamp)))
        .filter(|(s, _)| s.space == space_id)
        .collect();
    skills.sort_by(|(a, ta), (b, tb)| {
        b.usage_count.cmp(&a.usage_count)
            .then(b.cited_count.cmp(&a.cited_count))
            .then(tb.cmp(ta))
    });
    Ok(skills.into_iter().take(limit).map(|(s, _)| s).collect())
}

/// Increment cited_count; returns the NEW count (for promotion threshold), None if absent.
pub async fn bump_cited(adapter: &Arc<dyn MemoryAdapter>, space_id: &str, slug: &str) -> anyhow::Result<Option<u64>> {
    match get_skill(adapter, space_id, slug).await? {
        Some(mut s) => { s.cited_count = s.cited_count.saturating_add(1); let n = s.cited_count; put_skill(adapter, &s).await?; Ok(Some(n)) }
        None => Ok(None),
    }
}

/// Increment usage_count; false if absent.
pub async fn bump_usage(adapter: &Arc<dyn MemoryAdapter>, space_id: &str, slug: &str) -> anyhow::Result<bool> {
    match get_skill(adapter, space_id, slug).await? {
        Some(mut s) => { s.usage_count = s.usage_count.saturating_add(1); put_skill(adapter, &s).await?; Ok(true) }
        None => Ok(false),
    }
}
```

(Confirm `MemoryEntry.timestamp` is the field name for the recency tiebreak; confirm `MemoryCategory`/`adapter.store`/`adapter.get`/`adapter.list` signatures match the existing `put_skill`/`get_skill` usage.)

- [ ] **Step 3: Update the existing P1c tests for the new fields/signatures**

Every `Skill { … }` literal in the test module gains `space: "default".into(), usage_count: <n>`. Calls to `get_skill`/`top_skills`/`bump_cited` gain a `space_id` arg. Then ADD:

```rust
#[tokio::test]
async fn top_skills_orders_by_usage_then_cited_and_scopes_by_space() {
    let a = InMemoryAdapter::new();
    put_skill(&a, &Skill{slug:"x".into(),space:"s1".into(),name:"X".into(),body:"".into(),usage_count:1,cited_count:9,keywords:vec![],status:"draft".into()}).await.unwrap();
    put_skill(&a, &Skill{slug:"y".into(),space:"s1".into(),name:"Y".into(),body:"".into(),usage_count:5,cited_count:0,keywords:vec![],status:"draft".into()}).await.unwrap();
    put_skill(&a, &Skill{slug:"z".into(),space:"s2".into(),name:"Z".into(),body:"".into(),usage_count:99,cited_count:0,keywords:vec![],status:"draft".into()}).await.unwrap();
    let top = top_skills(&a, "s1", 10).await.unwrap();
    assert_eq!(top.iter().map(|s| s.slug.clone()).collect::<Vec<_>>(), vec!["y".to_string(), "x".to_string()]); // usage 5 > 1; s2 excluded
}

#[tokio::test]
async fn same_slug_different_space_no_collision() {
    let a = InMemoryAdapter::new();
    put_skill(&a, &Skill{slug:"dup".into(),space:"s1".into(),name:"A".into(),body:"a".into(),usage_count:0,cited_count:0,keywords:vec![],status:"draft".into()}).await.unwrap();
    put_skill(&a, &Skill{slug:"dup".into(),space:"s2".into(),name:"B".into(),body:"b".into(),usage_count:0,cited_count:0,keywords:vec![],status:"draft".into()}).await.unwrap();
    assert_eq!(get_skill(&a,"s1","dup").await.unwrap().unwrap().name, "A");
    assert_eq!(get_skill(&a,"s2","dup").await.unwrap().unwrap().name, "B");
}

#[tokio::test]
async fn bump_usage_and_cited_scoped() {
    let a = InMemoryAdapter::new();
    put_skill(&a, &Skill{slug:"s".into(),space:"sp".into(),name:"S".into(),body:"".into(),usage_count:0,cited_count:0,keywords:vec![],status:"draft".into()}).await.unwrap();
    assert_eq!(bump_cited(&a,"sp","s").await.unwrap(), Some(1));
    assert!(bump_usage(&a,"sp","s").await.unwrap());
    assert!(!bump_usage(&a,"sp","absent").await.unwrap());
    let s = get_skill(&a,"sp","s").await.unwrap().unwrap();
    assert_eq!((s.cited_count, s.usage_count), (1, 1));
}
```

- [ ] **Step 4: Test + build** — `cd src-tauri && cargo test --lib memory_adapter::skills 2>&1 | tail -15` → green; `cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/p3-skills-store-migration
git add src-tauri/src/memory_adapter/skills.rs
git commit -m "feat(memory_adapter): extend skills facade — space scoping + usage_count ranking + bump_usage (P3-skills)"
```

---

### Task 2: Migration module (Procedure nodes → Skill)

**Files:** Create `src-tauri/src/proactive/skill_migration.rs`; modify `src-tauri/src/proactive/mod.rs` (`pub mod skill_migration;`) + `src-tauri/src/app.rs` (boot spawn).

- [ ] **Step 1: Read the sources** — `MemoryGraphStore::list_nodes_by_kind` (store.rs:170 — confirm args: kind + maybe space/limit) + `get_active_version` (609) + `MemoryNode` fields (`id`, `title`, `space_id`, `metadata_json`) + how `cited_count`/`usage_count`/`status` are read from `metadata_json` (see `list_top_skills_by_usage`'s `json_extract`). Read `proactive/memory_migration.rs::migrate_episodes` for the marker/idempotency/spawn idiom.

- [ ] **Step 2: Pure mapping + migration fn**

```rust
//! P3-skills — one-time migration of memory_graph Procedure-node skills into the
//! adapter "skills" namespace. Idempotent, marker-gated, boot-safe (mirrors
//! gbrain_page_migration / migrate_episodes). Versioning collapsed to the Active
//! version (latest-wins, per P1c).

use std::sync::Arc;
use crate::memory_adapter::{skills::{self, Skill}, MemoryAdapter};
use crate::memory_graph::store::MemoryGraphStore;
use crate::proactive::skill_parser::normalize_title_for_dedup;

const SKILL_MIGRATION_MARKER: &str = "__skills_migrated_v1__";

/// Map a Procedure node (+ its Active version body + metadata) → Skill.
/// `meta` is the parsed metadata_json (cited_count/usage_count/status/keywords).
fn node_to_skill(space: &str, title: &str, body: String, cited: u64, usage: u64, status: String, keywords: Vec<String>) -> Skill {
    Skill {
        slug: normalize_title_for_dedup(title),
        space: space.to_string(),
        name: title.to_string(),
        body,
        usage_count: usage,
        cited_count: cited,
        keywords,
        status,
    }
}

pub async fn migrate_skills(store: &Arc<MemoryGraphStore>, adapter: &Arc<dyn MemoryAdapter>) -> usize {
    // Idempotency: skip if the completion marker exists (in any space — store the marker under a reserved space "__migration__").
    if matches!(skills::get_skill(adapter, "__migration__", SKILL_MIGRATION_MARKER).await, Ok(Some(_))) {
        return 0;
    }
    // Read all Procedure nodes (recon the exact list_nodes_by_kind call; iterate spaces if it is space-scoped).
    let nodes = match /* store.list_nodes_by_kind(Procedure, …) */ Ok::<Vec<_>, ()>(vec![]) {
        Ok(n) => n, Err(_) => { tracing::warn!("skill migration: list failed; skip"); return 0; }
    };
    let mut migrated = 0usize; let mut all_ok = true;
    for node in nodes {
        // body = Active version content (fallback to node's own content/title if none)
        // cited/usage/status/keywords parsed from node.metadata_json
        let skill = node_to_skill(/* … from node … */);
        if let Err(e) = skills::put_skill(adapter, &skill).await { tracing::warn!(error=%e, "skill migrate: put failed"); all_ok = false; continue; }
        migrated += 1;
    }
    if all_ok {
        let marker = Skill { slug: SKILL_MIGRATION_MARKER.into(), space: "__migration__".into(), name: "skills migrated (P3)".into(), body: String::new(), usage_count: 0, cited_count: 0, keywords: vec![], status: "_migration_marker".into() };
        let _ = skills::put_skill(adapter, &marker).await;
    }
    tracing::info!(migrated, all_ok, "skill migration pass complete");
    migrated
}
```

> The `node_to_skill` call + the `list_nodes_by_kind` invocation + the `metadata_json` field extraction are recon'd in Step 1 and filled with the real shapes — extract a pure `node_to_skill`-style helper fed concrete fields so it is unit-testable without the store (mirror P2b's `page_detail_to_page`). Also ensure `top_skills`/`get_learned_skill` reads filter out the `__migration__` space / `_migration_marker` status so the marker never surfaces.

- [ ] **Step 3: Boot spawn in app.rs** — after `bucket_seal_adapter` + `memory_graph_store` are built, fire-and-forget (mirror the proactive-episode-migration spawn):

```rust
{
    let adapter = bucket_seal_adapter.clone() as Arc<dyn crate::memory_adapter::MemoryAdapter>;
    let store = memory_graph_store.clone();
    tauri::async_runtime::spawn(async move {
        let n = crate::proactive::skill_migration::migrate_skills(&store, &adapter).await;
        tracing::info!(migrated = n, "P3-skills: skill migration spawn complete");
    });
}
```

- [ ] **Step 4: Tests** — pure `node_to_skill` mapping (fields → Skill); marker idempotency (marker present → `migrate_skills` returns 0 without reading). Use the in-memory adapter + a stub/skip for the store read (the testable seam is `node_to_skill` + the marker check; a live store read is a gated integration test).

- [ ] **Step 5: Build + test + commit**

`cargo test --lib skill_migration 2>&1 | tail` green; `cargo build` clean.
```bash
git add src-tauri/src/proactive/skill_migration.rs src-tauri/src/proactive/mod.rs src-tauri/src/app.rs
git commit -m "feat(proactive): skill_migration — Procedure nodes → adapter skills (boot, idempotent) (P3-skills)"
```

---

### Task 3: config flag `skill_store_repoint_enabled`

**Files:** `src-tauri/src/memubot_config.rs` (mirror `gbrain_read_repoint_enabled` exactly: field + `default_*` fn + manual `impl Default` entry + 2 tests).

- [ ] **Step 1–4:** Add `#[serde(default = "default_skill_store_repoint_enabled")] pub skill_store_repoint_enabled: bool,` + `fn default_skill_store_repoint_enabled() -> bool { true }` + `skill_store_repoint_enabled: true,` in `impl Default` + `skill_store_repoint_enabled_defaults_on` and `memory_os_deserializes_without_skill_store_repoint_field` tests (copy the sibling P2c-1 tests, swap the field).
- [ ] **Step 5:** `cargo test --lib skill_store_repoint 2>&1 | tail -8` → pass; build clean.
- [ ] **Step 6:** commit `feat(config): skill_store_repoint_enabled (default on) (P3-skills)` (path `src-tauri/src/memubot_config.rs`).

---

### Task 4: W — write repoint (`store_skill_as_procedure` path)

**Files:** `src-tauri/src/proactive/service.rs` (~2095) + `src-tauri/src/proactive/skill_parser.rs` (the `store_skill_as_procedure` call site / a new adapter-backed path).

- [ ] **Step 1:** Read `service.rs:~2080–2120` — it calls `store_skill_as_procedure(&refs.memory_graph_store, skill, &space_id)`. Confirm `refs`/the service holds (or can get) the `bucket_seal_adapter` + the config flag.
- [ ] **Step 2:** Gate: when `skill_store_repoint_enabled`, write via the facade instead of memory_graph:

```rust
if skill_store_repoint_enabled {
    let s = crate::memory_adapter::skills::Skill {
        slug: crate::proactive::skill_parser::normalize_title_for_dedup(&skill.name),
        space: space_id.clone(),
        name: skill.name.clone(),
        body: crate::proactive::skill_parser::build_version_content(skill),
        usage_count: 0, cited_count: 0, keywords: skill.keywords.clone(), status: "draft".into(),
    };
    // preserve existing usage_count/cited_count/status if the skill already exists (dedup = update, not reset):
    if let Ok(Some(existing)) = crate::memory_adapter::skills::get_skill(&adapter, &space_id, &s.slug).await {
        let s = crate::memory_adapter::skills::Skill { usage_count: existing.usage_count, cited_count: existing.cited_count, status: existing.status, ..s };
        crate::memory_adapter::skills::put_skill(&adapter, &s).await.ok();
    } else {
        crate::memory_adapter::skills::put_skill(&adapter, &s).await.ok();
    }
} else {
    crate::proactive::skill_parser::store_skill_as_procedure(&refs.memory_graph_store, skill, &space_id)?; // unchanged
}
```

> Confirm `ParsedSkill` fields (`name`, `keywords`) + `build_version_content(skill)` (skill_parser.rs:216, the version body builder — reuse it for `body`). Thread the `bucket_seal_adapter` + flag into `service.rs` where the skill loop runs (read the flag from the config the service already holds, or `state`/`refs`). Preserve-on-update prevents a re-learn from zeroing an existing skill's counts.

- [ ] **Step 3:** build clean; commit `feat(proactive): site W — learned-skill write repoints to adapter skills (P3-skills)` (paths: service.rs + skill_parser.rs if touched).

---

### Task 5: R — rank-read repoint (`memu_tools.rs`)

**Files:** `src-tauri/src/agent/tools/memu_tools.rs` (~168).

- [ ] **Step 1:** Read ~150–185 — it calls `store.list_top_skills_by_usage(&self.space_id, limit)` and maps the result. Confirm the tool holds (or can get) the `bucket_seal_adapter` + the flag.
- [ ] **Step 2:** Gate: when `skill_store_repoint_enabled`, `let skills = crate::memory_adapter::skills::top_skills(&adapter, &self.space_id, limit).await?;` and map `Skill` → the tool's output shape (name/body/keywords) the same way the memory_graph result was mapped. Else the unchanged `list_top_skills_by_usage` path.
- [ ] **Step 3:** build clean; commit `feat(agent): site R — top-skills tool reads adapter skills (P3-skills)`.

---

### Task 6: G — get repoint (`get_learned_skill`)

**Files:** `src-tauri/src/tauri_commands.rs` (~8532).

- [ ] **Step 1:** Read `get_learned_skill` — confirm its args (slug + space?) + return shape + that `state` is in scope.
- [ ] **Step 2:** Gate: when on, `crate::memory_adapter::skills::get_skill(&adapter, &space_id, &slug)` → map to the command's return type; else unchanged. (Derive `space_id` the same way the command does today — recon it.)
- [ ] **Step 3:** build clean; commit `feat(memory): site G — get_learned_skill reads adapter skills (P3-skills)`.

---

### Task 7: C — cite + usage-bump repoint (`feedback.rs`)

**Files:** `src-tauri/src/proactive/feedback.rs` (the cited/usage bump SQL) + the `record_skill_cited` caller + `PROMOTION_THRESHOLD`.

- [ ] **Step 1:** Read `feedback.rs` — the citation path bumps `metadata_json.$.cited_count` + `$.usage_count`; the "unhelpful" path bumps `usage_count` only. Recon `PROMOTION_THRESHOLD` + where draft→promoted flips.
- [ ] **Step 2:** Gate each bump path: when on, the citation path → `skills::bump_cited(&adapter, space, slug)` (returns new count) + `skills::bump_usage(...)`; if the returned cited count `>= PROMOTION_THRESHOLD`, read the skill, set `status = "promoted"`, `put_skill`. The "unhelpful" path → `skills::bump_usage` only. Else the unchanged SQL. Map the node-id/title used by feedback.rs to the skill `slug` (`normalize_title_for_dedup` or the stored mapping — recon how feedback.rs identifies the skill).
- [ ] **Step 3:** build clean; if `record_skill_cited`/promotion is unit-testable, assert the threshold flip; commit `feat(proactive): site C — skill cite/usage bump + promotion repoint to adapter (P3-skills)`.

---

### Task 8: Whole-slice verification

- [ ] `cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] `cargo test --lib memory_adapter::skills`, `--lib skill_migration`, `--lib skill_store_repoint`, `--lib proactive` → green (modulo known unrelated env failures).
- [ ] `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] confirm: `grep -rn "skill_store_repoint_enabled" src-tauri/src/` shows the 4 repoint sites + the config; `grep -n "migrated_v1\|skill_migration" src-tauri/src/proactive/skill_migration.rs`.
- [ ] `gitnexus_detect_changes()` before the PR.

## Adjacent-edit checklist (PR body)

- **`skills.rs` fn signatures changed** (added `space_id` + new fields) → P1c had no live callers, so only the facade's own tests + the new P3 call sites use them (no external breakage).
- **`MemoryOsConfig` new `#[serde(default)]` field** + manual `impl Default` → backward-compatible (deserialize-without-field test).
- New boot spawn in `app.rs` (`[Stage 3]`-style migration spawn).
- No schema migration; memory_graph code retained (gated) — deleted in P4.

## PR shape

One branch `claude/p3-skills-store-migration`, one PR with a `## Commits (bisectable)` table (Tasks 1–7 = 7 commits). Title: `feat(memory): P3-skills — migrate skill_parser store to adapter (gated, faithful)`. Body: extends skills facade (space + usage_count); migrates Procedure nodes; repoints write/rank/get/cite behind `skill_store_repoint_enabled`; versioning dropped (P1c); memory_graph retained until P4; P3-edges separate.

## Self-review notes

- **Spec coverage:** §1 facade → Task 1; §2 migration → Task 2; §3 gate+4 sites → Tasks 3–7 (W/R/G/C); usage_count site → Task 7. ✔
- **Type consistency:** `Skill` (+space,+usage_count) consistent across facade/migration/repoints; `bump_cited -> Option<u64>` (count) used by Task 7's promotion; `top_skills(adapter, space_id, limit)` used by Task 5; `get_skill(adapter, space_id, slug)` by Task 6. ✔
- **Bisectability:** Task 1 (facade, tests-only callers) compiles; Task 2 (migration, uses facade) compiles; Task 3 (flag) compiles; Tasks 4–7 each gate one site (flag from Task 3, facade from Task 1) — compile independently (the `else` branch keeps memory_graph). ✔
- **Follow-the-recon items** (flagged, not placeholders): `list_nodes_by_kind` exact args + `metadata_json` extraction (Task 2 Step 1); `MemoryEntry.timestamp` name (Task 1); `ParsedSkill` fields + `build_version_content` (Task 4); how each site derives `space_id` + identifies the skill slug (Tasks 5–7); `PROMOTION_THRESHOLD` (Task 7). Each has explicit "read the site first" guidance.
