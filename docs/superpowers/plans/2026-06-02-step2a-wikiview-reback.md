# Step 2a — WikiView Re-back onto memory_graph EntityPages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Re-back the WikiView frontend from gbrain MCP calls onto memory_graph EntityPages: add the missing backend commands, migrate existing pages (in-process from bucket_seal), and rewrite WikiView's data layer. gbrain stays alive (only WikiView stops using it).

**Architecture:** memory_graph EntityPages (nodes + versions + edges) are the wiki store. Backend `memory_entity_page_*` Tauri commands give WikiView full parity. A one-time in-process migration reads the bucket_seal `pages` facade → EntityPages. `cargo`/`tsc`/`vitest` are the guards.

**Verification:** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` + `cargo clippy --lib`; `cd ui && npx tsc --noEmit 2>&1 | head -20` (baseline has 41 pre-existing errors — compare delta) + `npm test -- --run 2>&1 | tail -15`.

**Key facts (recon, file:line):**
- EntityPage store (`src-tauri/src/memory_graph/store.rs`): `create_entity_page` (:1152), `find_entity_page_by_slug` (:1313), `list_entity_pages` (:1354), `create_version` (:546), `get_edges_to` (:708). Node FTS: `memory_graph/search.rs:25 fts_search`.
- **auto-link format** (`memory_graph/auto_link.rs:78`): regex `\[\[\s*entity:\s*([a-z0-9][a-z0-9-]{0,127})\s*\]\]` + `[[node:UUID]]`. **Bare `[[slug]]` is NOT matched** → migration/put must normalize `[[slug]]`→`[[entity:slug]]`.
- bucket_seal pages (`memory_adapter/pages.rs`): `list_all(adapter) -> Vec<Page>` (:49, filters `_migration_marker`); `Page { slug, title, page_type, body, tags }` (:14). `get_page` (:41).
- Existing EntityPage commands in `tauri_commands.rs` (~:7026-7170): `memory_entity_page_create/get/find_by_slug/list/append_timeline/synthesize_now`.
- FE: `ui/src/lib/gbrain-browse.ts` (8 `gbrain*` fns + DTOs PageSummary/PageDetail/Backlink/VersionMeta); `ui/src/components/memory/WikiView.tsx` (~434 lines, calls them).
- `EntityPageMetadata` (`memory_graph/entity_page.rs:37`): `slug, aliases, subkind, timeline, contradictions, enrichment_tier, ...`.

---

## Task 1: Backend EntityPage commands (full parity)

**Files:** `src-tauri/src/memory_graph/store.rs` (new methods), `src-tauri/src/tauri_commands.rs` (new commands), `src-tauri/src/main.rs` (macro entries).

- [ ] **Step 1: Read** `store.rs` `create_entity_page` (:1152), `find_entity_page_by_slug` (:1313), `create_version` (:546), `get_edges_to` (:708); `entity_page.rs` `EntityPageMetadata`; `auto_link.rs` (the `[[entity:slug]]` regex + how create_entity_page invokes the auto-link hook). Confirm how `create_entity_page` parses/stores metadata + runs auto-link, so the new `put` mirrors it.

- [ ] **Step 2: Add a `[[slug]]`→`[[entity:slug]]` normalizer** (e.g. in `auto_link.rs` or a small helper): regex-replace bare `[[<slug>]]` (where `<slug>` is `[a-z0-9][a-z0-9-]*` and NOT already `entity:`/`node:`-prefixed) → `[[entity:<slug>]]`. Unit-test: `[[foo-bar]]`→`[[entity:foo-bar]]`; `[[entity:x]]` unchanged; `[[node:uuid]]` unchanged; code-fenced `[[x]]` left alone (mirror auto_link's fence-skipping if it has it). This guards backlink parity for migrated + edited content.

- [ ] **Step 3: Store methods** on `MemoryGraphStore`:
  - `entity_page_put(space_id, slug, raw_markdown) -> Result<MemoryNodeDetail>`: normalize the markdown (Step 2); `find_entity_page_by_slug(space, slug)` → if `Some(node)`: `create_version` superseding the active (content = normalized markdown), re-run auto-link; else `create_entity_page(space, slug, title_from_frontmatter, normalized_markdown, metadata_from_frontmatter)`. Parse frontmatter (reuse the project's existing frontmatter parser — grep `frontmatter`/`split_frontmatter`) for title/type→subkind/tags→aliases.
  - `entity_page_versions(node_id) -> Vec<{version_id, created_at, content}>`: query `memory_versions` by node_id (all statuses), newest first.
  - `entity_page_revert(node_id, version_id) -> MemoryNodeDetail`: fetch the target version's content, `create_version` (new active) cloning it. Non-destructive.
  - `entity_page_backlinks(node_id) -> Vec<{from_slug, link_type}>`: `get_edges_to(node_id)` → for each edge, load the source node; **keep only sources whose kind == EntityPage with a slug**; map `relation_kind`→`link_type`.
  - `entity_page_stats(space_id) -> {page_count, chunk_count: Option, embedded_count: Option}`: `page_count` = count EntityPage nodes; chunk/embedded → `None` (not an EntityPage concept).
  - `entity_page_orphans(space_id) -> Vec<{slug, title}>`: EntityPages where `get_edges_to` is empty.
  - `entity_page_search(space_id, query, limit) -> Vec<{slug, title, snippet}>`: `fts_search(space, query, limit*N)` then filter to `kind==EntityPage`, map to slug+title+snippet.

- [ ] **Step 4: Tauri commands** in `tauri_commands.rs` wrapping each (`memory_entity_page_put/versions/revert/backlinks/stats/orphans/search`), `state.memory_graph_store`-backed, returning serde DTOs that mirror the gbrain wire shape (so FE remap is minimal). Register all in the `invoke_handler!` macro in `main.rs`.

- [ ] **Step 5: Tests** (`#[cfg(test)]` in store.rs, in-memory store): put creates-then-versions (slug upsert → 2 versions, latest active); revert clones; backlinks remap + filters non-EntityPage sources; orphans; the `[[slug]]` normalizer.

- [ ] **Step 6: Build + clippy + test** — `cargo build 2>&1 | grep -E "^error"` (none); `cargo clippy --lib` (none); `cargo test --lib memory_graph::` (green).

- [ ] **Step 7: Commit** — `feat(memory): EntityPage Tauri commands for WikiView parity (put/versions/revert/backlinks/stats/orphans/search) + [[slug]] normalizer (Step 2a)`

---

## Task 2: One-time migration — bucket_seal pages → EntityPages

**Files:** Create `src-tauri/src/memory_adapter/pages_to_entitypage_migration.rs`; modify `src-tauri/src/app.rs` (spawn at boot) + the migration module's `mod` registration.

- [ ] **Step 1: Read** an existing marker-gated migration (`memory_adapter/gbrain_page_migration.rs` or `skill_migration.rs`) for the marker + idempotency pattern. Read `pages::list_all`.

- [ ] **Step 2: Write the migration**:
```rust
// marker: a sentinel EntityPage slug or a settings flag, e.g. "__pages_to_entitypage_v1__"
pub async fn migrate_pages_to_entity_graph(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
) -> anyhow::Result<usize> {
    if /* marker present */ { return Ok(0); }
    let pages = crate::memory_adapter::pages::list_all(adapter).await.unwrap_or_default();
    let mut migrated = 0;
    for p in pages {
        if store.find_entity_page_by_slug(space_id, &p.slug)?.is_some() { continue; } // idempotent
        // reconstruct raw_markdown (p.body is already full markdown); entity_page_put normalizes + parses frontmatter
        if store.entity_page_put(space_id, &p.slug, &p.body).is_ok() { migrated += 1; }
    }
    /* set marker */
    tracing::info!(migrated, "pages→EntityPage migration complete");
    Ok(migrated)
}
```
   Fully in-process (no gbrain). Idempotent (slug-exists skip + marker). Best-effort (errors logged, never block boot).

- [ ] **Step 3: Spawn at boot** in `app.rs` alongside the existing page migration spawn (find where `migrate_gbrain_pages` is spawned; add this one). Use the same spawn/error posture.

- [ ] **Step 4: Test** — fake adapter returning known `Page`s → migration creates EntityPages (find_by_slug non-None after); re-run → migrated=0 (idempotent).

- [ ] **Step 5: Build + test** — clean; `cargo test --lib pages_to_entitypage` green.

- [ ] **Step 6: Commit** — `feat(memory): one-time bucket_seal pages → EntityPage migration (in-process, idempotent) (Step 2a)`

---

## Task 3: WikiView FE rewrite

**Files:** `ui/src/lib/gbrain-browse.ts` (rewrite impl), `ui/src/components/memory/WikiView.tsx`.

- [ ] **Step 1: Read** `gbrain-browse.ts` (the 8 `gbrain*` fns + their DTOs + invoke names) and `WikiView.tsx` (how it calls them + the slug↔detail flow + version drawer + backlinks panel + stats bar).

- [ ] **Step 2: Rewrite `gbrain-browse.ts`** — swap each gbrain invoke for the new command (keep the exported fn names + DTO shapes so WikiView barely changes):
  - `gbrainListPages` → `invoke('memory_entity_page_list', {space_id, subkind, limit})`.
  - `gbrainGetPage(slug)` → `invoke('memory_entity_page_find_by_slug', {space_id, slug})` → map to PageDetail (`compiled_truth`/`raw_markdown` ← version content; node_id retained for follow-up calls).
  - `gbrainSearch` → `memory_entity_page_search`.
  - `gbrainGetBacklinks(slug)` → resolve slug→node_id (from the loaded detail) → `memory_entity_page_backlinks(node_id)`.
  - `gbrainGetStats` → `memory_entity_page_stats` (render `chunk/embedded` as "—" when null).
  - `gbrainFindOrphans` → `memory_entity_page_orphans`.
  - `gbrainGetVersions(slug→node_id)` → `memory_entity_page_versions(node_id)`.
  - `gbrainRevertVersion(node_id, version_id)` → `memory_entity_page_revert`.
  - `gbrainPutPage(slug, content)` → `memory_entity_page_put(space_id, slug, content)`.
  - Add a `space_id` source (the active space — grep how other FE memory calls get it; likely a constant `"default"` or an atom).
- [ ] **Step 3: Adjust `WikiView.tsx`** for node_id-keyed follow-ups (backlinks/versions/revert need node_id from the loaded page detail, not slug). Keep the UI/UX identical. Rename the lib file to `entity-page-browse.ts` (or keep the filename + new impl — minimize import churn; keep filename to avoid touching unrelated imports).

- [ ] **Step 4: Tests** — update/port the WikiView vitest (mock the new `memory_entity_page_*` invoke responses; assert list/detail/edit/backlinks/versions render). Mirror the existing gbrain-mock test structure.

- [ ] **Step 5: Verify** — `cd ui && npx tsc --noEmit 2>&1 | head -20` (no NEW errors vs the 41 baseline) + `npm test -- --run 2>&1 | tail -15` (WikiView + touched green; pre-existing Kaleidoscope/MemoryModule fails ignored).

- [ ] **Step 6: Commit** — `refactor(ui): WikiView reads/writes memory_graph EntityPages instead of gbrain (Step 2a)`

---

## Task 4: Whole-slice verification + ship

- [ ] **Step 1:** `cargo build` + `cargo clippy --lib` clean; `cd ui && npx tsc --noEmit` (delta vs 41-baseline = 0 new) + `npm test -- --run` (no new fails).
- [ ] **Step 2: Gates:** WikiView (`grep -rn "gbrain" ui/src/components/memory/WikiView.tsx ui/src/lib/*browse*`) → no `gbrain*` invoke calls (uses `memory_entity_page_*`). gbrain MCP still alive (2a doesn't touch it) — the agent + write-path still use it (until 2b/2c).
- [ ] **Step 3: Ship** — push → PR (Commits table T1-T3) → rebase-merge → sync → cleanup → reindex.
- [ ] **Step 4: Post-merge soak (manual):** open Memory → Wiki tab: existing pages appear (migration ran); open a page → backlinks + version history show; edit + save → new version; revert works; search works; stats bar shows page_count (chunk/embedded as "—"). Confirm `[[slug]]` links in a migrated page produced backlinks (the normalizer + auto-link worked).

---

## Self-Review

- **Spec coverage:** 7 commands + normalizer (T1), in-process migration from bucket_seal (T2), WikiView rewrite (T3), verify (T4). ✓
- **Backlink-parity risk** (spec's biggest): addressed by the T1 Step-2 `[[slug]]`→`[[entity:slug]]` normalizer + T4 Step-4 soak check. ✓
- **Migration source** = bucket_seal `pages` (in-process, no gbrain dep) per the review correction. ✓
- **No placeholders:** real signatures + file:line + the migration code; FE mapping is 1:1 per gbrain call. ✓
- **Transient gap** (extractor pages gbrain-only until 2b): per-spec, not closed here. gbrain stays alive. ✓
- **Size:** large but bisectable (backend → migration → FE); each commit compiles + tests.
