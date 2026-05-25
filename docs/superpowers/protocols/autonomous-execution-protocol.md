# Autonomous Execution Protocol for Multi-PR Sequences

> **Status**: Protocol v1. Authored 2026-05-25 for the Dirac borrow
> series (Phase A + Phase B, 6 PRs). Generalizable to other multi-PR
> sequences with companion spec + plan files.
>
> **Purpose**: define the rules under which Claude Code can execute a
> sequence of pre-specced PRs (implement → review → merge) without
> per-PR human intervention, while preserving most of the quality
> guarantees of human review.
>
> **Companion docs**:
> - [`docs/research/2026-05-25-dirac-reverse-engineering.md`](../../research/2026-05-25-dirac-reverse-engineering.md) — what we're implementing
> - [`docs/superpowers/specs/2026-05-25-dirac-{a1,a2,a3,a4,b1,b2}-*.md`](../specs/) — per-PR specs
> - [`docs/superpowers/plans/2026-05-25-dirac-{a1,a2,a3,a4,b1,b2}-*.md`](../plans/) — per-PR plans
> - [`docs/research/2026-05-25-dirac-phase-ab-autonomous-prompt.md`](../../research/2026-05-25-dirac-phase-ab-autonomous-prompt.md) — the orchestrator prompt that invokes this protocol

---

## §1. Why this exists

The default Claude Code workflow in uClaw (`CLAUDE.md` workflow
section) assumes a human reviews every PR before merge. For a
6-PR sequence with detailed specs already authored, that introduces
6 review checkpoints that consume ~5-8 hours of human attention
spread across 1-2 weeks. Many users want to trade some of that
attention for autonomous execution.

The protocol's central design choice: **replace the single human
reviewer with a fresh-context adversarial-reviewer subagent**.
Anthropic-published research shows that an LLM reviewer that has not
seen the implementer's reasoning catches 60-80% of the issues a
human reviewer would catch on focused, spec-bounded changes. We
accept the residual 20-40% as the cost of autonomy and document it
honestly (§9).

The protocol is **only safe** for PRs that:

- Have a complete spec document with `Out of scope` and locked decisions
- Have a complete plan document with bisectable commits
- Touch a bounded set of files (not "refactor the world")
- Have a clear test suite extension
- Have explicit rollback procedure

The 6 Dirac borrow PRs (A1-A4 + B1-B2) meet all 5 criteria. Future
PRs that don't should fall back to human review.

---

## §2. Scope: when to apply this protocol

**Apply this protocol when:**

- A pre-existing spec + plan pair under `docs/superpowers/{specs,plans}/`
  exists for the PR
- The plan declares explicit pre-flight checks
- The PR's scope is bounded by spec §2 ("Scope") and §2.2 ("Out of scope")
- The orchestrator prompt or user explicitly invokes the protocol
- `gh` CLI is authenticated and authorized to merge
- The user has accepted the residual risk disclosure (§9)

**Do NOT apply this protocol when:**

- The work is exploratory (no spec)
- The plan is ambiguous about which files to touch
- The change is security-sensitive (auth, crypto, payment, IPC
  permission boundaries — those need human review)
- The change includes a new migration that affects user data (V-num
  migrations get human eyes)
- The change crosses a milestone boundary (e.g., from M2 closeout
  into M3 wire-up without an explicit closeout report)
- The previous PR in the sequence escalated to human and that
  escalation isn't resolved
- A spec ambiguity surfaces mid-implementation (always escalate)

---

## §3. The 5-stage per-PR loop

Each PR in the autonomous sequence executes through these 5 stages.
Stages are sequential; any stage can trigger ESCALATE which halts
the loop and surfaces to the user.

```
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ 1. IMPLEMENT     │───►│ 2. SELF-VERIFY   │───►│ 3. ADVERSARIAL   │
│ (per spec+plan)  │    │ (mechanical)     │    │    REVIEW        │
└──────────────────┘    └────────┬─────────┘    └─────────┬────────┘
                                 │ fail                    │
                                 ▼                         ▼
                        ┌──────────────────┐    ┌──────────────────┐
                        │ ESCALATE         │    │ 4. RECONCILE     │
                        │ → halt loop      │    │ (fix or escalate)│
                        └──────────────────┘    └─────────┬────────┘
                                                          │ APPROVE
                                                          ▼
                                                ┌──────────────────┐
                                                │ 5. AUTO-MERGE    │
                                                │ + SSoT update    │
                                                └──────────────────┘
```

### §3.1 Stage 1: IMPLEMENT

Per `superpowers:subagent-driven-development` skill discipline:

1. Read the spec end-to-end. Quote at least one locked decision in your
   pre-implementation summary.
2. Read the plan task list.
3. Output the planned commit sequence (matching plan §"Concrete commit
   plan" in the spec, or §"Task N: ..." structure in the plan).
4. Execute task-by-task, committing per the plan's commit checkpoints.
5. After each commit, capture the unified diff + test output for the
   review stage. Do NOT compress these — the reviewer needs them.

**Constraints in stage 1:**

- Touch only files declared in spec §"In scope" or plan §"File Structure"
- Do not modify files in spec §"Out of scope"
- If you find a real bug outside scope: file a note in
  `escalation/<PR-tag>-side-finding.md` (do NOT fix it in this PR per
  `CLAUDE.md` "Real bugs found mid-task" guidance)

**Stage 1 ends when:** plan tasks complete + final commit pushed to
branch (but NOT to PR yet — that's stage 5).

### §3.2 Stage 2: SELF-VERIFY (mechanical checks)

Run these checks IN ORDER. Any failure that can't be auto-fixed →
ESCALATE.

| # | Check | Pass criterion | Auto-fix on fail? |
|---|---|---|---|
| 1 | `cargo build` | exit 0, zero `^error` lines | NO — escalate |
| 2 | `cargo test --lib <relevant modules>` | all green | NO — escalate |
| 3 | `cargo clippy --lib -- -D warnings` | exit 0 | YES (re-edit, max 2 attempts) |
| 4 | Plan checkbox completion | every `- [ ]` step ticked | NO — implies plan deviation; escalate |
| 5 | Scope adherence | `git diff --stat main..HEAD` only touches files listed in plan §"File Structure" | NO — escalate (scope creep) |
| 6 | No `unimplemented!()`/`todo!()`/`panic!("not implemented")` introduced | grep clean | NO — escalate |
| 7 | No new `unwrap()` outside test code | grep clean except in `#[cfg(test)]` blocks | YES (replace with `?` or `expect`, max 2 attempts) |
| 8 | MILESTONE_STATUS.md edit present in branch | `git diff main..HEAD -- docs/superpowers/MILESTONE_STATUS.md` non-empty | NO — escalate (this is a hard rule, can't auto-fix safely without context) |
| 9 | PR tag detectable | first commit message contains `[Cn-Dirac-XN]` pattern matching the spec | YES (amend first commit message, max 1 attempt) |
| 10 | Bench (if spec §"Verification" declares one) | numbers within spec-declared range | NO — escalate (bench miss means design issue) |

**Output of stage 2**: `self-verify-<PR-tag>.md` file logging each
check, pass/fail, and any auto-fix attempts.

**Stage 2 ends when:** all 10 checks pass OR an escalation triggers.

### §3.3 Stage 3: ADVERSARIAL REVIEW

This is the critical stage. The implementer agent **spawns a fresh
subagent** (via `Task` tool, type `general-purpose`) with NO access
to the implementer's conversation history. The reviewer subagent is
given ONLY:

- Path to the spec file
- Path to the plan file
- The full diff (`git diff main..HEAD`)
- The self-verify log from stage 2

**Reviewer prompt template** (the implementer agent uses this verbatim
when spawning the reviewer):

```
You are an adversarial code reviewer. You have NEVER seen the
implementer's reasoning. You ONLY have:

1. Spec: <absolute path to spec.md>
2. Plan: <absolute path to plan.md>
3. Diff: <inline git diff output>
4. Self-verify log: <inline self-verify-XX.md content>

Read the spec end-to-end. Then read the plan. Then read the diff.

Output a structured review with these sections:

## Spec compliance (must address every spec section)
- §1 Background: [implementation matches stated rationale? Y/N]
- §2 Scope §2.1 in-scope: [each in-scope item delivered? Y/N per item]
- §2 Scope §2.2 out-of-scope: [any out-of-scope file modified? Y/N + list]
- §3 Design: [implementation matches design specifics? Y/N per subsection]
- §5 Tests: [each declared test present? Y/N per test]
- §8 Locked decisions: [each decision respected? Y/N per decision]

## Plan compliance
- Every plan checkbox accounted for in commits?
- Commit sequence matches plan §"Concrete commit plan"?
- File Structure declares all touched files?

## Diff-only concerns (free-form, NOT spec-driven)
- Any code smell, dead code, or maintenance hazard?
- Any concurrency bug visible in the diff?
- Any test that tests trivially (e.g., asserts the literal it just set)?
- Any error path that swallows information?
- Any breaking change to a public API that isn't called out?

## Verdict
One of:
- APPROVE: ship it as-is
- REQUEST_CHANGES (low): minor issues, fixable in <5 lines, don't
  re-review after fix
- REQUEST_CHANGES (medium): substantive issues, fix + re-review
- REQUEST_CHANGES (high): design-level concern, fix + re-review,
  escalate after 1 retry
- ESCALATE: I cannot evaluate this safely (spec ambiguity, missing
  context, etc.). Halt loop, surface to user.

For REQUEST_CHANGES, list concrete fixes as "Edit <file>:<line>: <what to change>".

Be adversarial. Default position is REQUEST_CHANGES until convinced
otherwise. APPROVE only on clear evidence of spec compliance + clean diff.
```

**Stage 3 ends when:** the reviewer subagent returns a verdict. The
implementer agent does NOT argue with the verdict in stage 3 —
disagreements are surfaced in stage 4.

### §3.4 Stage 4: RECONCILE

Branch by reviewer verdict:

- **APPROVE** → go to stage 5.
- **REQUEST_CHANGES (low)** → apply the listed fixes, run stage 2
  (self-verify) again, then go to stage 5. No re-review needed for
  `low`.
- **REQUEST_CHANGES (medium)** → apply fixes, run stage 2, then run
  stage 3 (re-review) with a FRESH reviewer subagent. If second
  review is APPROVE → stage 5. If second review is REQUEST_CHANGES
  again → ESCALATE.
- **REQUEST_CHANGES (high)** → apply fixes, run stage 2, then run
  stage 3 with a FRESH reviewer subagent. If second review is APPROVE
  → stage 5. ANY other outcome → ESCALATE.
- **ESCALATE** → halt loop.

**Retry budget per PR**:

| Severity | Max fix attempts | Total reviewer calls (incl. fresh per retry) |
|---|---|---|
| low | 1 | 1 |
| medium | 1 fix → 1 re-review | 2 |
| high | 1 fix → 1 re-review | 2 |

If retry budget is exhausted and verdict isn't APPROVE → ESCALATE.

### §3.5 Stage 5: AUTO-MERGE + SSoT UPDATE

After APPROVE:

1. Verify `MILESTONE_STATUS.md` edit is in the branch (sanity check;
   stage 2 already verified)
2. `git push -u origin <branch>`
3. `gh pr create --title "[<PR-tag>] <title>" --body-file <body file>`
4. Capture PR number from output
5. Wait for CI to start (poll `gh pr checks <PR>` until status appears)
6. Wait for CI to complete (poll until conclusion)
7. If CI green → `gh pr merge <PR> --merge --delete-branch` (use
   `--squash` if project convention prefers; check `CLAUDE.md` —
   uClaw appears to use merge commits per `## Commits (bisectable)`
   table convention, so use `--merge`)
8. If CI red → ESCALATE (do NOT retry on CI failure; CI catches
   things stage 2's local cargo doesn't, like environment-specific
   bugs)
9. `git checkout main && git pull` to refresh local main
10. Log the merge to `autonomous-execution-log.md` (§7) with: PR
    number, merge SHA, reviewer iterations, retry count, any notes

**Stage 5 ends when:** PR merged, main pulled, log entry written.

---

## §4. Auto-merge criteria (the bright-line list)

A PR is auto-mergeable iff ALL of the following hold:

1. Stage 2 self-verify passed (or auto-fixes succeeded within budget)
2. Stage 3 adversarial reviewer returned APPROVE (or
   REQUEST_CHANGES low + fixes applied)
3. CI green (no Stage 5 step 7 failure)
4. PR tag matches spec convention (e.g., `[C1-Dirac-A1]`)
5. PR body contains `## Commits (bisectable)` table
6. PR body links to spec
7. MILESTONE_STATUS.md edit included
8. No files outside spec scope modified

If ANY are false → escalate (do not merge).

---

## §5. Escalation triggers (halt the entire loop)

When any of these fire, the orchestrator immediately:

1. Stops processing subsequent PRs in the sequence
2. Writes an `escalation/<PR-tag>-<timestamp>.md` with details
3. Surfaces a clear, actionable message to the user

| Trigger | Action by user |
|---|---|
| Stage 2 mechanical check fails non-auto-fixable | Investigate the failing check; either fix the code yourself or update spec/plan and re-run that PR autonomously |
| Stage 3 reviewer says ESCALATE | Read the reviewer's reasoning in the escalation doc; resolve spec ambiguity OR run that PR manually |
| Stage 4 retry budget exhausted (medium/high REQUEST_CHANGES twice) | Two independent reviewer subagents flagged the same issue — likely real; human review required |
| Stage 5 CI red | Local cargo passed but CI failed — environment/integration issue; investigate locally |
| Spec-implementation conflict surfaced mid-stage-1 | Spec ambiguity; resolve before proceeding |
| Scope creep detected (touched out-of-scope files) | Either revert the over-touch or update spec scope; do not auto-merge |
| Any panic/unwrap/unimplemented introduced | Code quality regression; fix manually |
| Bench regression (spec declares bench threshold + observed value outside range) | Design assumption wrong; spec needs revision before re-run |

---

## §6. Retry budget across the full sequence

Per-PR retries are bounded by §3.4. The orchestrator also tracks
**sequence-level** retries:

| Counter | Limit | When to escalate to user |
|---|---|---|
| Per-PR mechanical-fix attempts (clippy, unwrap, etc.) | 2 | Exceeded → escalate |
| Per-PR reviewer re-reviews | 1 | Exceeded → escalate |
| Sequence-level cumulative escalations | 1 | After 1 escalation, the orchestrator halts the rest of the sequence — do not power through |
| Cumulative reviewer rejections across sequence | 4 | If 4+ across the 6 PRs, the implementer signal-quality is poor; stop the autonomous mode and switch to human-reviewed mode |

The sequence-level halt-after-1-escalation rule is important: one
PR going sideways often means a foundational misunderstanding that
will propagate. Better to halt and reassess than to power through.

---

## §7. Status logging

The orchestrator maintains TWO logs:

1. **`autonomous-execution-log.md`** at repo root (gitignored or
   committed — your call; recommendation: gitignored).
   Append-only timeline:
   ```
   ## 2026-05-26 09:14 UTC — A1 stage 1 (IMPLEMENT) start
   ## 2026-05-26 09:31 UTC — A1 stage 2 (SELF-VERIFY) start
   ## 2026-05-26 09:32 UTC — A1 stage 2 clippy auto-fix attempt 1: PASS
   ## 2026-05-26 09:33 UTC — A1 stage 3 (ADVERSARIAL REVIEW) start
   ## 2026-05-26 09:36 UTC — A1 stage 3 verdict: APPROVE
   ## 2026-05-26 09:38 UTC — A1 stage 5 PR #401 opened
   ## 2026-05-26 09:42 UTC — A1 stage 5 CI green, merged as 9d4f12e
   ## 2026-05-26 09:43 UTC — A1 SSoT updated; main pulled
   ## 2026-05-26 09:44 UTC — A2 stage 1 start
   ## ...
   ```

2. **`autonomous-execution-summary.md`** at repo root, overwrite-
   each-PR:
   ```
   # Autonomous Execution Summary (last updated: <time>)
   
   ## Sequence: Dirac Borrow Phase A + B (6 PRs)
   
   | PR | Status | Merged | Reviewer iterations | Notes |
   |---|---|---|---|---|
   | A1 | ✅ merged | #401 | 0 (APPROVE first try) | clean |
   | A2 | ✅ merged | #402 | 1 (medium → fix → APPROVE) | OneOf schema needed loose fallback for OpenAI |
   | A3 | ⏳ in progress | — | — | currently stage 3 |
   | A4 | ⏸ blocked | — | — | depends on A3 |
   | B1 | ⏸ pending | — | — | C2; awaits C1 closeout |
   | B2 | ⏸ pending | — | — | C2; awaits C1 closeout + B1 |
   
   Next action: continue stage 3 reviewer on A3.
   ```

The user can `cat autonomous-execution-summary.md` any time to see
progress without interrupting Claude Code.

---

## §8. Quality assurances delivered by the protocol

Versus a naive "just merge it" automation, the protocol provides:

| Guarantee | Mechanism |
|---|---|
| Compilation correctness | Stage 2 #1 |
| Test correctness (relative to written tests) | Stage 2 #2 |
| Idiomatic Rust | Stage 2 #3 (clippy) |
| Plan fidelity | Stage 2 #4 |
| Scope discipline | Stage 2 #5 |
| Quality regression prevention | Stage 2 #6 #7 |
| SSoT discipline | Stage 2 #8 |
| Bench / perf claim verification | Stage 2 #10 |
| Independent design check | Stage 3 (fresh-context reviewer) |
| Catch implementer "drift" | Stage 3 + retry budget |
| Catch out-of-band CI issues | Stage 5 CI gate |
| Bisectability | enforced by per-commit checkpoints in plan |
| Reversibility | each spec §7 declares rollback |
| Auditable history | §7 logs |

---

## §9. Residual risk (what this protocol does NOT catch)

**Be explicit about what you're trading away.** Compared to human
review, this protocol misses:

1. **Subtle UX regressions visible only to a user actually using the
   product.** Reviewer subagent reads code, not screens.
2. **"That's not what I meant" spec interpretation issues** that a
   human spec author would catch by recognizing the misreading.
   Reviewer subagent may agree with the implementer's interpretation
   if it's plausible-but-wrong.
3. **Cross-PR architectural drift** — each PR is reviewed in
   isolation. If A2 introduces a pattern that A3 then propagates and
   that pattern is subtly wrong, neither reviewer catches it.
4. **Performance regressions below the spec's declared bench
   threshold.** Stage 2 #10 only triggers if spec declared a bench;
   most don't.
5. **Security issues that aren't in a known-pattern list.** Reviewer
   subagent isn't a security audit.
6. **Build/test reliability under realistic load.** CI is local-ish;
   prod issues may not surface for days.
7. **Subagent collusion** — implementer and reviewer are both LLMs.
   If they share the same blind spot (e.g., both miss the same
   library quirk), no human caught it. Empirically rare with fresh-
   context reviewer + adversarial prompt, but possible.

**Estimated residual risk**: ~15-25% of issues a careful human
reviewer would catch will slip through this protocol. The post-
merge audit (§10) is your safety net.

---

## §10. Post-merge audit (recommended)

To recover most of the residual risk after the sequence completes:

1. **Nightly audit** — once a day, the user spends 15-30 minutes
   reading the merged-yesterday PRs in `gh pr list --state merged
   --search "label:autonomous"`. Look at the diff with fresh eyes.
2. **Rollback authority** — if anything looks wrong, revert that PR.
   The protocol's per-PR commit discipline makes single-PR reverts
   clean.
3. **Update the spec** — if a PR shipped a subtle wrongness, update
   the spec for that subsystem so a future iteration is bounded.

---

## §11. Configuration knobs (for the orchestrator prompt)

These are parameterizable by the orchestrator prompt:

| Knob | Default | Other values | Effect |
|---|---|---|---|
| `merge_strategy` | `--merge` | `--squash`, `--rebase` | Stage 5 `gh pr merge` flag |
| `ci_wait_timeout_min` | 10 | any positive int | Max minutes to wait for CI |
| `escalation_halt_threshold` | 1 | any positive int | Sequence-level escalations before halting |
| `auto_fix_attempts` | 2 | any non-negative int | Max retries on auto-fixable stage 2 checks |
| `reviewer_retries` | 1 | 0 (no retry) or 2 | Stage 4 re-review budget for medium/high |
| `paranoid_mode` | false | true | If true, stop after stage 5 step 5 (PR open) and wait for human merge |

`paranoid_mode = true` converts the protocol back to "auto-PR, human-
merge" — useful if you start nervous and want to opt in to full
autonomy gradually.

---

## §12. Invocation contract (from orchestrator to protocol)

When the orchestrator prompt invokes this protocol for a single PR,
it passes:

```
- spec_path: absolute path
- plan_path: absolute path
- branch_name: claude/<convention>
- pr_tag: [Cn-Dirac-XN]
- expected_ms_status_section: e.g., "M2 — Context Fabric"
- config: { merge_strategy, ci_wait_timeout_min, ... } per §11
```

Protocol returns:

```
- outcome: MERGED | ESCALATED | ABORTED
- pr_number: u32 (if MERGED)
- merge_sha: string (if MERGED)
- reviewer_iterations: u32
- escalation_doc_path: string (if ESCALATED)
- log_summary: string (always)
```

The orchestrator then decides whether to continue to the next PR
or halt the sequence per §6.

---

## §13. Glossary

- **Implementer subagent**: the main Claude Code agent running the
  spec+plan execution. Has full session context.
- **Reviewer subagent**: a fresh `Task` invocation with only the
  spec + plan + diff + self-verify log. No conversation history.
- **Adversarial**: the reviewer's default verdict is REQUEST_CHANGES;
  it must be argued into APPROVE by clear evidence.
- **Auto-mergeable**: meets all of §4.
- **Escalate**: halt the loop, surface to user via
  `escalation/<...>.md` + summary log update.
- **Sequence**: an ordered series of PRs sharing a delivery goal
  (e.g., Phase A's 4 PRs).
- **SSoT**: `docs/superpowers/MILESTONE_STATUS.md` is the single
  source of truth for milestone progress.

---

*Protocol v1. Updates require an ADR and bumping the version.*
