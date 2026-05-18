# Memory OS Foundation — Phase 1 Summary

**Status:** in progress (this PR).
**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`
**Plan:** `docs/superpowers/plans/agent-memory-os.md` § Phase 1.

## What this PR adds

Phase 1 is the **schema-only foundation** of the three-layer Memory OS.
Everything is additive — no V1-V33 table is touched, no existing IPC
command is modified.

| # | Commit | What |
|---|---|---|
| 1 | `feat(memory): add MemoryNodeKind::EntityPage variant` | 10th enum variant + tests + 2 adjacent exhaustive-match fixes (reflection.rs, recall.rs). |
| 2 | `feat(db): V34 — Memory OS Foundation Phase 1 schema` | Three new tables: `memory_edge_audit`, `wiki_artifacts`, `memory_health_findings`. All `IF NOT EXISTS`. Tests cover create + idempotency + FK cascade + dismiss-filter. |
| 3 | `feat(memory): EntityPage metadata schema (compiled_truth + timeline doctrine)` | `entity_page.rs` module defining `EntityPageMetadata` / `TimelineEntry` / `Contradiction`. Pure JSON convention over `memory_nodes.metadata_json` — **no schema change for metadata itself**. |
| 4 | `feat(memory): store.rs CRUD for EntityPage nodes` | `create_entity_page` / `find_entity_page_by_slug` / `list_entity_pages` / `append_timeline_entry`. Atomic via `with_transaction`. Case-insensitive per-space slug uniqueness. |
| 5 | `feat(ipc): tauri commands for EntityPage CRUD + invoke_handler registration` | Five new `memory_entity_page_*` commands. Registered in `main.rs::invoke_handler!`. Frontend `tauri-bridge.ts` wrappers + `types.ts` shapes. |
| 6 | `feat(memory-os): Phase 1 feature flag + summary doc` | `MemubotConfig.memory_os.entity_page_enabled` (default `true`). All five commands return a structured error when disabled. This document. |

## What this PR does NOT add

- Auto-link post-hook (that's Phase 2 — writing `[[entity:slug]]` in a
  version still creates no edges until then).
- AI Wiki view / `WikiView.tsx` (that's Phase 3).
- Health/Lint scenarios that populate `memory_health_findings`
  (Phase 4-5).
- The 9 page types from Tommy's framework (that's Cognitive Phase 8).
- NER / Timeline Engine / Dream Cycle (that's Engines Phase 15+).

`memory_health_findings` and `wiki_artifacts` are created empty and stay
empty after Phase 1; subsequent phases populate them.

## How to verify locally

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib memory_graph::models 2>&1 | tail -10
cd src-tauri && cargo test --lib memory_graph::entity_page 2>&1 | tail -10
cd src-tauri && cargo test --lib memory_graph::store::tests::entity 2>&1 | tail -10
cd src-tauri && cargo test --lib db::migrations::tests::v34 2>&1 | tail -10

cd ui && npx tsc --noEmit 2>&1 | head -10

# Schema spot-check after running the app once:
sqlite3 ~/.uclaw/uclaw.db "SELECT name FROM sqlite_master \
  WHERE type='table' AND name IN ('memory_edge_audit','wiki_artifacts','memory_health_findings')"
# Expected: 3 rows.
```

## How to disable / roll back

The Migration is additive and idempotent; the only thing you might want
to disable is the new IPC surface. Edit `~/.uclaw/memubot_config.json`:

```json
{ "memory_os": { "entity_page_enabled": false } }
```

Then restart. All five `memory_entity_page_*` commands will return:

```
EntityPage feature is disabled (memory_os.entity_page_enabled = false ...).
Enable it and restart to use EntityPage commands.
```

Existing `EntityPage` rows on disk are untouched and will reappear when
the flag is re-enabled.

If you want to physically remove the rows: V34 tables are safe to
`DROP TABLE` (no Foundation code reads them); just be aware that re-
running migrations will re-create empty versions.

## Adjacent edits called out per CLAUDE.md

Files touched outside `memory_graph/` and the V34 migration block:

- `src-tauri/src/memory_graph/reflection.rs` — exhaustive match in
  `generate_route_path` gained an `EntityPage => entity/<slug>` arm.
- `src-tauri/src/memory_graph/recall.rs` — `capitalize_kind` gained the
  display name.
- `src-tauri/src/main.rs` — registered five new commands in
  `invoke_handler!`.
- `CLAUDE.md` — flipped V33 to `merged`, added V34 row as in-progress.
- `ui/src/lib/types.ts` + `tauri-bridge.ts` — new shapes + wrappers.

Both composers (`ChatInput.tsx`, `AgentView.tsx`) are untouched in
Phase 1 — Phase 1 has no UI surface beyond the IPC commands.
