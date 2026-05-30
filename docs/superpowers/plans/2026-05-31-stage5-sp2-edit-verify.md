# 阶段 5 SP2 — Edit Verify: Read-back + Incremental Structured Lint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add two post-edit verification signals to the builtin `edit`/`write_file` path: (1) **read-back byte-compare** — after writing, re-read the file and confirm the intended bytes landed (catch silent write failures); (2) **incremental structured-format lint** — for JSON/YAML/TOML files, parse pre- and post-edit content in-process and flag only *newly-introduced* parse breakage ("your edit made this file invalid"), as an advisory attached to the tool output.

**Scope (locked):** read-back + in-process structured lint. **Deferred** (shares the LSP-deferral rationale — project-scoped, slow, no tree-sitter precursor): code-level linting (clippy/tsc/eslint/ruff) + semantic LSP-delta diagnostics → a dedicated future effort.

**Architecture:** New `agent/tools/builtin/edit_verify.rs` — two pure-ish helpers: `read_back_verify(path, expected) -> Result<(), VerifyError>` (re-read + byte-compare) and `incremental_structured_lint(path, pre, post) -> Option<LintFinding>` (extension → serde parse of pre+post → new-error delta). Wired into `edit.rs`'s apply path: read-back is a **hard error** on mismatch (silent-failure guard); lint is **advisory** (attached to `EditResult`, never fails the edit).

**Tech Stack:** Rust, `serde_json`/`serde_yml`/`toml` (all present), `tokio::fs`, `anyhow`/`tracing`. No new deps.

---

## Source-of-truth references

- `agent/tools/builtin/edit.rs` — apply path writes via `fs::write(&full_path, &content)` (~line 115 in `execute_single_file`; also `apply_validated_single_file` ~414). `EditResult::Applied { path, diff, edit_count }`. SP1's fuzzy chain produces `content`. After the write is where read-back + lint hook.
- hermes `tools/file_operations.py` — `_lint_json_inproc` (379), `_lint_yaml_inproc`, `_lint_toml_inproc` (trivial: parse, return ok/err-with-line/col). Port the JSON/YAML/TOML ones (skip `_lint_python_inproc` — needs Python `compile`; uClaw has no equivalent for arbitrary code without tree-sitter). The baseline/delta idea (only report NEW errors) is the "incremental" part.
- Workspace deps: `serde_json = "1"`, `toml = "0.8"`, `serde_yml = "0.0.12"` (src-tauri/Cargo.toml). Use these for in-process parse.
- `agent/tools/builtin/edit.rs` `EditResult` / the tool's output struct — where to attach the advisory lint finding (a new optional field, surfaced in the tool result text the model sees).

---

## CRITICAL facts

1. **read-back is a hard error; lint is advisory.** A read-back mismatch means the write silently failed (encoding/partial/permission) → return an error so the model knows the edit didn't land. A lint finding means the edit produced malformed JSON/YAML/TOML → attach it as a warning to the successful `Applied` result (the edit DID land; the model is told it may be broken). Never fail an edit because of a lint finding (the model may be mid-multi-edit).
2. **Incremental = only newly-introduced errors.** Lint the PRE-edit content too. If pre was already invalid (the file was broken before this edit), do NOT report it (not this edit's fault). Report only when post is invalid AND (pre was valid OR pre's error is different). This avoids nagging about pre-existing breakage — the gap-audit's "过滤既有错误,只报本次引入的".
3. **Structured formats only.** JSON (`.json`), YAML (`.yaml`/`.yml`), TOML (`.toml`). Other extensions → no lint (return None). Code files (.rs/.ts/.py) are NOT linted here — that's the deferred LSP/code-lint effort. No tree-sitter in uClaw → no cheap code-syntax check.
4. **Hot-path-safe.** Both signals are file-scoped + in-process (re-read + parse — microseconds). No subprocess spawn, no toolchain dependency, no project scan. Safe to run synchronously in the edit apply path.
5. **No tool-schema change.** `edit`/`write_file` inputs unchanged. Output gains an optional advisory lint field; read-back failure uses the existing error channel.

---

## File Structure

| File | New/Mod | Purpose | LoC |
|---|---|---|---|
| `agent/tools/builtin/edit_verify.rs` | new | `read_back_verify` + `incremental_structured_lint` + `LintFinding`/`VerifyError` + ~14 tests | ~320 (incl. ~180 tests) |
| `agent/tools/builtin/edit.rs` | mod | after each write: `read_back_verify` (hard err) + `incremental_structured_lint` (advisory → EditResult); thread the pre-edit `content` (already read as `original`/`content` before the splice) into the lint | ~+40 |
| `agent/tools/builtin/mod.rs` | mod | `mod edit_verify;` | +1 |

Est. ~220 source + ~180 tests.

---

## Adaptation responsibilities

1. **Read edit.rs's apply sites** — both `execute_single_file` (~115) and `apply_validated_single_file` (~414) write. The pre-edit content is available (read as `original`/`content` before the splice). Pass `(path, pre_content, post_content)` to the lint after the write; `read_back_verify(path, post_content)` after the write.
2. **read_back_verify** — `let actual = tokio::fs::read_to_string(path).await?; if actual != expected { Err(VerifyError::ReadBackMismatch { path, expected_len, actual_len }) }`. (Compare full strings; the error reports lengths + a short divergence hint, not the full content.) On a binary/non-UTF8 file the write path already uses `read_to_string` so this matches.
3. **incremental_structured_lint** — match `path.extension()`:
   - `json` → `serde_json::from_str::<serde_json::Value>(post)`; if Err and `serde_json::from_str(pre).is_ok()` (or pre's err msg differs) → `Some(LintFinding { format: "json", message: <err with line/col> })`.
   - `yaml`/`yml` → `serde_yml::from_str::<serde_yml::Value>`.
   - `toml` → `toml::from_str::<toml::Value>`.
   - else → `None`.
   "Different error" check: compare the two parse-error strings; if pre is also invalid with the *same* message, suppress (pre-existing).
4. **LintFinding surfacing** — add `lint_warning: Option<String>` to `EditResult::Applied` (or the tool's output struct) and include it in the text the tool returns to the model (e.g. append `\n⚠ lint: {message}`). Verify how `EditResult` renders to the model's tool-result.
5. **read-back failure channel** — return the existing `ToolError` variant for an I/O/verify failure (so the model sees "edit failed: read-back mismatch — the write did not persist"). The checkpoint (SP3) already snapshotted pre-edit state, so a failed edit is recoverable.
6. **Multi-edit batches** — if `edit` applies multiple edits to one file sequentially, read-back + lint once after the final write (or per-write — per-write read-back is safer for catching which edit broke it; lint once at the end is fine since it's whole-file). Keep it simple: verify after the file's final write.
7. **Pre-commit hooks** — no `--no-verify`.

---

## Tasks

### Task 1: `edit_verify.rs` — read-back + structured lint + tests

- [ ] **Step 1: Write failing tests** (pure-fn, no edit.rs yet): 
  - `read_back_verify`: matching content → Ok; tampered/short content → Err (use a temp file written then compared).
  - `incremental_structured_lint`: valid→invalid JSON → Some (new error); valid→valid → None; **invalid→invalid (pre-existing) → None** (the incremental filter); valid→invalid YAML → Some; valid→invalid TOML → Some; non-structured extension (.rs) → None; pre-invalid→post-valid (edit FIXED it) → None.
  (~14 tests.)

- [ ] **Step 2: Run → fail.** `cd src-tauri && cargo test --lib agent::tools::builtin::edit_verify 2>&1 | tail`

- [ ] **Step 3: Implement** `edit_verify.rs`:
```rust
// SPDX-License-Identifier: Apache-2.0
//! Post-edit verification (阶段 5 SP2): read-back byte-compare (catch silent
//! write failures) + incremental structured-format lint (JSON/YAML/TOML —
//! report only NEWLY-introduced parse breakage). Code-level / semantic lint
//! (clippy/tsc/LSP) is a deferred effort (project-scoped; no tree-sitter).

pub struct LintFinding { pub format: &'static str, pub message: String }

/// Re-read `path` and confirm it equals `expected`. Err on mismatch (the
/// write silently failed).
pub async fn read_back_verify(path: &std::path::Path, expected: &str) -> anyhow::Result<()> { /* ... */ }

/// In-process incremental structured lint. Returns a finding ONLY when the
/// post-edit content is invalid AND that breakage is newly introduced by
/// this edit (pre was valid, or had a different error). None for valid
/// content, pre-existing breakage, or non-structured extensions.
pub fn incremental_structured_lint(path: &std::path::Path, pre: &str, post: &str) -> Option<LintFinding> { /* ... */ }

// private: lint_json/lint_yaml/lint_toml (parse → Result<(), String>), is_newly_broken(pre_err, post_err)
```

- [ ] **Step 4: Run → pass + wire `mod edit_verify;`. Commit.**
```bash
cd src-tauri && cargo test --lib agent::tools::builtin::edit_verify 2>&1 | tail
git add src-tauri/src/agent/tools/builtin/edit_verify.rs src-tauri/src/agent/tools/builtin/mod.rs
git commit -m "feat(agent): edit_verify — read-back + incremental structured lint (SP2.1 of 阶段 5)"
```

### Task 2: wire into edit.rs

- [ ] **Step 1: Read edit.rs apply sites** (execute ~115, apply_validated ~414) + the `EditResult` shape + how it renders to the model.

- [ ] **Step 2: Add `lint_warning: Option<String>`** to `EditResult::Applied` (and any constructor). Surface it in the model-facing tool result text.

- [ ] **Step 3: After each write**, in the apply path:
```rust
fs::write(&full_path, &content).await.map_err(...)?;
// SP2: read-back (hard error on silent-write-failure)
edit_verify::read_back_verify(&full_path, &content).await
    .map_err(|e| ToolError::Execution(format!("read-back verify failed: {e}")))?;
// SP2: incremental structured lint (advisory)
let lint_warning = edit_verify::incremental_structured_lint(&full_path, &original, &content)
    .map(|f| format!("{}: {}", f.format, f.message));
```
Thread `lint_warning` into the `Applied` result. `original` is the pre-edit content (already read).

- [ ] **Step 4: Integration tests** in edit.rs: (a) a normal edit applies + read-back passes (no regression); (b) an edit that breaks a `.json` file → `Applied` with a `lint_warning` (edit still succeeds); (c) an edit to an already-broken `.json` that stays broken → NO lint_warning (incremental filter). (~3 tests.)

- [ ] **Step 5: Run + build + commit.**
```bash
cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail && cargo build 2>&1 | grep -E "^error" | head
git add src-tauri/src/agent/tools/builtin/edit.rs
git commit -m "feat(agent): wire read-back + lint into the edit tool (SP2.2 of 阶段 5)"
```

### Task 3: Verification

- [ ] `cd src-tauri && cargo test --lib agent::tools::builtin::edit_verify 2>&1 | tail` (~14 pass).
- [ ] `cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail` (existing + 3 new pass).
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cd src-tauri && cargo test --lib agent 2>&1 | tail -5` (broader green; 2 pre-existing failures unchanged).
- [ ] `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "edit_verify|edit\.rs" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty — serde deps already present).
- [ ] **Incremental sanity**: the invalid→invalid test confirms pre-existing breakage is NOT reported.
- [ ] **No-regression**: a normal valid edit produces no lint_warning + read-back passes.

---

## Self-Review

- ✅ Spec coverage: read-back byte-compare (silent-failure guard), incremental structured lint (JSON/YAML/TOML, new-errors-only). Code/semantic lint deferred (documented, shares LSP rationale).
- ✅ No placeholders — the lint bodies are "serde parse pre+post, delta" with the exact deps + the incremental filter spec.
- ✅ Type consistency: `read_back_verify(&Path, &str) -> anyhow::Result<()>`, `incremental_structured_lint(&Path, &str, &str) -> Option<LintFinding>`, `LintFinding { format, message }`, `EditResult::Applied { ..., lint_warning: Option<String> }` consistent.
- ✅ Risk-scaled: touches the edit apply path but both signals are hot-path-safe (in-process, file-scoped); read-back is a hard guard (with SP3 checkpoint as the recovery net), lint is advisory (never fails the edit).
- Decisions: structured-format in-process lint only (hot-path-safe, dep-free); code/LSP semantic lint deferred; read-back hard-error + lint advisory; incremental = suppress pre-existing breakage.
