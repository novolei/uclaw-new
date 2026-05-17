# Plan-driven workflow hotfix — quad-fix

**Spec:** Root-cause investigation captured in conversation 2026-05-18 (五子棋 session
`50596741-c7b2-4898-a5df-2a3bbd20e6f6`). Four compounding bugs blocked the plan-driven
agent workflow:

1. **Silent termination** — agent emitted "现在添加游戏逻辑和事件处理：" (14 chars) with no
   tool call after 5 productive iterations. `dispatcher::text_signals_plan_work` relevance
   gate rejected the Chinese phrasing (no match for "计划/步骤/任务/待办" nor for
   continuation markers "继续/接下来/下一步/正在"), so the plan-guard skipped its nudge and
   the loop returned `TextAction::Return(LoopOutcome::Response)` cleanly. Skip path logged
   only at `tracing::debug!`, invisible at default INFO level. UI saw `chat:stream-complete`
   and reported "done" with plan still 0/9.
2. **plan_update honesty reminder over-corrected** — `plan.rs` tool result for
   `done:true` told the LLM to *"undo this update (call plan_update again with done:false)
   and perform the actual work first"*. Observed effect: turn 3 marked step 0 done →
   turn 4 marked step 0 not-done → agent did write_file/edit but never re-called
   plan_update for the rest of the run.
3. **PlanViewer shows "Unnamed Plan"** — backend writes `task: "{title}"` in YAML, frontend
   regex reads `^title:` — silent fallback to literal `'Unnamed Plan'`.
4. **`.uclaw` invisible in Files panel** — `files_rail/ignore.rs` has `.uclaw` both in
   `SKIP_DIRS` AND the generic dotfile filter. Users can't see agent-written plan files.

PR #67 fixed glob/grep tools' `.uclaw` descend, but the files-rail walker was never
updated. PR shape: **one branch, 4 bisectable commits, one PR.**

## Out of scope (P1, future PR)

- Auto-switching SafetyMode to `Plan` when user message contains planning keywords.
  Design decision (auto vs. toast vs. system-prompt suggestion) needs brainstorming.
- Replacing the relevance-gate keyword list with a cheaper semantic classifier.
- Auditing other tool reminders for similar over-correction patterns.

## Tasks

### Task 1 — dispatcher: relevance gate Chinese coverage + visibility + tool-intent fallback

**Files:** `src-tauri/src/agent/dispatcher.rs`

**Tests first** (`#[cfg(test)] mod tests` already at L2119+):

- `relevance_gate_recognises_chinese_action_verbs` — assert
  `text_signals_plan_work("现在添加游戏逻辑和事件处理：")` returns `true`.
- `relevance_gate_recognises_now_starting_resuming_markers` — assert true for
  "现在开始构建棋盘", "目前实现胜利检测", "马上编写事件处理".
- `relevance_gate_force_passes_short_text_with_large_output` — direct unit on the new
  helper `signals_truncated_plan_continuation(text_len, output_tokens)` covering
  `(14, 1722) → true`, `(300, 1722) → false`, `(14, 50) → false`.
- Keep all existing tests green (the "头好疼" non-hijack case is the load-bearing
  negative — must remain `false`).

**Implementation:**

1. Extend continuation markers with `"现在","目前","马上","即将","开始","下面"` (Mandarin
   "now-starting" semantics that `"正在"` doesn't cover).
2. Extend plan keywords with `"实现","添加","编写","完成"` (action verbs the agent emits
   when announcing intent to act on a plan step).
3. New helper `signals_truncated_plan_continuation(text_len: usize, output_tokens: u32)
   -> bool` — true when `output_tokens > 800 && text_len < 100`. Wire it into the plan
   guard at `handle_text_response` as a final fallback before skip — large-output +
   tiny-text means the model likely composed thinking/tool calls but emitted only a
   transition stub.
4. Upgrade the "Plan guard skipped" `tracing::debug!` at L1135-1140 to `tracing::warn!`
   and include `output_tokens` and `text_preview` fields so post-mortems work without a
   debug build.

### Task 2 — plan_update: soften the honesty reminder

**File:** `src-tauri/src/agent/tools/builtin/plan.rs`

**Test:** No new unit test (text content is a string literal in tool_result and the
existing dispatcher-side guard at lines 1188-1239 still enforces actual mutation
evidence — that's the load-bearing check). Verify with manual diff review only.

**Implementation:** Replace the `done:true` reminder with a shorter statement of fact
that removes the "call plan_update again with done:false" instruction. The honesty
discipline lives in the dispatcher's mutation-evidence guard, not in this reminder.

New text (illustrative):
```
Step {N} marked DONE in {file}.

Reminder: plan_update only updates the checkbox. Make sure the actual work
(write_file / edit / bash) happened first — users see code on disk, not checkmarks.
```

### Task 3 — PlanViewer: accept `title:` OR `task:`

**Files:** `ui/src/components/agent/PlanViewer.tsx`, new
`ui/src/components/agent/PlanViewer.test.tsx`.

**Test first** (vitest + RTL via `renderWithProviders`):

- `renders task field from YAML frontmatter` — feed the exact backend output (`---\ntask:
  "网页五子棋小游戏开发计划"\nstatus: in_progress\n---\n\n## Goal\n...`) and assert the rendered
  title is `"网页五子棋小游戏开发计划"`, not `"Unnamed Plan"`.
- `renders title field for forward compatibility` — feed `title:` variant, assert it
  still works.
- `falls back to "Unnamed Plan" when neither present` — preserve existing behavior.

**Implementation:** Change `/^title:\s*(.+)$/m` to `/^(?:title|task):\s*(.+)$/m`. The
existing `parsePlanMarkdown` is a pure function — export it for direct testing.

### Task 4 — files-rail/ignore: unhide `.uclaw`

**File:** `src-tauri/src/files_rail/ignore.rs`

**Test first:**

- `should_ignore_does_not_hide_uclaw_dir` — assert
  `should_ignore(".uclaw", /*is_dir=*/true)` returns `false`.
- `should_ignore_still_hides_other_dotdirs` — assert true for `.git`, `.cache`,
  `.venv`.
- `should_ignore_still_allows_env_and_gitignore` — keep existing exception behavior.

**Implementation:** Remove `".uclaw"` from `SKIP_DIRS` AND add `".uclaw"` to the
dotfile-exception list at L20 (alongside `.gitignore` and `.env`).

## Verification

- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — no errors
- `cd src-tauri && cargo test --lib dispatcher::tests 2>&1 | tail -20` — all green
- `cd src-tauri && cargo test --lib agent::tools::builtin::plan 2>&1 | tail -10` — green
- `cd src-tauri && cargo test --lib files_rail::ignore 2>&1 | tail -10` — green
- `cd ui && npx tsc --noEmit 2>&1 | head -10` — no errors
- `cd ui && npm test -- --run PlanViewer 2>&1 | tail -20` — green

## PR shape

Branch `worktree-fix-plan-guard-quad` → `main`. Title:
`fix: unblock plan-driven workflow (relevance gate + plan reminder + UI title + .uclaw visibility)`.
Body includes the `## Commits (bisectable)` table per CLAUDE.md convention.
