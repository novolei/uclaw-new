# P3-skills — skill_parser Store Migration → Adapter (gated, faithful) Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P3** (migrate memory_graph rich writers → the extended adapter), first slice **P3-skills**. Builds on **P1c** (`memory_adapter/skills.rs` facade). The sibling slice **P3-edges** (tool_memory's co-used-tools graph → `edges.rs`, with its own edge-weight + tool-stat-node gaps) is separate and later. Mirrors the migration pattern of **P2b** (`gbrain_page_migration`) and sub-project C (`proactive/memory_migration`).

## Problem

`skill_parser` (`src-tauri/src/proactive/skill_parser.rs`) is one of two **exempted** writers still writing the otherwise-frozen `memory_graph`: it persists learned skills as `Procedure` `MemoryNode`s + an Active `MemoryVersion` + keywords, ranked by usage for the LLM's "top skills" tool, with citation-driven draft→promoted promotion. Until it migrates onto the adapter, `memory_graph` cannot retire and P4 cannot remove the freeze hook.

Recon found the P1c `skills.rs` facade was scoped **more minimally than `skill_parser` needs**: `list_top_skills_by_usage` is `WHERE space_id=? … ORDER BY usage_count DESC, cited_count DESC, updated_at DESC`, but the facade has **no space scoping** and **no `usage_count`** (ranks by `cited_count` only), and **no versioning** (P1c's deliberate latest-wins). Versioning stays dropped; space + usage_count must be added for a faithful migration (the approved decision).

## Decision (P3-skills scope)

Extend the `skills.rs` facade to parity (space scoping + `usage_count` ranking), one-time-migrate the existing Procedure-node skills into the adapter `"skills"` namespace, and repoint `skill_parser`'s four touch points (write, rank-read, get, cite+promote) onto the facade behind a new `skill_store_repoint_enabled` flag (default on; rollback restores memory_graph). memory_graph keeps the skill data until P4 (the gate's rollback path); the repoint is replace (not dual-write) since memory_graph is being retired.

Out of scope: **P3-edges** (tool_memory); P4 (remove memory_graph + freeze hook). Versioning is intentionally dropped (latest-wins, per P1c).

## Design

### §1 Extend `skills.rs` facade (parity)

```rust
pub struct Skill {
    pub slug: String,        // bare normalized-title key (dedup)
    pub space: String,       // NEW — space_id scope
    pub name: String,
    pub body: String,        // the Active version content (latest-wins)
    pub usage_count: u64,    // NEW — primary ranking signal
    pub cited_count: u64,
    pub keywords: Vec<String>,
    pub status: String,      // "draft" / "promoted"
}
```

Single `"skills"` namespace; the adapter entry **key = `{space}\u{1}{slug}`** (space-qualified, so identical slugs in different spaces don't collide); `Skill.slug` stays the bare title key. Facade fns gain `space_id`:

- `put_skill(adapter, &skill)` — key from `skill.space` + `skill.slug`; latest-wins overwrite (= dedup).
- `get_skill(adapter, space_id, slug)` — `Some` only when the stored `space == space_id`.
- `top_skills(adapter, space_id, limit)` — list `"skills"`, filter `space == space_id`, sort **`usage_count` DESC → `cited_count` DESC → `MemoryEntry.timestamp` DESC**, take `limit`. (Mirrors the SQL `ORDER BY`.)
- `bump_cited(adapter, space_id, slug) -> Option<u64>` — read-modify-write `cited_count += 1`; **returns the new count** (so the caller can apply the promotion threshold); `None` if absent.
- `bump_usage(adapter, space_id, slug) -> bool` — **new** — `usage_count += 1`; `false` if absent.

(The migration marker page is filtered out of `top_skills`/`list` by a reserved slug + an excluded `status`/marker convention — same approach as P2b's `_migration_marker`.)

### §2 Migration — `proactive/skill_migration.rs` (new) or extend `memory_migration.rs`

One-time, idempotent, marker-gated, fire-and-forget at boot (the `migrate_episodes`/`gbrain_page_migration` idiom):

- Read **all** `MemoryNodeKind::Procedure` nodes from `MemoryGraphStore` (the plan recons the all-nodes-by-kind read — `list_top_skills_by_usage` is space+limit-scoped, so a broader read or per-space iteration is needed; capture the read that yields every Procedure node + its space_id + metadata).
- For each: fetch the **Active `MemoryVersion`** content → `body`; map `node.title→name`, node keywords→`keywords`, metadata `cited_count`/`usage_count`/`status`→fields, `node.space_id→space`, `slug = normalize_title_for_dedup(title)`.
- `pages`-style `put_skill` into `"skills"` (idempotent by the space-qualified key). Completion marker `__skills_migrated_v1__` (a reserved-slug `Skill` written only after a fully-successful pass — the P2b completion-marker pattern; a partial pass leaves no marker → retries next boot).
- Boot spawn in `app.rs` after `bucket_seal_adapter` + the graph store are built (the proactive-episode-migration spawn site to mirror). Infallible: graph-absent / per-node error → log + skip; never blocks boot.

### §3 Gate + repoint the four touch points

`MemoryOsConfig.skill_store_repoint_enabled: bool` (default `true`; rollback = false → the memory_graph paths below run unchanged). Thread `state.bucket_seal_adapter` (as `Arc<dyn MemoryAdapter>`) into each site (P2a-1-style where the site is graph-store-only):

| # | Site | Today | Repointed (when flag on) |
|---|---|---|---|
| **W** | `proactive/service.rs:~2095` → `skill_parser::store_skill_as_procedure(graph_store, skill, space_id)` | writes Procedure node + version + keywords | `skills::put_skill` (map `ParsedSkill`→`Skill`; slug = normalized title; `put_skill`'s overwrite = the fuzzy-dedup-by-slug) |
| **R** | `agent/tools/memu_tools.rs:168` `store.list_top_skills_by_usage(space_id, limit)` | space-ranked Procedure list | `skills::top_skills(adapter, space_id, limit)` |
| **G** | `tauri_commands.rs:8532` `get_learned_skill` | reads a Procedure node | `skills::get_skill(adapter, space_id, slug)` |
| **C** | `tauri_commands.rs` `record_skill_cited` | `cited_count++`, draft→promoted at `PROMOTION_THRESHOLD` | `skills::bump_cited` → if returned count ≥ threshold, set `status="promoted"` via `put_skill` |

- **`usage_count` increment:** recon where memory_graph bumps `usage_count` today (skill *use*, distinct from citation) and repoint that to `skills::bump_usage` — so the primary ranking signal stays live (else `top_skills` degrades to cited-only over time). If no live usage-bump site exists (usage_count only set at migration), note it and rank on the migrated value.
- **Replace, not dual-write:** memory_graph is being retired; the gate is the rollback. Repointed reads return adapter-only results (migration made the adapter complete).

### Data flow

```
boot: skill_migration (marker absent) → Procedure nodes (+Active version, +metadata) → put_skill into "skills"  [adapter complete]
learn:  store_skill_as_procedure → put_skill(space, slug)            [W]
rank:   memu top-skills tool → top_skills(space, limit)              [R]  (usage_count→cited_count→ts)
get:    get_learned_skill → get_skill(space, slug)                   [G]
cite:   record_skill_cited → bump_cited → promote at threshold       [C]
use:    <usage site> → bump_usage(space, slug)                       (keeps ranking live)
flag off ⇒ all four run the unchanged memory_graph paths (rollback)
```

## Error handling

Migration: P2b's infallible boot posture (graph-absent / per-node error → log+skip; marker only on full success; retries next boot). Repoint: each site, when the flag is on, uses the adapter; `put_skill`/`bump_*` errors log + are non-fatal to the surrounding pipeline (skill learning is best-effort today). Flag off → unchanged memory_graph behaviour.

## Testing

1. **Facade** (extend P1c's `skills.rs` tests, in-memory adapter): space isolation (`get_skill`/`top_skills` scoped to `space_id`; two spaces, same slug, no collision); `top_skills` orders by `usage_count` DESC then `cited_count` DESC then ts; `bump_usage` increments + `false` when absent; `bump_cited` returns the new count.
2. **Migration mapping** (pure, like P2b's `page_detail_to_page`): a Procedure node + Active version + metadata → `Skill` with correct fields; marker idempotency (marker present → skip; partial → re-run).
3. **Repoint sites** build-verified; where unit-testable (the `record_skill_cited` promotion logic), test count→status flip at threshold.
4. `cargo build` + `cargo test --lib memory_adapter::skills` + `--lib skill` + clippy clean.

## Scope / files

| File | Change |
|---|---|
| `src-tauri/src/memory_adapter/skills.rs` | extend `Skill` (`space`, `usage_count`); space-scope + usage-rank the fns; `bump_usage`; `bump_cited` returns count; tests |
| `src-tauri/src/proactive/skill_migration.rs` (or extend `memory_migration.rs`) | **new** — Procedure→Skill migration + marker + tests |
| `src-tauri/src/app.rs` | boot spawn for the skill migration |
| `src-tauri/src/memubot_config.rs` | `skill_store_repoint_enabled` flag + default + Default + tests |
| `src-tauri/src/proactive/service.rs` + `skill_parser.rs` | **W** write repoint |
| `src-tauri/src/agent/tools/memu_tools.rs` | **R** rank repoint |
| `src-tauri/src/tauri_commands.rs` | **G** get + **C** cite/promote repoint |

**Out of scope:** **P3-edges** (tool_memory → `edges.rs` + edge-weight + tool-stat-node home); **P4** (remove memory_graph rich-writer code + the freeze hook).

## Risk

Medium-high. Touches the learned-skill pipeline (write/rank/get/cite) across several modules + a facade extension + a migration. Mitigated: faithful facade extension (space + usage_count) avoids silent ranking/scoping degradation; migration-first makes the adapter complete before reads repoint; everything gated by `skill_store_repoint_enabled` (default on, rollback restores memory_graph, whose data is retained until P4). The main fidelity watch-item is the **`usage_count` increment site** — if missed, ranking decays to cited-only; the plan recons + repoints it. One branch, bisectable: facade extension → migration → flag → repoint sites.
