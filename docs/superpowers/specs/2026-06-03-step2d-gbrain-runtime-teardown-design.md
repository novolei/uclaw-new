# Step 2d — gbrain Runtime Teardown (Bun + PGLite) Design

**Date:** 2026-06-03
**Status:** Design (recon done; pending spec review → plan)
**Part of:** Step 2 (retire gbrain). The CLOSEOUT. Follows 2a (WikiView re-back #644), 2b (page-write reroute #645), 2c (agent tools + graph viz native #646). 2a/2b/2c moved every functional consumer (WikiView, page writes, agent tools, graph viz) off gbrain; 2d removes everything that still keeps the gbrain MCP server alive — the transport, the boot/seed, the Bun + PGLite runtime, the bundle/dev placeholders. **Goal: TRUE zero external runtimes — the binary ships no Bun, no PGLite, no Python (Python already gone in Step 3); a fresh worktree builds with no gbrain placeholders.**

## Problem

After 2c the gbrain MCP server still **boots and connects** (with an empty tool allowlist), kept alive by these still-live consumers + plumbing (recon, file:line):

1. **`router::load_context`** (`memory_adapter/router.rs:266`) hard-codes `sources.push("gbrain")` → calls `GbrainAdapter::recall` (`memory_adapter/gbrain.rs`) on **every chat message** (unified recall). This is the one live runtime path into gbrain.
2. **`GbrainAdapter`** is registered in `memory_adapters_map["gbrain"]` (`app.rs:1099`).
3. **`gbrain_page_migration`** (`memory_adapter/gbrain_page_migration.rs`) — a one-time bucket_seal-backfill, spawned every boot (`app.rs:1184`), marker-gated; reads gbrain via `browse::list_pages`/`get_page`.
4. **Pure utils inside `gbrain::browse`** — `build_raw_markdown` (`browse.rs:164`, called by `memorization/service.rs:499`) + `split_frontmatter` (`browse.rs:183`, called by `page_dual_write.rs:16`). No network; must survive.
5. **`chat_extractor`** (`gbrain/chat_extractor.rs`) — the proactive chat→page extractor fired from `turn_runner.rs:147`; writes via `write_page` (NOT gbrain MCP) since 2b. gbrain-namespace by convention only; must survive.
6. **Boot/transport/Bun/PGLite**: `seed_bundled_gbrain` + `ensure_bundled_gbrain_initialized` + `GbrainCliTransport` + the `is_bundled_gbrain` connect branch (`mcp/mod.rs`); `find_bun_path` + `find_gbrain_entry` + `write_gbrain_launcher_files` + AppState `gbrain_mcp_id`/`gbrain_init_status` fields + the Stage-3 boot block (`app.rs` + `main.rs`).
7. **Eval/diagnostics/Tauri cmds**: `run_memory_gbrain_eval` + `memory_inventory` gbrain probe + `eval/adapters/memory.rs`; `restart_gbrain_mcp`; the `GbrainStatus`/`GbrainInitStatus` block in `get_system_diagnostics`.
8. **Bundle/config/scripts/FE/docs**: `tauri.conf.json` `bundle.resources` entries `"bunembed/bun": "bun"` + `"gbrain-source": "gbrain"`; `.gitignore` lines for `src-tauri/bunembed` + `src-tauri/gbrain-source`; `scripts/setup-bun-runtime.sh` + `setup-gbrain-source.sh` + `init-gbrain.sh`; FE SystemTab gbrain status + DeveloperOptions setup-script cards + `gbrain-result.tsx` + `gbrain-browse.ts` + mocks; CONTEXT.md + the `uclaw-memory-graph-freeze` skill.

The **placeholder requirement** (`mkdir bunembed gbrain-source && touch ...` before `cargo build` in a fresh worktree) exists because `tauri_build::build()` validates every `bundle.resources` key with `path.exists()` at compile time and panics if missing. Removing the two gbrain resource entries eliminates the requirement.

## Decision (approved 2026-06-03)

- **One spec, phased plan** — the recon-derived 8-phase safe teardown order, mapped to bisectable tasks.
- **Delete the gbrain eval harness** (`run_memory_gbrain_eval` etc.) — it only probes the gbrain MCP server; meaningless once gbrain is gone. Not repointed.
- **Only forced relocations, no cosmetic renames.** Deleting `src/gbrain/` forces relocating `chat_extractor` + the two pure utils; do that (keeping names). Do NOT rename the misnamed-but-functional `GbrainPolicyTarget` (writes via `write_page` since 2b), the `gbrain_extractor_*` flag/pipeline (proactive extractor still live), the `gbrain_drafts` draft-inbox dir, or `IngestError::Gbrain` — they work; renaming is cosmetic → deferred follow-up (see §Deferred).
- **Automation `gbrain_room_*` schema stubs**: `ScopedGbrainSchemaTool` returns `Err("not connected")` (schema-only, never functional) — remove it (+ `spec_declares_gbrain` + the `gbrain_room_*` permission branch) since gbrain is retired; if removal entangles more than the recon scope, leave + note (don't expand).

## Design — phased teardown

Each phase keeps the build green; the compiler + grep gates are the guard. Order is dependency-safe: cut/relocate consumers BEFORE removing transport+boot.

### Phase 1 — relocate forced survivors (compile-only, no behavior change)
- Move `build_raw_markdown` + `split_frontmatter` (the pure markdown-frontmatter utils) from `gbrain/browse.rs` → a new focused module `src/memory_graph/page_markdown.rs` (EntityPages live in memory_graph; this is their content-format util home). Update callers: `memorization/service.rs:499`, `page_dual_write.rs:16`. Bring the relevant `#[cfg(test)]` tests along.
- Move `gbrain/chat_extractor.rs` → a non-gbrain module (recommend `src/memory_graph/chat_extractor.rs` — it produces EntityPage proposals for the memory layer; alternative `memorization/`). Keep the type/fn names; update the callers in `turn_runner.rs` (~:147,:159) + any `crate::gbrain::chat_extractor` path. Bring its tests.
- After this, `gbrain/` contains only network/transport/DTO code (browse network fns, cli_format, scoped, mod) — all deleted in later phases.

### Phase 2 — cut the live recall path
- `router::load_context` (`router.rs:~266`): remove the hard-coded `sources.push("gbrain")` (+ any "gbrain" arm in the source loop). bucket_seal/EntityPage recall remains (same posture as 2c's recall-leg removals). After this, `GbrainAdapter::recall` is never called at runtime.

### Phase 3 — remove the page migration
- `app.rs` (~:1178-1191): remove the `migrate_gbrain_pages` fire-and-forget spawn. Delete `src/memory_adapter/gbrain_page_migration.rs` + its `mod` decl. (The marker on users' machines becomes irrelevant; gbrain has no source to read post-2d.)

### Phase 4 — remove `GbrainAdapter`
- `app.rs:1099-1104`: delete the `GbrainAdapter::new(...)` construction + `memory_adapters_map.insert("gbrain", ...)`. Delete `src/memory_adapter/gbrain.rs`. Remove `pub mod gbrain;` / `pub use gbrain::GbrainAdapter;` + the migration `mod` from `memory_adapter/mod.rs`. (The gbrain `browse` DTOs `PageDetail`/`PageSummary`/`SearchHit` used only by the adapter+migration die with them.)

### Phase 5 — delete the gbrain eval harness + restart cmd
- `tauri_commands.rs`: delete `run_memory_gbrain_eval` (+ `run_memory_gbrain_eval_probe`/`probe_gbrain_write_recall`/`call_gbrain_eval_tool`/`build_memory_gbrain_eval_report` helpers) + `restart_gbrain_mcp`. `main.rs`: remove their `invoke_handler!` entries. `eval/adapters/memory.rs` + `eval/memory_inventory.rs`: delete the gbrain probe/adapter bits (or the files if wholly gbrain). Remove the gbrain setup-script entries from `SETUP_SCRIPT_ALLOWLIST` (and `run_setup_script` if it becomes dead).

### Phase 6 — remove diagnostics gbrain block + FE
- `tauri_commands.rs` `get_system_diagnostics`: remove the `GbrainStatus`/`gbrain` + `gbrain_init` fields + PGLite path probe + `classify_gbrain_error` + the `pglite_*` error strings. `mcp/mod.rs`: `GbrainInitStatus` enum.
- FE: `SystemTab.tsx` (the gbrain status block, `GbrainInitRow`, the `restart_gbrain_mcp` button, pglite labels), `gbrain-result.tsx` (gbrain tool-result renderer), `dev-tauri-mock.ts` + `SystemTab.test.tsx` gbrain fixtures.

### Phase 7 — remove the transport + boot (the Bun + PGLite teardown)
- `mcp/mod.rs`: delete `GbrainCliTransport` (struct+impl), `bundled_gbrain_config`, `is_legacy_bundled_gbrain_script_wrapper`, `is_bundled_gbrain`, `cleanup_stale_pglite_lock`, `is_brain_initialized`, `ensure_bundled_gbrain_initialized`, `seed_bundled_gbrain`, the gbrain CLI error-classification helpers, the `is_bundled_gbrain` branch in `connect_server_shared`, + their tests.
- `app.rs`: delete `find_bun_path`, `find_gbrain_entry`, `write_gbrain_launcher_files`, `is_packaged_resource_dir`, `system_bun_candidates*`, `first_working_bun`, the `gbrain_mcp_id`/`gbrain_init_status` AppState fields + inits, + the gbrain launcher/find_bun tests.
- `main.rs`: delete the entire Stage-3 gbrain init/seed boot block (`find_bun_path`/`find_gbrain_entry`/`write_gbrain_launcher_files`/`ensure_bundled_gbrain_initialized`/`seed_bundled_gbrain` + the `gbrain_mcp_id`/`gbrain_init_status` slot assignments).

### Phase 8 — bundle / config / scripts / src/gbrain / docs / FE setup-scripts
- `tauri.conf.json`: remove `"bunembed/bun": "bun"` + `"gbrain-source": "gbrain"` from `bundle.resources`.
- `.gitignore`: remove `src-tauri/bunembed` + `src-tauri/gbrain-source`.
- Delete dirs `src-tauri/bunembed/` + `src-tauri/gbrain-source/`. Delete `scripts/setup-bun-runtime.sh` + `setup-gbrain-source.sh` + `init-gbrain.sh`.
- Delete the remaining `src/gbrain/` directory entirely (`browse.rs`, `cli_format.rs`, `scoped.rs`, `mod.rs` — survivors already relocated in Phase 1) + `pub mod gbrain;` in `lib.rs`.
- **Automation**: remove `ScopedGbrainSchemaTool` + `spec_declares_gbrain` + the `gbrain_declared` threading (`automation/runtime/tool_registry.rs`, `automation/runtime/service.rs`) + the `gbrain_room_*` permission branch (`automation/permissions.rs`) — IF it stays within recon scope; else leave + note.
- FE: `DeveloperOptionsSection.tsx` setup-script cards for the 3 gbrain scripts + `embedding-endpoint.ts` `SETUP_SCRIPTS` entries (`setup-bun-runtime`/`setup-gbrain-source`/`init-gbrain`); delete `gbrain-browse.ts` (its last function `gbrainFullGraph` already invokes the native cmd — repoint its sole importer `MemoryModule.tsx` to a `memory-graph` lib fn OR keep the file as a thin native shim; the plan decides — KEEP if deleting cascades into DualNebulaView types). Confirm no FE caller breaks (tsc).
- Docs: CONTEXT.md (the Bun+gbrain setup lines), the `uclaw-memory-graph-freeze` skill (gbrain-source/bun setup refs).

## Deferred (cosmetic, NOT in 2d — per decision)
`GbrainPolicyTarget` → `EntityPagePolicyTarget`; `gbrain_extractor_enabled`/`gbrain_extractor_daily_token_budget` flag + `GbrainExtractorPipeline` + `set_gbrain_extractor_pipeline` + `today_gbrain_extract_tokens` cost-tag → `page_extractor`/`chat_extractor` naming; `gbrain_drafts` inbox dir → `drafts`; `IngestError::Gbrain` → already has `Storage` (drop `Gbrain` when no caller). All functional, gbrain-runtime-independent; rename in a later cosmetic-cleanup PR (flag renames need serde-default tolerance + migration care). Record as a follow-up.

## Error handling

Phase 2's recall-leg removal is best-effort posture (bucket_seal recall remains). Removing the boot block: the gbrain server simply never seeds/connects — no fallback needed (no consumer left). Diagnostics: the FE SystemTab loses its gbrain card; `get_system_diagnostics` returns without the gbrain field (FE must tolerate the field's absence — Phase 6 updates the FE type).

## Testing

1. **Phase 1 relocation**: `cargo test --lib memory_graph::page_markdown memory_graph::chat_extractor` (relocated tests green); callers compile.
2. **Phase 2-7 removals**: compiler-guided; `cargo build` + `cargo clippy --lib` clean after each; `cargo test --lib memory_adapter mcp memubot_config app` green (update tests asserting gbrain boot/adapter/diagnostics).
3. **FE**: `cd ui && npx tsc --noEmit` delta empty (the gbrain status type removal must update all consumers); vitest for SystemTab/memory views green.
4. **Grep gates**: `grep -rn "GbrainCliTransport\|find_bun\|find_gbrain_entry\|seed_bundled_gbrain\|bundled_gbrain_config\|ensure_bundled_gbrain_initialized\|GbrainAdapter\|gbrain_page_migration\|run_memory_gbrain_eval\|restart_gbrain_mcp\|GbrainInitStatus\|crate::gbrain\|src/gbrain" src-tauri/src` → empty (only the deferred-cosmetic names — GbrainPolicyTarget, gbrain_extractor, gbrain_drafts — may remain, with a note).
5. **The core acceptance**: a FRESH `git worktree` with NO gbrain placeholders (`bunembed/`/`gbrain-source/` not created) runs `cargo build` successfully (proves the tauri.conf resource-existence panic is gone). Run the app: boot log has no `bun`/`gbrain`/`PGLite`/`seed_bundled_gbrain` lines; agent + WikiView + DualNebulaView all function (native).

## Scope / files (summary)

| Area | Change |
|---|---|
| `memory_graph/page_markdown.rs` (new), `memory_graph/chat_extractor.rs` (moved) | relocate forced survivors; update callers (memorization, page_dual_write, turn_runner) |
| `memory_adapter/router.rs` | drop `sources.push("gbrain")` |
| `memory_adapter/gbrain.rs`, `gbrain_page_migration.rs`, `mod.rs` | delete adapter + migration + decls |
| `app.rs`, `main.rs` | delete GbrainAdapter wiring, migration spawn, find_bun/find_gbrain_entry/launcher, AppState gbrain fields, Stage-3 boot block + tests |
| `mcp/mod.rs` | delete GbrainCliTransport + bundled_gbrain_config + seed/ensure_initialized + connect branch + GbrainInitStatus + helpers + tests |
| `tauri_commands.rs`, `main.rs` | delete eval harness + restart_gbrain_mcp + diagnostics gbrain block + setup-script allowlist gbrain entries |
| `eval/adapters/memory.rs`, `eval/memory_inventory.rs` | delete gbrain probe/adapter |
| `automation/runtime/{tool_registry,service}.rs`, `automation/permissions.rs` | remove ScopedGbrainSchemaTool + spec_declares_gbrain + gbrain_room_* perms (if in scope) |
| `src/gbrain/` (dir), `lib.rs` | delete the module |
| `tauri.conf.json`, `.gitignore`, `scripts/setup-bun-runtime.sh`/`setup-gbrain-source.sh`/`init-gbrain.sh`, `src-tauri/bunembed/`, `src-tauri/gbrain-source/` | remove bundle entries + ignore lines + scripts + dirs |
| FE: `SystemTab.tsx`, `DeveloperOptionsSection.tsx`, `embedding-endpoint.ts`, `gbrain-result.tsx`, `gbrain-browse.ts`, `dev-tauri-mock.ts`, `SystemTab.test.tsx` | remove gbrain diagnostics + setup-script cards + renderer |
| `CONTEXT.md`, `.claude/skills/uclaw-memory-graph-freeze/SKILL.md` | remove Bun+gbrain setup references |

## Risk

Med-High (broad blast radius — boot sequence + AppState + FE + bundle). Mitigations: phased order cuts consumers before transport (each phase compiles); the compiler enumerates every removal site; grep + the fresh-build acceptance test gate the finish-line. The one behavior change is Phase 2's recall-leg removal (bucket_seal recall already covers it — 2c precedent). gbrain is fully dead after 2c (no functional consumer), so removal is low-semantic-risk. Deferred cosmetic renames keep 2d's scope bounded. After 2d: **Step 2 closed — zero external runtimes.**
