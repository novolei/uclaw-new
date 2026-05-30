# 阶段 5 SP4 — On-Demand Read Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Close the last 阶段 5 gap — `read_file` currently emits the WHOLE file (a large file blows the context window on the first read). Add `offset`+`limit` line-range paging + a hard ~100K-char cap with a truncation footer that steers the model to page (or grep), and tighten the `read_file`/`grep` tool descriptions with anti-pattern guidance ("prefer this over shell cat/sed/grep"). Preserve uClaw's existing anchor-token + hash-dedup machinery.

**Scope (locked):** read contract core (paging + cap + hints + descriptions). NOT in scope: ripgrep-backed grep (the hand-rolled grep keeps working); config-overridable cap (a `pub const` now; config tuning is a trivial PR16-style follow-up).

**Architecture:** Enhance `ReadFileTool::execute` (in `agent/tools/builtin/file.rs`): parse optional `offset`(1-based start line)/`limit`(max lines); `record_read` over the FULL file (anchors stay stable across paged reads), then emit only the windowed `(anchor, line)` slice; enforce `MAX_READ_CHARS` (~100K) as a hard ceiling — when reading whole + over budget, emit the fitting prefix + a truncation footer with the file's total line/char count + a "use offset/limit or grep" hint. Tighten the `read_file` + `grep` descriptions.

**Tech Stack:** Rust, existing `read_file` machinery (anchor_state, hashing). No new deps.

---

## Source-of-truth references

- `agent/tools/builtin/file.rs` — `ReadFileTool` (`name="read_file"`, line 37). `execute` (74-148): reads `content` via `fs::read_to_string`, computes `current_hash`, `assume_hash` short-circuit (110-119), else `record_read(&full_path, &lines)` → zip `anchors`+`lines` → emit `render_anchor_line` per line (120-145). `parameters_schema` (48) currently `{path, assume_hash}`. `WriteFileTool` below (162) — untouched. `path_args` (70).
- `agent/anchor_state.rs` — `GLOBAL_ANCHOR_STATE_MANAGER.record_read(path, &lines) -> Vec<anchor tokens>` (aligns tokens to the prior read via Myers diff) + `render_anchor_line(token, line)`. **record_read must see the FULL file** (so tokens are stable regardless of which window is emitted); the paging slices the EMIT, not the record.
- `agent/tools/builtin/search.rs` — the `grep` tool (`name="grep"`, line 13) + `glob` tool. `description` (14) — tighten with anti-pattern guidance.
- hermes `tools/file_tools.py` — `_DEFAULT_MAX_READ_CHARS = 100_000`, offset+limit paging, the targeted-read hint when reading whole. The contract to mirror.

---

## CRITICAL facts

1. **record_read over the FULL file, emit only the window.** `record_read(&full_path, &lines_full)` returns anchors for every line — anchor tokens MUST stay stable across reads regardless of the offset/limit window (the `edit` tool targets by anchor). Slice `anchors.iter().zip(lines).skip(offset-1).take(limit)` for the EMIT only. Do NOT record_read over a sliced `lines` (that would shift/destabilize tokens).
2. **The hash short-circuit is whole-file.** `current_hash`/`assume_hash` are computed on the full `content` — paging doesn't change that. The "no changes since last read" short-circuit stays as-is (it's a whole-file dedup, orthogonal to paging).
3. **Char cap is a hard ceiling, not just a default.** Even with an explicit `limit`, the emitted slice is capped at `MAX_READ_CHARS` (emit lines until the char budget, then the truncation footer). Prevents `limit: 100000` from blowing context. When NO offset/limit + file ≤ cap → behaves exactly as today (no regression).
4. **Truncation footer is actionable.** When truncated: `\n[truncated: shown lines {start}-{end} of {total_lines} ({shown_chars}/{total_chars} chars). Read more with offset/limit, or use grep to find specific content.]` — the model learns to page instead of re-reading whole.
5. **No tool-schema break.** `offset`/`limit` are OPTIONAL (default: whole file up to the cap). Existing callers (path-only / path+assume_hash) behave identically when the file is under the cap.

---

## File Structure

| File | Mod | Change | LoC |
|---|---|---|---|
| `agent/tools/builtin/file.rs` | mod | `read_file`: `offset`/`limit` params + `MAX_READ_CHARS` cap + windowed emit + truncation footer + description tighten + tests | ~+130 (incl. ~80 tests) |
| `agent/tools/builtin/search.rs` | mod | tighten the `grep` description (anti-pattern guidance) | ~+5 |

Est. ~140 source + ~80 tests.

---

## Adaptation responsibilities

1. **Read `read_file::execute` + `anchor_state::record_read`** — confirm record_read is over the full `lines` + returns per-line anchors; the paging slices the zip for emit only.
2. **`MAX_READ_CHARS`** — `pub const MAX_READ_CHARS: usize = 100_000;` (note in a comment: config-overridable is a follow-up). Count chars (not bytes) for the cap (`.chars().count()` or track as the emit string grows).
3. **`offset`/`limit` schema** — add to `parameters_schema`: `offset` (integer, 1-based start line, optional, default 1), `limit` (integer, max lines, optional). Validate: offset ≥ 1; offset > total_lines → emit a "file has N lines; offset M is past EOF" note (not an error). limit ≤ 0 → treat as unlimited (cap-bounded).
4. **Windowed emit + cap** — after `record_read`: `let start = offset.saturating_sub(1); let windowed = anchors.iter().zip(&lines).skip(start).take(limit_or_max)`. Build the output, tracking emitted chars; stop when `emitted_chars + next_line_len > MAX_READ_CHARS`, set a `truncated` flag + the last-emitted line index. Append the footer when truncated OR when the window didn't reach EOF (so the model knows there's more).
5. **Header + short-circuit unchanged** — the `[File Hash: 0x...]` header + the assume_hash short-circuit stay. The footer is appended after the windowed lines, before the trailing-newline trim logic.
6. **Trailing-newline shape** — preserve the existing logic, but it only applies when emitting to EOF without truncation; when truncated/windowed, the footer follows so the trailing-`\n` trim is moot (the footer is the last line).
7. **Descriptions** — `read_file`: append "Prefer this over shell `cat`/`sed`/`head`/`tail` — it gives stable edit anchors + paging." `grep`: append "Prefer this over shell `grep`/`rg` — it's workspace-scoped." Keep concise.
8. **Tests** — small file (no offset/limit) → unchanged full emit (no regression, no footer). Large file (> cap, no limit) → truncated prefix + footer with correct counts. offset+limit → correct window emitted + anchors stable (same token for a given line whether read whole or windowed). offset past EOF → note. explicit limit but over char cap → capped + footer. ~7-8 tests. Verify anchor stability: read whole, note a line's token; read the same line via offset/limit, assert the SAME token (record_read over full file → stable).
9. **Pre-commit hooks** — no `--no-verify`.

---

## Tasks

### Task 1: paging + cap + footer in read_file

- [ ] **Step 1: Read `read_file::execute` + `anchor_state::record_read`/`render_anchor_line`.**

- [ ] **Step 2: Write failing tests** in file.rs:
  - `read_small_file_unchanged_no_footer` — a 5-line file, no offset/limit → emits all 5 anchored lines, no truncation footer (byte-identical to current behavior).
  - `read_offset_limit_emits_window` — a 20-line file, `offset=5, limit=3` → emits lines 5-7 only.
  - `read_anchor_stable_across_window` — read whole, capture line 10's anchor token; read `offset=10, limit=1`, assert the same token (record_read-over-full keeps tokens stable).
  - `read_large_file_truncates_with_footer` — a file > MAX_READ_CHARS (use a small test override or a generated big string), no limit → emits a prefix + a footer containing the total line count + "offset/limit" + "grep".
  - `read_offset_past_eof_returns_note` — `offset=999` on a 10-line file → a "past EOF" note, no crash.
  - `read_explicit_limit_still_capped` — `limit=100000` on a big file → still capped at MAX_READ_CHARS + footer.
  (~7 tests.) For the cap tests, either make `MAX_READ_CHARS` test-overridable (a param/const the test can shrink) OR generate content > 100K chars. Prefer a private `read_windowed(content, offset, limit, max_chars)` helper that tests call with a small `max_chars` — keeps tests fast + the cap logic pure/unit-testable.

- [ ] **Step 3: Implement.** Extract the window+cap logic into a pure helper `fn select_window(lines: &[String], anchors: &[Token], offset: usize, limit: Option<usize>, max_chars: usize) -> (Vec<rendered_line>, Option<TruncationInfo>)` (testable without I/O), called from `execute`. Add `offset`/`limit` to the schema + parse. Build the footer from `TruncationInfo`. Keep the header + short-circuit + trailing-newline logic intact for the non-truncated whole-file path.

- [ ] **Step 4: Run → pass.** `cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail`

- [ ] **Step 5: Commit.**
```bash
git add src-tauri/src/agent/tools/builtin/file.rs
git commit -m "feat(agent): read_file offset/limit paging + 100K char cap + truncation footer (SP4.1 of 阶段 5)"
```

### Task 2: anti-pattern descriptions

- [ ] **Step 1:** Tighten `ReadFileTool::description` (file.rs:38) — append the "Prefer over shell cat/sed/head/tail; supports offset/limit paging" guidance + document the new params.

- [ ] **Step 2:** Tighten `grep` tool `description` (search.rs:14) — append "Prefer over shell grep/rg — workspace-scoped, respects the working tree."

- [ ] **Step 3:** A small test asserting the descriptions mention the params/anti-pattern (greppable contract), OR just confirm the schema includes `offset`/`limit`. Build + run.

- [ ] **Step 4: Commit.**
```bash
git add src-tauri/src/agent/tools/builtin/file.rs src-tauri/src/agent/tools/builtin/search.rs
git commit -m "feat(agent): anti-pattern read/grep tool descriptions (SP4.2 of 阶段 5)"
```

### Task 3: Verification

- [ ] `cd src-tauri && cargo test --lib agent::tools::builtin::file 2>&1 | tail` (existing + ~7 new pass).
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cd src-tauri && cargo test --lib agent 2>&1 | tail -5` (broader green; 2 pre-existing failures unchanged).
- [ ] `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "builtin/file|builtin/search" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **No-regression**: a small-file read (path only) is byte-identical to pre-SP4 (no footer, full anchored emit, hash header, short-circuit intact).
- [ ] **Anchor stability**: the `read_anchor_stable_across_window` test confirms paging doesn't destabilize edit anchors.

---

## Self-Review

- ✅ Spec coverage: offset/limit paging + ~100K cap + truncation footer/hint + anti-pattern descriptions. Ripgrep + config-cap explicitly deferred (locked scope).
- ✅ No placeholders — the window/cap logic is a named pure helper `select_window` with the exact slice + char-budget contract; footer format specified.
- ✅ Type consistency: `select_window(&[String], &[Token], usize, Option<usize>, usize) -> (Vec<String>, Option<TruncationInfo>)`, `MAX_READ_CHARS: usize`, schema `offset`/`limit` optional.
- ✅ Risk-scaled: enhances the read tool (medium blast radius); the cap/paging is additive + default-preserving (under-cap whole reads unchanged); the key risk (anchor destabilization) is gated by the stability test + the record_read-over-full rule.
- Decisions: record_read over full file (anchor stability) + slice emit; hard char cap even with explicit limit; const cap (config = follow-up); ripgrep deferred.
