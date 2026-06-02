# Step 3b-4 — memU Teardown (zero Python) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Delete the dead memU path (Python bridge, client, adapter, boot, unused fields, internal commands) so the app runs **zero Python**, while keeping the frontend IPC surface stable (stub the 3 frontend-facing memU commands; a separate frontend PR removes the UI later).

**Architecture:** Pure deletion. The Rust compiler is the primary verifier — any dangling `crate::memu` reference is a build error, not a silent runtime bug. Ordering removes all USAGES (commands → fields → boot) before deleting the MODULE, so every commit compiles.

**Tech Stack:** Rust, Tauri.

**Scope decision (confirmed):** backend-only. Frontend-facing commands `get_memu_status` / `restart_memu_bridge` + `SystemDiagnosticsReport.memu` are **stubbed to a stable offline/no-op** response (keep `MemUBridgeStatus` as the stub type). `memu_embed_text` + `/v1/embeddings` STAY (in-process, Step 3b-1). A follow-up frontend PR removes `memuOnlineAtom`/`memuConsolidatingAtom` + the dock indicator + these stubbed commands.

**Key facts (recon, file:line):**
- Module: `src-tauri/src/memu/` (`mod.rs`, `bridge.rs`, `client.rs`, `memu_bridge.py`, `__pycache__/`). `MemUAdapter`: `src-tauri/src/memory_adapter/memu.rs` + exports in `memory_adapter/mod.rs` (`pub mod memu;`, `pub use memu::MemUAdapter;`).
- 9 `memu_client` fields: `AppState` (`app.rs:208`), `MemorizationService` (`service.rs:56`, setter `:150-154`), `SkillSearchTool` (`skill_search.rs:54`, `with_memu` `:73-75`), `LocalApiService` (`server.rs:53`), `ApiState` (`routes.rs:21`), `HybridSearchEngine` (`hybrid_search.rs:254`, `new` `:259`), `MemoryOsRuntimeConfig`+`ProactiveService` (`proactive/service.rs:639,650`), `ProactiveRecallService` (`proactive_recall.rs:73`).
- Boot: `app.rs` `try_init_memu` (~`:1361-1404`) + call (`:645`) + `MemUAdapter::new` reg (`:1135-1140`); `main.rs` set_app_handle (`:125-131`), clone (`:209`), `set_memu_client` (`:271`), shutdown (`:982,996-1000`).
- Commands: `restart_memu_bridge` (macro `main.rs:1288`), `get_memu_status` (macro `:1485`), `memu_embed_text` (macro `:1486`, KEEP). Diagnostics: `MemUBridgeStatus` (`tauri_commands.rs:263`), `SystemDiagnosticsReport.memu` (`:305`), `get_system_diagnostics` (`:333`, memU section `:369-389`). Eval: `run_memory_gbrain_eval` (`:560`) → `run_memory_gbrain_eval_probe` (`:619`) → `probe_memu_write_recall` (`:648`) + memu inventory target (`:797`). `dev_trigger_proactive` (memorize_with_config call).
- Config: `EmbeddingEndpointConfig` (`memubot_config.rs:114-155`, Default `:913-924`) — drop `model` + `fastembed_model`, keep `base_url`/`dimensions`/`embed_timeout_secs`. Factory `:7337` comment in `score/embed/factory.rs:2-9`.
- Bundle: `tauri.conf.json` resources `"pyembed/python"`, `"src/memu/memu_bridge.py"`. Dirs `pyembed/` (root) + `src-tauri/pyembed/`. Script `scripts/setup-python-env.sh`.

---

## Task 1: Stub frontend-facing memU commands/diagnostics + trim eval/dev memU legs

**Files:** `tauri_commands.rs`.

- [ ] **Step 1: Stub `get_memu_status`** — replace the body that queries `state.memu_client.health_check()` with a static offline response (keep the command + its macro entry + return shape):
```rust
#[tauri::command]
pub async fn get_memu_status(_state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    // memU removed (Step 3b-4). Stub kept for frontend IPC stability until the
    // frontend memU indicator is retired in a follow-up.
    Ok(serde_json::json!({ "online": false, "reason": "removed" }))
}
```
- [ ] **Step 2: Stub `restart_memu_bridge`** — no client to restart:
```rust
#[tauri::command]
pub async fn restart_memu_bridge(_state: State<'_, AppState>) -> Result<(), String> {
    // memU removed (Step 3b-4). No-op stub for frontend IPC stability.
    Ok(())
}
```
- [ ] **Step 3: Stub the diagnostics memU section** (`get_system_diagnostics` `:369-389`): replace the `state.memu_client` snapshot/health logic with a constant offline `MemUBridgeStatus` (keep the struct + the `memu` field on `SystemDiagnosticsReport`):
```rust
    let memu = MemUBridgeStatus { alive: false, /* ...other fields → None/default... */ };
```
(Read `MemUBridgeStatus`'s fields at `:263` and fill defaults/None; keep `db_path` as the computed `data_dir.join("memory/memu.db")` string if the struct has it — harmless.)
- [ ] **Step 4: Trim the eval-harness memU leg** — in `run_memory_gbrain_eval_probe` (`:619`) remove the `probe_memu_write_recall(...)` call (`:628`) and the `memu` inventory target (`:797`); delete `probe_memu_write_recall` (`:648`). Keep the gbrain/bucket_seal legs of the harness. If `run_memory_gbrain_eval` takes a `memu_client` param (`:620,649`), drop it.
- [ ] **Step 5: Trim `dev_trigger_proactive`** — remove its `memorize_with_config` memU call (and the `memu_client` fetch). If memU was the command's only action, replace the body with an error/no-op `Err("dev_trigger_proactive: memU removed".into())`; else keep its non-memU parts.
- [ ] **Step 6: Build** — `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` (none). `MemUClient` is still referenced by the held fields (removed next task) — that's fine, it still compiles.
- [ ] **Step 7: Commit** — `refactor(memu): stub frontend-facing memU commands + trim eval/dev legs (Step 3b-4)`

---

## Task 2: Strip the 9 `memu_client` fields + constructors + call sites

**Files:** `app.rs`, `memorization/service.rs`, `agent/tools/builtin/skill_search.rs`, `agent/tools/registry_build.rs`, `local_api/server.rs`, `local_api/routes.rs`, `proactive/hybrid_search.rs`, `proactive/service.rs`, `proactive/proactive_recall.rs`.

- [ ] **Step 1: Remove each field + its init + setter**, and drop the param from each `new()`/builder + update every call site:
  - `AppState.memu_client` (`app.rs:208`) + the `Self{}` assignment (the `try_init_memu` call is removed in Task 3; here just drop the field — temporarily assign from the call, or reorder with Task 3; simplest: do the field removal + the try_init_memu removal TOGETHER — see note).
  - `MemorizationService.memu_client` + `set_memu_client` (`service.rs:56,150-154`); drop the `set_memu_client` call in `main.rs:271`.
  - `SkillSearchTool.memu_client` + `with_memu` (`skill_search.rs:54,73-75`); drop `.with_memu(...)` in `registry_build.rs`.
  - `LocalApiService.memu_client` (`server.rs:53`) + `ApiState.memu_client` (`routes.rs:21`) + the `LocalApiService::new` param + the `main.rs` construction arg + the `ApiState{}` assignment.
  - `HybridSearchEngine.memu_client` (`hybrid_search.rs:254`) + `new` param (`:259`) + call site in `proactive/service.rs`.
  - `ProactiveRecallService.memu_client` (`proactive_recall.rs:73`) + `new` param + call site.
  - `MemoryOsRuntimeConfig.memu_client` + `ProactiveService.memu_client` (`proactive/service.rs:639,650`) + `ProactiveService::new` threading + the `main.rs` construction.

  **Note on AppState ordering:** removing `AppState.memu_client` couples to Task 3's `try_init_memu` removal. Do the `AppState.memu_client` field + `try_init_memu` removal together (fold that part of Task 3 here if cleaner) so AppState compiles. The implementer may merge Task 2+3's AppState/boot bits into one coherent step if the borrow/order demands it — keep the commit compiling.

- [ ] **Step 2: Build after each file** — `cargo build 2>&1 | grep -E "^error"`. Fix ripples (call sites passing the removed arg). The compiler enumerates every site.
- [ ] **Step 3: Test** — `cargo test --lib proactive memorization local_api 2>&1 | tail` (no regressions; tests that constructed these with a memU client drop the arg).
- [ ] **Step 4: Commit** — `refactor(memu): drop unused memu_client fields + constructor params across 9 structs (Step 3b-4)`

---

## Task 3: Delete boot wiring + MemUAdapter

**Files:** `app.rs`, `main.rs`, `memory_adapter/memu.rs`, `memory_adapter/mod.rs`.

- [ ] **Step 1: `app.rs`** — delete `fn try_init_memu` (~`:1361-1404`), its call (`:645`, if not already removed in Task 2), the `MemUAdapter::new` registration + `memory_adapters_map` insert (`:1135-1140`), and all `MEMU_LLM_*`/`FASTEMBED_MODEL`/`MEMU_*` env construction. Remove `use crate::memu::*` imports.
- [ ] **Step 2: `main.rs`** — delete the `set_app_handle` block (`:125-131`), the `memu_client` clone (`:209`), the `set_memu_client` call (`:271`, if not removed in Task 2), the shutdown block (`:982,996-1000`).
- [ ] **Step 3: Delete `MemUAdapter`** — `git rm src-tauri/src/memory_adapter/memu.rs`; remove `pub mod memu;` + `pub use memu::MemUAdapter;` from `memory_adapter/mod.rs`.
- [ ] **Step 4: Build + test** — clean; `cargo build 2>&1 | grep -E "^error"` (none). Now nothing constructs the bridge/client/adapter.
- [ ] **Step 5: Commit** — `refactor(memu): delete boot wiring (try_init_memu, health-check, shutdown) + MemUAdapter (Step 3b-4)`

---

## Task 4: Delete the memU module + config + bundle + pyembed + setup script

**Files:** `src/memu/`, `lib.rs`, `memubot_config.rs`, `score/embed/factory.rs`, `tauri.conf.json`, `pyembed/`, `scripts/setup-python-env.sh`.

- [ ] **Step 1: Delete the module** — `git rm -r src-tauri/src/memu/`. Remove `pub mod memu;` from `lib.rs` (and any remaining `use crate::memu::*` the compiler flags). Remove `restart_memu_bridge` + `get_memu_status` macro entries? NO — they're stubbed (Task 1), keep them. Confirm `memu_embed_text` macro entry stays.
- [ ] **Step 2: Config** (`memubot_config.rs`) — remove `model` + `fastembed_model` from `EmbeddingEndpointConfig` (the field, the `Default` impl entries `:913-924`, and any DTO/serde mirror — grep `fastembed_model`/`\.model` usage first; the gbrain-config-pointer comment goes too). Update the `base_url`/`dimensions` doc comments to note the in-process OnnxEmbedder.
- [ ] **Step 3: Factory comment** (`score/embed/factory.rs:2-9`) — update the `:7337` routing comment: it's now "the local in-process endpoint (OnnxEmbedder)," not "the memU default."
- [ ] **Step 4: Bundle** (`tauri.conf.json`) — remove `"pyembed/python": "python"` and `"src/memu/memu_bridge.py": "memu_bridge.py"` from `bundle.resources`. Leave bun/gbrain (Step 2).
- [ ] **Step 5: pyembed + setup script** — `git rm -r` the tracked parts of `pyembed/` (root + `src-tauri/pyembed/` — check what's tracked vs gitignored; the resource placeholders are gitignored, so likely only a `.gitkeep`/symlink is tracked) and `git rm scripts/setup-python-env.sh`. If pyembed is fully gitignored, just note it for local cleanup.
- [ ] **Step 6: Build + clippy** — `cargo build 2>&1 | grep -E "^error"` (none); `cargo clippy --lib 2>&1 | grep -E "^error"` (none). The build is the proof: zero dangling `crate::memu`.
- [ ] **Step 7: Commit** — `feat(memu): delete memu module + pyembed + setup script + memU config/bundle entries — zero Python (Step 3b-4)`

---

## Task 5: Whole-slice verification + ship

- [ ] **Step 1: Build + clippy + full test** — `cargo build`, `cargo clippy --lib`, `cargo test --lib` (targeted: memory_graph, proactive, memorization, local_api, memory_bucket_seal) all green.
- [ ] **Step 2: Gates**
  - `grep -rn "MemUClient\|MemUBridge\|crate::memu\|try_init_memu\|MEMU_LLM\|FASTEMBED" src-tauri/src/` → empty (no live refs).
  - `grep -rn "memu" src-tauri/src/ | grep -vE "map_memu_type_to_kind|memu_type|memu_embed_text|memu.db|get_memu_status|restart_memu_bridge|MemUBridgeStatus|stub|removed"` → only the intentional stubs/extractor-metadata remain.
  - `grep -rn "memu_bridge\|setup-python-env\|pyembed/python" tauri.conf.json scripts/ src-tauri/` → no live references.
- [ ] **Step 3: Ship** — push → PR (Commits table T1-T4) → rebase-merge → sync → cleanup → reindex.
- [ ] **Step 4: Post-merge soak (manual, in PR checklist):** rebuild + restart; the boot log must show **NO** `Initializing memU bridge`, **NO** `memu_bridge::stderr`, **NO** `FastEmbed model loaded`, **NO** Python subprocess. `/v1/embeddings` still serves (in-process); recall + extraction still work. → **zero Python confirmed.**

---

## Self-Review

- **Spec coverage:** module+adapter+pyembed+script delete (T4), 9 fields (T2), boot (T3), 3 frontend-facing stubbed + eval/dev trimmed (T1), config+bundle (T4), verify (T5). ✓
- **Keep-list honored:** OnnxEmbedder/deps, `/v1/embeddings`+`memu_embed_text`, `:7337` routing, `map_memu_type_to_kind`, gbrain/Bun — none deleted; only comments updated. ✓
- **Each commit compiles:** usages removed (T1 commands → T2 fields → T3 boot) before the module delete (T4); compiler enumerates ripples. ✓
- **No placeholders:** stub code given inline; deletion targets file:line-keyed; the AppState/try_init_memu ordering coupling is flagged with a merge note. ✓
- **Finish-line:** after T4, zero `crate::memu`, zero Python, bridge never spawns. Frontend memU UI removal is the documented follow-up (not a half-cut — the backend is fully gone; the FE just shows a stable offline stub until its own PR). ✓
