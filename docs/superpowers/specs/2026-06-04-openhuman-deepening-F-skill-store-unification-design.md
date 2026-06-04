# openhuman Deepening · Slice F — Skill Store Unification Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** openhuman rich-memory deepening ([[project-openhuman-deepening]]). Slice F — collapse the TWO parallel skill stores into one: make the memory_graph **Procedure node** the single source of truth and demote the bucket_seal `skills` namespace to a **derived recall projection** (semantic-search only), kept in sync continuously like Slice A's `recall_projection`.

## Problem

Skills live in two stores (recon-confirmed):

1. **memory_graph Procedure nodes** (`proactive/skill_parser.rs`) — the rich, authoritative store: `MemoryNode{kind=Procedure}` + versioned `memory_versions` (supersession chain) + `memory_keywords` + rich metadata (`signals`, `validation_hint`, `category`, `anti_patterns`, `lifecycle`, `tags`, `usage_count`, `cited_count`, `returned_count`, `promoted_at`), D1/D2 dedup, and the GEP promotion anchor (`skill_promote_min_returned_count` keyed on node id). Written continuously by `store_skill_as_procedure` (extraction). Read by manifest injection (`skills_manifest::list_promoted_learned_skills`), `list_learned_skills`, `list_invocable_skills`.

2. **bucket_seal `skills` facade** (`memory_adapter/skills.rs`) — a thin latest-wins copy: `Skill{slug, space, name, body, usage_count, cited_count, keywords, status}` in the `"skills"` adapter namespace (FTS-searchable via `recall_hybrid`). **Populated only by the one-time boot migration `skill_migration::migrate_skills`** (Procedure → adapter, P3-skills). Read by `resolve_slash_skill` (the `/skill-name` path — its Procedure fallback was deliberately removed), `get_learned_skill`, and `skills::search` (semantic). Written ongoing by `bump_cited`/`bump_usage` read-modify-write.

The split means: the adapter copy goes stale after boot (extraction writes Procedure, not the adapter), `bump_*` mutate the adapter copy that diverges from Procedure's counters, and there are two parallel write models. The adapter's ONE genuine capability the Procedure store lacks is **semantic FTS search** over skills.

## Decision (approved 2026-06-04, refined after plan-recon)

**Plan-recon correction:** the recent P3-skills migration declared the adapter "the new source of truth" and memory_graph "read-only legacy" + removed the Procedure read-fallback — BUT skill **extraction never stopped writing Procedure**. So P3 was a one-time copy that left every *post-boot* extracted skill un-projected (invisible to the adapter read paths: slash / `load_skill` / `skill_search` / `top_skills`). And the adapter read/write surface is large (7+ prod sites incl. agent tools), with `usage_count`/`cited_count` living in BOTH stores.

Approved approach — **Procedure authoritative + adapter as a CONTINUOUS projection; reads stay on the projection, writes go to Procedure**:

- **Procedure node = single source of truth** for skill data AND counters (richer: version history, rich metadata, GEP anchor, dedup). This re-establishes the openhuman "memory_graph authoritative + bucket_seal recall face" pattern (Slice A) and reverses P3's never-completed "adapter authoritative" intent.
- **Adapter `skills` namespace = a content-hash-idempotent projection**, refreshed on every authoritative write (extraction + counter bump). The projection's job: be the always-current read/search face.
- **Reads stay UNCHANGED on the projection** (lowest blast radius): `resolve_slash_skill` body read, `get_learned_skill`, `load_skill`, `skill_search` (semantic), `top_skills` — they read the now-always-current projection. No read repointing.
- **Writes go to Procedure + re-project**: counter bumps (`usage_count`/`cited_count`) and lifecycle promotion mutate Procedure metadata, then re-project so the read face stays current. The adapter's own `bump_cited`/`bump_usage` (read-modify-write the projection) are replaced by a `bump_skill_and_reproject(store, adapter, space, slug, …)` helper.
- **No migration, no new config** (both schemas exist).

## Design

### §1 `project_skill` — the derived projection
New `proactive/skill_parser.rs` (or `memory_adapter/skill_projection.rs` — plan pins home) function, mirroring `recall_projection::project_fact`:
```
project_skill(adapter: &Arc<BucketSealAdapter>, node: &MemoryNode, body: &str)
```
Builds a `Skill` from the Procedure node (slug = normalized title or node id; name = title; body = active version content; usage_count/cited_count/keywords/status from metadata) and `put_skill`s it into the `"skills"` namespace. Best-effort (log on error; the Procedure write is authoritative). Idempotent: `put_skill` is keyed `space\x01slug` (latest-wins), so re-projecting the same skill overwrites one row.

Call `project_skill` at the end of BOTH `store_skill_as_procedure` (create path) AND `upgrade_existing_skill` (merge path) — so the projection tracks every authoritative write.

### §2 Backfill existing Procedure skills
A marker-gated one-time backfill (mirror Slice A's `recall_projection_backfill`): on boot, if marker `__skill_projection_backfill_v1__` absent, enumerate all Procedure skill nodes per space, `project_skill` each, then set the marker (all-ok gate). This replaces `skill_migration::migrate_skills` as the population mechanism — but unlike the old one-time migration, the projection now ALSO stays live via §1.

### §3 Reads stay on the projection (NO repointing)
`resolve_slash_skill` (body read), `get_learned_skill`, `load_skill`, `skill_search` (semantic), `top_skills` all keep reading the adapter `skills` projection. Because §1 keeps it continuously current, the split bug ("new skills invisible") is fixed without touching any read path. This is the lowest-blast-radius unification.

### §4 Writes go to Procedure + re-project
- New `bump_skill_and_reproject(store, adapter, space_id, slug, bump_cited: bool, bump_usage: bool) -> Option<u64>`: resolve the Procedure node by slug (`find_learned_skill_by_normalized_title`), bump its `metadata.usage_count`/`cited_count` (store helpers `bump_skill_usage` exists; add `bump_skill_cited` mirroring it), re-`project_skill`, return the new cited_count (for the promotion threshold check).
- New `promote_skill_and_reproject(store, adapter, space_id, slug)`: set Procedure `metadata.lifecycle='promoted'` (via `update_node`), re-project.
- Repoint the WRITE call sites — `resolve_slash_skill`, `record_skill_cited`, `load_skill` (usage bump), `skill_search` (usage bump on returned hits), and the `service.rs` reinforcement get/put — to use these helpers instead of `skills::bump_cited`/`bump_usage`/`put_skill`.
- Remove `skill_migration::migrate_skills` + its boot call (superseded by §2 backfill + §1 continuous projection).
- In `memory_adapter/skills.rs`: keep `Skill`, `put_skill` (now called only by `project_skill`/projection), `get_skill`/`top_skills`/`search` (read face); **remove** `bump_cited`/`bump_usage` (replaced by the Procedure-authoritative helpers).

## Data flow (after F)

```
extraction → store_skill_as_procedure / upgrade_existing_skill (Procedure = source of truth)
           → project_skill(adapter, node, body)  // refresh "skills" projection (FTS read face)
boot (once) → skill_projection_backfill: all Procedure skills → projection (marker-gated; replaces migrate_skills)
reads (UNCHANGED): /slug body, get, load_skill, skill_search, top_skills → adapter projection (always current)
writes: counter bump / lifecycle promote → Procedure metadata → re-project (read face stays current)
```

## Out of scope

Enriching the projection `Skill` with the full rich metadata (the projection stays thin — it's a search index, not the truth); changing the GEP promotion path; changing the on-disk SKILL.md / SkillsRegistry discovery (orthogonal — disk skills are a separate tier); a config gate (the projection is strictly-better continuous sync).

## Error handling

`project_skill` best-effort: errors log + swallow, never block the authoritative Procedure write (mirror Slice A posture). Backfill is all-ok-gated (marker only set if every projection succeeded, so a partial failure retries next boot). Read repointing is null-safe (missing Procedure node → None, same as today's missing adapter row).

## Testing

1. **`project_skill`**: a Procedure node → a `skills` namespace row with matching slug/name/body/counters; re-project (same content) overwrites one row (idempotent).
2. **Create + merge project**: `store_skill_as_procedure` and `upgrade_existing_skill` each leave the projection in sync with the node's active version + counters.
3. **Backfill**: N existing Procedure skills + absent marker → N projection rows + marker set; re-run is a no-op.
4. **resolve_slash_skill**: `/known-slug` resolves via Procedure (no adapter dependency); unknown slug → None.
5. **bump**: `bump_usage`/`bump_cited` increments the Procedure metadata counter AND the projection reflects it.
6. **semantic search still works**: `skills::search` returns the projected skill for a semantic query.
7. `cargo build`/clippy clean; `cargo test --lib` for `proactive::skill_parser`, `memory_adapter::skills`, and the broad dependent set; grep gate: `migrate_skills` gone, no remaining `skills::bump_cited`/`bump_usage` callers.

## Scope / files

| File | Change |
|---|---|
| `proactive/skill_parser.rs` | `project_skill` + call it in `store_skill_as_procedure` + `upgrade_existing_skill`; counter bump helper usage |
| `memory_graph/store.rs` | `bump_skill_counter` (if absent) + confirm `find_learned_skill_by_normalized_title`/`list_top_learned_skills` |
| skill projection backfill (new, mirror `recall_projection_backfill.rs`) | marker-gated boot backfill + spawn in `app.rs` |
| `memory_adapter/skills.rs` | keep `put_skill`/`get_skill`/`top_skills`/`search`; remove `bump_cited`/`bump_usage` |
| `proactive/skill_migration.rs` | remove `migrate_skills` + its boot call (`app.rs`) |
| `tauri_commands.rs` | repoint `resolve_slash_skill`/`get_learned_skill`/`bump`/`top_skills` reads to Procedure |

## Risk

Med. Touches the slash-command + counter read paths (user-visible). Main risks: (1) **slug↔title mapping** — the adapter keyed on `slug`, Procedure on `title`; the projection must derive a stable slug from the node so `/slug` resolves (plan pins `normalize_title` → slug, consistent both ways); (2) **counter source of truth** — moving `bump_*` to Procedure metadata must keep the projection in sync (re-project on bump); (3) removing `migrate_skills` must not orphan skills only present in the adapter (none should exist — extraction always wrote Procedure; the adapter was always a downstream copy — the backfill re-derives from Procedure); (4) the Slice-C/D fixture lesson for any new store helper. Bisectable: project_skill + wire → backfill → repoint reads → remove dead writers → verify. After F, skills have one source of truth (rich, versioned Procedure) with a thin always-current semantic-search projection.
