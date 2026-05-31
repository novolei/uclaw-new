# P1c — Skill-Store Adapter Layer (thin facade) Design

**Date:** 2026-05-31
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P1** (MemoryAdapter capability growth), final slice **P1c** (versioning/ranking → a ranked keyed skill store). Unblocks **P3** (migrate skill_parser's learned-skill store off the frozen memory_graph). Completes P1's three capability slices (page / graph / skill). Follows the P1a (`pages.rs`) / P1b (`edges.rs`) thin-facade pattern.

## Problem

P3 will migrate `proactive/skill_parser.rs`'s learned-skill store off memory_graph onto the adapter. skill_parser currently uses rich memory_graph ops: `find_learned_skill_by_normalized_title` (write-time dedup), `list_top_learned_skills` (top-N by `cited_count`), `create_version` (supersedes chain), `create_keyword` (keyword index), plus `cited_count` increment + decay. The `MemoryAdapter` + bucket_seal have no skill/ranking/version concept.

## Decision (P1c scope)

Add a thin, additive **skill-store facade** over the existing `MemoryAdapter` methods: a `Skill` keyed by normalized slug + `put_skill`/`get_skill`/`top_skills`/`bump_cited`. **Latest-wins, no version history** (the supersedes chain is gbrain-graph richness; `list_top` ranks by `cited_count`, not version, so version history is not load-bearing for the dedup/ranking/recall behaviors P3 needs — approved). Normalized-title dedup = keying by slug; ranking = sort by a `cited_count` field; keywords ride in content (recall-matchable); `cited_count`++/decay = read-modify-write. No trait change, no live wiring, no skill_parser change (P3 repoints).

## Design

### New module `src-tauri/src/memory_adapter/skills.rs`

```rust
use std::sync::Arc;
use crate::memory_adapter::{MemoryAdapter, MemoryCategory};

const SKILLS_NAMESPACE: &str = "skills";

/// A learned skill — the ranked-keyed-store subset of skill_parser's record.
/// `slug` is the normalized-title key (write-time dedup = same slug overwrites).
/// Version history is NOT modeled (latest-wins); see the convergence ADR P1c.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    pub slug: String,
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub cited_count: u64,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub status: String,
}

/// Upsert a skill (dedup by `slug`, latest-wins overwrite). key = slug, content = JSON(Skill).
pub async fn put_skill(adapter: &Arc<dyn MemoryAdapter>, skill: &Skill) -> anyhow::Result<()> {
    let content = serde_json::to_string(skill)?;
    adapter.store(SKILLS_NAMESPACE, &skill.slug, &content, MemoryCategory::Core, None).await
}

/// Fetch a skill by slug (normalized-title dedup query). None if absent or unparseable.
pub async fn get_skill(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<Option<Skill>> {
    match adapter.get(SKILLS_NAMESPACE, slug).await? {
        Some(entry) => Ok(serde_json::from_str::<Skill>(&entry.content).ok()),
        None => Ok(None),
    }
}

/// Top-N skills by `cited_count` descending (ranking ≈ list_top_learned_skills).
/// List-scans the namespace (fine for the learned-skill volume; an index is later).
pub async fn top_skills(adapter: &Arc<dyn MemoryAdapter>, limit: usize) -> anyhow::Result<Vec<Skill>> {
    let entries = adapter.list(Some(SKILLS_NAMESPACE), None, None).await?;
    let mut skills: Vec<Skill> = entries
        .into_iter()
        .filter_map(|e| serde_json::from_str::<Skill>(&e.content).ok())
        .collect();
    skills.sort_by(|a, b| b.cited_count.cmp(&a.cited_count));
    skills.truncate(limit);
    Ok(skills)
}

/// Increment a skill's `cited_count` by 1 (read-modify-write). Returns `false`
/// if the skill is absent (no-op). (Decay etc. compose via get_skill + put_skill.)
pub async fn bump_cited(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<bool> {
    match get_skill(adapter, slug).await? {
        Some(mut skill) => {
            skill.cited_count = skill.cited_count.saturating_add(1);
            put_skill(adapter, &skill).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}
```

Declared `pub mod skills;` in `memory_adapter/mod.rs` + re-export `Skill`/`put_skill`/`get_skill`/`top_skills`/`bump_cited`.

- **Facade, not trait** — over `store`/`get`/`list`; no trait change.
- **Dedup by slug** — `put_skill` keys on the normalized `slug` → same slug overwrites (≈ `find_learned_skill_by_normalized_title` + upsert). Caller normalizes the title into `slug` (P3 reuses skill_parser's `normalize_title_for_dedup`).
- **Ranking** — `top_skills` sorts by `cited_count` desc (≈ `list_top_learned_skills`).
- **`cited_count`** mutation — `bump_cited` is read-modify-write; decay (and any other field mutation) composes via `get_skill` + `put_skill`.
- **keywords** ride in `Skill.keywords` (in content) → recall/FTS can match; no dedicated keyword-index API (YAGNI).

## Data flow

```
put_skill(adapter, Skill{slug,...,cited_count,keywords})
  → adapter.store("skills", slug, JSON(Skill), Core, None)   [dedup/overwrite by slug]
get_skill(adapter, slug) → adapter.get("skills", slug) → Some(Skill)/None
top_skills(adapter, n)   → adapter.list("skills") → parse → sort by cited_count desc → take n
bump_cited(adapter, slug)→ get_skill → cited_count+1 → put_skill → true (false if absent)
```

(Not invoked by any live path in P1c. P3 wires skill_parser's store_skill_as_procedure / list_top / cited_count to it.)

## Error handling

All fns propagate the adapter's `anyhow::Result`. Malformed stored content → `get_skill` None / skipped in `top_skills`, never panics. `bump_cited` on an absent slug → `Ok(false)` (no-op).

## Testing

1. `put_skill` then `get_skill` round-trips all fields (slug/name/body/cited_count/keywords/status).
2. Dedup/overwrite: two `put_skill` with the same `slug` → one stored entry; `get_skill` returns the latest (e.g. updated `cited_count`).
3. `top_skills`: several skills with varying `cited_count` → returned sorted desc, truncated to `limit`.
4. `bump_cited`: increments by 1 (get→+1→store) and returns `true`; on an absent slug → `false` (no entry created).
5. `get_skill`/`top_skills` skip malformed (non-`Skill`) content; serde `#[serde(default)]` lets older content (no cited_count/keywords/status) parse.
6. `cargo test --lib memory_adapter::skills` + build clean + clippy clean; broader `memory_adapter` green; `Cargo.toml` unchanged.

(Reuse the in-memory `MemoryAdapter` test stub pattern from `pages.rs`/`edges.rs` — `list(Some(ns))` returns all entries in `ns`.)

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/skills.rs` | **new** — `Skill` + `put_skill`/`get_skill`/`top_skills`/`bump_cited` + in-memory-stub tests |
| `memory_adapter/mod.rs` | `pub mod skills;` + re-exports |

**Out of scope (later):** version history (supersedes chains); bigram fuzzy-dedup; a dedicated keyword index; decay policy; an index for `top_skills` (list-scan is fine for the learned-skill volume); **P3** the skill_parser migration + repointing; **P2** gbrain. No live wiring in P1c.

## Risk

Low. Pure additive facade over existing tested adapter methods; no trait change, no live wiring, no skill_parser change. `top_skills` list-scan is O(skills) — acceptable for the learned-skill volume (documented as a later optimization, not a silent cap). One branch, bisectable. Completes P1's three capability slices.
