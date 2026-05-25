# Dirac-A1 — Tool-Use/Result Pair Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `agentic_loop::purge_orphaned_tool_results` to symmetrically repair both directions of orphaned tool_use/tool_result pairs after compaction, eliminating intermittent Anthropic API rejections (`tool_use without matching tool_result`).

**Architecture:** Single pure-function change — no new types, no new modules, no DB. Add a third pass after the existing two-pass logic that, for each non-compacted Assistant message with a `ToolUse`, ensures the next non-compacted User message has a matching `ToolResult`, inserting a placeholder if needed.

**Tech Stack:** Rust only. No new crates. Reuses existing `ChatMessage` / `ContentBlock` enum + existing test module.

**Spec:** `docs/superpowers/specs/2026-05-25-dirac-a1-tool-pairing-repair-design.md`

**PR tag:** `[C1-Dirac-A1]` (allocate concrete `C1-T<N>` number against MILESTONE_STATUS before opening PR)

---

## File Structure

### Modified files

| Path | What changes |
|---|---|
| `src-tauri/src/agent/agentic_loop.rs:493-524` | `purge_orphaned_tool_results` gets Step C (placeholder insertion). +60-80 lines including helpers. |
| `src-tauri/src/agent/agentic_loop.rs::mod tests` (around line 1207) | +5 new tests, all inline. ~150 lines test code. |
| `docs/superpowers/MILESTONE_STATUS.md` | One-line addition under M2 detailed status (Step 4.3) |

**No new files. No new modules. No schema migration.**

---

## Pre-flight

- [ ] **Step 0.1: Branch + baseline check**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/dirac-a1-tool-pairing-repair
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -20
echo "=== rust build ===" && (cd src-tauri && cargo build 2>&1 | grep -E "^error" | head)
echo "=== existing tests ===" && (cd src-tauri && cargo test --lib agent::agentic_loop::tests::test_purge_orphaned 2>&1 | tail -10)
```

Expected:
- Drift check GREEN or YELLOW (not RED). If RED — STOP, raise the
  drift flag in the first reply to the user; do not proceed without
  ack.
- `cargo build` clean
- 6 existing `test_purge_orphaned_tool_results_*` tests passing

- [ ] **Step 0.2: Confirm SSoT + integration strategy understanding**

Read:
- [`docs/superpowers/MILESTONE_STATUS.md`](../MILESTONE_STATUS.md) §M2
- [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](2026-05-22-pr-integration-strategy.md) §7 (C1 ordering)

Verify: this PR is a C1 sub-task (M2 closeout). It does NOT start C2 work.

---

## Task 1: Helper — `find_next_active_message_idx`

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs`

- [ ] **Step 1.1: Add private helper above `purge_orphaned_tool_results`**

Add immediately before line 493:

```rust
/// Find the index of the next message at or after `from_idx` whose
/// `compacted` flag is `false`. Returns `None` if no such message exists.
///
/// Used by the pair-repair logic in `purge_orphaned_tool_results` to skip
/// over compacted messages (which remain in the array but are not sent
/// to the model) when locating the user turn that should contain
/// `ToolResult` blocks for an assistant's `ToolUse` blocks.
fn find_next_active_message_idx(messages: &[ChatMessage], from_idx: usize) -> Option<usize> {
    messages.iter()
        .enumerate()
        .skip(from_idx)
        .find(|(_, msg)| !msg.compacted)
        .map(|(idx, _)| idx)
}

#[cfg(test)]
mod find_next_active_tests {
    use super::*;
    // 2-3 inline tests for the helper itself
}
```

Tests for the helper (in the small adjacent module):

- [ ] **Step 1.2: Helper unit tests (inline `find_next_active_tests` mod)**

```rust
#[test]
fn test_find_next_active_skips_compacted() {
    let mut msgs = vec![
        ChatMessage::user("u1"),
        ChatMessage::assistant("a1"),
        ChatMessage::user("u2"),
    ];
    msgs[0].compacted = true;
    msgs[1].compacted = true;
    assert_eq!(find_next_active_message_idx(&msgs, 0), Some(2));
}

#[test]
fn test_find_next_active_none_at_end() {
    let mut msgs = vec![ChatMessage::user("u1")];
    msgs[0].compacted = true;
    assert_eq!(find_next_active_message_idx(&msgs, 0), None);
}
```

Verify:
```bash
cd src-tauri && cargo test --lib agent::agentic_loop::find_next_active_tests
# Expect: 2 passing
```

**Commit checkpoint:** Do NOT commit yet — combine with Task 2 in commit 1.

---

## Task 2: Add Step C — placeholder insertion for orphan `ToolUse`

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs:493-524`

- [ ] **Step 2.1: Identify the placeholder content constant**

Add at top of the file, after existing imports:

```rust
/// Placeholder string inserted into a fabricated `ToolResult` when a
/// `ToolUse`'s matching result was lost to compaction. Kept const so it
/// can be matched on by rollout-replay tools and surfaced in the trace UI.
pub(crate) const COMPACTED_TOOL_RESULT_PLACEHOLDER: &str =
    "[result missing — compacted before next turn]";
```

- [ ] **Step 2.2: Extend `purge_orphaned_tool_results` with Step C**

Replace the function body. The existing two-step logic stays; add a
third pass:

```rust
pub fn purge_orphaned_tool_results(messages: &mut [ChatMessage]) {
    // ─── Step A: collect active tool_use IDs (unchanged) ───────────
    let mut active_tool_call_ids = std::collections::HashSet::new();
    for msg in messages.iter() {
        if !msg.compacted {
            for block in &msg.content {
                if let ContentBlock::ToolUse { id, .. } = block {
                    active_tool_call_ids.insert(id.clone());
                }
            }
        }
    }

    // ─── Step B: drop orphan ToolResult blocks (unchanged) ─────────
    for msg in messages.iter_mut() {
        if !msg.compacted {
            msg.content.retain(|block| {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    active_tool_call_ids.contains(tool_use_id)
                } else {
                    true
                }
            });
            if msg.content.is_empty() {
                msg.compacted = true;
            }
        }
    }

    // ─── Step C: insert placeholder ToolResult for orphan ToolUse ──
    // For each non-compacted Assistant message with ToolUse blocks,
    // ensure the next non-compacted User message contains a matching
    // ToolResult for each of those ids. Insert placeholders as needed.
    repair_orphan_tool_use_placeholders(messages);
}

fn repair_orphan_tool_use_placeholders(messages: &mut Vec<ChatMessage>) {
    // Collect (assistant_idx, [orphan_ids]) tuples first to avoid
    // holding mut borrow during the mutation pass.
    let mut to_repair: Vec<(usize, Vec<String>)> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        if msg.compacted || msg.role != Role::Assistant {
            continue;
        }
        let tool_use_ids: Vec<String> = msg.content.iter()
            .filter_map(|b| if let ContentBlock::ToolUse { id, .. } = b {
                Some(id.clone())
            } else { None })
            .collect();
        if tool_use_ids.is_empty() { continue; }

        let next_active = find_next_active_message_idx(messages, i + 1);
        let already_matched: std::collections::HashSet<String> = match next_active {
            Some(idx) if messages[idx].role == Role::User => {
                messages[idx].content.iter()
                    .filter_map(|b| if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                        Some(tool_use_id.clone())
                    } else { None })
                    .collect()
            }
            _ => std::collections::HashSet::new(),
        };

        let orphans: Vec<String> = tool_use_ids.into_iter()
            .filter(|id| !already_matched.contains(id))
            .collect();
        if !orphans.is_empty() {
            to_repair.push((i, orphans));
        }
    }

    // Apply repairs (iterate in reverse so insertions don't shift
    // unprocessed indices)
    for (i, orphan_ids) in to_repair.into_iter().rev() {
        let placeholders: Vec<ContentBlock> = orphan_ids.into_iter()
            .map(|id| ContentBlock::ToolResult {
                tool_use_id: id,
                content: COMPACTED_TOOL_RESULT_PLACEHOLDER.into(),
                is_error: false,
            })
            .collect();

        let next_active = find_next_active_message_idx(messages, i + 1);
        match next_active {
            Some(idx) if messages[idx].role == Role::User => {
                // Append to existing user message
                messages[idx].content.extend(placeholders);
            }
            _ => {
                // Synthesize new user message at i+1
                let new_msg = ChatMessage {
                    role: Role::User,
                    content: placeholders,
                    compacted: false,
                    ..Default::default()
                };
                messages.insert(i + 1, new_msg);
            }
        }
    }
}
```

> **Note:** `messages: &mut [ChatMessage]` won't allow `insert`. Change
> the signature to `&mut Vec<ChatMessage>` and update the 4 call sites
> at lines 624, 798, 969, and `force_compact_sync:624` accordingly.
> They all already pass `&mut reason_ctx.messages` which is `Vec`, so
> the change is mechanical.

- [ ] **Step 2.3: Update call sites for `&mut Vec`**

Adjust 4 call sites — they currently look like
`purge_orphaned_tool_results(&mut reason_ctx.messages)`. The `messages`
field of `ReasoningContext` is already `Vec<ChatMessage>` so the
`&mut Vec` borrow Just Works. **Verify** with `grep -n
"purge_orphaned_tool_results" src-tauri/src/agent/agentic_loop.rs` —
all 4 should compile after the signature change.

- [ ] **Step 2.4: Local build + existing tests still pass**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty

cd src-tauri && cargo test --lib agent::agentic_loop::tests::test_purge_orphaned
# Expect: 6 existing tests still passing (Step C is additive)
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(agent/agentic_loop): insert placeholder ToolResult for orphan ToolUse on compaction

Adds Step C to purge_orphaned_tool_results: for each non-compacted
Assistant message with ToolUse blocks, ensures the next non-compacted
User message contains a matching ToolResult, inserting a placeholder
with content COMPACTED_TOOL_RESULT_PLACEHOLDER if needed. Synthesizes
a new User message at i+1 if no active User follows the Assistant.

Repairs the broken half of the pairing invariant that the existing
retain() loop only enforced in one direction. Eliminates intermittent
Anthropic API rejections (\"tool_use without matching tool_result\")
after compaction lands on a boundary mid-tool-call.

Spec: docs/superpowers/specs/2026-05-25-dirac-a1-tool-pairing-repair-design.md
Inspired by Dirac ContextManager.ensureToolResultsFollowToolUse
(src/core/context/context-management/ContextManager.ts:287-393)."
```

---

## Task 3: Five new tests

**Files:**
- Modify: `src-tauri/src/agent/agentic_loop.rs` (test module around line 1207)

- [ ] **Step 3.1: Add `..._inserts_placeholder_for_orphan_tool_use`**

```rust
#[test]
fn test_purge_orphaned_tool_results_inserts_placeholder_for_orphan_tool_use() {
    let mut messages = vec![
        ChatMessage::assistant_with_tool_use("calling tool", "call_A", "ls", json!({})),
        ChatMessage::user("ack but no tool_result yet"),
    ];
    purge_orphaned_tool_results(&mut messages);

    // User message now has placeholder
    let user_results: Vec<_> = messages[1].content.iter()
        .filter_map(|b| if let ContentBlock::ToolResult { tool_use_id, content, .. } = b {
            Some((tool_use_id.clone(), content.clone()))
        } else { None })
        .collect();
    assert_eq!(user_results.len(), 1);
    assert_eq!(user_results[0].0, "call_A");
    assert!(user_results[0].1.contains("result missing"));
}
```

- [ ] **Step 3.2: Add `..._inserts_synthesized_user_msg_when_missing`**

```rust
#[test]
fn test_purge_orphaned_tool_results_inserts_synthesized_user_msg_when_missing() {
    let mut messages = vec![
        ChatMessage::user("initial"),
        ChatMessage::assistant_with_tool_use("done", "call_X", "ls", json!({})),
    ];
    purge_orphaned_tool_results(&mut messages);
    assert_eq!(messages.len(), 3, "should synthesize trailing user message");
    assert_eq!(messages[2].role, Role::User);
    let placeholder = messages[2].content.iter()
        .find_map(|b| if let ContentBlock::ToolResult { tool_use_id, .. } = b {
            Some(tool_use_id.clone())
        } else { None });
    assert_eq!(placeholder, Some("call_X".into()));
}
```

- [ ] **Step 3.3: Add `..._idempotent`**

```rust
#[test]
fn test_purge_orphaned_tool_results_idempotent() {
    let mut messages = vec![
        ChatMessage::assistant_with_tool_use("done", "call_Y", "ls", json!({})),
        ChatMessage::user("placeholder"),
    ];
    purge_orphaned_tool_results(&mut messages);
    let after_first = messages.clone();
    purge_orphaned_tool_results(&mut messages);
    assert_eq!(messages, after_first, "second call should be a no-op");
}
```

- [ ] **Step 3.4: Add `..._mixed_orphan_directions`**

```rust
#[test]
fn test_purge_orphaned_tool_results_mixed_orphan_directions() {
    // Direction 1: tool_use surviving, tool_result lost
    // Direction 2: tool_result surviving, tool_use compacted
    let mut messages = vec![
        ChatMessage::assistant_with_tool_use("a1", "call_lost", "ls", json!({})),
        ChatMessage::user("u1"), // empty content; tool_result for call_lost was compacted
        ChatMessage::user_with_tool_result("call_phantom", "stale output"),
    ];
    messages[0].compacted = false;
    messages[1].compacted = false;
    messages[2].compacted = false;

    purge_orphaned_tool_results(&mut messages);

    // Direction 1: placeholder inserted in messages[1]
    assert!(messages[1].content.iter().any(|b|
        matches!(b, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_lost")
    ));

    // Direction 2: stale tool_result removed from messages[2]
    assert!(!messages[2].content.iter().any(|b|
        matches!(b, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_phantom")
    ));
}
```

- [ ] **Step 3.5: Add `..._respects_compacted_boundary`**

```rust
#[test]
fn test_purge_orphaned_tool_results_respects_compacted_boundary() {
    let mut messages = vec![
        ChatMessage::assistant_with_tool_use("compacted", "call_C", "ls", json!({})),
        ChatMessage::user_with_tool_result("call_C", "result"),
        ChatMessage::user("active follow-up"),
    ];
    messages[0].compacted = true; // tool_use is in compacted msg

    purge_orphaned_tool_results(&mut messages);

    // Step B should have dropped the orphan tool_result from messages[1]
    assert!(!messages[1].content.iter().any(|b|
        matches!(b, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_C")
    ));
    // Step C should NOT have inserted a placeholder (the tool_use is compacted, not active)
    assert!(messages.iter().all(|m|
        !m.content.iter().any(|b|
            matches!(b, ContentBlock::ToolResult { content, .. } if content.contains("result missing"))
        )
    ));
}
```

- [ ] **Step 3.6: Run all tests**

```bash
cd src-tauri && cargo test --lib agent::agentic_loop::tests::test_purge_orphaned 2>&1 | tail -15
# Expect: 11 passing (6 existing + 5 new), 0 failing
```

If any helper constructor (`ChatMessage::assistant_with_tool_use`,
`ChatMessage::user_with_tool_result`) doesn't exist, **add it as a
test-only constructor** in the existing test module before the new
tests. Don't pollute production code with test constructors.

**Commit checkpoint:**
```
git add -A
git commit -m "test(agent/agentic_loop): cover orphan ToolUse repair paths

Adds 5 unit tests for Step C of purge_orphaned_tool_results:
- inserts placeholder for orphan ToolUse
- synthesizes trailing User message when missing
- idempotent on repeated calls
- handles both pairing directions in one pass
- still respects compacted boundary (orphan ToolResult dropped)"
```

---

## Task 4: SSoT update

**Files:**
- Modify: `docs/superpowers/MILESTONE_STATUS.md`

- [ ] **Step 4.1: Add one-line entry under M2 detailed status**

In §M2 — Context Fabric, append to the relevant slice / sub-task table:

```
| C1-Dirac-A1 | tool_use/tool_result pair repair on compaction | #<PR-number> |
```

If unsure which sub-table to use, place it under M2-B (Context
Management) — the repair concerns compacted context invariants.

- [ ] **Step 4.2: Update header `**After PR**:` line**

Change to: `**After PR**: #<PR-number> (Dirac-A1 — tool pairing repair)`

- [ ] **Step 4.3: Run drift check, confirm GREEN**

```bash
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
# Expect: GREEN or YELLOW; if RED, investigate (likely unrelated)
```

**Commit checkpoint:**
```
git add docs/superpowers/MILESTONE_STATUS.md
git commit -m "docs(MILESTONE_STATUS): record C1-Dirac-A1 completion"
```

---

## Task 5: Final verification + PR

- [ ] **Step 5.1: Full verification sweep**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head           # empty
cd src-tauri && cargo test --lib 2>&1 | tail -5                       # all passing
cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | tail -5      # clean
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -5  # GREEN/YELLOW
```

Paste each command's tail output in the PR description so reviewer
sees evidence rather than claim.

- [ ] **Step 5.2: Push branch + open PR**

```bash
git push -u origin claude/dirac-a1-tool-pairing-repair
gh pr create \
  --title "[C1-Dirac-A1] feat(agent): tool_use/tool_result pair repair on compaction" \
  --body-file <(cat <<'EOF'
## Summary

Extends `purge_orphaned_tool_results` to symmetrically repair both
directions of orphaned tool_use/tool_result pairs after compaction.

## Why

Existing implementation only handles `orphan_tool_result` (delete).
Anthropic API rejects `orphan_tool_use` (`tool_use without matching
tool_result`) — intermittent crash when compaction lands on a
boundary mid-tool-call.

Inspired by Dirac `ContextManager.ensureToolResultsFollowToolUse`
(reverse-engineered in `docs/research/2026-05-25-dirac-reverse-engineering.md` §2.1).

## Commits (bisectable)

| Commit | Purpose |
|---|---|
| 1 | feat: insert placeholder ToolResult for orphan ToolUse |
| 2 | test: 5 new tests covering Step C + idempotency |
| 3 | docs(MILESTONE_STATUS): record C1-Dirac-A1 completion |

## Verification

\`\`\`
$ cargo test --lib agent::agentic_loop::tests::test_purge_orphaned
test result: ok. 11 passed; 0 failed
\`\`\`

## Spec

`docs/superpowers/specs/2026-05-25-dirac-a1-tool-pairing-repair-design.md`

## Closes

- C1-Dirac-A1 (per Phase A plan in `docs/research/2026-05-25-dirac-reverse-engineering.md` §7.2)
EOF
)
```

- [ ] **Step 5.3: Self-merge gate**

Do not merge until:
- [ ] CI green
- [ ] One reviewer (or self if working solo per project conventions) approved
- [ ] PR title carries `[C1-Dirac-A1]` tag
- [ ] PR body has Commits (bisectable) table

---

## Rollback procedure

If a regression surfaces post-merge:

```bash
git revert <merge-commit-sha>
git push
```

Pre-PR state restores: orphan ToolUse remains broken (intermittent),
orphan ToolResult drop still works. No data corruption either way —
repair is in-memory only.

---

## Closes / unblocks

- C1-Dirac-A1 ✓
- Unblocks: C2 ContextManager wire-up (B2 in research plan) — safe to
  wire knowing post-compaction histories are API-valid
- Drives M2 progress ~+3% (M-Wireup category)

---

## Task A (autonomous mode only) — Self-verify + adversarial review + auto-merge

> **Skip this task** if executing manually (you'll review the PR
> yourself before merge). **Run this task** if invoked by the
> autonomous orchestrator (see
> [`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)).

- [ ] **Step A.1: Stage 2 self-verify checklist**

Run each check; abort to ESCALATE on any non-auto-fixable failure:

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head            # MUST be empty
cargo test --lib agent::agentic_loop 2>&1 | tail -10  # all green incl. new tests
cargo clippy --lib -- -D warnings 2>&1 | tail -5      # clean (auto-fix 2 attempts)

# Plan checkbox completion
grep -c "^- \[x\]" docs/superpowers/plans/2026-05-25-dirac-a1-tool-pairing-repair.md  # all expected ticked

# Scope check
git diff --stat main..HEAD
# Expected files ONLY: src-tauri/src/agent/agentic_loop.rs + docs/superpowers/MILESTONE_STATUS.md
# Any extra file → ESCALATE

# Quality grep
git diff main..HEAD -- src-tauri/src | grep -E "(unimplemented!|todo!|panic!\(.*not impl)" && echo "ESCALATE" || echo "clean"
git diff main..HEAD -- src-tauri/src | grep -E "\.unwrap\(\)" | grep -v "cfg(test)" && echo "review unwraps" || echo "no new unwraps"

# SSoT
git diff main..HEAD -- docs/superpowers/MILESTONE_STATUS.md | wc -l  # MUST be > 0
```

Log all results to `autonomous-execution-log.md`.

- [ ] **Step A.2: Spawn adversarial reviewer subagent**

Use the `Task` tool with `general-purpose` type, prompt = protocol §3.3 reviewer template. Pass:
- `spec_path`: absolute path to spec
- `plan_path`: absolute path to this plan
- `diff`: output of `git diff main..HEAD`
- `self_verify_log`: content of Stage A.1 log

Wait for verdict. Capture verbatim.

- [ ] **Step A.3: Reconcile per protocol §3.4**

- APPROVE → Step A.4
- REQUEST_CHANGES (low) → apply fixes inline, re-run Step A.1, then Step A.4
- REQUEST_CHANGES (medium/high) → apply fixes, re-run Step A.1, spawn a NEW reviewer subagent (Step A.2 fresh context), then Step A.4 if APPROVE — else ESCALATE
- ESCALATE → write `escalation/C1-Dirac-A1-<timestamp>.md`, halt

- [ ] **Step A.4: Open PR + auto-merge**

```bash
git push -u origin claude/dirac-a1-tool-pairing-repair

PR_NUMBER=$(gh pr create \
  --title "[C1-Dirac-A1] feat(agent): tool_use/tool_result pair repair on compaction" \
  --body-file <(...PR body per Task 5 Step 5.2...) \
  --json number -q .number)

# Wait for CI to start, then complete
gh pr checks $PR_NUMBER --watch --interval 30 --required

# If CI green
gh pr merge $PR_NUMBER --merge --delete-branch

# Refresh local main
git checkout main && git pull
```

CI red → ESCALATE (do not retry).

- [ ] **Step A.5: Log merge in autonomous-execution-log.md + autonomous-execution-summary.md**

Per protocol §7. Then return `{ outcome: MERGED, pr_number, merge_sha, reviewer_iterations: N }` to orchestrator.
