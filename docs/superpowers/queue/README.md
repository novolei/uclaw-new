# Queue-Driven Execution

> Generic pattern for chaining milestone-aligned PRs in sequence.
> Created 2026-05-22 as part of the closed-loop progress tracking
> bootstrap (see [`../plans/2026-05-22-pr-integration-strategy.md`](../plans/2026-05-22-pr-integration-strategy.md)).

---

## Three execution modes

### Mode 1: Attended sessions, queue file drives priority

Use for milestone close-out work where each PR needs human review.

- Queue file lives at `docs/superpowers/queue/<phase>-execution-queue.md`
- Agent reads file at session start, picks first `[ ]`, executes, marks `[x]`
- Trigger: user says "继续 C1 队列下一项" / "next slice" / "queue next"
- Skill `uclaw-milestone-closed-loop` includes a "queue execution" rule
  (see §"Queue execution" in that skill)

**Active queues**:
- [`C1-execution-queue.md`](C1-execution-queue.md) — M2 closeout (7 items)

**Archived**: `archive/<date>-<phase>-completed.md`

### Mode 2: Unattended scheduled chain

Use for big sweeps (M3 / M4 with 10+ PRs) where you want overnight
progression. Each scheduled task ends by creating the next one.

Template prompt at [`../specs/2026-05-22-mode-2-scheduled-chain-prompt.md`](../specs/2026-05-22-mode-2-scheduled-chain-prompt.md).

To kick off:
```
User: 开始 mode 2 自我繁衍链,起点 C1.1 PR-1,每 90 分钟一个 slot,8 个任务后停下来等我 review
```

Agent invokes `mcp__scheduled-tasks__create_scheduled_task` with the
template prompt, sets `fireAt` to the next slot. The agent run for that
slot reads the queue, does the work, and schedules the *next* task
before exiting — forming the chain.

**Circuit breakers** (each scheduled task self-aborts the chain when):
- `cargo build` fails (don't continue stacking broken code)
- drift-check returns RED
- Queue is empty
- Hits the user-configured max (e.g. 8 tasks)

### Mode 3: Sub-agent parallel delegation

Use only for queue items that are **declaratively parallel-safe**
(e.g. M4 adapter wire-up: T3 FS / T4 Git / T5 Browser are independent).

- Master agent reads queue
- For each parallel-marked item, spawns a sub-agent via `Agent(subagent_type="general-purpose", isolation="worktree")`
- Sub-agent works in isolated worktree (own git tree, no main conflict)
- Master collects results, opens umbrella PR linking sub-results

Queue items declare parallel-safety via a `parallel: true` marker;
items without the marker are serial-only.

---

## CLI helper

```bash
./scripts/queue-next.sh                  # Show C1 queue + next item
./scripts/queue-next.sh C2               # Show C2 queue (when it exists)
./scripts/queue-next.sh --done C1.1-PR-1 # Mark item as done (also accepts PR#)
```

---

## When to create a new queue file

When starting a new C* phase or milestone closeout:

1. Look at the strategy doc §7 (C1/C2/C3 sequences) for the item list
2. Copy [`C1-execution-queue.md`](C1-execution-queue.md) as a starting template
3. Replace items per the new phase
4. Commit + push as part of the kickoff PR for that phase

---

## Conventions

- Every queue item has a `Spec` link (write the spec first per closed-loop §5.1 Rule 3)
- Every item has a `Done means` definition — pass/fail clearly
- Every item has a `Branch` name in `prep/<slug>` form
- Items can be commented out / temporarily skipped with `<!-- ... -->` if blocked
- Don't reorder items casually — dependencies are encoded by position
