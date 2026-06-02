# Step 3b-4 — memU Teardown (zero Python) Design

**Date:** 2026-06-02
**Status:** Design (recon complete; pending spec review → plan)
**Part of:** Memory two-layer finish-line (ADR `2026-06-01-memory-two-layer-terminal-state.md`), Step 3 (remove memU) — the FINAL sub-slice. Follows 3b-1 (embedder), 3b-2 (recall), 3b-3 (extraction) + the OnnxEmbedder truncation hotfix. Soak-validated 2026-06-02 (0 Add_1, recall + proactive extraction working, live `/v1/embeddings` >512 input returns a vector). After 3b-4: **zero Python, zero external embedding runtime** (Bun/gbrain removal is the separate Step 2).

## Problem

The production memory pipeline is fully off memU (embedding → in-process OnnxEmbedder; recall → bucket_seal; write/extraction → native MemoryExtractor). What remains is dead weight: the `MemUClient`/`MemUBridge`/Python bridge, the embedded Python runtime, unused `memu_client` struct fields, three diagnostic/dev callers, and the `MemUAdapter`. This slice deletes all of it. Pure deletion — no behavior change to the live pipeline.

## Decisions (clear defaults from recon — no forks)

- **Keep `/v1/embeddings` + `memu_embed_text`** — both are in-process (Step 3b-1, backed by `state.bucket_seal_embedder`); only their dead `memu_client` field drops. (`memu_embed_text`'s name is now a misnomer but renaming is cosmetic; defer.)
- **Keep the `:7337` factory routing** (`build_embedder`: `base_url.contains(":7337")` → `OnnxEmbedder`). It now means "the local in-process endpoint," and gbrain (Step 2, still live) calls `localhost:7337/v1/embeddings`. Only update the stale "memU default" comment. A cleaner routing (sentinel vs port-string) is a Step-2-era cleanup.
- **`EmbeddingEndpointConfig`**: KEEP `base_url` (routing + remote), `dimensions` (OnnxEmbedder init), `embed_timeout_secs` (remote OpenAiCompat); DELETE `model` + `fastembed_model` (memU-only, OnnxEmbedder hardcodes bge-small). Update default comments.
- **`memu.db`**: leave on disk (no Rust writer remains; harmless). No migration needed (data already mirrored to bucket_seal/memory_graph; episodic in bucket_seal).
- **Eval probe + dev trigger** (`probe_memu_write_recall`, `dev_trigger_proactive`): DELETE (dev/eval-only; they call memU directly). Simplify/trim the enclosing eval-harness command rather than repoint.
- **`.claude/hooks/check-memory-graph.sh`** (per-user, gitignored): remove locally — out of the committed surface; note in the PR.

## Delete-list (recon, file:line keyed)

**Files/dirs:**
- `src-tauri/src/memu/` (whole module: `mod.rs`, `bridge.rs`, `client.rs`, `memu_bridge.py`, `__pycache__/`)
- `src-tauri/src/memory_adapter/memu.rs` (the `MemUAdapter`)
- `pyembed/` (root) + `src-tauri/pyembed/` (symlinks/structure)
- `scripts/setup-python-env.sh` (memU/FastEmbed env setup)

**Struct fields (`Option<Arc<MemUClient>>`, now unused) + their `new()`/setter init:**
`AppState` (`app.rs:208`), `MemorizationService` (`service.rs:56` + `set_memu_client` `:150-154`), `SkillSearchTool` (`skill_search.rs:54` + `with_memu` `:73-75`), `LocalApiService` (`server.rs:53`), `ApiState` (`routes.rs:21`), `HybridSearchEngine` (`hybrid_search.rs:254`), `MemoryOsRuntimeConfig` + `ProactiveService` + `ProactiveRecallService` (`proactive/service.rs:639,650`; `proactive_recall.rs:73`). Removing these ripples through their constructors — drop the param at every call site (mostly `None` / `state.memu_client` clones).

**Boot wiring:**
- `app.rs`: `fn try_init_memu` (~`:1361-1404`), its call (`:645`), the `memu_client` field in `Self{}`, the `MemUAdapter::new` registration (`:1135-1140`) + `memory_adapters_map` insert, all `MEMU_LLM_*`/`FASTEMBED_MODEL`/`MEMU_*` env construction.
- `main.rs`: the `set_app_handle` block (`:125-131`), `memu_client` clone (`:209`), `set_memu_client` call (`:271`), shutdown block (`:982,996-1000`).

**Tauri commands (fn + `invoke_handler!` macro entry in main.rs):**
- DELETE `restart_memu_bridge` (cmd + macro `:1288`), `get_memu_status` (cmd + macro), the memU section of `get_system_diagnostics` + `MemUBridgeStatus` struct (stub the diagnostics field or drop it — check frontend usage).
- DELETE `probe_memu_write_recall` + trim its caller in the memory eval-harness command.
- DELETE `dev_trigger_proactive`'s `memorize_with_config` call (or the whole dev command if memU was its point).
- KEEP `memu_embed_text` (in-process) — just ensure it no longer touches memU.

**Module exports:** `memory_adapter/mod.rs` `pub mod memu;` + `pub use memu::MemUAdapter;`. `lib.rs`/`memu/mod.rs` removal. All `use crate::memu::*` imports across the ~9 files.

**Config:** `memubot_config.rs` — drop `model` + `fastembed_model` from `EmbeddingEndpointConfig` (field + Default + any DTO/serde). `tauri.conf.json` — drop `"pyembed/python"` + `"src/memu/memu_bridge.py"` bundle entries.

## Keep-list (DO NOT delete — load-bearing)

- **OnnxEmbedder + its deps**: `ort`, `tokenizers`, `ndarray`, `half` in Cargo.toml; `score/embed/onnx.rs`, `model_download.rs`, `factory.rs`.
- **`/v1/embeddings` route + `memu_embed_text` command** (in-process).
- **`EmbeddingEndpointConfig.{base_url, dimensions, embed_timeout_secs}`** + the `:7337` routing.
- **`map_memu_type_to_kind`** (`reflection.rs`) — a string-mapper for the extractor's `memory_type`; name says "memu" but it's the extractor taxonomy. KEEP.
- **bucket_seal, memory_graph, gbrain, Bun** — gbrain + Bun are Step 2, untouched here.
- **The retired `enforce_freeze` no-op + the hooks** — already handled in 3b-3.

## Verification

1. `cargo build` + `cargo clippy --lib` clean — the compiler is the primary guard (any dangling `crate::memu` ref fails the build).
2. **Gate:** `grep -rn "memu\|MemU\|MemUClient\|MemUBridge\|fastembed\|pyembed\|FASTEMBED" src-tauri/src/` returns only: `map_memu_type_to_kind` + `memu_type` metadata (extractor), `memu_embed_text` (in-process cmd, or renamed), and possibly a `memu.db` diagnostics-path string. NO `MemUClient`/`MemUBridge`/`crate::memu`/`try_init_memu`/bridge env vars.
3. `grep -rn "memu_bridge\|setup-python-env\|pyembed" tauri.conf.json scripts/ src-tauri/` → no live references (the deleted files + bundle entries gone).
4. Full test suite (`memory_graph`, `proactive`, `memorization`, `local_api`, `memory_bucket_seal`) green — no regression from field/ctor removals.
5. **Manual (post-merge soak):** app boots with **no memU bridge spawn / no Python subprocess / no FastEmbed load** in the log; `/v1/embeddings` still serves (in-process); recall + extraction still work. The boot log line `Initializing memU bridge` + `memu_bridge::stderr` lines must be GONE.

## Scope / files (summary)

Delete `src/memu/`, `memory_adapter/memu.rs`, `pyembed/`, `setup-python-env.sh`; strip `memu_client` fields + ctors across ~9 structs; delete 3 commands + macro entries + boot wiring in `app.rs`/`main.rs`; drop 2 config fields + 2 bundle entries; fix imports. 

**Out of scope:** gbrain + Bun (Step 2); the `SymphonyService` `workflow_version` bug (separate); the `:7337`-routing cleanup + `memu_embed_text` rename (cosmetic, deferrable).

## Risk

LOW — pure deletion of an already-dead path; the Rust compiler catches any dangling reference (unlike a behavior change, a missed deletion fails the build, not silently at runtime). The only judgment points are the diagnostics struct (stub vs drop — check frontend) and the eval-harness trim. Bisectable: imports+fields+ctors → commands+boot+macro → module+adapter+config+bundle → verify. Each commit compiles. After the final commit, the app has zero Python and the memU bridge never spawns.
