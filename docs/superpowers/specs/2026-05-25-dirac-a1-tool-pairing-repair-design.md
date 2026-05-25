# Dirac-A1 — Tool-Use/Result Pair Repair on Compaction (C1)

> **Context**: Phase A item #1 from
> [`docs/research/2026-05-25-dirac-reverse-engineering.md`](../../research/2026-05-25-dirac-reverse-engineering.md) §7.2.
> Companion plan: [`plans/2026-05-25-dirac-a1-tool-pairing-repair.md`](../plans/2026-05-25-dirac-a1-tool-pairing-repair.md).
> **C1 slot**: M2 closeout — small, low-risk, ships before C1 closes.

## 1. Background

`src-tauri/src/agent/agentic_loop.rs:493::purge_orphaned_tool_results`
runs after every compaction (`force_compact`, `compress_context_if_needed`,
soft / hard truncation paths — 4 call sites: lines 624, 798, 969, plus
624 via `force_compact_sync`). Its job is to keep the Anthropic
`tool_use`/`tool_result` pairing invariant after we mark some messages
`compacted = true`.

**The current implementation is asymmetric — it only handles one of the
two ways pairing can break:**

| Compaction outcome | Current behavior | Anthropic API result |
|---|---|---|
| `tool_use` survived, `tool_result` was compacted | ⚠️ `tool_use` remains in active content → **next API call rejected** with `tool_use without matching tool_result` | ❌ broken |
| `tool_result` survived, `tool_use` was compacted | ✅ orphan `tool_result` is filtered out by `retain()` at line 510 | ✓ |

The first row is the bug. It bites when compaction lands mid-conversation
on a boundary where an assistant turn (with tool_use blocks) is kept but
the following user turn (with tool_result blocks) is compacted — for
example, when `compression_keep_turns` is even and tool calls cross the
boundary.

**Dirac's fix** (reverse-engineered, citation in research doc §2.1):
`src/core/context/context-management/ContextManager.ts:287-393::ensureToolResultsFollowToolUse`
adds the *symmetric* repair — for every `tool_use` ID without a matching
`tool_result` in the immediately-following user message, **insert a
placeholder `{type:'tool_result', tool_use_id, content:"result missing"}`**.
Verified at line 361 of Dirac source. This converts a broken history
into a valid one without the LLM ever calling the corresponding tool
again — the placeholder reads as "that operation completed but we lost
the output," and the LLM moves on.

## 2. Scope

Single PR. One subsystem (`agent::agentic_loop`).

### 2.1 In scope

- Extend `purge_orphaned_tool_results` to **also** insert placeholder
  `ContentBlock::ToolResult` blocks for orphaned `ToolUse` blocks.
- New helper logic: after the existing retain step, walk the active
  messages in pairs and ensure the user message immediately following
  each assistant message contains a `ToolResult` for every `ToolUse` in
  the assistant message. Missing ones get a placeholder appended.
- Placeholder content: `"[result missing — compacted before next turn]"`
  (slightly more descriptive than Dirac's bare `"result missing"`; aids
  debug when reading rollout JSONL).
- Five new unit tests covering all 4 pairing topologies.

### 2.2 Out of scope

- Changing the compaction heuristics themselves (soft/hard thresholds,
  `find_safe_compaction_boundary`) — that's a separate concern, addressed
  in M2-B / M2-H Slice 3 work.
- Provider-specific handling — Anthropic is the strict one; OpenAI/Gemini
  paths benefit too but the bug only crashes on Anthropic.
- Streaming-time repair (mid-turn). The compaction-time repair handles
  the durable history; streaming-time pairing is handled by
  `stream_response_handler` and is out of scope.

## 3. Design

### 3.1 The algorithm (pseudocode)

```
fn purge_orphaned_tool_results(messages):
    # Existing step A: collect active tool_use IDs
    active_ids = {block.id for msg in messages if !msg.compacted
                            for block in msg.content
                            if block is ToolUse}

    # Existing step B: drop orphan ToolResult blocks
    for msg in messages where !msg.compacted:
        msg.content.retain(|b| b is not ToolResult or b.tool_use_id in active_ids)
        if msg.content.empty(): msg.compacted = true

    # NEW step C: insert placeholders for orphan ToolUse blocks
    for (i, msg) in messages.enumerate() where !msg.compacted and msg.role == Assistant:
        unmatched = []
        for block in msg.content where block is ToolUse:
            if not next_user_message_has_result_for(messages, i, block.id):
                unmatched.push(block.id)
        if not unmatched.is_empty():
            ensure_next_user_message_exists(messages, i)   # insert empty user msg if needed
            for id in unmatched:
                messages[i+1].content.push(ToolResult {
                    tool_use_id: id,
                    content: "[result missing — compacted before next turn]",
                    is_error: false,
                })
```

`next_user_message_has_result_for(msgs, i, id)`:
- skips past any compacted messages between i and the next active user
  message (compacted messages are present in the array but not sent to
  the model)
- returns `true` iff the next active user message has a `ToolResult`
  with `tool_use_id == id`
- returns `false` if there's no active user message after i (then we
  insert one)

### 3.2 Why insert into the next user message vs append to the assistant

Anthropic's API requires the user message *immediately following* an
assistant with `tool_use` to contain matching `tool_result` blocks.
The repair must put placeholders in the user message, not the
assistant.

If no user message exists after the assistant (e.g., compaction left
the assistant message as the trailing message), we synthesize one:
```rust
let placeholder_user = ChatMessage {
    role: Role::User,
    content: vec![/* placeholders */],
    compacted: false,
    ..Default::default()
};
messages.insert(i + 1, placeholder_user);
```

### 3.3 Idempotency

`purge_orphaned_tool_results` is called from 4 sites and may be called
repeatedly within a turn. The repair step must be idempotent: once a
placeholder is in place, calling again should be a no-op.

This falls out naturally: on second call, the next user message *does*
contain a `ToolResult` for the unmatched `ToolUse` id, so the loop
skips it.

### 3.4 Interaction with `compacted = true`

The existing logic marks messages `compacted` when their content
becomes empty. The repair must:

1. Insert placeholders *before* the empty check (otherwise a message
   whose only original content was an orphan `ToolUse` would get
   `compacted=true` and we'd lose the chance to repair).
2. **Solution**: do the insert step *first* (now Step C above runs
   before the empty-check), or restructure to do the empty-check after
   placeholder insertion. The plan picks structure (1) for minimal diff.

Wait — the existing code does retain + empty-check in one loop. To
keep the diff minimal, we add Step C *after* the existing two-step
pass, and Step C must not depend on `compacted` flags being final.
Reread carefully:

- Existing: retain `ToolResult`s with matched `tool_use` → if empty,
  mark `compacted`.
- New: for each assistant with `tool_use`, ensure next user has
  `ToolResult` for each id.

These don't interfere — Step C only adds, never removes; the
empty-check in the existing code only marks messages with no content,
and Step C either adds to an existing user message or inserts a new
one (with non-empty content guaranteed). So order-of-operations is
safe either way.

## 4. Interfaces

No public API change.

- `purge_orphaned_tool_results(&mut [ChatMessage])` keeps the same
  signature.
- 4 call sites untouched.
- `ChatMessage` and `ContentBlock` enum untouched.

## 5. Tests

In `agent::agentic_loop` `mod tests`, alongside the existing 6
`test_purge_orphaned_tool_results_*` tests (lines 1207-1352).

| Test | Scenario | Assertion |
|---|---|---|
| `..._inserts_placeholder_for_orphan_tool_use` | Assistant has `tool_use[id="A"]`, next user msg empty | Next user gets `ToolResult{id:"A", "[result missing — ...]"}` appended |
| `..._inserts_synthesized_user_msg_when_missing` | Assistant has `tool_use`, no following user msg | New user msg inserted at i+1 with placeholder |
| `..._idempotent` | Run repair twice on same broken history | Second call is a no-op (no duplicate placeholders) |
| `..._mixed_orphan_directions` | One pair where `tool_use` survived but `tool_result` was compacted, another where the opposite | Both repaired correctly in one pass |
| `..._respects_compacted_boundary` | A `tool_use` exists in a compacted message — its id is NOT in active set — orphan `tool_result` in active set should still be dropped (existing behavior) | Verify existing path still works |

All tests are extensions of existing fixture-style tests using
`ChatMessage` constructors already in `agentic_loop.rs` test module.

## 6. Verification

### 6.1 Local

```bash
cd src-tauri && cargo test --lib agent::agentic_loop::tests::test_purge_orphaned
# Expect: 11 passing (6 existing + 5 new), 0 failing

cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty (no errors)

cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | tail -5
# Expect: clean
```

### 6.2 Regression bench (not gating PR; nice to have)

Replay a 50-turn fixture session known to trigger compaction mid-tool-call.
Before: API rejection on turn N. After: turn N succeeds with placeholder
in history, no rollout corruption.

Fixture suggestion: synthesize one in `tests/fixtures/compaction-pairing/`
— a JSONL rollout that compacts at turn 12 mid-`tool_use`. Optional;
defer if not blocking.

## 7. Migration / rollback

- **Migration**: none. No DB, no on-disk format change. Pure code.
- **Backward compat**: existing rollout JSONL files unaffected — repair
  runs at compaction time, doesn't rewrite historical data.
- **Rollback**: revert PR. The asymmetric purge is the prior steady
  state. Risk: as soon as users hit a compaction-mid-tool-call again
  they'll see the API rejection bug return — but that's the pre-PR
  state, no new damage.
- **Feature flag**: not needed. The repair is strictly safer than the
  current behavior; no behavior change for histories that don't have
  orphan `tool_use` blocks.

## 8. Decisions (locked 2026-05-25)

### 8.1 Placeholder text wording

- Chose `"[result missing — compacted before next turn]"` over Dirac's
  `"result missing"`.
- **Why**: rollout JSONL viewers will show this string to developers
  debugging; the extra context (`compacted before next turn`) tells
  them the cause without re-tracing. Costs 4 extra tokens per
  placeholder — negligible.

### 8.2 No `is_error: true` on placeholder

- Chose `is_error: false` (i.e., placeholder masquerades as success).
- **Why**: Anthropic's docs treat `is_error: true` as "the tool ran and
  failed" — the model may then try to retry or recover. We don't want
  that; we want the model to move on as if the prior tool succeeded
  and we just lost the log. `is_error: false` + the human-readable
  "[result missing ...]" string achieves both.
- **Edge case**: if the LLM specifically branched on the prior tool's
  output, it may now make a wrong follow-up decision. Accepted —
  compaction already loses information; this is a graceful degradation,
  not a regression.

### 8.3 Synthesize user message only when needed

- Chose: only insert a synthetic `User` message at `i+1` if the next
  active message is NOT already `User`.
- **Why**: minimize array mutation. In >95% of cases the next active
  message will already be a `User` turn (assistant turns alternate
  with user turns); we just append to it.

## 9. Concrete commit plan

```
Commit 1: feat(agent/agentic_loop): insert placeholder ToolResult for orphan ToolUse on compaction
          (the core fix — extend purge_orphaned_tool_results with the new symmetric repair)
Commit 2: test(agent/agentic_loop): 5 new tests covering orphan ToolUse repair + idempotency
Commit 3: docs(MILESTONE_STATUS): mark C1-Dirac-A1 complete (one-line SSoT update)
```

Three commits, ~150 lines of diff total. Bisectable.

## 10. Estimated effort

- Coding: 2-3 hours
- Tests: 1-2 hours
- Bench fixture (optional): 1 hour
- **Total: 0.5 day** (matches research doc estimate)

## 11. Closes / unblocks

- Reduces compaction-time API rejection rate to ~0 (from the
  intermittent-but-known-to-bite current state)
- Unblocks ContextManager wire-up in C2 (B2) — wire-up is safer
  knowing the post-compaction history is always API-valid
- Drives M2 progress: counts as M-Wireup (cleans up a known M2-B/H
  gap), contributes ~3% to M2 closeout

## 12. Autonomous execution mode

When this PR is executed via the autonomous orchestrator (see
[`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)),
the per-PR loop applies with these task-specific notes:

- **Scope check** (Stage 2 #5): only files listed in plan §"File
  Structure" should appear in `git diff --stat`. Test-only constructors
  in the test module are in-scope.
- **Reviewer focus** (Stage 3): the reviewer must specifically verify
  (a) signature change `&mut [..]` → `&mut Vec<..>` propagated to all
  4 call sites, (b) Step C is additive (existing Step A/B behavior
  unchanged on histories without orphan `ToolUse`), (c) idempotency
  test (#3 in §5) is a real idempotency test, not a tautology.
- **No-bench**: §6 declares no required bench. Stage 2 #10 doesn't apply.
- **Risk class**: LOW — pure additive logic, no schema/migration.
