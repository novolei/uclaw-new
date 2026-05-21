# Slice 3 收尾验证 — 50 轮会话 token 走势对比

**Status:** Slice 3 closed. All four pieces in main; verification harness sketched.
**Date:** 2026-05-21
**Refs:** Slice 3-A (PR #367 / Bundle 4-6), Slice 3-B (#378 / Bundle 10), Slice 3-C (#379 / Bundle 11)

## What Slice 3 shipped

| Piece | PR | What it does |
| --- | --- | --- |
| Slice 3-A: `/compact` LLM-driven StructuredFold | #367 (orig) + Bundle 4-6 hotfixes | At `/compact` the last N turns are folded into a structured 8-section summary that replaces the verbose history. Saves ~5-10× input tokens after a compaction. |
| Slice 3-B: M2-D diff-based re-injection observability | #378 | Per-iter FragmentSnapshot diff over `memory_context`. Today: logs only. Next: drives a `<context_diff>` block instead of full re-injection. |
| Slice 3-C: M2-I 4-breakpoint cache placement | #379 | Adds 2 message-level `cache_control: ephemeral` markers (last + 2nd-to-last user) on top of the existing system + last-tool markers. Full 4/4 budget. |
| Followup: L2 depth-pruning type-preserving | #377 (Bundle 9) | Re-enabled L2 normalize at DEFAULT_MAX_NESTING_DEPTH (Bundle 5 hotfix had to `usize::MAX` it). Type-safe now. |

## Why 50 turns

The Anthropic prompt cache TTL is ~5 min in ephemeral mode. A 50-turn
conversation sustained over ~3-5 minutes is the natural envelope to
observe:

1. The "first turn" cost (full input fresh) — sets the ceiling
2. The "n-th turn" steady-state cost (cache reads dominate)
3. Compaction events: token usage falls sharply at the `/compact`
   boundary, then ramps again
4. Diff-injection events: when memory_context drifts (recall finds
   new candidates), the M2-D log surfaces the moment

## Verification approach (no automated harness this round)

Building a full 50-turn driver that talks to the real Anthropic API
needs API keys + budget. We don't want to spend production budget for
this round. **Manual verification using the existing dev environment**
is the right scope:

### Step 1 — Snapshot baseline

Before pulling any Slice 3 PR, in a fresh dev session:

```bash
git checkout 9af1998   # one commit before Bundle 9
cd ui && npm run tauri:dev
# Send 5 short messages to an Agent session, e.g.:
#   "你好" / "今天几号" / "memory.md 里有什么" / "list 3 things" / "OK"
# Open Settings → Diagnostics → 复制 Token Budget 区域的 5 个 turn 的数字
```

Expected (pre-Slice 3): every turn re-sends the full system + tool +
history → input tokens grow linearly with turn N.

### Step 2 — Snapshot post-Slice-3

Same dev session but on `main` (Slice 3 + all bundles):

```bash
git checkout main
cd ui && npm run tauri:dev
# Same 5 messages.
# Also try /compact after turn 3 to verify the structured fold UI.
```

Expected:
- Iteration 1 of any turn: similar input tokens to baseline (cache
  cold)
- Iteration 2+ of the same turn: massively lower input tokens
  (cache_read_input_tokens dominates)
- After `/compact`: input_tokens drops sharply on the next turn

### Step 3 — Verify the new tracing surfaces

```bash
# In the running dev binary, tail logs:
grep -E "M2-D|cache_breakpoints|deep_nests_pruned" /tmp/uclaw-dev.log
```

Expected log lines:

- `[M2-D] memory_context first injection` — turn 1 of each session
- `[M2-D] memory_context unchanged vs prior iter (cache hit expected)`
  — within a multi-iter turn
- `[L2] normalized tool schemas examples_dropped=N enums_deduped=M
  deep_nests_pruned=K tool_count=T` — at least once if any tool schema
  has deep nesting (most don't, so K=0 is normal)
- For Anthropic specifically (Claude models): the 4 cache_control
  markers travel in the body; verify with `grep cache_control` on the
  outbound request payload if we add an additional verbose-logging
  flag (not in this slice).

## Token-savings expected order of magnitude

Combining all of Slice 1-3:

| Layer | Approx savings |
| --- | --- |
| L1 truncation (turn caps) | -10-20% on long-history turns |
| L2 schema normalize | -200-500 tokens / turn from `description.examples` drop |
| L3 per-turn skill top-K | -300-700 tokens / turn when skills don't all qualify |
| L5 image stripping (DeepSeek/Kimi) | n/a for text-only models; -2-10K when screenshots present |
| L7 + M2-G fold (/compact) | -50-80% on the post-/compact turn |
| M2-D observability | 0 today; primes future re-injection saving |
| M2-I caching (Anthropic) | -80-90% on iter 2+ within a turn (cache_read_input_tokens) |

**Steady-state target:** a 50-turn conversation with Claude should
average input_tokens per turn that is ~30-40% of what the same
conversation would have cost on `main@9af1998` (pre-Slice 3). The
non-Claude providers (DeepSeek / Kimi) get the L1-L5 savings but not
M2-I (no prompt caching on those APIs).

## What's NOT in this verification

- **Automated regression** — there's no CI job comparing token counts
  across PRs. Manual diff is sufficient until a paying user reports
  a regression.
- **Full M2-D diff-injection** — Slice 3-B is observability-only. The
  real wire-up (sending `<context_diff>` to the LLM in place of full
  memory_context) is M2-D phase 2, separate slice.
- **Provider caching for non-Anthropic** — DeepSeek/Kimi don't expose
  `cache_control`-equivalent fields. Their input-token cost stays
  linear in conversation length until those vendors ship caching.

## Sign-off

Slice 3 closes with this doc. The remaining backlog items
(L2 depth re-design completed in Bundle 9, Slice 3-B/C in Bundles
10/11, this verification doc) are all merged. Next slice can start
M2-D phase 2 (real diff-injection content) or M3 (registry/agent
orchestration) — both are documented in `2026-05-20-uclaw-agent-platform-north-star.md`.
