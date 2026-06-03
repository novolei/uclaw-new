# Step 2d — gbrain Runtime Teardown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Remove everything that still keeps the gbrain MCP server (Bun + PGLite) alive — transport, boot/seed, adapter, migration, eval, diagnostics, bundle/dev placeholders — so the binary ships zero external runtimes and a fresh worktree builds with no gbrain placeholders. Closes Step 2.

**Architecture:** Phased dependency-safe teardown — relocate the two forced survivors (pure markdown utils + chat_extractor) out of `gbrain::`, cut the live recall consumer, then delete migration → adapter → eval → diagnostics → transport+boot → bundle/config/scripts/`src/gbrain/`/docs. Each task compiles; the compiler + grep gates + a fresh-build acceptance test are the guard.

**Tech Stack:** Rust (Tauri, rusqlite), TypeScript/React (FE diagnostics removal), tauri.conf.json bundle, shell scripts. No new deps; no language change.

**Key facts (recon, file:line):**
- **Forced survivors to relocate (Phase 1):** `gbrain::browse::build_raw_markdown` (`gbrain/browse.rs:164`, caller `memorization/service.rs:499`) + `gbrain::browse::split_frontmatter` (`browse.rs:183`, caller `memory_adapter/page_dual_write.rs:16`); `gbrain/chat_extractor.rs` (whole module, callers `agent/dispatcher/turn_runner.rs:~147,159`, fired by the `GbrainExtractorPipeline` in `dispatcher/mod.rs`).
- **Live recall consumer:** `memory_adapter/router.rs:266` `sources.push("gbrain")` in `load_context` (called `tauri_commands.rs:1908`).
- **Adapter + migration:** `memory_adapter/gbrain.rs` (`GbrainAdapter`, `MemoryAdapter` impl, constructed `app.rs:1099-1104`, registered `memory_adapters_map["gbrain"]`); `memory_adapter/gbrain_page_migration.rs` (spawned `app.rs:1184`); decls in `memory_adapter/mod.rs:24,25,37`.
- **Eval/restart:** `run_memory_gbrain_eval` (`tauri_commands.rs:509`, registered `main.rs:1263`) + helpers (`probe_gbrain_write_recall`, `call_gbrain_eval_tool`, `build_memory_gbrain_eval_report`); `restart_gbrain_mcp` (`tauri_commands.rs:1272`, registered `main.rs:1271`); `eval/adapters/memory.rs` + `eval/memory_inventory.rs` gbrain probe.
- **Diagnostics:** `get_system_diagnostics` (`tauri_commands.rs:322`) `GbrainStatus`/`gbrain`/`gbrain_init` fields + PGLite probe + `classify_gbrain_error` + `pglite_*` strings; `GbrainInitStatus` enum (`mcp/mod.rs:91`). FE: `SystemTab.tsx` gbrain block + `GbrainInitRow` + restart button + pglite labels; `gbrain-result.tsx` (renders OLD gbrain tool results — dead since 2c); `dev-tauri-mock.ts`/`SystemTab.test.tsx` gbrain fixtures.
- **Transport + boot:** `mcp/mod.rs` `GbrainCliTransport` (struct+impl ~1048-1485), `bundled_gbrain_config` (577), `is_legacy_bundled_gbrain_script_wrapper` (609), `is_bundled_gbrain` (1555), `cleanup_stale_pglite_lock` (1567), `is_brain_initialized` (2954), `ensure_bundled_gbrain_initialized` (2987), `seed_bundled_gbrain` (2103), error-classification helpers (195-249), the `is_bundled_gbrain` connect branch (2751-2761), `GbrainInitStatus` (91). `app.rs` `find_bun_path` (1388), `find_gbrain_entry` (1515), `write_gbrain_launcher_files` (1565), `is_packaged_resource_dir` (1367), `system_bun_candidates*` (1437/1443), `first_working_bun` (1462), AppState `gbrain_mcp_id` (383)/`gbrain_init_status` (390) + inits (1324-1326) + the `gbrain_launcher_tests`/find_bun tests (~1923-2280). `main.rs` Stage-3 gbrain boot block (~595-801).
- **Bundle/config/scripts:** `tauri.conf.json:56-57` (`"bunembed/bun":"bun"`, `"gbrain-source":"gbrain"`); `.gitignore:7,12`; `scripts/setup-bun-runtime.sh`/`setup-gbrain-source.sh`/`init-gbrain.sh`; dirs `src-tauri/bunembed/`+`src-tauri/gbrain-source/`. `tauri_commands.rs` `SETUP_SCRIPT_ALLOWLIST` (1082-1086) gbrain entries. FE `DeveloperOptionsSection.tsx` setup-script cards + `embedding-endpoint.ts` `SETUP_SCRIPTS` (43-45).
- **src/gbrain dir:** `browse.rs` (network fns, dead after adapter removal), `cli_format.rs` (transport-only), `scoped.rs` (unused), `mod.rs`; `pub mod gbrain;` `lib.rs:49`.
- **Automation gbrain_room_*:** `ScopedGbrainSchemaTool` (`automation/runtime/tool_registry.rs`, returns `Err("not connected")` — schema-only), `spec_declares_gbrain` (`automation/runtime/service.rs:68`), `gbrain_room_*` perms (`automation/permissions.rs`).
- **Docs:** `CONTEXT.md` (14-16, 53-54, 205); `.claude/skills/uclaw-memory-graph-freeze/SKILL.md` (56, 81).
- **Placeholder mechanism:** `tauri_build::build()` validates every `bundle.resources` key with `path.exists()` at compile → panic if missing. Removing the 2 gbrain entries (Phase 8) eliminates the bunembed/gbrain-source placeholder need.
- **Worktree:** `/Users/ryanliu/Documents/uclaw-worktrees/step2d-gbrain-runtime-teardown` (branch `claude/step2d-gbrain-runtime-teardown`). NEVER `git stash` / `--no-verify`. Build: `cd src-tauri && cargo build 2>&1 | grep -E "^error"`. FE tsc: symlink `ui/node_modules` → `/Users/ryanliu/Documents/uclaw/ui/node_modules` if missing.

---

## Task 1: Relocate forced survivors out of `gbrain::` (compile-only)

**Files:** Create `src-tauri/src/memory_graph/page_markdown.rs` + `src-tauri/src/memory_graph/chat_extractor.rs`; modify `src-tauri/src/memory_graph/mod.rs`, `gbrain/browse.rs`, `gbrain/chat_extractor.rs` (delete after move), `gbrain/mod.rs`, `memorization/service.rs`, `memory_adapter/page_dual_write.rs`, `agent/dispatcher/turn_runner.rs`, `agent/dispatcher/mod.rs`.

- [ ] **Step 1: Move the pure markdown utils.** Create `memory_graph/page_markdown.rs`; move `build_raw_markdown` + `split_frontmatter` (and any private helpers they call + their `#[cfg(test)]` tests) verbatim from `gbrain/browse.rs`. Add `pub mod page_markdown;` to `memory_graph/mod.rs`. Update callers: `memorization/service.rs:499` (`crate::gbrain::browse::build_raw_markdown` → `crate::memory_graph::page_markdown::build_raw_markdown`) + `memory_adapter/page_dual_write.rs:16` (`browse::split_frontmatter` → `crate::memory_graph::page_markdown::split_frontmatter`). Leave the now-removed fns gone from `browse.rs`.
- [ ] **Step 2: Move chat_extractor.** Move `gbrain/chat_extractor.rs` → `memory_graph/chat_extractor.rs` (verbatim, incl. tests). Add `pub mod chat_extractor;` to `memory_graph/mod.rs`; remove `pub mod chat_extractor;` from `gbrain/mod.rs`. Update callers in `agent/dispatcher/turn_runner.rs` (~:147,:159 — `crate::gbrain::chat_extractor::*` → `crate::memory_graph::chat_extractor::*`) + any other `gbrain::chat_extractor` path (grep). Keep all type/fn names (no rename — `MIN_ACT_CONFIDENCE`, the extract fn, etc.).
- [ ] **Step 3: Build + clippy + test.** `cargo build 2>&1 | grep -E "^error"` (empty); `cargo clippy --lib 2>&1 | grep "warning: "` (no new); `cargo test --lib memory_graph::page_markdown memory_graph::chat_extractor 2>&1 | tail` (relocated tests green).
- [ ] **Step 4: Commit.** `refactor(memory): relocate page-markdown utils + chat_extractor out of gbrain:: into memory_graph (Step 2d)`

---

## Task 2: Cut the live gbrain recall path

**Files:** `src-tauri/src/memory_adapter/router.rs`.

- [ ] **Step 1: Drop the gbrain source.** In `load_context` (`router.rs:~253-282`), remove the hard-coded `sources.push("gbrain")` (line ~266) + any `"gbrain"` arm in the per-source match/loop. bucket_seal/EntityPage recall remains. (Mirror 2c's recall-leg removals — best-effort posture, no fallback needed.)
- [ ] **Step 2: Build + clippy + test.** clean; `cargo test --lib memory_adapter::router 2>&1 | tail` (update any test asserting the gbrain source in load_context).
- [ ] **Step 3: Commit.** `refactor(memory): drop gbrain leg from load_context unified recall (bucket_seal/EntityPage cover it) (Step 2d)`

---

## Task 3: Remove the gbrain page migration + `GbrainAdapter`

**Files:** `src-tauri/src/app.rs`, `memory_adapter/gbrain.rs` (delete), `memory_adapter/gbrain_page_migration.rs` (delete), `memory_adapter/mod.rs`, `gbrain/browse.rs`.

- [ ] **Step 1: Remove the migration.** In `app.rs` (~:1178-1191) delete the `migrate_gbrain_pages` `tauri::async_runtime::spawn` block. Delete `src/memory_adapter/gbrain_page_migration.rs` + its `mod` decl in `memory_adapter/mod.rs:25`.
- [ ] **Step 2: Remove the adapter.** In `app.rs:1099-1104` delete the `GbrainAdapter::new(...)` construction + the `memory_adapters_map.insert("gbrain", ...)`. Delete `src/memory_adapter/gbrain.rs`. Remove `pub mod gbrain;` (`mod.rs:24`) + `pub use gbrain::GbrainAdapter;` (`mod.rs:37`).
- [ ] **Step 3: Drop now-dead browse network fns** that were only called by the adapter+migration (`list_pages`/`get_page`/`search`/`put_page`/`get_backlinks`/`get_versions`/`get_stats`/`find_orphans`/`revert_version` + their `parse_*` helpers + `call_gbrain` + the DTOs `PageDetail`/`PageSummary`/`SearchHit`/`Backlink`/etc.) from `gbrain/browse.rs` IF now caller-less — but `cli_format` (transport, Task 6) may still reference some; grep `browse::` + the DTOs across `src/` and remove only the genuinely caller-less. If the whole of `browse.rs` is now dead except items used by `cli_format`/transport, leave it for Task 7's wholesale `src/gbrain/` delete and just note. (Goal: no clippy errors; pub fns don't dead-code-warn, so leaving them is acceptable.)
- [ ] **Step 4: Build + clippy + test.** clean; `cargo test --lib memory_adapter 2>&1 | tail` (update tests asserting the gbrain adapter/migration).
- [ ] **Step 5: Commit.** `refactor(memory): remove GbrainAdapter + gbrain page migration (no live consumer after 2a-2c) (Step 2d)`

---

## Task 4: Delete the gbrain eval harness + restart cmd

**Files:** `src-tauri/src/tauri_commands.rs`, `main.rs`, `eval/adapters/memory.rs`, `eval/memory_inventory.rs`.

- [ ] **Step 1: Delete the Tauri cmds.** Remove `run_memory_gbrain_eval` (`tauri_commands.rs:509`) + its helpers (`run_memory_gbrain_eval_probe`, `probe_gbrain_write_recall`, `call_gbrain_eval_tool`, `build_memory_gbrain_eval_report`, `run_memory_inventory_smoke` if gbrain-only) + `restart_gbrain_mcp` (`:1272`) + the `GbrainSmoke*`/scorecard structs they use. Remove their `invoke_handler!` entries in `main.rs` (`:1263`, `:1271`).
- [ ] **Step 2: Delete the eval adapter/inventory gbrain bits.** In `eval/adapters/memory.rs` + `eval/memory_inventory.rs`: delete the gbrain probe/adapter (the `"mcp__gbrain__list_pages"` fixture, `probe_gbrain_inventory`, etc.). If a file is wholly gbrain-probe, delete it + its `mod` decl; if mixed, remove only the gbrain parts.
- [ ] **Step 3: Build + clippy + test.** clean; `cargo test --lib eval tauri_commands 2>&1 | tail` (the pre-existing `memory_gbrain_eval` failing test is removed with the harness — confirm it's gone, not newly-failing).
- [ ] **Step 4: Commit.** `refactor(memory): delete gbrain eval harness + restart_gbrain_mcp cmd (probes a runtime being retired) (Step 2d)`

---

## Task 5: Remove the diagnostics gbrain block (Rust + FE)

**Files:** `src-tauri/src/tauri_commands.rs`, `mcp/mod.rs`; FE `ui/src/components/settings/SystemTab.tsx`, `ui/src/components/agent/tool-renderers/gbrain-result.tsx` (delete), `ui/src/lib/dev-tauri-mock.ts`, `ui/src/components/settings/SystemTab.test.tsx`.

- [ ] **Step 1: Rust diagnostics.** In `get_system_diagnostics` (`tauri_commands.rs:322`): remove the `gbrain: GbrainStatus` + `gbrain_init` fields from the report struct + their population (the MCP `status("gbrain")` read, PGLite path probe ~:352-358, `classify_gbrain_error` + `pglite_*` strings ~:740-881). Remove the `GbrainStatus` struct. Remove `GbrainInitStatus` (`mcp/mod.rs:91` + Default).
- [ ] **Step 2: FE.** In `SystemTab.tsx`: remove the `GbrainStatus`/`GbrainInitStatus` TS types, the `gbrain`/`gbrain_init` report fields, `gbrainOperational`, the gbrain status rendering block, `GbrainInitRow`, the `restart_gbrain_mcp` button, and pglite labels. Delete `gbrain-result.tsx` (renders the removed `mcp__gbrain__*` tool results — dead since 2c; confirm no importer: `grep -rn "gbrain-result" ui/src`). Remove gbrain fixtures from `dev-tauri-mock.ts` + `SystemTab.test.tsx`.
- [ ] **Step 3: Build + tsc + test.** `cargo build` clean; symlink `ui/node_modules` (see header) then `cd ui && npx tsc --noEmit` (delta empty — the report-type change must update all consumers); `npm test -- --run 2>&1 | tail` (SystemTab tests green/updated).
- [ ] **Step 4: Commit.** `refactor(diag): remove gbrain status from system diagnostics + FE SystemTab + dead gbrain-result renderer (Step 2d)`

---

## Task 6: Remove the gbrain MCP transport + boot (Bun + PGLite)

**Files:** `src-tauri/src/mcp/mod.rs`, `app.rs`, `main.rs`.

- [ ] **Step 1: Remove the boot block (main.rs).** Delete the entire Stage-3 gbrain init/seed block (`~595-801`): `find_bun_path`/`find_gbrain_entry`/`write_gbrain_launcher_files` calls, `ensure_bundled_gbrain_initialized`, `seed_bundled_gbrain`, the `gbrain_mcp_id`/`gbrain_init_status` slot assignments. Keep the surrounding Stage-3 structure intact (other services).
- [ ] **Step 2: Remove the find/launcher fns + AppState fields (app.rs).** Delete `find_bun_path`, `find_gbrain_entry`, `write_gbrain_launcher_files`, `is_packaged_resource_dir`, `system_bun_candidates*`, `first_working_bun`. Remove AppState `gbrain_mcp_id` (`:383`) + `gbrain_init_status` (`:390`) fields + their inits (`:1324-1326`). Delete the `gbrain_launcher_tests` module + the `find_bun_path*` tests (~:1923-2280).
- [ ] **Step 3: Remove the transport (mcp/mod.rs).** Delete `GbrainCliTransport` (struct + impl + its `McpTransport` impl), `bundled_gbrain_config`, `is_legacy_bundled_gbrain_script_wrapper`, `is_bundled_gbrain`, `cleanup_stale_pglite_lock`, `is_brain_initialized`, `ensure_bundled_gbrain_initialized`, `seed_bundled_gbrain`, the gbrain CLI error-classification helpers (`classify_gbrain_cli_failure`/`gbrain_cli_error_hint`/`gbrain_cli_error_payload` ~195-249), and the `is_bundled_gbrain` branch in `connect_server_shared` (~2751-2761 — that branch now becomes the normal `StdioTransport` path or is removed if gbrain is the only bundled-CLI case). Delete all associated tests (`seed_bundled_gbrain_*`, etc.).
- [ ] **Step 4: Build + clippy + test.** `cargo build 2>&1 | grep -E "^error"` (empty); `cargo clippy --lib 2>&1 | grep "warning: "` (no new — esp. no unused imports left from the transport removal); `cargo test --lib mcp app 2>&1 | tail -20` (green; update/remove tests asserting gbrain boot/seed).
- [ ] **Step 5: Commit.** `refactor(mcp): remove GbrainCliTransport + gbrain boot/seed + find_bun/find_gbrain_entry (no Bun/PGLite launch) (Step 2d)`

---

## Task 7: Delete `src/gbrain/`, bundle/config/scripts, automation stubs, FE setup-scripts, docs + fresh-build acceptance

**Files:** `src-tauri/src/gbrain/` (delete dir), `lib.rs`, `tauri.conf.json`, `.gitignore`, `scripts/*`, `src-tauri/bunembed/` + `gbrain-source/` (delete dirs), `automation/runtime/{tool_registry,service}.rs`, `automation/permissions.rs`, `tauri_commands.rs` (SETUP_SCRIPT_ALLOWLIST), FE `DeveloperOptionsSection.tsx` + `embedding-endpoint.ts`, `CONTEXT.md`, `.claude/skills/uclaw-memory-graph-freeze/SKILL.md`.

- [ ] **Step 1: Delete `src/gbrain/`.** `grep -rn "crate::gbrain\|gbrain::browse\|gbrain::cli_format\|gbrain::scoped" src/` → confirm only the now-removable items remain (after Tasks 1/3/6). Delete the `src-tauri/src/gbrain/` directory entirely (`browse.rs`/`cli_format.rs`/`scoped.rs`/`mod.rs`) + `pub mod gbrain;` in `lib.rs:49`.
- [ ] **Step 2: Automation gbrain_room_* stubs.** Remove `ScopedGbrainSchemaTool` (`automation/runtime/tool_registry.rs`) + the `gbrain_declared`/`spec_declares_gbrain` threading (`automation/runtime/service.rs:68`) + the `gbrain_room_*` permission arm (`automation/permissions.rs`). If removal cascades beyond these (e.g. a shared registry-build signature used widely), STOP, do the minimal removal that compiles, and note the remainder for follow-up.
- [ ] **Step 3: Bundle/config/scripts.** `tauri.conf.json`: remove `"bunembed/bun": "bun"` + `"gbrain-source": "gbrain"` from `bundle.resources`. `.gitignore`: remove `src-tauri/bunembed` + `src-tauri/gbrain-source`. Delete `scripts/setup-bun-runtime.sh` + `setup-gbrain-source.sh` + `init-gbrain.sh`. Remove the 3 gbrain entries from `SETUP_SCRIPT_ALLOWLIST` (`tauri_commands.rs:1082-1086`) — if the allowlist/`run_setup_script` becomes empty/dead, remove it too.
- [ ] **Step 4: FE setup-scripts.** Remove the `setup-bun-runtime`/`setup-gbrain-source`/`init-gbrain` entries from `embedding-endpoint.ts` `SETUP_SCRIPTS` (~:43-45) + their descriptors; remove their cards in `DeveloperOptionsSection.tsx` (if the section becomes empty, leave a minimal placeholder or remove the section per what reads cleanly). KEEP `ui/src/lib/gbrain-browse.ts` (functional native page-browse shim — its `gbrainFullGraph` invokes the native cmd; renaming is deferred cosmetic).
- [ ] **Step 5: Delete dev dirs + docs.** Delete `src-tauri/bunembed/` + `src-tauri/gbrain-source/` directories. Update `CONTEXT.md` (remove the Bun+gbrain runtime/setup lines 14-16, 53-54, 205) + `.claude/skills/uclaw-memory-graph-freeze/SKILL.md` (lines 56, 81 gbrain-source/bun setup refs).
- [ ] **Step 6: FRESH-BUILD ACCEPTANCE.** With `bunembed/`+`gbrain-source/` now deleted, run `cd src-tauri && cargo build 2>&1 | grep -E "^error"` → MUST be empty (proves the tauri.conf resource-exists panic is gone — no gbrain placeholders needed). `cargo clippy --lib` clean. `cd ui && npx tsc --noEmit` delta empty.
- [ ] **Step 7: Commit.** `refactor(build): delete src/gbrain + bunembed/gbrain-source + bundle/scripts/automation/FE/docs refs — zero external runtimes (Step 2d / Step 2 closeout)`

---

## Task 8: Whole-slice verification + ship

- [ ] **Step 1: Build + clippy + tests.** `cargo build` + `cargo clippy --lib` clean; `cargo test --lib memory_graph memory_adapter mcp app eval tauri_commands 2>&1 | grep "test result:"` (green; the only failures should be the pre-existing browser/shell/skill_marketplace ones — confirm the gbrain_eval failure is GONE, removed in Task 4). `cd ui && npx tsc --noEmit` delta empty + `npm test -- --run` SystemTab/memory views green.
- [ ] **Step 2: Grep gates (want empty):** `grep -rn "GbrainCliTransport\|find_bun\|find_gbrain_entry\|seed_bundled_gbrain\|bundled_gbrain_config\|ensure_bundled_gbrain_initialized\|GbrainAdapter\|gbrain_page_migration\|run_memory_gbrain_eval\|restart_gbrain_mcp\|GbrainInitStatus\|crate::gbrain\|GbrainStatus" src-tauri/src` → empty (deferred-cosmetic names — `GbrainPolicyTarget`, `gbrain_extractor`, `gbrain_drafts`, `IngestError::Gbrain` — may remain). `grep -rn "bunembed\|gbrain-source" src-tauri tauri.conf.json .gitignore scripts` → empty. `ls src-tauri/src/gbrain src-tauri/bunembed src-tauri/gbrain-source 2>&1` → all "No such file".
- [ ] **Step 3: Ship.** push → PR (Commits table T1-T7) → rebase-merge → sync parent main → worktree remove + branch cleanup → `npx gitnexus analyze` reindex.
- [ ] **Step 4: Post-merge soak (manual):** fresh `cargo tauri dev` (or run) — boot log has NO `bun`/`gbrain`/`PGLite`/`seed_bundled_gbrain` lines; agent works, WikiView shows EntityPages, DualNebulaView renders, `memory_put_page`/recall function — all native, zero external runtime. **Step 2 closed.**

---

## Self-Review

- **Spec coverage:** Phase 1→T1, Phase 2→T2, Phase 3+4→T3, Phase 5→T4, Phase 6→T5, Phase 7→T6, Phase 8→T7, verify→T8. ✓ All spec phases + the deferred-cosmetic boundary + the fresh-build acceptance mapped.
- **Ordering keeps each commit compiling:** relocate survivors (T1) → cut recall consumer (T2) → delete migration+adapter (T3) → eval (T4) → diagnostics+FE (T5) → transport+boot (T6) → delete src/gbrain + bundle/config (T7, after all gbrain:: consumers gone). ✓
- **Type/name consistency:** relocated `build_raw_markdown`/`split_frontmatter` → `memory_graph::page_markdown::*`; `chat_extractor` → `memory_graph::chat_extractor::*` (names unchanged) — used consistently in T1's caller updates. ✓
- **No placeholders:** real file:line + relocation targets + grep gates + the explicit fresh-build acceptance (T7 Step 6). The two flagged impl-judgment points (browse.rs partial-vs-wholesale delete in T3 → resolved by T7 wholesale; automation cascade bound in T7 Step 2) are explicit. ✓
- **Deferred (NOT in plan, per decision):** `GbrainPolicyTarget`/`gbrain_extractor` flag+pipeline/`gbrain_drafts` dir/`IngestError::Gbrain` renames — functional, gbrain-runtime-independent; cosmetic follow-up. ✓
- **Finish-line:** after T7, no gbrain runtime/transport/boot/adapter/bundle; fresh build needs no gbrain placeholder (grep + fresh-build gated). Step 2 closed. ✓
