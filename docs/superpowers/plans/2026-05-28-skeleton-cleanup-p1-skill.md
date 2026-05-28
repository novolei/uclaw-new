# Skeleton Cleanup P1 — Skill Dead Code Kill · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove ~425 LoC of dead/unreachable code in the skill subsystem (`agent/skill_selection/` module + 2 dead renderers in `skills.rs` + 2 stranded fields in `token_budget/snapshot.rs`) with zero functional change to the live skill/prompt path.

**Architecture:** Pure deletion. No new code, no new tests, no abstraction change. The deletion targets are independently verified to have zero non-test callers. Live skill path (`SkillsRegistry::load_*` → `match_skills` + `skills_manifest::build_skills_manifest` → `format_for_system_prompt_xml` → `ChatDelegate::set_skills_manifest_block`) is untouched. TDD discipline takes the form: **grep-verify zero callers (red gate) → delete → `cargo build` + `cargo test --lib agent::` (green gate)**.

**Tech Stack:** Rust 2021, Tauri 2, `cargo`, `grep` / `ripgrep` for caller verification.

---

## Background facts verified against current code (HEAD `7805e3ca` on main, 2026-05-28)

These anchors come from the triage subagent's analysis (`docs/superpowers/specs/2026-05-28-skeleton-cleanup-assessment.md` §1.A). Re-verify at task start; line numbers should be stable (post-assessment-doc, no code change).

### Target 1 — `agent/skill_selection/` module (393 LoC, 1 commit old)

- Files:
  - `src-tauri/src/agent/skill_selection/select.rs` (362 LoC; contains `select_top_k` fn + `SkillCandidate`/`SelectionQuery`/`SelectionStats` structs + `DEFAULT_TOP_K`/`DEFAULT_METADATA_BUDGET_TOKENS` consts + tests)
  - `src-tauri/src/agent/skill_selection/mod.rs` (31 LoC; re-exports the public surface)
- Module declared at `src-tauri/src/agent/mod.rs:28` (`pub mod skill_selection;`)
- **Live callers (non-test) — verified zero** by subagent grep. Only references outside the module are:
  - `agent/mod.rs:28` (pub mod declaration — not a call)
  - `agent/token_budget/snapshot.rs:38-40` (2 stranded `u32` fields named `l3_skills_selected` / `l3_skills_dropped_for_budget` that are never assigned anywhere — see Target 3)
- All 12 calls to `select_top_k` live inside `#[cfg(test)]` blocks at `select.rs:155+`.
- Last touched: commit `76303003` (2026-05-26, the creation commit; no follow-up).
- Live replacement: `skills_manifest::build_skills_manifest` (`src-tauri/src/skills_manifest.rs:43`), called from `tauri_commands.rs`.

### Target 2 — dead skill renderers in `skills.rs` (32 LoC)

- `src-tauri/src/skills.rs:674-691` — `pub fn build_skill_prompt(&self, message: &str) -> String` (18 LoC)
- `src-tauri/src/skills.rs:694-707` — `pub fn combined_system_prompt(&self) -> String` (14 LoC)
- **Live callers — verified zero**. `grep -rn "build_skill_prompt\|combined_system_prompt"` returns only the two definition lines.
- Live replacement: `SkillsRegistry::format_for_system_prompt_xml` (`skills.rs:712`, Pi-spec `<available_skills>` XML), called from `tauri_commands.rs:2060` and `:11257` via `delegate.set_skills_manifest_block(manifest)`.

### Target 3 — stranded snapshot fields (~5 LoC)

- `src-tauri/src/agent/token_budget/snapshot.rs:38-40` — `l3_skills_selected: u32` and `l3_skills_dropped_for_budget: u32` on the `TokenBudgetSnapshot` struct.
- **Never written anywhere in production** (verified zero `.l3_skills_selected =` and `.l3_skills_dropped_for_budget =` assignments outside their own definition).
- These were the statistics fields `skill_selection::SelectionStats` would have populated.

### Live skill path that **stays intact** (do NOT touch)

| Stage | Symbol | File:line |
|---|---|---|
| Loading | `SkillsRegistry::load_*` | `skills.rs` |
| Matching | `SkillsRegistry::match_skills(&str)` | `skills.rs:600` |
| Ranking | `skills_manifest::build_skills_manifest` | `skills_manifest.rs:43` |
| Rendering | `SkillsRegistry::format_for_system_prompt_xml` | `skills.rs:712` |
| Injection | `delegate.set_skills_manifest_block(manifest)` | `dispatcher.rs:673` (caller `tauri_commands.rs:2060/11257`) |
| Token control | `skill_search_used: AtomicBool` (sticky suppression) | `dispatcher.rs:102/824` |
| On-demand retrieval | `skill_search` tool | `agent/tools/builtin/skill_search.rs` |

---

## Pre-flight (before Task 1)

1. **Confirm main HEAD baseline:** `git -C /Users/ryanliu/Documents/uclaw status -sb` → `## main...origin/main` (in sync) at SHA `7805e3ca` or later.
2. **Baseline test count:** `cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib agent:: 2>&1 | tail -5` → expect 773 passed, 2 failed (`shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long` — pre-existing, unrelated; verified earlier this branch doesn't touch those files).
3. **Re-verify file:line anchors:**
   ```bash
   grep -n "pub fn build_skill_prompt\|pub fn combined_system_prompt\|pub fn format_for_system_prompt_xml" /Users/ryanliu/Documents/uclaw/src-tauri/src/skills.rs
   grep -n "l3_skills_selected\|l3_skills_dropped_for_budget" /Users/ryanliu/Documents/uclaw/src-tauri/src/agent/token_budget/snapshot.rs
   grep -n "pub mod skill_selection" /Users/ryanliu/Documents/uclaw/src-tauri/src/agent/mod.rs
   ls /Users/ryanliu/Documents/uclaw/src-tauri/src/agent/skill_selection/
   ```
   Expected: anchors as listed above. If a line drifted slightly, note actual line; the surrounding code shape is what matters.
4. **Optional but recommended:** run `gitnexus_impact({target: "select_top_k", direction: "upstream"})` to confirm zero upstream callers via the freshly-rebuilt index (per CLAUDE.md). Expected: empty.
5. **Create worktree + branch:**
   ```bash
   git worktree add -b claude/skeleton-cleanup-p1-skill \
       /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/bunembed
   git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill status -sb
   ```
   Expected: `## claude/skeleton-cleanup-p1-skill` and 3 symlinks created (gitignored resources).

All paths in Task 1 below are relative to the worktree root: `/Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill`.

---

## File structure

### Deletions (3 files, 4 sites)

| Action | Path | LoC |
|---|---|---|
| Delete file | `src-tauri/src/agent/skill_selection/select.rs` | 362 |
| Delete file | `src-tauri/src/agent/skill_selection/mod.rs` | 31 |
| Delete dir | `src-tauri/src/agent/skill_selection/` (empty after above) | — |
| Modify file | `src-tauri/src/agent/mod.rs:28` (drop `pub mod skill_selection;`) | −1 |
| Modify file | `src-tauri/src/skills.rs:674-707` (drop 2 dead methods) | −34 |
| Modify file | `src-tauri/src/agent/token_budget/snapshot.rs:38-40` (drop 2 stranded fields + doc comment if any) | −3 to −5 |

Net: ~−425 LoC.

### Files that stay intact (no edits — verify by absence from diff)

- `src-tauri/src/skills.rs` other than the 32-line removal — `format_for_system_prompt_xml`, `match_skills`, `SkillsRegistry::load_*`, all tests, etc. stay.
- `src-tauri/src/skills_manifest.rs` — completely untouched.
- `src-tauri/src/tauri_commands.rs` — the 2 callers at `:2060/:11257` stay.
- `src-tauri/src/agent/dispatcher.rs` — `set_skills_manifest_block`, `skills_manifest_block` field, `skill_search_used` AtomicBool all stay.
- `src-tauri/src/agent/tools/builtin/skill_search.rs` — completely untouched.
- `src-tauri/src/agent/token_budget/snapshot.rs` — only the 2 stranded fields go; the rest of `TokenBudgetSnapshot` stays.

---

## Task 1: Kill the dead skill skeleton

**Files** — see "File structure" table above.

**Discipline note:** Each deletion step is gated by a grep verification immediately before it (the "red gate" substitute for failing-test-first). The final commit happens after `cargo build` + `cargo test --lib agent::` pass (the "green gate"). Steps below run from the worktree root.

### Step 1: Verify zero non-test callers for `skill_selection`

- [ ] **Step 1**

Run:
```bash
grep -rn "select_top_k\|SkillCandidate\|SelectionQuery\|SelectionStats\|DEFAULT_TOP_K\|DEFAULT_METADATA_BUDGET_TOKENS\|skill_selection::" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/ --include="*.rs" | grep -v "src/agent/skill_selection/"
```

Expected: **AT MOST** the following two lines (each is a non-call):
- `src-tauri/src/agent/mod.rs:28: pub mod skill_selection;`  *(module declaration, not a call)*
- `src-tauri/src/agent/token_budget/snapshot.rs:38-40` — `l3_skills_selected` / `l3_skills_dropped_for_budget` field names *(do NOT mention the module path; if grep returns them via the symbol match it's the stranded fields, see Step 3)*

If grep returns **any other line**, STOP — there's a hidden caller and the deletion premise is wrong. Report the call site and ask the controller before proceeding.

### Step 2: Verify zero non-test callers for the 2 dead skill renderers

- [ ] **Step 2**

Run:
```bash
grep -rn "build_skill_prompt\|combined_system_prompt" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/ --include="*.rs"
```

Expected: exactly the two definition lines in `skills.rs`:
- `src-tauri/src/skills.rs:674: pub fn build_skill_prompt(...) -> String {`
- `src-tauri/src/skills.rs:694: pub fn combined_system_prompt(&self) -> String {`

If grep returns any other line, STOP and report.

### Step 3: Verify zero writes to the stranded snapshot fields

- [ ] **Step 3**

Run:
```bash
grep -rn "\.l3_skills_selected\|\.l3_skills_dropped_for_budget\|l3_skills_selected:\|l3_skills_dropped_for_budget:" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/ --include="*.rs"
```

Expected: only the **field definitions** in `snapshot.rs:38-40`. Specifically NO lines of the form `.l3_skills_selected = X` or struct-literal `l3_skills_selected: X` outside the definition.

If grep returns assignment/read lines other than the definition, STOP — the deletion premise is wrong. Report and abort.

### Step 4: Delete the `skill_selection` module's two files

- [ ] **Step 4**

Run:
```bash
rm /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/agent/skill_selection/select.rs
rm /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/agent/skill_selection/mod.rs
rmdir /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/agent/skill_selection/
```

Expected: both files gone, directory removed. Verify with `ls src-tauri/src/agent/skill_selection/ 2>&1` → "No such file or directory".

### Step 5: Remove the `pub mod skill_selection;` declaration

- [ ] **Step 5**

Open `/Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/agent/mod.rs`. At line 28 there is exactly one line:

```rust
pub mod skill_selection;
```

Delete that one line. If a `pub use skill_selection::*` or `pub use skill_selection::{...}` re-export exists immediately adjacent (rare — the subagent didn't find one), delete that too. Verify with:

```bash
grep -n "skill_selection" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/agent/mod.rs
```

Expected: empty.

### Step 6: Remove the 2 dead skill renderers from `skills.rs`

- [ ] **Step 6**

Open `/Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/skills.rs`. At lines 674-691 there is the entire `build_skill_prompt` method (including any preceding doc comment); at lines 694-707 there is the entire `combined_system_prompt` method.

The structure around these (read first to find exact boundaries):

```rust
    // ... some doc comment for build_skill_prompt ...
    pub fn build_skill_prompt(&self, message: &str) -> String {
        // ~18 lines body
    }

    // ... some doc comment for combined_system_prompt ...
    pub fn combined_system_prompt(&self) -> String {
        // ~14 lines body
    }

    // — followed by format_for_system_prompt_xml at :712, which STAYS —
    pub fn format_for_system_prompt_xml(&self) -> String {
        // ... LIVE method, do NOT touch ...
    }
```

Delete **both methods AND their immediately preceding doc comments (`///` lines)** but leave `format_for_system_prompt_xml` and everything after it untouched.

Verify with:

```bash
grep -n "pub fn build_skill_prompt\|pub fn combined_system_prompt\|pub fn format_for_system_prompt_xml\|pub fn match_skills" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/skills.rs
```

Expected: only `format_for_system_prompt_xml` and `match_skills` remain.

### Step 7: Remove the 2 stranded fields from `token_budget/snapshot.rs`

- [ ] **Step 7**

Open `/Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/agent/token_budget/snapshot.rs`. At lines 38-40 (approximately — verify exact lines first) there are two struct field declarations:

```rust
    pub l3_skills_selected: u32,
    pub l3_skills_dropped_for_budget: u32,
```

(plus possibly a `///` doc comment line above each.) Delete the 2 field lines + any immediately preceding `///` doc lines that describe only these 2 fields.

If a struct-literal constructor (`TokenBudgetSnapshot { ... }`) anywhere in the file sets these fields (e.g., `l3_skills_selected: 0`), delete those lines too — find via:

```bash
grep -n "l3_skills_selected\|l3_skills_dropped_for_budget" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/agent/token_budget/snapshot.rs
```

Expected after deletion: zero matches in this file.

If `Default::default()` is derived/implemented for `TokenBudgetSnapshot` and a manual `Default` impl references these fields, update the manual `Default` to drop them (the `#[derive(Default)]` path needs no change).

### Step 8: Verify build is clean

- [ ] **Step 8**

Run:
```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: empty (no error lines).

If errors appear:
- "cannot find type `SelectionStats`" or similar → an extra reference was missed; grep for the offending symbol and remove it.
- "cannot find module `skill_selection`" → a `use crate::agent::skill_selection::*` exists somewhere not caught by Step 1's grep; find via `grep -rn "skill_selection" src/` and remove.
- "missing field `l3_skills_selected`" → a constructor of `TokenBudgetSnapshot` was missed; find and update.

Resolve any error and re-run until empty.

### Step 9: Verify the agent test suite is at the same baseline

- [ ] **Step 9**

Run:
```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri && cargo test --lib agent:: 2>&1 | tail -10
```

Expected: `test result: FAILED. 771 passed; 2 failed; ...` — exactly **2 fewer pass than the 773 baseline**, because the deletion removed `skill_selection::tests::*` (the entire test module inside `select.rs`). The 2 pre-existing unrelated failures (`shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long`) MUST still be the only failures.

**Critical:** the 2 fewer passes is expected — those were tests of the deleted module. If you see MORE than 2 fewer passes, OR if a NEW test is now failing that was passing at baseline, STOP and report.

Note: the actual baseline count was 773 with `skill_selection`'s ~12 tests included. After deletion the live count drops by however many tests `skill_selection/select.rs` had — verify by running `grep -c "fn .*test\|^\s*#\[test\]\|#\[tokio::test\]" src-tauri/src/agent/skill_selection/select.rs` BEFORE deletion (at Step 1, alongside the caller verification) and confirm the new pass count = `773 − that count`.

### Step 10: Final orphan-reference sweep

- [ ] **Step 10**

Run:
```bash
grep -rn "select_top_k\|SkillCandidate\|SelectionQuery\|SelectionStats\|DEFAULT_TOP_K\|DEFAULT_METADATA_BUDGET_TOKENS\|skill_selection\|build_skill_prompt\|combined_system_prompt\|l3_skills_selected\|l3_skills_dropped_for_budget" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri/src/ --include="*.rs"
```

Expected: empty (zero matches anywhere).

If any line returns, it's an orphan reference. Common categories:
- A stale `///` doc comment in another file mentioning the deleted symbol → safe to leave (Rust doc comments aren't enforced) but **prefer to update** to mention the live replacement (e.g., "see `format_for_system_prompt_xml`").
- A `pub use` re-export at the crate level missed by Step 5 → must remove.

Decide per-line and clean up. Re-run until empty.

### Step 11: Run a broader regression sanity check (optional but recommended)

- [ ] **Step 11**

Run:
```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill/src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: total pass count drops by the same amount as Step 9 (the deleted tests are the only difference); the same 2 pre-existing unrelated failures remain.

This catches deletions accidentally breaking tests in other modules (`automation`, `safety`, `browser`, etc.). Should not be needed if Step 9 already covered the impact, but it's cheap insurance.

### Step 12: Commit

- [ ] **Step 12**

Stage exactly the files modified/deleted:

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill add -A src-tauri/src/agent/skill_selection/ \
    src-tauri/src/agent/mod.rs \
    src-tauri/src/skills.rs \
    src-tauri/src/agent/token_budget/snapshot.rs

# Verify staged set looks clean
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill diff --cached --stat
```

Expected: 4 files in the stat — 2 deletions (`select.rs`, `mod.rs`), 2 modifications (`agent/mod.rs`, `skills.rs`, `snapshot.rs`).

(If `git add -A src-tauri/src/agent/skill_selection/` doesn't stage the deletions because the directory is gone, fall back to `git -C ... rm -r src-tauri/src/agent/skill_selection/` and then `git add` the modified files.)

Commit:

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill commit -m "chore(agent): kill dead skill_selection module + dead skill renderers (P1)

P1 of the 阶段 2 skeleton cleanup series (see
docs/superpowers/specs/2026-05-28-skeleton-cleanup-assessment.md).

Removes ~425 LoC of unreachable code in the skill subsystem:

- agent/skill_selection/ entire module (393 LoC): select_top_k, SkillCandidate,
  SelectionQuery, SelectionStats, DEFAULT_TOP_K, DEFAULT_METADATA_BUDGET_TOKENS.
  1-commit-old (2026-05-26), zero non-test callers, never wired into production.
  Live ranking path is skills_manifest::build_skills_manifest (E3 + StrategyBias).
- skills.rs dead renderers (32 LoC): build_skill_prompt, combined_system_prompt.
  Predate the Pi-spec XML format; superseded by format_for_system_prompt_xml at
  :712 (which is called from tauri_commands.rs:2060/11257 — live path stays).
- agent/token_budget/snapshot.rs (~5 LoC): stranded l3_skills_selected and
  l3_skills_dropped_for_budget fields, never written anywhere.

Live skill/prompt path is unchanged:
SkillsRegistry::load_* → match_skills + skills_manifest::build_skills_manifest →
format_for_system_prompt_xml → ChatDelegate::set_skills_manifest_block →
skill_search_used AtomicBool token control. skill_search tool unaffected.

Zero behavior change. The token-budget-aware top-K skill selection that
skill_selection was designed for but never implemented is a long-standing
minor design gap, addressed natively by Pi's formatSkillsForSystemPrompt in
阶段 3 (Pi single AgentApi handle convergence)."
```

Verify with:
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill log --oneline -1
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p1-skill status -sb
```

Expected: one new commit on `claude/skeleton-cleanup-p1-skill`; working tree clean.

---

## Self-Review

**1. Spec coverage (against `docs/superpowers/specs/2026-05-28-skeleton-cleanup-assessment.md` §1.A):**
- §1.A item: `agent/skill_selection/` (393 LoC) — Steps 4-5. ✓
- §1.A item: `skills.rs::build_skill_prompt` + `combined_system_prompt` (32 LoC) — Step 6. ✓
- §1.A item: `agent/token_budget/snapshot.rs::l3_skills_*` (~5 LoC) — Step 7. ✓
- §1.A total estimate ~425 LoC — matches sum (393 + 32 + ~5). ✓
- §1.A "極低 risk" — preserved by 3 grep gates (Steps 1-3) and 2 regression gates (Steps 8-9). ✓

**2. Placeholder scan:**
- No "TBD" / "TODO" / "implement later" in the plan. ✓
- The line numbers in Steps 6 and 7 ("approximately 674-691", "approximately 38-40") have explicit verify-with-grep steps to handle drift. Not placeholders — they're explicit drift-tolerance instructions. ✓
- Step 9's "exactly 2 fewer passes" is computed at Step 1 — the implementer runs `grep -c "fn .*test"` to know the exact expected drop. Not a placeholder. ✓
- Step 10's "common categories" of orphan references are categorized with actions, not deferred. ✓

**3. Type consistency:**
- `SkillsRegistry`, `format_for_system_prompt_xml`, `match_skills`, `skills_manifest::build_skills_manifest`, `set_skills_manifest_block`, `skill_search_used` — all live-path names used in the plan match the actual code (verified by the recon grep at the top).
- Deletion targets named with their full paths + line numbers, consistent throughout the plan.
- Test count baseline (773) matches Slice 1a/1b precedent; the 2 known pre-existing failures named consistently.

No spec gaps, no placeholders, no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 0.5 person-day.
- **Risk:** very low. 3 grep gates + 2 regression gates.
- **Files touched:** 4 (2 deleted, 2 modified) + 1 directory removed.
- **Net LoC:** −425.
- **PR shape:** 1 worktree → 1 commit → 1 PR. Squash on land is fine; no internal fix-arcs expected.
- **No new tests written.** Existing tests are the green gate.
- **No Open Decisions block P1.** P1 is the pure-zero-risk warmup; P2-P5 carry the decision dependencies.
