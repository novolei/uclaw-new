# Item 3 — Carry-Overs (ripgrep grep · config read-cap · checkpoint prune) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Land the three deferred carry-overs from 阶段 5 SP4 + the checkpoint work as one bisectable branch (3 commits): (3a) a ripgrep fast-path for `grep` with graceful fallback; (3b) make the `read_file` 100K char cap config-overridable; (3c) bound the shadow-checkpoint store's growth with an age-based prune. Each is small, additive, low-blast-radius.

**Branching:** This branch **stacks on item 2** (`claude/item2-project-check` @ `ea7339ab`) because 3b AND 3c both add fields to `memubot_config.rs`, which item 2 also edited — branching off main would force 3-way conflicts on that file. Branch `claude/item3-carryovers` FROM the item-2 tip. The controller merges item 2 first, then item 3.

**Tech Stack:** Rust. `tokio::process::Command` (rg + git), existing config (memubot_config), existing checkpoint git plumbing. No new deps.

---

## Source-of-truth references (verified)

- **3a** `agent/tools/builtin/search.rs` — `GrepTool` (`name="grep"`). `execute` (37): parses `pattern`/`path`/`include`, builds a `regex::Regex`, calls `search_dir`, joins results with `\n`, emits `"No matches found."` when empty (50). `search_dir` (60-103): recursive `tokio::fs::read_dir`, skips dotdirs except `.uclaw` + `node_modules` + `target`, per-file `read_to_string` + line regex, pushes `format!("{relative}:{line_num+1}: {line}")` (95), **hard cap 50 total results** (96). `match_glob` (105): `*`/`*.*` → all, `*.ext` → suffix, else exact. THIS is the format + cap + skip contract the rg path must reproduce.
- **3b** `agent/tools/builtin/file.rs` — `pub const MAX_READ_CHARS: usize = 100_000;` (9); `select_window(lines, anchors, offset, limit, max_chars)` (69) is already parameterized on `max_chars`; `ReadFileTool::execute` calls `select_window(..., MAX_READ_CHARS)` (277). `ReadFileTool::new(workspace)` (mirror EditTool). Runtime registration: `tauri_commands.rs:10712` + `:15244` (adjacent to the EditTool sites item 2 wired).
- **3c** `agent/code_checkpoint.rs` — checkpoints are a **per-project commit chain** under `refs/uclaw/<project_hash>` (`REFS_PREFIX="refs/uclaw"` line 45; `ref_name_for(dir_hash)` 581; `project_hash` 564). `list` (244) does `git log <ref> -n 50`. `run_git(&self, args, working_dir, index_file) -> (bool, String, String)` (392) is the git shell-out. `ensure_checkpoint` (120) NEVER errors (debug+false on failure) — prune must match that infallible-by-design posture. Store at `uclaw_home/checkpoints/store`. Single construction site: `app.rs:1169` `CheckpointStore::new(checkpoint_store_dir)`.
- `memubot_config.rs` — `MemoryOsConfig` (the PR16/item2 field home); `#[serde(default = "fn")]` + `default_*` fn + manual `Default` entry pattern.

---

## CRITICAL facts

1. **3a — preserve the output contract exactly.** The rg path MUST emit the same `"{relative}:{line}: {text}"` lines, the same total-50 cap, and the same `"No matches found."`/join-`\n` behavior. On ANY rg problem (binary absent → spawn error, non-zero exit that isn't "no matches", unparseable output) fall back to the existing `search_dir`. rg exit code 1 = "no matches" (NOT an error → empty results, not fallback); exit ≥2 = real error → fallback. rg additionally honors `.gitignore` — document this as an intentional improvement (the agent shouldn't grep build artifacts anyway), and still pass `--glob '!target' --glob '!node_modules'` so behavior stays close.
2. **3b — config cap with a SAFE FLOOR.** A misconfigured `0`/tiny value must not make every read truncate to nothing. Clamp the effective cap to `max(configured, 1_000)`. Default = the current `MAX_READ_CHARS` (100_000) so an unconfigured deployment is byte-identical to today. Keep `MAX_READ_CHARS` as the default constant.
3. **3c — prune must be infallible + correct git.** Age-based WHOLE-REF deletion only (no history rewriting): enumerate `refs/uclaw/*`, read each tip's committer timestamp, delete refs older than the cutoff, then `git gc --prune=now`. This bounds the dominant growth source (abandoned-session chains accumulating forever). An active session's own chain keeps growing within the window — acceptable; chain-length-cap-via-rerooting is explicitly a documented FUTURE enhancement (heavy: requires re-rooting the commit chain). prune returns counts, never propagates a hard error to callers (best-effort, like `ensure_checkpoint`).
4. **No silent scope creep.** 3a/3b/3c are independent commits. 3b + 3c each add ONE config field. Do not refactor the surrounding tool/registration code beyond the wiring.
5. **Pre-commit hooks** — no `--no-verify`. Note `code_checkpoint.rs` uses `uclaw_home` (not `dirs::home_dir`) — keep that.

---

## File Structure

| File | Task | Change | LoC |
|---|---|---|---|
| `agent/tools/builtin/search.rs` | 3a | `try_ripgrep(...) -> Option<Vec<String>>` (spawn rg, parse, None on any problem) + `execute` tries it first, falls back to `search_dir`; tests | ~+90 (incl ~40 test) |
| `memubot_config.rs` | 3b,3c | `read_file_max_chars: usize` (default 100_000) + `checkpoint_prune_max_age_days: u64` (default 14) + serde defaults + Default entries + tests | ~+40 |
| `agent/tools/builtin/file.rs` | 3b | `ReadFileTool` gains `max_read_chars: usize` (default `MAX_READ_CHARS`) + `with_max_read_chars(n)` builder (clamps floor 1_000); `execute` uses `self.max_read_chars`; test | ~+30 |
| `tauri_commands.rs` | 3b | wire `:10712` + `:15244` ReadFileTool with `.with_max_read_chars(cfg.memory_os.read_file_max_chars)` (site 2 = capture before closure, like item 2) | ~+8 |
| `agent/code_checkpoint.rs` | 3c | `prune(max_age_days: u64) -> PruneStats` (for-each-ref + delete-stale + gc) + `PruneStats` + tests | ~+110 (incl ~50 test) |
| `app.rs` | 3c | best-effort fire-and-forget prune call after `CheckpointStore::new` (spawn_blocking / detached; reads `checkpoint_prune_max_age_days`) | ~+12 |

Est. ~190 source + ~140 tests.

---

## Tasks

### Task 3a: ripgrep fast-path for grep
- [ ] In `search.rs`, add `async fn try_ripgrep(&self, search_path: &Path, pattern: &str, include: Option<&str>) -> Option<Vec<String>>`:
  - Spawn `rg --line-number --no-heading --color=never --glob '!target' --glob '!node_modules' [--glob <include-as-rg-glob>] -e <pattern> <search_path>` via `tokio::process::Command` (`kill_on_drop`, stdin null, stdout/stderr piped). Map the existing `include` (`*.rs` etc.) to an rg `--glob` (`*.rs`). For `*`/`*.*` pass no glob.
  - On spawn error → `None` (fallback). On exit code 1 (no matches) → `Some(vec![])`. On exit code ≥2 → `None` (fallback). On exit 0 → parse stdout lines.
  - rg emits `<path>:<line>:<text>` (absolute since we pass an absolute `search_path`). Convert `<path>` to workspace-relative (`strip_prefix(&self.workspace_root)`), re-emit as `"{relative}:{line}: {text}"` (NOTE the space after the 2nd colon — match `search_dir`'s exact `"{}:{}: {}"`). Cap at 50 total (truncate). Return `Some(results)`.
  - Be byte-safe: operate on `str` splits (`splitn(3, ':')`), never byte-index.
- [ ] In `execute`: after building `search_path`/`include` and validating the regex (keep the regex validation so an invalid pattern still errors fast the same way), do `let results = match self.try_ripgrep(&search_path, pattern, include).await { Some(r) => r, None => { let mut r = Vec::new(); self.search_dir(&search_path, &re, include, &mut r).await?; r } };` then the existing empty/join formatting.
- [ ] Tests: (1) `try_ripgrep` parse helper — feed sample rg stdout (absolute paths under a temp workspace) → relative-formatted lines, capped at 50 (extract the PARSING into a pure `fn parse_rg_output(stdout, workspace_root) -> Vec<String>` so it's testable without spawning rg). (2) An `execute` integration test in a temp dir with a match → finds it (works whether rg present or not, since fallback covers absence). (3) include-glob mapping unit test. Gate any test that REQUIRES rg on an rg-presence probe.
- [ ] Build + `cargo test --lib agent::tools::builtin::search`; commit: `feat(agent): ripgrep fast-path for grep with fallback (item3.3a)`

### Task 3b: config-overridable read cap
- [ ] `memubot_config.rs`: add `read_file_max_chars: usize` (`#[serde(default = "default_read_file_max_chars")]`, `fn default_read_file_max_chars() -> usize { 100_000 }`, Default entry). Test: default is 100_000 + omitted-field deserialize.
- [ ] `file.rs`: `ReadFileTool` gains `max_read_chars: usize`; `new` sets it to `MAX_READ_CHARS`; add `pub fn with_max_read_chars(mut self, n: usize) -> Self { self.max_read_chars = n.max(1_000); self }`. `execute` passes `self.max_read_chars` to `select_window` instead of the bare const. Test: `with_max_read_chars(0)` clamps to 1_000; default equals `MAX_READ_CHARS`.
- [ ] `tauri_commands.rs`: wire `:10712` (read `cfg.memory_os.read_file_max_chars` from `state.memubot_config.read().await`) + `:15244` (capture before the closure, like item 2's `epc_*_for_factory`) → `.with_max_read_chars(...)`.
- [ ] Build + `cargo test --lib agent::tools::builtin::file memubot_config`; commit: `feat(agent): config-overridable read_file char cap (item3.3b)`

### Task 3c: checkpoint store age-based prune
- [ ] `code_checkpoint.rs`: add
  ```rust
  #[derive(Debug, Default, Clone, PartialEq)]
  pub struct PruneStats { pub refs_deleted: usize, pub refs_kept: usize }
  ```
  and `pub fn prune(&self, max_age_days: u64) -> PruneStats`:
  - If `!self.store_dir.join("HEAD").exists()` → return default (nothing to do).
  - `git for-each-ref --format='%(refname) %(committerdate:unix)' refs/uclaw` (via `run_git` with a working_dir — `run_git` needs a working dir for env; use the store/any dir; if `run_git` requires a real working dir, pass the store_dir's path or refactor to a store-scoped git call — recon `run_git`/`git_env` to see what working_dir is used for and pass something valid).
  - Compute cutoff = now − max_age_days·86400. (Time: use `std::time::SystemTime::now()` — this is runtime, NOT a workflow script, so SystemTime is fine.) For each ref with `committerdate < cutoff` → `git update-ref -d <ref>` (count deleted); else count kept.
  - After deletions, `git gc --prune=now` (best-effort; ignore failure).
  - NEVER panic / propagate a hard error — on any git failure, return whatever stats accumulated (mirror `ensure_checkpoint`'s infallible posture). Returning `PruneStats` (not `Result`) enforces this.
  - `max_age_days == 0` → treat as "disabled" (return default, delete nothing) so a user can turn prune off.
- [ ] `memubot_config.rs`: add `checkpoint_prune_max_age_days: u64` (default 14) + serde default + Default entry + test.
- [ ] `app.rs`: after `CheckpointStore::new(...)` (line ~1169), fire a best-effort detached prune: clone the store (or wrap in Arc — recon how it's stored), read `checkpoint_prune_max_age_days`, and `tokio::task::spawn_blocking(move || store.prune(days))` (or a detached thread) so startup isn't blocked. If the store isn't `Clone`/`Arc`, call `prune` synchronously BEFORE moving it into shared state IF that's cheap (gc on a small store is fast) — but prefer detached. Recon the construction context at app.rs:1169 and pick the lightest correct wiring; flag what you chose.
- [ ] Tests (code_checkpoint.rs): (1) `prune` on an empty/uninit store → default stats, no panic. (2) Create 2 checkpoints in a temp store for a temp working dir, `prune(14)` with both fresh → `refs_kept >= 1, refs_deleted == 0`. (3) `prune(0)` → disabled, deletes nothing. (Faking an OLD ref to assert deletion needs controlling committerdate — if hard hermetically, cover the cutoff arithmetic in a tiny pure helper `fn is_stale(committer_unix: i64, now_unix: i64, max_age_days: u64) -> bool` and unit-test THAT directly, plus assert the fresh-ref kept case end-to-end.)
- [ ] Build + `cargo test --lib agent::code_checkpoint`; commit: `feat(agent): age-based prune for shadow checkpoint store (item3.3c)`

### Task 3d: Verification
- [ ] `cargo build 2>&1 | grep -E "^error"` clean.
- [ ] `cargo test --lib agent::tools::builtin::search`, `...::file`, `agent::code_checkpoint`, `memubot_config` all pass.
- [ ] `cargo test --lib agent 2>&1 | tail` — net green (the SAME 2 pre-existing failures only: `shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long`).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "search\.rs|builtin/file|code_checkpoint|memubot_config|tauri_commands"` clean.
- [ ] `git diff <item2-tip> -- src-tauri/Cargo.toml` empty (no new deps).
- [ ] **3a no-regression**: with rg ABSENT (or via the fallback test), grep returns identical results to the old walker.
- [ ] **3b no-regression**: default (unconfigured) read is byte-identical to today (cap 100_000).
- [ ] **3c safety**: prune on a store with only fresh checkpoints deletes nothing; `prune(0)` disabled.

---

## Self-Review
- ✅ Spec coverage: 3a ripgrep+fallback, 3b config cap, 3c age-prune — all three carry-overs, one commit each.
- ✅ No placeholders — rg flags + exit-code semantics, the floor-clamp, the for-each-ref/cutoff/gc sequence, and the testable pure helpers (`parse_rg_output`, `is_stale`) are all concrete.
- ✅ Type consistency: `try_ripgrep(&self, &Path, &str, Option<&str>) -> Option<Vec<String>>`; `with_max_read_chars(usize) -> Self` (floor 1_000); `prune(u64) -> PruneStats { refs_deleted, refs_kept }`; config fields `read_file_max_chars: usize`/`checkpoint_prune_max_age_days: u64`.
- ✅ Risk-scaled: all additive + default-preserving (rg falls back; cap defaults to 100_000; prune age default 14 + 0=off). Stacked on item 2 to avoid `memubot_config.rs` conflicts.
- Decisions: rg honors .gitignore (documented improvement) + identical output format/cap; config cap floor-clamped; age-based whole-ref prune only (re-rooting deferred); prune infallible (returns stats, not Result); prune fired best-effort at startup.
