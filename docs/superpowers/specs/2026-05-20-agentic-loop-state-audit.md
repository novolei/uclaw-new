# Agentic Loop State Audit — M1-T2b

**Date**: 2026-05-20
**Author**: Cowork (Claude Sonnet 4.6) on behalf of the DRI
**Scope**: `src-tauri/src/agent/agentic_loop.rs` (882 lines, DMZ file)
**Purpose**: Prerequisite for M1-T2c — wrapping `run_agentic_loop` in
`runtime::task::SessionTask`. Catalogs every mutable state slot, classifies
per-turn vs per-session, and identifies the R-1 / R-6 risk surface so the
wrap is provably safe.

## TL;DR for reviewers

| Risk | Symptom | Source |
|---|---|---|
| **R-1 HIGH** | `reason_ctx.force_text = true` is sticky — once truncation triggers it, every subsequent turn in the session forces text output even if the original cause is gone | line 354, no reset path |
| **R-1 MEDIUM** | Token counters (`total_input_tokens`, `total_output_tokens`) accumulate forever inside `ReasoningContext` — fine for the cost of a single session, surprising for long sessions that span days | line 127–129 |
| **R-6 HIGH** | The loop polls `delegate.check_signals()` only at the start of each iteration. An in-flight LLM call (line 110) or `delegate.execute_tool_calls` (line 389) cannot be interrupted — they run to completion before the loop notices `LoopSignal::Cancel` | lines 68, 110, 389 |
| **R-6 MEDIUM** | `LoopOutcome::Cancelled { partial_code }` preserves the partial code buffer but does NOT reset `force_text`, the nudge counter, or rotate the session state out of `Interrupted` | lines 72–80 |
| **R-1 LOW** | `consecutive_tool_intent_nudges` is a local var — correctly scoped per loop invocation. Documented for completeness | line 56 |
| **R-1 LOW** | `truncation_count` is a local var — same as above | line 55 |

The HIGH-risk items are the reason M1-T2c needs a careful wrap, not a
mechanical one. The wrap must (a) thread a `CancellationToken` through
`delegate` so the LLM call and tool execution can be aborted, and
(b) reset `force_text` either at the start of each `RegularTask::run`
or when the cause of the previous truncation has cleared.

## Full state catalog

### A. Local variables in `run_agentic_loop` (per-invocation)

These are correctly scoped — they live for the duration of one
`run_agentic_loop` call and disappear when the function returns. **R-1
safe.** Reproduced here for the audit record.

| Var | Type | Line | Reset on |
|---|---|---|---|
| `truncation_count` | `usize` | 55 | function entry (0) + line 391 on successful tool calls |
| `consecutive_tool_intent_nudges` | `usize` | 56 | function entry (0) + line 191 on non-intent text + line 391 on successful tool calls |
| `iteration` | `usize` | 62 | for-loop variable (1..=max_iterations) |
| `output` | `RespondOutput` | 110 | re-assigned each iteration |
| `text`, `thinking`, `thinking_signature` | various | pattern match | match-binding, per iteration |
| `blocks` | `Vec<ContentBlock>` | several | local to each match arm |
| `assistant_msg` | `ChatMessage` | 386 | local to tool-call branch |
| `tool_calls` | `Vec<ToolCall>` | pattern match | match-binding |

### B. `&mut ReasoningContext` fields (per-session, the R-1 surface)

These are the **shared mutable state** that the wrap needs to be careful
about. Categorized by reset behavior:

#### B.1 Always reset (safe)

| Field | Type | Used at | Reset by |
|---|---|---|---|
| `messages` | `Vec<ChatMessage>` | every iteration appends | the *user* via `Session` lifecycle |
| `thread_state` | `ThreadState` | every iteration writes | the loop's return path sets one of {Processing, Completed, Failed, Interrupted, AwaitingApproval} |

#### B.2 Accumulates across the session (intentional)

| Field | Type | Used at | Lifetime |
|---|---|---|---|
| `total_input_tokens` | `u32` | line 127 | monotonically grows for the session |
| `total_output_tokens` | `u32` | line 128 | monotonically grows for the session |
| `partial_code_buffer` | `Option<(String, String)>` | line 73, persisted into `LoopOutcome::Cancelled` | replaced/cleared by streaming code-block reassembly |

**Note**: Token counters reset only when the session is destroyed
(`SessionManager::remove_session`). This is fine for cost reporting but
M1-T6 (TokenUsage rework) will move per-turn accounting into the
`ModelTurn` task event and leave `total_*` as a derived view.

#### B.3 Sticky one-way flags (the R-1 HIGH item)

| Field | Type | Set at | Reset path |
|---|---|---|---|
| `force_text` | `bool` | line 354 (truncation cascade triggers it) | **(none — sticky)** |

`force_text = true` means "this turn must produce text output, not tool
calls". It's correct to set it when the model has hit `max_truncations`
in a single iteration, but it's *not* correct that it stays true forever.
Subsequent turns where the model is no longer in a truncation cascade
still get the constraint applied.

**M1-T2c remedy**: at the start of `RegularTask::run`, the wrap resets
`reason_ctx.force_text = false`. This is safe because the only callers
of `run_agentic_loop` are session-driven and a new user message implies a
fresh turn intent.

### C. `delegate: &dyn LoopDelegate` async boundary (the R-6 surface)

Every call to `delegate.*` is an async point. The loop currently does
NOT poll cancellation between these calls. The 7 such points are:

| Line | Call | Cancellation today |
|---|---|---|
| 68 | `delegate.check_signals().await` | This *is* the cancellation poll, but it's only called once per iteration |
| 92 | `compress_context_if_needed(reason_ctx, config, delegate).await` | No cancellation — runs to completion |
| 95 | `delegate.before_llm_call(reason_ctx, iteration).await` | No cancellation |
| 110 | `delegate.call_llm(reason_ctx, iteration).await` | **No cancellation — the killer.** A streaming LLM response can take 30+ seconds with no interrupt opportunity. |
| 169 | `delegate.on_tool_intent_nudge(&text, reason_ctx).await` | No cancellation |
| 308 | `delegate.handle_text_response(...).await` | No cancellation |
| 389 | `delegate.execute_tool_calls(tool_calls, reason_ctx).await` | **No cancellation — equally bad.** Tool execution can include shell commands, file edits, web requests, etc. |

**M1-T2c remedy**: the wrap passes a `CancellationToken` into a new
`LoopDelegate::set_cancellation_token(&self, token: CancellationToken)`
method (or extends the existing trait signatures with `token: &CancellationToken`).
The two HIGH-risk callsites (line 110 + line 389) must observe the token via
`uclaw_utils_async_utils::OrCancelExt::or_cancel(&token)`. The MEDIUM ones
can be best-effort — the worst case is a tool execution that wraps up
before noticing cancellation, which is the existing behavior.

A pragmatic intermediate approach is to make `LoopDelegate` trait
methods themselves accept `&CancellationToken` (or store it in
`ReasoningContext`). The exact mechanism is M1-T2c's choice; this spec
just enumerates the surface that must be addressed.

## Test matrix for M1-T2c

M1-T2c MUST pass these tests before merging:

| # | Scenario | Expected |
|---|---|---|
| 1 | Submit a message → assistant replies → submit another | `force_text` is `false` at start of turn 2 even if turn 1 hit truncation |
| 2 | Submit message → 2s into a long stream, user submits a new message | First task's `RegularTask::run` returns within 100ms (GRACEFUL_SHUTDOWN_TIMEOUT) with a `TaskTermination::Cancelled` |
| 3 | Submit message → mid-tool-call, user submits a new message | Tool call is allowed to finish (best-effort) but no new tool call is queued; second task starts after the in-flight tool returns |
| 4 | Submit message → LLM call panics | `TaskTermination::Panicked(_)` recorded, no zombie task left on the scheduler |
| 5 | Submit message → completes normally | `TaskTermination::Completed`, `force_text` reset, token counters accumulated correctly |
| 6 | Spawn 3 review tasks (M1-T2c follow-up) → user submits a message | Review tasks are NOT preempted (TaskKind::Review has `is_user_preemptible() == false`) |

Tests 1, 2, 4, 5 are the minimum for M1-T2c to land. Tests 3 and 6
can land in follow-ups but the M1-T2c PR description must explicitly
note them as deferred.

## Risk-register linkage

| Risk | ADR § | This spec catches |
|---|---|---|
| **R-1** (State leak across turns) | docs/adr/2026-05-20-uclaw-agent-platform-north-star.md § "Risks" | The `force_text` sticky-flag bug — fixed in M1-T2c |
| **R-6** (Cancellation can't unwind in-flight) | same | The LLM + tool-call cancellation gap — fixed in M1-T2c |
| **R-7** (Compression / context drift) | same | Out of scope for M1-T2c; M2-G addresses |

## Open questions for the DRI

1. **Should `consecutive_tool_intent_nudges` move from local var to
   `ReasoningContext`?** Pro: visible in checkpoints. Con: another R-1
   surface to maintain. **Recommendation**: leave it local. The nudge
   counter is an iteration-tactic detail, not a session-level invariant.

2. **Should `LoopDelegate` trait methods take `&CancellationToken` or
   should the token live on `ReasoningContext`?** The trait-method
   approach is more explicit (caller can audit which methods observe
   cancellation) but more verbose (7 method signatures to extend).
   The `ReasoningContext` approach is invasive only at the entry point
   but couples cancellation state to the context object. **Recommendation**:
   trait method, because the explicit-ness aids the M1-T2c reviewer.

3. **Should the wrap reset `force_text` at start of run OR when the
   condition that set it has cleared?** Reset-at-start is simpler and
   covers 100% of the bug; condition-reset is finer-grained but
   requires tracking "did we truncate this turn?". **Recommendation**:
   reset-at-start. The cost is one extra LLM turn that might have
   benefited from `force_text`; the benefit is a robust invariant.

These are flagged in the M1-T2c PR description for explicit DRI sign-off.

## Sign-off

This spec is required to be cited in the M1-T2c PR body. M1-T2c is
allowed to update this document if the implementation reveals new
surface; the audit table here is the source of truth as of 2026-05-20.

---

References:
- `BEHAVIOR.md §"DMZ Files Need Two-Session Review"` — agentic_loop.rs is DMZ
- `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` — R-1, R-6 risk register
- `uclaw-upgrade-implementation-plan.md` — M1-T2c task description
- `src-tauri/src/runtime/task.rs` (M1-T2a, PR #305) — the `SessionTask` trait + `TaskScheduler`
