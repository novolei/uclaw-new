# Plan tracking: session-history-driven (P0.5 hotfix)

**Spec:** Discovered during 2026-05-18 04:46 follow-up test on the gomoku session
(`50596741-c7b2-4898-a5df-2a3bbd20e6f6`). User sent "继续" after a 1.5-hour break;
agent did 6 read-only tool calls, emitted "好的！现在来制定...开发计划：" and
loop terminated silently. P0 was not even applicable: the existing plan file's
mtime was 1.5h old (> 300s window), so `plan_state::pending_plan_steps()`
returned `None` and the whole plan-guard branch was skipped before any of P0's
improvements (relevance gate, WARN log, truncated-stub fallback) could fire.

**Root cause:** `pending_plan_steps()` discovers active plans via a 5-minute
mtime window. That window is correct for "fresh session that just stopped
talking" but wrong for "user comes back tomorrow and types 继续". Resume
workflows have no in-window mtime to anchor against.

**Fix:** Replace mtime-based discovery with message-history-driven tracking.
The current ReasoningContext carries the full message history loaded from the
session DB — that history already records every `plan_write` / `plan_update`
tool call this session made. Scanning it gives the authoritative active plan
filename per session, regardless of mtime. Fall back to the existing mtime path
only when there's no plan-tool history (covers truly fresh sessions or external
plan creation).

## Out of scope

- Reworking plan_state to be DB-driven (would need queries against `agent_turns`;
  adds I/O on every text response). Message-history scan is free since messages
  are already in memory.
- Changing the 300s mtime fallback constant. It still serves the "agent created
  a plan via something other than plan_write" edge case correctly.
- The P1 plan-mode auto-switching brainstorm (deferred).

## Tasks

### Task 1 — agent::plan_state: targeted lookup

**File:** `src-tauri/src/agent/plan_state.rs`

**Tests first (in the existing test module):**

- `pending_plan_steps_in_file_returns_count_for_named_file_regardless_of_mtime`
  — write an old plan file (sleep then `set_mtime` to 24h ago not needed since
  the new fn IGNORES mtime), assert `pending_plan_steps_in_file(root, name)`
  returns the undone count.
- `pending_plan_steps_in_file_returns_none_when_status_completed` — same as
  the mtime variant but for the explicit-file path.
- `pending_plan_steps_in_file_returns_none_when_file_missing` — typo'd
  filename or deleted file → `None`.
- `pending_plan_steps_in_file_returns_none_when_path_traversal` — defend
  against `..` segments leading outside `.uclaw/plans/`.

**Implementation:** New function with signature
`pub fn pending_plan_steps_in_file(workspace_root: Option<&Path>, filename: &str) -> Option<usize>`.
Reads `<root>/.uclaw/plans/<filename>` directly, applies the same `status:
completed` and `count_undone_steps` logic as the existing function, but
**skips the mtime check**. Path traversal guard: reject filenames containing
`/`, `\\`, or `..`.

### Task 2 — agent::dispatcher: extract active plan from message history

**File:** `src-tauri/src/agent/dispatcher.rs`

**Tests first** (new module `mod active_plan_history_tests`):

- `extracts_filename_from_recent_plan_update` — build a ChatMessage history
  containing a plan_update tool_use with `filename: "x.md"`, assert
  `extract_active_plan_from_history(&msgs) == Some("x.md")`.
- `extracts_filename_from_plan_write_result` — history with plan_write
  tool_use + matching ToolResult containing "Plan created at /workspace/.uclaw/plans/y.md",
  assert returns `Some("y.md")`.
- `returns_most_recent_when_multiple_plan_calls_exist` — multiple
  plan_write/plan_update calls, latest wins.
- `ignores_failed_plan_calls` — ToolResult with `is_error: true` for
  plan_write → skipped.
- `returns_none_when_no_plan_history` — history with only bash/read_file,
  returns None.

**Implementation:** New helper
`fn extract_active_plan_from_history(messages: &[ChatMessage]) -> Option<String>`.
Walk forward, maintain (a) pending plan_write id → filename map for pairing
with ToolResult, (b) latest seen filename. plan_update reads `arguments.filename`
directly; plan_write needs ToolResult parsing — use a regex like
`r"Plan created at .+/([^/]+\.md)"`. Both update the "latest" tracker so the
final value wins.

### Task 3 — wire targeted lookup into handle_text_response

**File:** `src-tauri/src/agent/dispatcher.rs`

**No new tests** for this wiring step — it's a one-line branch swap whose
behavior is fully covered by Task 1 + Task 2 test suites plus the existing
plan_guard_relevance_tests.

**Implementation:** In `handle_text_response`, change

```rust
if let Some(undone) = crate::agent::plan_state::pending_plan_steps(
    self.workspace_root.as_deref(), 300
) { ... }
```

to:

```rust
let undone_opt = match extract_active_plan_from_history(&reason_ctx.messages) {
    Some(filename) => crate::agent::plan_state::pending_plan_steps_in_file(
        self.workspace_root.as_deref(), &filename
    ),
    None => crate::agent::plan_state::pending_plan_steps(
        self.workspace_root.as_deref(), 300
    ),
};
if let Some(undone) = undone_opt { ... }
```

The downstream relevance gate, MAX_PLAN_GUARD_NUDGES cap, and WARN log all
stay identical — they're the safety net for "wrong plan, unrelated text".

## Verification

- `cd src-tauri && cargo test --lib agent::plan_state` — all green (+4 new)
- `cd src-tauri && cargo test --lib agent::dispatcher` — all green (+5 new)
- `cd src-tauri && cargo build` — no errors
- Manual: in a session that called plan_write more than 5 minutes ago, send
  "继续" / "now add the game logic" → expect plan-guard nudge instead of
  silent termination

## PR shape

Branch `worktree-fix-plan-tracking-session-aware` → `main`. Title:
`fix(agent): plan guard tracks active plan via message history, not just mtime`.
Body lists Task 1-3 commits as bisectable table.
