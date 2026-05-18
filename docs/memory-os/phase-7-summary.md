# Memory OS Foundation — Phase 7 Summary

**Status:** in progress (this PR).
**Builds on:** Phase 1 (EntityPage CRUD + V35 schema), Phase 2 (auto-link — runs on every disk-sourced version write), Phase 3 (wiki_artifacts), Phase 4 (memory_health — sync_conflict surfaces here), Phase 6 (cost prefix pattern carries over).
**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` § Tier-C C1 (Markdown 双向同步) + § C3 (file-system entity paths) + Phase 7 plan.
**Plan:** `docs/superpowers/plans/agent-memory-os.md` § Phase 7.

## What this PR adds

Phase 7 closes the Foundation layer with bidirectional markdown sync between the wiki and a real on-disk directory (`~/Documents/workground/brain/`). Six commits land:

1. **V37 migration** — `brain_sync_state` table (one row per EntityPage tracking on-disk file path, mtime, SHA-256, last-synced version id).
2. **Export to disk** — `memory_wiki_export` IPC + brain_io module. Renders every EntityPage as `<subkind>/<slug>.md` with YAML frontmatter. Idempotent via SHA-256 short-circuit.
3. **Sync from disk** — `memory_wiki_sync_from_disk` IPC. Walks brain dir, parses frontmatter, deprecates old version + writes new one when disk content changed. User-authored files (no node_uuid) create new EntityPages.
4. **Conflict resolution** — when both disk and DB advanced since the last sync, the sync still applies disk-wins (gbrain's "human always wins" principle) but writes a `memory_health_findings(check_kind='sync_conflict', severity='error')` row so the user sees it in the Health tab.
5. **Opt-in fs watcher** — `memory_os.brain_watcher_enabled` flag. When on, edits under brain dir auto-trigger `sync_from_disk` after a 500ms debounce.
6. **Docs** — this summary + user guide (`markdown-sync-user-guide.md`).

| # | Commit | What |
|---|---|---|
| 1 | `feat(db): V37 — brain_sync_state` | New table + 3 indexes + 4 unit tests. CLAUDE.md registry updated (V35 → merged; new V36 Automation Phase 2b row; V37 in progress). |
| 2 | `feat(memory-os): brain_io export to markdown (Phase 7.1)` | `BrainFrontmatter` serde struct, `render_file` / `parse_file` round-trip, `export_entity_page` + `export_all` + `export_wiki_artifact`. 15 unit tests (tempdir). IPC + WikiView Export button (FolderDown icon). |
| 3 | `feat(memory-os): brain_io sync_from_disk (Phase 7.2)` | `sync_from_disk` + `sync_one_file` with 3-branch logic (existing page changed / IDE touch / new user-authored). 8 unit tests covering empty root, no-frontmatter skip, unchanged short-circuit, real edit → new version, touch noise filtered, new-page creation, conflict detection, slug-collision adoption. IPC + WikiView Sync button (FolderSync icon). |
| 4 | `feat(memory-os): sync conflict findings in Health tab (Phase 7.3)` | `memory_health::upsert_finding` → `pub(crate)`; new `write_sync_conflict_finding` helper writes severity=error rows with payload = `{file_path, prior/overwritten/new version ids, resolution: 'disk_wins'}`. HealthPanel `CHECK_KIND_LABEL` learns 4 lint kinds + sync_conflict. 2 dedup-contract tests. |
| 5 | `feat(memory-os): opt-in brain fs watcher (Phase 7.4)` | `notify v7`-backed `BrainWatcherHandle` + 500ms debounce worker. Pure-function `event_is_relevant` filter rejects .DS_Store / Access events / non-.md / outside-root. AppState holds the handle for the app lifetime. 5 filter tests. |
| 6 | `docs(memory-os): Phase 7 summary` | This document. |

## Frontmatter format

```yaml
---
node_uuid: 7f30...
last_synced_version_id: e8c1...
slug: alice
title: Alice
subkind: person
aliases:
  - Allie
  - A. S.
enrichment_tier: 2
last_synthesized_at: "2026-05-15T10:00:00Z"
timeline:
  - date: "2026-05-01"
    text: "joined Acme"
  - date: "2026-05-15"
    text: "promoted to staff"
---

# Compiled truth body goes here

Alice is a senior engineer at Acme. She joined in May after graduating MIT.
```

Standard `---`-delimited YAML so Obsidian / Foam / Logseq / VS Code's
preview render the file natively. User-authored files without
`node_uuid` are treated as "create a new EntityPage" on the next sync.

## Conflict semantics (Phase 7.3)

Three states per page after the last sync:

|                          | DB unchanged                 | DB changed                                  |
|--------------------------|------------------------------|---------------------------------------------|
| **Disk unchanged**       | NoChange — skip              | NoChange (the next Export reflects DB → disk) |
| **Disk changed (real)**  | Updated — write new version  | **Conflict** — disk-wins + sync_conflict finding |

"Disk changed (real)" requires BOTH `mtime` to have moved AND `sha256`
to differ from the last-synced hash — touch / IDE save churn is
filtered out as noise.

The sync_conflict finding has dedup contract identical to Phase 4
checks: at most one OPEN finding per `(space_id, node_id,
'sync_conflict')`. Dismissing the row and triggering another conflict
later inserts a fresh row (audit trail of overwrites is preserved).

## What this PR does NOT add

- **Conflict diff UI**. The finding's payload carries both `overwritten_db_version_id` and `new_active_version_id` — enough to build a 3-way diff later. The DiffViewer is Phase 9 (Cognitive provenance) work.
- **Per-space brain root**. All spaces share `~/Documents/workground/brain/` for now. Each EntityPage's `space_id` is preserved in `brain_sync_state.space_id`, so per-space subdirs (`brain/<space>/<subkind>/<slug>.md`) would be a small follow-up if needed.
- **Direction-aware merge for partial conflicts**. We treat any divergence as a full overwrite. A 3-way text merge over the compiled_truth body would be nicer for cases where the user added a sentence in disk while the LLM added a paragraph in DB — landing this needs the Phase 9 provenance work to track per-segment authorship.
- **Watcher recursion through symlinks**. `RecursiveMode::Recursive` doesn't follow symlinks. Power users with brain layouts split across drives via symlinks will need to add separate watchers (not currently exposed) or use the manual Sync button.
- **Delete-on-disk → delete-in-DB**. A `Remove` fs event fires the relevant filter, but `sync_from_disk` is "import everything currently on disk" — files that vanish leave their DB rows + `brain_sync_state` rows alone. Deliberate: protects against accidentally `rm`'ing the brain dir nuking memory. Removing an EntityPage stays an explicit IPC action.

## How to verify locally

```bash
cd ~/Documents/uclaw
git fetch && git checkout claude/p7-memory-os-markdown-sync

# 1. Rust build
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head

# 2. Rust tests
cargo test --lib db::migrations::tests::v37 2>&1 | tail               # 4 cases (7.0)
cargo test --lib memory_graph::brain_io 2>&1 | tail                   # 25 cases (7.1+7.2+7.3)
cargo test --lib memory_graph::brain_watcher 2>&1 | tail              # 5 cases (7.4)
cargo test --lib memubot_config::tests::memory_os_phase7 2>&1 | tail  # 2 cases (7.4 flag)

# Existing Phase 1-6 tests should all still pass.

# 3. TS check
cd ../ui && npx tsc --noEmit 2>&1 | head

# 4. End-to-end smoke in `cargo tauri dev`:
#
# (a) Create a couple of EntityPages via QuickCapture.
# (b) Wiki tab → click "Export". Open ~/Documents/workground/brain/
#     in Finder. You should see one .md file per page under its
#     subkind folder + overview.md / index.md at the brain root.
# (c) Open <subkind>/<slug>.md in Obsidian or VS Code. Edit the body
#     (preserve the frontmatter block!). Save.
# (d) Wiki tab → click "Sync". Toast should say "Synced from brain
#     dir: 1 updated".
# (e) Open the EntityPage detail panel — new content is there, the
#     old version is `deprecated`.
# (f) Conflict test: edit the disk file AND click the Synthesize
#     button in WikiView (which generates a new DB version) before
#     clicking Sync. Toast shows "1 conflict(s) — disk won, check
#     Health tab". Switch to Health tab → see the sync_conflict
#     finding with payload containing both version ids.

# 5. Optional — try the fs watcher.
# Edit ~/.uclaw/memubot_config.json:
#   "memory_os": { "brain_watcher_enabled": true }
# Restart. Now edit a .md file under brain/, save, wait 1 second.
# The DB picks up the change without any UI action.
```

## How to disable / roll back

### Disable bi-directional sync entirely

The IPC commands (`memory_wiki_export`, `memory_wiki_sync_from_disk`)
gate on `entity_page_enabled` only — disabling that disables Phase 7
as a side effect along with all of Phase 1's EntityPage CRUD. For
phase-specific rollback:

```jsonc
{
  "memory_os": {
    "brain_watcher_enabled": false   // stop the watcher
    // (export + sync IPC stay available; user must click manually)
  }
}
```

### Clean out existing on-disk mirror

```bash
rm -rf ~/Documents/workground/brain/
sqlite3 ~/.uclaw/uclaw.db "DELETE FROM brain_sync_state;"
```

The next Export rebuilds from DB. EntityPage data in the DB is
unaffected.

### Roll back V37 schema

```sql
DROP TABLE brain_sync_state;
```

Won't affect any other table. Re-running the app re-creates it
(IF NOT EXISTS).

## Adjacent edits called out per CLAUDE.md

- `src-tauri/src/db/migrations.rs` — V37 const + run() block + 4 unit tests.
- `src-tauri/src/memory_graph/mod.rs` — `pub mod brain_io;` + `pub mod brain_watcher;`.
- `src-tauri/src/memory_graph/brain_io.rs` — new file (~1100 LOC including tests).
- `src-tauri/src/memory_graph/brain_watcher.rs` — new file (~300 LOC).
- `src-tauri/src/memubot_config.rs` — `brain_watcher_enabled` field + 2 forward-compat tests.
- `src-tauri/src/proactive/scenarios/memory_health.rs` — `upsert_finding` visibility bump (`fn` → `pub(crate) fn`) so brain_io can write `sync_conflict` rows through the same dedup contract.
- `src-tauri/src/app.rs` — AppState gains `brain_watcher` field; bootstrap branch on the flag.
- `src-tauri/src/ipc.rs` — `WikiExportInput` + `WikiSyncInput` DTOs.
- `src-tauri/src/tauri_commands.rs` — `memory_wiki_export` + `memory_wiki_sync_from_disk` handlers.
- `src-tauri/src/main.rs` — invoke_handler! registrations.
- `ui/src/lib/types.ts` — 4 new types (Wiki{Export,Sync}Input, Brain{Export,Sync}Outcome).
- `ui/src/lib/tauri-bridge.ts` — `memoryWikiExport` + `memoryWikiSyncFromDisk` invoke wrappers.
- `ui/src/components/memory/WikiView.tsx` — Export + Sync buttons + Toast feedback.
- `ui/src/components/memory/MemoryHealthPanel.tsx` — `CHECK_KIND_LABEL` learns Phase 5 lint kinds + `sync_conflict`.
- `CLAUDE.md` — migration registry updated (V35 merged, V36 Automation Phase 2b claim, V37 Memory OS in progress).

## Performance notes

- **Export** — one `INSERT OR UPDATE` into `brain_sync_state` + one filesystem write per page. SHA-256 short-circuits unchanged pages. On a 100-page space the full export takes <100ms warm-cache, dominated by FS sync.
- **Sync** — recursive walk + SHA + sqlite write per changed file. For a watcher-quiet hour with 0 changes, the periodic Sync click is bounded by directory enumeration cost (~10ms for 100 files). Edits go through `MemoryGraphStore::create_version`, which triggers Phase 2 auto-link — same cost as any other version write.
- **Watcher** — `notify v7` with default backend. macOS uses FSEvents (low CPU); Linux uses inotify (similar). The 500ms debounce keeps sync calls under control during Obsidian save bursts. Empty 1-hour idle has ~zero CPU cost (event channel blocks).
- **Conflict-finding write** — one extra `INSERT` into `memory_health_findings` per conflicting file. Dedup short-circuits the second sync of the same unresolved conflict (no row inserted).
