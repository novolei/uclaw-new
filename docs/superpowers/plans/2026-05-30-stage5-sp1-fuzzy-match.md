# 阶段 5 SP1 — 9-Strategy Fuzzy-Match Chain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Port hermes's 9-strategy fuzzy-match chain into a pure Rust `fuzzy_match` module and route uClaw's builtin `edit` tool through it, so LLM `old_text` drift (whitespace/indent/escape/unicode) applies instead of hard-failing. Exact matches unchanged.

**Architecture:** New `agent/tools/builtin/fuzzy_match.rs` with `fuzzy_find_and_replace(content, old, new, replace_all) -> Result<FuzzyOutcome, String>` (9 strategies, first-match-wins, escape-drift guard, byte-safe position mapping). `edit.rs`'s 3 exact-find sites route through it. No tool-schema change.

**Tech Stack:** Rust, `regex` (present), optional `unicode-normalization` (verify). Reference: `/Users/ryanliu/Documents/hermes-agent/tools/fuzzy_match.py` (703 lines — port verbatim, char-indices → byte-safe spans).

---

## Source-of-truth references

- **hermes `tools/fuzzy_match.py`** (703 lines) — THE source. `fuzzy_find_and_replace` (line 50) + 9 `_strategy_*` fns + `_detect_escape_drift` + `_apply_replacements` + mapping helpers (`_build_orig_to_norm_map`, `_map_positions_norm_to_orig`, `_find_normalized_matches`, `_map_normalized_positions`, `_unicode_normalize`). Strategy order (line 73-83): exact, line_trimmed, whitespace_normalized, indentation_flexible, escape_normalized, trimmed_boundary, unicode_normalized, block_anchor, context_aware.
- `agent/tools/builtin/edit.rs` (1431 LoC) — exact-find sites: `execute_single_file` (~239 `content.find(old_text)` + ~242 `.or_else`), `validate_single_file` (~357), `apply_validated_single_file` (~414). Output `EditResult::Applied { path, diff, edit_count }`. `generate_diff`. Verify the `replace_all`/uniqueness expression.
- `agent/tools/builtin/mod.rs` (or the builtin module decl) — add `mod fuzzy_match;`.
- spec: `docs/superpowers/specs/2026-05-30-stage5-sp1-fuzzy-match-design.md`.

---

## CRITICAL facts

1. **Byte spans, not char indices.** Python indexes by char; Rust slices by byte. Every `_strategy_*` returns `Vec<(usize, usize)>` **byte ranges**. `apply_replacements` splices by byte span, **sorted descending** (so earlier splices don't shift later offsets). **Never split a UTF-8 boundary** — assert `is_char_boundary` / map via `char_indices`. The norm↔orig mapping (esp. `unicode_normalized`, where NFC changes byte length) must yield char-boundary-safe original byte spans. This is the #1 correctness risk — a bad span corrupts the file.
2. **Escape-drift guard is safety-critical** — port `_detect_escape_drift` verbatim. Non-exact strategy + `new` contains `\'`/`\"` present in `old` but absent in the matched region → `Err` (blocks corruption). Test it.
3. **First-match-wins, exact first** — exact match = identical to today (no regression). Fuzzy only kicks in when exact fails.
4. **No tool-schema change** — `edit` input/output contract unchanged; only the internal find becomes tolerant + logs the strategy.

---

## File Structure

| File | New/Mod | Purpose | LoC |
|---|---|---|---|
| `agent/tools/builtin/fuzzy_match.rs` | new | 9 strategies + escape-drift + mapping + `fuzzy_find_and_replace` + ~25 tests | ~620 (incl. ~300 tests) |
| `agent/tools/builtin/edit.rs` | mod | route 3 find sites through `fuzzy_find_and_replace`; log `strategy`; consume `new_content` | ~+40 |
| `agent/tools/builtin/mod.rs` | mod | `mod fuzzy_match;` | +1 |

---

## Adaptation responsibilities

1. **Read `fuzzy_match.py` in FULL first** — port each strategy + helper faithfully. The mapping helpers are subtle; don't paraphrase.
2. **Byte-safety** (CRITICAL) — see CRITICAL #1. Add `debug_assert!(content.is_char_boundary(start) && content.is_char_boundary(end))` before every splice. Test with CJK + emoji content.
3. **`unicode-normalization` dep** — `grep -rn "unicode-normalization\|unicode_normalization\|unicode-segmentation" Cargo.toml src-tauri/Cargo.toml`. If present, use it for `unicode_normalized` (NFC). If absent: either add `unicode-normalization` (small, ubiquitous) OR implement a reduced strategy 7 (e.g. normalize common smart-quotes `'"` → ASCII, ignore full NFC) and document the reduction. Flag the choice in the report.
4. **`regex`** — present; use for `whitespace_normalized`'s `[ \t]+` collapse.
5. **edit.rs `replace_all`** — find how the tool expresses unique-vs-all (an `edit_count`? a flag?); map to `fuzzy_find_and_replace`'s `replace_all` bool. If the tool has no replace_all concept, default `replace_all=false` (require uniqueness — matches today's single-replace + the ambiguity guard).
6. **3 find sites → shared helper** — if execute/validate/apply duplicate the find, consider a private `fn find_and_replace(content, old, new, replace_all) -> Result<(String, &str strategy, usize), EditError>` used by all three, to avoid triple divergence. Match edit.rs's error type/shape on the `Err` path (return the SAME "not found"/"ambiguous" failure the tool returns today so callers see no contract change).
7. **Logging** — `tracing::info!(strategy = %outcome.strategy, count = outcome.match_count, path = %path)` on apply, so non-exact matches are observable (the gap-audit wants "edits fail less" to be measurable).
8. **Pre-commit hooks** — no `--no-verify`. This touches `agent/` — run `gitnexus_impact` mentally / keep the change additive to edit.rs's find step only.

---

## Tasks

### Task 1: `fuzzy_match.rs` — strategies 1-5 + apply + validation + tests

- [ ] **Step 1: Read `fuzzy_match.py` fully.** Note the contract + each strategy + mapping helpers.

- [ ] **Step 2: Write failing tests** for `fuzzy_find_and_replace` covering exact (single/multi/replace_all/ambiguity), line_trimmed, whitespace_normalized, indentation_flexible, escape_normalized, validation (empty old, identical), no-match. (~12 tests.) Each constructs `content`/`old`/`new` strings + asserts `new_content`, `match_count`, `strategy`, or the `Err` message substring.

- [ ] **Step 3: Implement** the module: `FuzzyOutcome` struct, `fuzzy_find_and_replace` (validation → strategy loop → ambiguity → escape-drift hook (stub returning None until Task 2) → apply), strategies 1-5 (exact, line_trimmed, whitespace_normalized, indentation_flexible, escape_normalized), `apply_replacements` (byte-safe, descending splice), and the mapping helpers strategies 2-4 need (`build_orig_to_norm_map` / `find_normalized_matches` / `map_normalized_positions`). Byte spans throughout.

- [ ] **Step 4: Run → pass.** `cd src-tauri && cargo test --lib agent::tools::builtin::fuzzy_match 2>&1 | tail`

- [ ] **Step 5: Commit.**
```bash
git add src-tauri/src/agent/tools/builtin/fuzzy_match.rs
git commit -m "feat(agent): fuzzy_match strategies 1-5 + apply + mapping (SP1.1 of 阶段 5)"
```

### Task 2: strategies 6-9 + escape-drift guard + tests

- [ ] **Step 1: Write failing tests** for trimmed_boundary, unicode_normalized (incl. **multi-byte byte-offset safety** + CJK/emoji splice), block_anchor, context_aware, escape-drift guard (`\'`/`\"` corruption blocked), strategy precedence (exact wins). (~13 tests.)

- [ ] **Step 2: Run → fail.**

- [ ] **Step 3: Implement** strategies 6-9 + `detect_escape_drift` (verbatim from hermes) + `unicode_normalize` (per adaptation #3) + `map_positions_norm_to_orig`. Wire `detect_escape_drift` into `fuzzy_find_and_replace` (replacing the Task-1 stub). Add `mod fuzzy_match;` to the builtin module decl.

- [ ] **Step 4: Run → pass + the multi-byte tests.** `cd src-tauri && cargo test --lib agent::tools::builtin::fuzzy_match 2>&1 | tail`

- [ ] **Step 5: Commit.**
```bash
git add src-tauri/src/agent/tools/builtin/fuzzy_match.rs src-tauri/src/agent/tools/builtin/mod.rs
git commit -m "feat(agent): fuzzy_match strategies 6-9 + escape-drift guard (SP1.2 of 阶段 5)"
```

### Task 3: wire into `edit.rs`

- [ ] **Step 1: Read edit.rs's 3 find sites** (execute ~239, validate ~357, apply ~414) + the `EditResult`/error shapes + how `new_text` is applied today.

- [ ] **Step 2: Route through fuzzy.** Replace the exact `content.find(old_text)` + `content.replace`/splice at each site with `fuzzy_find_and_replace(&content, old_text, new_text, replace_all)`:
  - validate site: `Err` → the tool's existing "not found"/"ambiguous" failure (same shape); `Ok` → proceed.
  - apply site: use `outcome.new_content`; `edit_count = outcome.match_count`; `tracing::info!(strategy=…)`.
  - execute site: same.
  - Prefer a shared `find_and_replace` helper (adaptation #6).

- [ ] **Step 3: Add edit.rs integration tests** — (a) a drifted `old_text` (extra leading whitespace) now applies + the file content is correct; (b) an exact `old_text` applies identically (no regression); (c) escape-drift `old_text`+`new_text` is blocked with the retry error. (~3 tests.)

- [ ] **Step 4: Run + build.** `cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail` + `cargo build 2>&1 | grep -E "^error" | head`

- [ ] **Step 5: Commit.**
```bash
git add src-tauri/src/agent/tools/builtin/edit.rs
git commit -m "feat(agent): route edit tool through fuzzy-match chain (SP1.3 of 阶段 5)"
```

### Task 4: Verification

- [ ] `cd src-tauri && cargo test --lib agent::tools::builtin::fuzzy_match 2>&1 | tail` (~25 pass).
- [ ] `cd src-tauri && cargo test --lib agent::tools::builtin::edit 2>&1 | tail` (existing + 3 new pass).
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cd src-tauri && cargo test --lib agent 2>&1 | tail -5` (broader agent tests green).
- [ ] `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "fuzzy_match|edit\.rs" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty, or only the unicode-normalization add if needed — flag it).
- [ ] **Byte-safety sanity**: confirm the multi-byte CJK/emoji splice tests pass (no panic, correct content).
- [ ] **No-regression**: exact-match edits produce identical results to pre-SP1.

---

## Self-Review

- ✅ Spec coverage: all 9 strategies (Tasks 1-2), escape-drift guard (Task 2), edit.rs integration (Task 3), no schema change.
- ✅ No placeholders — strategy bodies are "port verbatim from fuzzy_match.py" with the file + line refs + the byte-safety contract; the implementer translates Python→Rust (like PR14 ported gbrain marshalling from the typed shapes).
- ✅ Type consistency: `fuzzy_find_and_replace(&str,&str,&str,bool) -> Result<FuzzyOutcome,String>`, `FuzzyOutcome { new_content, match_count, strategy }`, strategy fns `(&str,&str)->Vec<(usize,usize)>` (byte spans), consistent across tasks.
- ✅ Risk-scaled: touches the live edit path (high blast radius) → full discipline + the byte-safety/multi-byte tests are the gate. Additive to the find step only; exact-match path unchanged.
- Decisions: byte spans (not char) for Rust safety; escape-drift verbatim; shared find helper across the 3 sites; unicode strategy uses a real NFC dep or a documented reduction.
