# openhuman Deepening · Slice F — Skill Store Unification Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** openhuman rich-memory deepening ([[project-openhuman-deepening]]). Slice F — collapse the TWO parallel skill stores into one: make the memory_graph **Procedure node** the single source of truth and demote the bucket_seal `skills` namespace to a **derived recall projection** (semantic-search only), kept in sync continuously like Slice A's `recall_projection`.

## Problem

Skills live in two stores (recon-confirmed):

1. **memory_graph Procedure nodes** (`proactive/skill_parser.rs`) — the rich, authoritative store: `MemoryNode{kind=Procedure}` + versioned `memory_versions` (supersession chain) + `memory_keywords` + rich metadata (`signals`, `validation_hint`, `category`, `anti_patterns`, `lifecycle`, `tags`, `usage_count`, `cited_count`, `returned_count`, `promoted_at`), D1/D2 dedup, and the GEP promotion anchor (`skill_promote_min_returned_count` keyed on node id). Written continuously by `store_skill_as_procedure` (extraction). Read by manifest injection (`skills_manifest::list_promoted_learned_skills`), `list_learned_skills`, `list_invocable_skills`.

2. **bucket_seal `skills` facade** (`memory_adapter/skills.rs`) — a thin latest-wins copy: `Skill{slug, space, name, body, usage_count, cited_count, keywords, status}` in the `"skills"` adapter namespace (FTS-searchable via `recall_hybrid`). **Populated only by the one-time boot migration `skill_migration::migrate_skills`** (Procedure → adapter, P3-skills). Read by `resolve_slash_skill` (the `/skill-name` path — its Procedure fallback was deliberately removed), `get_learned_skill`, and `skills::search` (semantic). Written ongoing by `bump_cited`/`bump_usage` read-modify-write.

The split means: the adapter copy goes stale after boot (extraction writes Procedure, not the adapter), `bump_*` mutate the adapter copy that diverges from Procedure's counters, and there are two parallel write models. The adapter's ONE genuine capability the Procedure store lacks is **semantic FTS search** over skills.

## Decision (approved 2026-06-04)

- **Procedure authoritative + adapter as derived projection** (Option A). The Procedure node is the single source of truth; the `skills` namespace becomes a content-hash-idempotent projection written on every Procedure skill write — mirroring Slice A (memory_graph authoritative, bucket_seal a recall face). The adapter is never an independent writer of skill *data*.
- **Full unification** (not minimal). Repoint all reads to Procedure, make the adapter projection-only, retire the one-time migration's role as the adapter's writer, and backfill existing Procedure skills into the projection.
- The adapter projection's sole remaining purpose is **semantic skill search** (`skills::search` via `recall_hybrid`). Exact lookups, counters, lifecycle, and listing all read Procedure.
- **No migration, no new config** (both stores' schemas already exist).

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

### §3 Repoint reads to Procedure
- **`resolve_slash_skill`** (`/skill-name` exact): query Procedure by normalized title/slug (`store.find_learned_skill_by_normalized_title(space, normalized)` already exists) instead of `skills::get_skill`. Return the active version content.
- **`get_learned_skill`**: read the Procedure node + active version instead of the adapter.
- **`bump_cited` / `bump_usage`**: bump the Procedure node's `metadata.cited_count` / `metadata.usage_count` (a store helper `bump_skill_counter(node_id, field)` — add if absent), then re-`project_skill` so the projection reflects the new counter. The adapter's own `bump_*` (read-modify-write on the namespace) are removed.
- **`top_skills`**: repoint to the existing Procedure ranker (`store.list_top_learned_skills` / `list_promoted_learned_skills`, already cited DESC/usage DESC/updated DESC).
- **`skills::search`** (semantic): UNCHANGED — keeps reading the `"skills"` projection via `recall_hybrid`. This is the projection's justification.

### §4 Retire the divergent writer model
- Remove `skill_migration::migrate_skills` (and its boot call) — superseded by §2's backfill + §1's continuous projection.
- In `memory_adapter/skills.rs`: keep `Skill`, `put_skill` (now called only by `project_skill`), `get_skill`/`top_skills`/`search` as the projection read/write surface; **remove** `bump_cited`/`bump_usage` (logic moves to Procedure §3). Keep `search` (semantic).

## Data flow (after F)

```
extraction → store_skill_as_procedure / upgrade_existing_skill (Procedure = source of truth)
           → project_skill(adapter, node, body)  // derived "skills" projection (FTS)
boot (once) → skill_projection_backfill: all Procedure skills → projection (marker-gated)
reads: /slug, get, counters, listing → Procedure ; semantic search → projection
counter bump → Procedure metadata + re-project
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
