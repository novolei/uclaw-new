# Item 2 — Project-Check-on-Edit Signal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Realize the deferred "semantic diagnostics on edit" (阶段 5 SP2's 3rd signal) as a **lightweight, config-gated, time-boxed project-check** — NOT a full LSP-client subsystem. After an edit to a code file, optionally run the project's check command for that file type (`cargo check`/`tsc --noEmit`/`ruff`/`py_compile`), parse diagnostics, surface errors touching the edited file as an advisory (like SP2's structured lint). Best-effort + short-timeout: fast file-scoped checkers report; slow whole-project checks time out + skip. Default OFF (opt-in) — the agent already runs checks itself; this is an early-warning convenience for users who want it.

**Why not LSP:** hermes's LSP client is ~4,263 LoC / 11 files + ongoing per-platform language-server install/lifecycle maintenance — disproportionate for uClaw's lightweight everyday/vibe-coding philosophy (decision recorded; full LSP is "only if pivoting to a heavy coding agent").

**Architecture:** A 3rd signal in `edit_verify.rs`: `project_check(path, workspace_root, cfg) -> Option<CheckFinding>` (async, shells out time-boxed). Extension → check command (config-overridable map). Run via `tokio::process::Command` with a timeout; parse JSON diagnostics (`cargo --message-format=json`, `ruff --output-format=json`) or exit-code+stderr; report diagnostics whose file == the edited file. Folded into the existing `Applied { lint_warning }` advisory channel. Gated by `edit_project_check_enabled` config (default false) + `edit_project_check_timeout_secs` (default 5).

**Tech Stack:** Rust, `tokio::process::Command` + `tokio::time::timeout`, `serde_json` (diagnostic parse), config via memubot_config. No new deps, no LSP subsystem.

---

## Source-of-truth references

- `agent/tools/builtin/edit_verify.rs` — `read_back_verify` (34), `incremental_structured_lint` (104), `LintFinding { format, message }` (15). Add `project_check` + `CheckFinding` here as signal 3.
- `agent/tools/builtin/edit.rs` — `ReadFileTool`/`EditTool` with `workspace_root` (19); `EditResult::Applied { path, diff, edit_count, lint_warning: Option<String> }` (77); the verify hook after write (read_back 284, structured_lint 289, attach 304); a second apply site `apply_validated_single_file` (~510). The check finding folds into `lint_warning` (append) OR a parallel `check_warning` — prefer appending to the existing advisory string to avoid a schema change.
- `memubot_config.rs` — config home (PR16 added `embed_timeout_secs`/`recall_semantic_max_scan` to existing structs the same way). Add `edit_project_check_enabled: bool` + `edit_project_check_timeout_secs: u64` (+ optional `edit_project_check_commands: HashMap<String,String>` for ext→cmd override) with `#[serde(default = "fn")]` + manual-Default values.
- `agent/code_checkpoint.rs` / `shell.rs` — the `tokio::process::Command` + timeout shell-out idiom to mirror (env, output capture, kill-on-timeout).
- hermes `agent/lsp/` — NOT ported (the decision). Reference only for the diagnostic-shape idea.

---

## CRITICAL facts

1. **Default OFF + best-effort + time-boxed** — `cargo check`/`tsc` are whole-project + slow (seconds-minutes); running inline on every edit would make the agent unusable. So: config-gated (default false), a short timeout (default 5s) that SKIPS (returns None) when exceeded, and any error/absent-tool → None. The timeout is self-regulating: fast file-scoped checkers (ruff, py_compile) complete + report; slow whole-project checks time out + skip silently. NEVER block or fail the edit.
2. **Advisory only** — like SP2's structured lint, a finding is appended to the `Applied` result's advisory text; it NEVER turns an edit into an error. The edit already landed.
3. **File-scoped reporting** — parse the checker's diagnostics + report only those whose file path == the edited file (cargo/tsc emit whole-project diagnostics; filter to the edited file so the model isn't spammed with unrelated errors).
4. **No baseline double-run** — running the check twice (pre+post) to diff is too expensive. Report post-edit diagnostics in the edited file with a clear "(may include pre-existing)" note. The model can tell from context; cheap + honest. (A future enhancement could cache a pre-edit baseline.)
5. **No new subsystem** — pure shell-out + JSON parse in `edit_verify.rs`. No language-server spawn/lifecycle/install. If the toolchain isn't installed, the spawn fails → None (graceful).

---

## File Structure

| File | Mod | Change | LoC |
|---|---|---|---|
| `agent/tools/builtin/edit_verify.rs` | mod | `project_check(path, workspace_root, cfg) -> Option<CheckFinding>` + `CheckFinding` + per-language runners (cargo/ruff/py_compile/tsc) + JSON-diagnostic parse + tests | ~+260 (incl. ~140 tests) |
| `memubot_config.rs` | mod | `edit_project_check_enabled` (default false) + `edit_project_check_timeout_secs` (default 5) + serde defaults + Default impl + 2 tests | ~+30 |
| `agent/tools/builtin/edit.rs` | mod | after structured_lint: if config enabled, `project_check(...)` → append to the advisory; thread the config (the tool gets it at construction or reads a global) | ~+40 |

Est. ~200 source + ~140 tests.

---

## Adaptation responsibilities

1. **`CheckFinding { language: &'static str, message: String }`** — `message` = the formatted diagnostics (file:line: msg, capped to ~N lines so it doesn't flood the advisory).
2. **Per-language runners** (best-effort, time-boxed via `tokio::time::timeout`):
   - `.rs` → `cargo check --message-format=json` in `workspace_root` (parse the JSON lines for `level=="error"` diagnostics whose `spans[].file_name` == the edited file). Will usually time out on real crates → None (acceptable; documented).
   - `.py` → `ruff check --output-format=json <file>` if `ruff` present, else `python3 -m py_compile <file>` (fast, file-scoped — the real value).
   - `.ts`/`.tsx`/`.js` → `tsc --noEmit` if a `tsconfig.json` exists (whole-project, likely times out → None) — or skip if no tsconfig. (Honest: TS rarely reports within the budget; document it.)
   - other extensions → None.
   Make the ext→command map config-overridable if cheap (`edit_project_check_commands`), else hardcode + note.
3. **Timeout + kill** — `tokio::time::timeout(Duration::from_secs(cfg.timeout), child-output)`; on timeout, kill the child (kill_on_drop or start_kill) + return None. Mirror the checkpoint/shell idiom.
4. **Config threading (VERIFIED seam — follow exactly):** Config home is `MemoryOsConfig` (PR16's home), reachable at the runtime tool-registration sites via `state.memubot_config.read().await.memory_os.<field>`. KEEP `EditTool::new(workspace)` unchanged (default = project-check disabled) so the 10+ test/descriptor call sites need ZERO churn. Add a field `project_check: Option<ProjectCheckCfg>` (None = disabled) + a builder `pub fn with_project_check(mut self, enabled: bool, timeout_secs: u64) -> Self` (sets `Some(ProjectCheckCfg { timeout_secs })` only when `enabled`, else leaves None). Wire ONLY the 2 runtime sites — `tauri_commands.rs:10719` and `:15250` — to read the two `memory_os` fields and append `.with_project_check(enabled, timeout)` to the existing `EditTool::new(workspace.clone())`. (Site 15250 is inside an `AgentTeamOrchestrator` closure — capture the two config values BEFORE the closure if `state` isn't in scope there; recon the closure's captures.) Default-off everywhere → `project_check` is gated on `self.project_check.is_some()`, so unconfigured/test paths never spawn a check (zero cost).
5. **Gate placement** — in `edit.rs`, only call `project_check` when `enabled` (else skip entirely — zero latency by default). Append its finding to the existing `lint_warning` advisory (e.g. `format!("{existing}\n⚠ check: {msg}")`).
6. **Tests** — pure-ish where possible: a `parse_cargo_diagnostics(json, edited_file) -> Vec<String>` pure fn (unit-test with sample cargo JSON); a `parse_ruff_diagnostics`. The shell-out + timeout path: an integration test with a tiny temp Python file + `py_compile` (fast, present in CI) asserting a syntax error is reported + a valid file → None + a timeout case (a fake slow command) → None. ~8-10 tests. Gate Rust/cargo tests behind tool-presence checks or use the pure parse tests for cargo JSON.
7. **Pre-commit hooks** — no `--no-verify`.

---

## Tasks

### Task 1: config fields
- [ ] Add `edit_project_check_enabled: bool` (default false) + `edit_project_check_timeout_secs: u64` (default 5) to the appropriate memubot_config struct, `#[serde(default = "fn")]` + manual Default + 2 tests (defaults + backward-compat deserialize). Mirror PR16's pattern.
- [ ] Commit: `feat(config): edit_project_check_enabled + timeout config (item2.1)`

### Task 2: project_check + per-language runners + parse
- [ ] Write failing tests: `parse_cargo_diagnostics` (sample JSON → errors in the edited file only), `parse_ruff_diagnostics`, py_compile integration (temp file syntax error → CheckFinding; valid → None), timeout → None, absent-tool → None.
- [ ] Implement `project_check` + `CheckFinding` + the runners + parsers + timeout/kill in `edit_verify.rs`.
- [ ] Run + commit: `feat(agent): edit_verify project_check signal (cargo/ruff/py_compile, time-boxed) (item2.2)`

### Task 3: wire into edit.rs (gated) + runtime registration
- [ ] Add `project_check: Option<ProjectCheckCfg>` field to `EditTool` (None default in `new`) + `with_project_check(self, enabled, timeout_secs) -> Self` builder. After `incremental_structured_lint`, `if let Some(cfg) = &self.project_check { project_check(&full_path, &self.workspace_root, cfg).await }` + append the finding to `lint_warning`. Both apply sites (execute + apply_validated_single_file).
- [ ] Wire `tauri_commands.rs:10719` + `:15250`: read `state.memubot_config.read().await.memory_os.{edit_project_check_enabled, edit_project_check_timeout_secs}` (capture before the 15250 closure if needed) → `.with_project_check(enabled, timeout)`.
- [ ] Integration test (in edit.rs): `EditTool::new(ws).with_project_check(true, 10)` + a temp `.py` with a syntax error → `Applied` advisory contains the check finding; plain `new(ws)` (disabled) → no check (no latency, no finding).
- [ ] Build + commit: `feat(agent): wire project_check into edit tool (gated) (item2.3)`

### Task 4: Verification
- [ ] `cargo test --lib agent::tools::builtin::edit_verify` + `...::edit` + `memubot_config` pass.
- [ ] `cargo build` clean; `cargo test --lib agent` net green (2 pre-existing failures unchanged).
- [ ] clippy clean on the touched files; `git diff main -- Cargo.toml` empty.
- [ ] **Default-off no-latency**: confirm a normal edit with the flag off does NOT spawn any check (zero cost) — the gate short-circuits before `project_check`.
- [ ] **Best-effort**: timeout/absent-tool → None → edit unaffected (no hang, no error).

---

## Self-Review
- ✅ Spec coverage: project-check signal (config-gated, time-boxed, advisory, file-scoped, fast-checkers-report/slow-skip). LSP subsystem explicitly NOT built (decision recorded).
- ✅ No placeholders — runners + parse + timeout are concrete; the config-threading is a recon-and-match-PR16 instruction.
- ✅ Type consistency: `project_check(&Path, &Path, &cfg) -> Option<CheckFinding>`, `CheckFinding { language, message }`, folded into `Applied { lint_warning }`.
- ✅ Risk-scaled + philosophy-aligned: lightweight (no LSP), default-off (zero cost unless opted in), best-effort + time-boxed (no hot-path latency), advisory (never breaks an edit). Fits the Pi-lightweight ADR.
- Decisions: no LSP port (philosophy); default-off opt-in (latency); single post-edit run + "(may include pre-existing)" note (no expensive baseline double-run); timeout self-regulates fast-vs-slow checkers.
