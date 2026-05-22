---
name: uclaw-milestone-closed-loop
description: Use BEFORE any milestone-related work (推进主线 / continue main line / advance milestone / open new PR for M-* / Bundle wire-up / M2 closeout / M3 wire-up / M4 wire-up / queue-next / 继续队列下一项 / next slice). Trigger phrases include "推进主线", "继续主线", "C1", "C2", "C3", "M2 收尾", "M3 wire-up", "M4 wire-up", "Bundle 17-B", "Bundle 17-C", "milestone closeout", "open prep branch", "next milestone slice", "main line drift", "drift check", "继续队列下一项", "next queue item", "next slice", "queue next", "Mode 2 chain". Loads the closed-loop progress tracking discipline: read MILESTONE_STATUS.md SSoT first, run drift-check script, classify intended PR, update SSoT after merge, and (if queue-driven) execute the next unchecked queue item per spec.
---

# uClaw — Milestone Closed-Loop Discipline

**Why this skill exists**: Bundle 18-27 (2026-05-21) showed how easily
tactical-vs-strategic drift accumulates — 18 PRs in one day, zero
milestone progress. PR #396 settings exposure recovered, but the loss
of 2026-05-21-pr-integration-strategy.md exposed a deeper hole: no
single source of truth for milestone state. This skill encodes the
discipline that closes that loop.

## ⚠️ MUST DO at session start (before any code edits)

```bash
# 1. Read the SSoT — current state of M0-M9
cat docs/superpowers/MILESTONE_STATUS.md | head -40

# 2. Read the strategy doc — methodology + cutoff criteria
cat docs/superpowers/plans/2026-05-22-pr-integration-strategy.md | head -80

# 3. Run drift check — see if previous week is healthy
./scripts/milestone-drift-check.sh --since "1 week ago"
```

If drift check reports RED or YELLOW, **flag it in the first response
to the user**. Don't quietly proceed with tactical work when a YELLOW
or RED alarm is already live.

## The 5-axis closed-loop process

Reference: `docs/superpowers/plans/2026-05-22-pr-integration-strategy.md` §5.

### Per-PR (every PR, < 2 min after merge)

1. **Tag the PR**: every PR title or commit message must carry a milestone
   tag, exactly one of:
   - `[M<N>-T<X>]` — direct task per plan §X.2 (e.g. `[M3-T2]`)
   - `[M<N>-T<X> pilot]` — pilot/types-only (e.g. `[M3-T6 pilot]`)
   - `[M<N>-T<X> wire-up]` — wire-up of an existing pilot (e.g. `[M2-D wire-up]`)
   - `[Slice <N>-<X>]` — bigger wire-up slice (e.g. `[Slice 4-A]`)
   - `[Bundle <N>]` — tactical bug fix / dogfood patch (e.g. `[Bundle 28]`)
   - `[Phase 0.5-T<X>]` — infrastructure work (rare post-2026-05-20)
   - `[Backlog]` — explicit "not in milestone" tag (use sparingly)
2. **Update MILESTONE_STATUS.md**: append PR # to the relevant cell;
   adjust % if the cell changed status. 1-line edit usually enough.
3. **For wire-up / closeout PRs**: think about whether this is M2 / M3 /
   M4 closeout-eligible per §6 cutoff standards. If yes, plan the
   `docs/superpowers/reports/<date>-M<N>-closeout.md` report.

### Per-week (Mondays, < 30 min)

```bash
./scripts/milestone-drift-check.sh --since "1 week ago" --update-status
```

- If YELLOW: leave a NOTE at top of MILESTONE_STATUS.md drift log.
- If RED: STOP. The next thing to do is finish in-flight milestone slice
  before opening any tactical Bundle PR.

### Per-month (month-end audit)

Update `uclaw-upgrade-implementation-plan.md` §34 with month-end snapshot,
bump plan version (v2.4 → v2.5 → ...), close completed milestones into
"Hall of fame" in MILESTONE_STATUS.md, link the closeout report.

## Decision rules (forcing functions)

### Rule 1: PRs that move milestone % MUST update MILESTONE_STATUS.md

The PR is not "done" until that 1-line edit is in. Don't merge a wire-up
PR without it.

### Rule 2: Bundle / Backlog PRs require explicit justification

If you're about to open a Bundle (tactical) PR:

1. Run drift check; if already RED alarm, **don't open it** — finish
   the milestone slice first
2. If GREEN/YELLOW, open it but include in PR description:
   - what it unblocks
   - which milestone it indirectly serves (or "pure dogfood reflex")
3. If 7-day Bundle count is at 5+ already, hard stop until next week
   (or until a wire-up PR ships to reset the counter)

### Rule 3: Spec-first for any wire-up

Before opening a `prep/` branch for wire-up of any M-T pilot:

1. Look for an existing spec in `docs/superpowers/specs/` covering it
2. If absent, write one first (`<date>-<task>-design.md` per the Bundle
   17-B/C example), commit, push — that's its own commit
3. THEN open the prep branch with implementation

Cuts ambiguity, makes review faster, prevents re-litigating design mid-PR.

### Rule 4: Closeout requires a retro

Per strategy §6: a milestone closes only when:
- ADR §16 Exit Criteria ✓
- Plan §X.3 DoD ✓
- Quantitative benchmark data exists
- Closeout report at `docs/superpowers/reports/<date>-M<N>-closeout.md`
- PR description says "Closes M<N>" + links above

## Queue execution (when user says "继续队列下一项" / "next slice" / "queue next")

The closed-loop has a queue artifact at
`docs/superpowers/queue/<phase>-execution-queue.md`. Each queue item is
a self-contained PR with a spec link + branch name + done criteria.

### Procedure when user triggers queue-next

1. Run the queue helper to see state:
   ```bash
   ./scripts/queue-next.sh                 # default: C1
   ./scripts/queue-next.sh C2              # if user specified
   ```
2. The output shows the **next unchecked item** with all its metadata.
3. Read its **Spec** link before any code work.
   - If spec is `_(needs to be written)_`: write the spec FIRST as a
     separate commit, push, **then** return to user for sign-off on
     the spec before opening the impl prep branch (don't bypass §5.1
     Rule 3 spec-first).
4. Run the closed-loop discipline at session start (read SSoT + drift check).
5. `git checkout -b <branch from queue item>`.
6. Execute commits per spec.
7. **After PR opens or merges** (depending on user's review preference):
   - Mark item done: `./scripts/queue-next.sh --done <item-title-prefix>`
     (e.g. `./scripts/queue-next.sh --done C1.1-PR-1` won't match because
     the title says "Bundle 17-B" — use that: `--done "Bundle 17-B"`)
   - Edit the queue file's "Actual PR" cell to fill the PR number
   - Update MILESTONE_STATUS.md to reflect new progress (e.g. M2 →
     58% if C1.1 PR-1 closes)
8. Report to user: "Done with item X. Next is Y (or queue complete)."

### Mode 2 (self-perpetuating chain)

If user invokes Mode 2 (overnight unattended chain), see the binding
template at
`docs/superpowers/specs/2026-05-22-mode-2-scheduled-chain-prompt.md`.
The skill loads the Mode-2 prompt and binds these circuit breakers:

- cargo build failure → STOP chain, notify
- drift-check RED + current PR is Bundle → STOP chain, notify
- Queue empty / MAX hit → graceful exit, notify
- git push conflict → STOP chain, "needs human merge"

DO NOT improvise Mode 2 without the template — the circuit breakers
are load-bearing.

---

## C1/C2/C3 execution sequence (current 2026-05-22 state)

Per strategy §7:

- **C1** = M2 closeout (6 sub-tasks, ~4-6 days)
- **C2** = M3 wire-up (6 sub-tasks per plan §5.2, ~6-8 weeks)
- **C3** = M4 wire-up (4 sub-tasks per plan §6.2, ~3-4 weeks)
- **Beyond C3**: M5 → M9, see plan §7-§11

**Strict order**: do not start C2 before C1 closes. Do not start C3
before C2 closes. The only exception is if a C2 sub-task is blocked
and a C3 sub-task is independent — open one C3 PR while waiting, but
**don't drift into "C3 is more fun, let me just do that"**.

## Design ↔ implementation traceability

Each milestone has paired references:

- **Design**: `uclaw-codex-comparison-and-design.md` §X
- **Implementation**: `uclaw-upgrade-implementation-plan.md` M<N>
- **Live state**: `docs/superpowers/MILESTONE_STATUS.md`
- **PR strategy**: `docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`

Cross-reference in spec docs + commit messages. Don't lose the design
intent during execution.

## Self-check before responding to user

Before sending any reply that involves milestone work, ask yourself:

- [ ] Have I read MILESTONE_STATUS.md current state?
- [ ] Did I run drift-check (or check the last run's output)?
- [ ] Am I about to open a Bundle PR? If so, is drift currently RED?
- [ ] Is the work I'm about to do tagged with the right [M-T] / [Bundle] / etc?
- [ ] Will my PR include the SSoT update?

If any answer is "no" or "I don't know", **address it in this turn**
before proceeding.

## Anti-patterns (things that broke main-line discipline before)

1. **"Just one more Bundle then I'll go back to M2"** — that's how
   Bundle 18-27 happened. The closed-loop's whole point is to make
   this anti-pattern visible immediately.
2. **Skipping the SSoT update because "the PR is the source of truth"**
   — no, the SSoT is the source of truth precisely because PR
   descriptions get buried.
3. **Writing implementation before spec for wire-up work** — leads to
   re-litigated design mid-review, 3× more turns.
4. **Closing a milestone without quantitative data** — "I think we're
   done" doesn't pass §6 cutoff. Bench data or retro report.

## When this skill DOESN'T apply

- Pure UX polish / typo fix not tied to any milestone work — Bundle tag,
  drift check, move on
- Documentation-only PR with no code change — still tag (`[docs]`),
  no SSoT update needed unless it touches MILESTONE_STATUS.md itself
- New verify script (like Bundle 26-B/26-D/27-B verification) — tag
  `[chore]` + a sentence in PR body about which milestone's regression
  catcher it serves

## Quick links

- **SSoT**: [`docs/superpowers/MILESTONE_STATUS.md`](../../docs/superpowers/MILESTONE_STATUS.md)
- **Strategy**: [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](../../docs/superpowers/plans/2026-05-22-pr-integration-strategy.md)
- **Drift check**: [`scripts/milestone-drift-check.sh`](../../scripts/milestone-drift-check.sh)
- **Verify script template**: [`scripts/verify/bundle-26bd-27b.sh`](../../scripts/verify/bundle-26bd-27b.sh)
  (+ sibling skill [`uclaw-tick-feature-verify`](../uclaw-tick-feature-verify/SKILL.md))
- **Plan doc**: [`uclaw-upgrade-implementation-plan.md`](../../uclaw-upgrade-implementation-plan.md)
- **Design doc**: [`uclaw-codex-comparison-and-design.md`](../../uclaw-codex-comparison-and-design.md)
