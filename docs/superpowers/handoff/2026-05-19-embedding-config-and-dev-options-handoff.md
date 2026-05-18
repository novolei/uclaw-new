# Sprint 2.2 followon #4 — Embedding endpoint config UI + developer-options panel

**Status:** ready to merge (after manual round-trip verification on Mac).
**Branch:** `worktree-gbrain-embedding-settings-ui`
**Base:** `main`
**Predecessor work:** PR #214 (embeddings endpoint backend), PR #215 (browser-atoms hotfix), and the Sprint 2.2 launcher/init series (PRs #205/#207/#209/#212).

## Why this PR exists

Three user-facing pain points that PR #214 (which exposed the backend
`/v1/embeddings` endpoint but no UI) left unresolved:

1. **No UI for the gbrain embedding config dance.** After PR #214 a user
   wanting to point gbrain at the local FastEmbed endpoint had to drop
   to a terminal and run three `~/.uclaw/gbrain/run.sh config set ...`
   commands — and remember to restart memU if they were also changing
   `FASTEMBED_MODEL`. This is a fragile manual ritual that gets the
   defaults wrong on a fresh install.
2. **No visibility into which FastEmbed model memU is actually serving.**
   The bridge spawned with whatever `FASTEMBED_MODEL` env was at process
   start — there was no way for a user to inspect or change it without
   editing config files by hand.
3. **No way to re-run setup scripts from the UI.** When `bunembed/`,
   `gbrain-source/`, `pyembed/`, or the PGLite brain got into a bad
   state, the only fix was a terminal `./scripts/setup-*.sh` call.
   Dev-mode users with terminals can manage; less-technical users on
   the same machine cannot.

## What changed

| File | Diff |
|---|---|
| `src-tauri/src/memubot_config.rs` | +`EmbeddingEndpointConfig` struct (base_url + model + dimensions + fastembed_model) wired into `MemubotConfig`; +4 unit tests covering defaults, JSON round-trip, missing-field fallback, save/load |
| `src-tauri/src/tauri_commands.rs` | +`EmbeddingEndpointPayload` IPC type; +`get_embedding_config` / `set_embedding_config` (gbrain-CLI-first, then-persist ordering so a partial failure can't poison config); +`SETUP_SCRIPT_ALLOWLIST` const; +`RunSetupScriptArgs/Result` types; +`run_setup_script` command (line-streaming stdout/stderr via Tauri events, `kill_on_drop`, 4-script allowlist); +`setup_script_tests` mod |
| `src-tauri/src/app.rs` | `try_init_memu` now injects `FASTEMBED_MODEL` from `MemubotConfig::load(...).embedding_endpoint.fastembed_model` so the very first memU spawn respects the user's saved model |
| `src-tauri/src/main.rs` | Registered `get_embedding_config`, `set_embedding_config`, `run_setup_script` in `invoke_handler!` |
| `ui/src/lib/embedding-endpoint.ts` | New shared TS module — types mirror Rust IPC shapes; `SETUP_SCRIPT_DESCRIPTORS` (label + description + supportsForce + expectedDurationSecs) drives the developer-options UI |
| `ui/src/components/settings/EmbeddingEndpointSection.tsx` | 4-field form (base_url / model / dimensions / fastembed_model) with dirty-tracking, Save, Reset, error/toast feedback |
| `ui/src/components/settings/DeveloperOptionsSection.tsx` | Collapsible dev-only panel with one ScriptCard per allowlist entry. Live stdout/stderr log tail (capped 500 lines), calibrated progress bar, `--force` confirmation gate for `init-gbrain` |
| `ui/src/components/settings/SystemTab.tsx` | Mount the two new sections between the diagnostic recovery actions and the report footer |

Net: ~750 lines added, 1 line removed, across 8 files. Six new unit tests
(4 schema + 2 allowlist) bring the backend test suite to 1381 passing.

## Defaults (out-of-the-box behavior)

`EmbeddingEndpointConfig::default()`:

- `base_url` = `http://localhost:7337/v1` — uClaw's own LocalApi server
  (`LocalApiConfig::default().port`), which PR #214 wired to a
  FastEmbed-backed `/v1/embeddings` route.
- `model` = `llama-server:bge-small-en-v1.5` — the `llama-server` recipe
  is the gbrain side that knows how to call our endpoint; the suffix is
  what the endpoint actually serves.
- `dimensions` = `384` — bge-small-en-v1.5 output width; must match
  whatever `fastembed_model` produces.
- `fastembed_model` = `BAAI/bge-small-en-v1.5` — the FastEmbed model id
  memU loads. To go multilingual, change to `BAAI/bge-m3` (1024 dim) and
  bump `dimensions` to 1024 in the same Save.

A fresh install — even before the user opens SystemTab — runs with these
defaults end-to-end, because Task 2's `try_init_memu` injection writes
`FASTEMBED_MODEL` into the memU bridge env at spawn time.

## Security posture for `run_setup_script`

`SETUP_SCRIPT_ALLOWLIST` is a hardcoded `&'static [&'static str]`
containing exactly four script names. Anything outside this set is
rejected before spawn. The pinning test
`allowlist_contains_exactly_the_four_documented_scripts` fails the build
if the list silently changes.

Argv is built from a fixed shape — `bash <resolved-path> --yes [--force
for init-gbrain only]` — and user-supplied strings are passed via
`Command::arg`, never interpolated into a shell string. The dev-only
gate is structural: the `scripts/` directory is not bundled into release
builds (`tauri.conf.json` doesn't list it as a resource), so the
`script_path.is_file()` check returns false and the command errors out
on a release install.

`kill_on_drop(true)` ensures that if the Tauri command future is dropped
(window close, app quit, frontend disconnect), the spawned bash child
gets SIGKILL — no orphan setup runs.

## Manual verification

Backend already green (`cargo test --lib` = 1381 passed, full
`cargo build` clean). For the UI round-trip:

1. `cargo tauri dev`, open Settings → System.
2. Scroll past "恢复操作"; you should see two new sections:
   - **Embedding 端点配置** (always visible). Confirm the four fields
     are pre-populated with the defaults above.
   - **开发者选项 [DEV]** (collapsed). Expand and confirm four
     ScriptCards appear in order: `setup-bun-runtime`,
     `setup-gbrain-source`, `setup-python-env`, `init-gbrain`. Only
     `init-gbrain` shows the red "重置" (--force) button.
3. **Embedding section save round-trip:**
   - Change `dimensions` from `384` to `384` (no-op) → "保存" should be
     disabled (`dirty` is false).
   - Change `base_url` to anything → "保存" enables. Hit it. Toast
     "已保存。如修改了 FastEmbed 模型，memU 已自动重启。" appears.
   - In a terminal: `~/.uclaw/gbrain/run.sh config get base_urls.llama-server`
     should show the new value. Revert in the UI and confirm again.
4. **`init-gbrain` no-op run:**
   - Expand "开发者选项", click "运行" on the **init-gbrain** card.
   - Progress bar should tick from 1% to ~95% (capped), then snap to
     100% green on completion.
   - The collapsible 输出日志 should show the script's stdout — for a
     brain that's already initialized, expect a short "已初始化" /
     "Already initialized" message and a `exit 0`.
5. **`init-gbrain --force` confirmation gate:**
   - Click the red "重置" button on the init-gbrain card.
   - The red confirmation banner should appear with "确认重置" / "取消".
   - Hit "取消" — banner disappears, nothing runs. (Do NOT run "确认重置"
     unless you actually want to nuke the brain.)

## Adjacent edits worth flagging in review

- `tauri_commands.rs` got a third top-level command registered in
  `main.rs::invoke_handler!`. Per CLAUDE.md this is an adjacent edit
  pattern, not scope creep — IPC commands MUST be in both files or
  fail at runtime.
- `app.rs::try_init_memu` reads `MemubotConfig::load(data_dir)` a
  second time within startup (the first load happens later in
  `AppState::initialize`). This is an extra disk read on cold start.
  Code-quality review flagged this as an `Important` follow-up — fix
  would require either passing `fastembed_model` as a `try_init_memu`
  parameter or reordering boot. Not blocking; tracked for a follow-on.

## Files index

- Plan: `docs/superpowers/plans/2026-05-19-embedding-config-and-dev-options.md`
- Predecessor handoff: `docs/superpowers/handoff/2026-05-19-gbrain-embeddings-endpoint-handoff.md`
- Bisectable commits on this branch (oldest → newest):
  - `2735e4a` — Task 1: EmbeddingEndpointConfig schema
  - `a0ea862` — Task 1 fix: default port 27270 → 7337
  - `9b37905` — Task 2: get/set_embedding_config IPCs + FASTEMBED_MODEL injection
  - `49f9b26` — Task 2 fix: gbrain-CLI-before-persist ordering
  - `8f9443e` — Task 3: run_setup_script with allowlist + event streaming
  - `ea3a498` — Task 4: embedding-endpoint TS types + helpers
  - `8fd1dad` — Task 5: EmbeddingEndpointSection component
  - `45a7ad2` — Task 6: DeveloperOptionsSection component
  - `65b1940` — Task 7: Mount sections in SystemTab
  - (this commit) — Task 8: handoff doc
