# Step 2a — WikiView Re-back onto memory_graph EntityPages Design

**Date:** 2026-06-02
**Status:** Design (decomposition + 2a-A=migrate, 2a-B=full-parity approved in brainstorming; pending spec review → plan)
**Part of:** Step 2 (retire gbrain — the last external runtime, Bun+PGLite). Step 2 decomposes into **2a (WikiView re-back, this slice)** → 2b (write reroute) → 2c (agent tools) → 2d (teardown). Follows the completed Step 3 (memU fully removed, zero Python). The two-layer terminal ADR (`2026-06-01-memory-two-layer-terminal-state.md`) retains memory_graph as the rich layer + retires gbrain as the duplicate.

## Problem

`WikiView` (`ui/src/components/memory/WikiView.tsx`) — the frontend wiki/pages UI — reads/writes gbrain via 8 MCP-backed calls (`gbrainListPages/GetPage/Search/GetBacklinks/GetStats/FindOrphans/GetVersions/RevertVersion/PutPage`, wired in `ui/src/lib/gbrain-browse.ts`). This is the hardest part of retiring gbrain (real UI work). Recon confirms **memory_graph EntityPages fully cover the wiki model** — slug + markdown body (version.content) + backlinks (edges) + versions + subkind, *plus* timeline/contradictions/enrichment-tiers — but the Tauri command surface has gaps (no update/revert/stats/orphans/versions-list/backlinks-wrapper/EntityPage-filtered-search). Existing user wiki content lives in gbrain's PGLite (the earlier `migrate_gbrain_pages` moved it only to the bucket_seal `pages` facade, NOT to memory_graph EntityPages).

## Decisions (approved)

- **2a-A = migrate.** A one-time, marker-gated migration reads all gbrain pages (while gbrain is still alive — this runs before 2d teardown) and creates memory_graph EntityPages, so the re-backed WikiView shows the user's existing wiki content. No content loss.
- **2a-B = full parity.** Add every missing command so WikiView keeps all features (edit, version history, revert, stats, orphans) — no degraded UI.

## Design

### §1 Backend Tauri commands (memory_graph EntityPage) — extend `tauri_commands.rs` + `memory_graph/store.rs`

Existing (reuse): `memory_entity_page_create`, `memory_entity_page_find_by_slug`, `memory_entity_page_list`, `memory_entity_page_append_timeline`. New:
- **`memory_entity_page_put(space_id, slug, raw_markdown)`** — slug-keyed upsert (maps gbrain `put_page`): `find_entity_page_by_slug` → if exists, create a new superseding `MemoryVersion` with `raw_markdown` as content; else `create_entity_page`. Parse the YAML frontmatter for `title`/`type`(→subkind)/`tags`(→aliases) into `EntityPageMetadata`; store the FULL `raw_markdown` (frontmatter + body) in `version.content` (round-trips as gbrain's `raw_markdown`). Re-run the Phase-2 auto-link hook on `[[entity:slug]]`.
- **`memory_entity_page_versions(node_id)`** — list `MemoryVersion` rows for the node → `[{id, snapshot_at, content_preview}]` (maps gbrain `get_versions`).
- **`memory_entity_page_revert(node_id, version_id)`** — create a new active version cloning the target version's content (non-destructive revert; maps gbrain `revert_version`).
- **`memory_entity_page_backlinks(node_id)`** — `get_edges_to(node_id)` → `[{from_slug, link_type}]` (resolve source node → its slug; `relation_kind` → `link_type`).
- **`memory_entity_page_stats(space_id)`** — `{page_count, chunk_count?, embedded_count?}`. `page_count` = EntityPage node count (accurate). `chunk_count`/`embedded_count` are bucket_seal concepts that don't map 1:1 to EntityPages → return as `Option` (FE status bar renders "—" when absent). Don't fabricate.
- **`memory_entity_page_orphans(space_id)`** — EntityPages with zero inbound edges (`get_edges_to` empty) → orphan summary.
- **`memory_entity_page_search(space_id, query, limit)`** — FTS over EntityPage version content via `memory_graph` node search **filtered to `kind=EntityPage`** (WikiView searches its own pages = EntityPages; the bucket_seal `pages` search stays the agent's recall path, not this). Returns `[{slug, title, snippet}]`. (Confirm `memory_graph` exposes a node-FTS search to add the kind filter to; if not, add one.)
- **`memory_entity_page_backlinks`** detail: `get_edges_to(node_id)` → **keep only source nodes that are themselves `EntityPage` (have a slug)** (gbrain backlinks are page→page; drop edges from Episodes/etc.) → `[{from_slug, link_type}]`.

All new commands registered in the `invoke_handler!` macro (`main.rs`). Unit-test the store-level logic (put upsert creates a version; revert clones; backlinks remap; orphans).

### §2 One-time migration `migrate_pages_to_entity_graph` (`memory_adapter/`)

**Source = the bucket_seal `pages` facade, NOT gbrain.** (Review correction: with `gbrain_read_repoint_enabled=true` (default) the gbrain read tools already serve from bucket_seal `pages`, and the earlier `migrate_gbrain_pages` already copied gbrain's PGLite content INTO bucket_seal `pages` (+ dual_write keeps it synced). So bucket_seal `pages` is the in-process, authoritative-enough copy — reading it avoids any gbrain-reachability dependency + retry-backoff.) Marker-gated (`__pages_to_entitypage_v1__`), spawned at boot, **fully in-process**: `pages::list_all(adapter)` → for each `Page {slug, title, body(full markdown), page_type, tags}`: `memory_entity_page_put(space, slug, body)` (idempotent — skip if the EntityPage slug already exists). Logs `migrated=N`. No gbrain dependency; no retry needed. Does NOT delete the bucket_seal pages (read-only source).

**Backlink-format check (parity risk):** gbrain page bodies may use bare `[[slug]]` while the Phase-2 auto-link hook parses `[[entity:slug]]`. The plan MUST verify the auto-link parser accepts the actual `[[...]]` format in migrated bodies (and normalize on write if not) — otherwise migrated pages' backlinks won't populate.

### §3 WikiView FE rewrite (`ui/src/lib/gbrain-browse.ts` + `components/memory/WikiView.tsx`)

Replace the 8 `gbrain*` Tauri calls with the `memory_entity_page_*` commands. Keep the UI/UX **identical** (sidebar list, detail+edit pane, backlinks panel, version drawer, stats bar, orphan badge). Map DTOs: gbrain `PageSummary`/`PageDetail`/`Backlink`/`VersionMeta` ← memory_graph node-detail/version/edge shapes (the command layer returns the same field names where practical to minimize FE churn). `gbrain-browse.ts` becomes `entity-page-browse.ts` (or keep the filename, swap the impl). WikiView uses `node_id` internally (memory_graph is UUID-keyed) while still slug-addressing for navigation + `[[links]]`.

### Data flow

```
WikiView (FE) → memory_entity_page_* Tauri commands → memory_graph EntityPages (nodes + versions + edges)
existing pages → §2 one-time migration (in-process, reads bucket_seal `pages`) → memory_graph EntityPages
backlinks: [[entity:slug]] in content → Phase-2 auto-link → edges → get_edges_to → WikiView backlinks
(WikiView no longer calls gbrain; the agent + write-path still do until 2b/2c)
```

## Transient-state note (acknowledged, closed by 2b)

After 2a, WikiView reads memory_graph EntityPages, but the WRITE path (chat_extractor → `mcp__gbrain__put_page`) still writes to gbrain until **2b** reroutes it. So in the 2a→2b window, agent-created pages land in gbrain (+ bucket_seal shadow) and won't appear in WikiView until 2b points writes at EntityPages. The §2 migration covers all EXISTING content; reflection/entity-synthesis already create EntityPages. This transient gap is acceptable for the chosen 2a-first ordering and closes in 2b.

## Error handling

New commands return `Result<_, Error>` (existing pattern). Migration is best-effort + idempotent (marker + slug-exists skip); a gbrain-unreachable migration logs + retries (backoff), never blocks boot. WikiView shows empty/error states as today.

## Testing

1. **Store unit tests**: `put` upsert (new slug → create; existing → new version, old superseded); `revert` (clones target version to a new active); `backlinks` remap; `orphans` (no inbound edges); `versions` list. In-memory `MemoryGraphStore`.
2. **Migration test**: a fake gbrain page source → migration creates EntityPages; idempotent on re-run (marker + slug skip).
3. **FE**: WikiView vitest — list/detail/edit/backlinks/versions render against mocked `memory_entity_page_*` invoke responses (mirror the existing gbrain-mock tests).
4. `cargo build` + clippy clean; `cd ui && npx tsc --noEmit` (no NEW errors vs baseline) + `npm test -- --run` (WikiView + touched tests green).

## Scope / files

| File | Change |
|---|---|
| `memory_graph/store.rs` + `memory_graph/entity_page.rs` | new store methods: put-upsert, versions, revert, backlinks, stats, orphans, search-filter |
| `tauri_commands.rs` + `main.rs` | 7 new `memory_entity_page_*` commands + macro entries |
| `memory_adapter/pages_to_entitypage_migration.rs` (new) | one-time marker-gated migration reading bucket_seal `pages` → EntityPages (in-process, no gbrain dep) |
| `app.rs` | spawn the migration at boot (alongside the existing page migration) |
| `ui/src/lib/gbrain-browse.ts` → entity-page browse | swap 8 gbrain calls → `memory_entity_page_*` invokes + DTO remap |
| `ui/src/components/memory/WikiView.tsx` | use the new browse lib; keep UI identical; node_id-keyed internally |

**Out of scope (later slices):** the write-path reroute (chat_extractor/dual_write → EntityPage) = 2b; the agent `mcp__gbrain__*` tools + `gbrain_prompt` = 2c; GbrainCliTransport/boot/Bun/PGLite/bundle/setup-scripts deletion = 2d. gbrain stays fully alive through 2a (the migration needs it).

## Risk

Med-High (the real UI work). Risks: (1) DTO/feature parity — mitigated by full-parity command set + keeping WikiView's UI identical + vitest; (2) **backlink-format parity** — the migration's biggest correctness risk: if migrated `[[...]]` bodies don't match the auto-link parser's expected `[[entity:slug]]`, backlinks silently don't populate (plan must verify + normalize); (3) the transient write-path gap (extractor pages gbrain-only until 2b) — acceptable, closes in 2b. The migration now reads bucket_seal `pages` in-process (no gbrain-reachability dependency — review correction). Bisectable: backend commands → migration → FE rewrite → verify. Each commit compiles + tests. Slice is large (backend + migration + FE WikiView rewrite) but coherent as one bisectable PR.
