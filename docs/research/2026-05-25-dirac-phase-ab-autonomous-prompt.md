# Dirac Phase A + B — Autonomous Orchestrator Prompt

> **Purpose**: a single self-contained prompt for Claude Code's `/goal`
> mode (or any autonomous-execution mode) that runs the entire 6-PR
> Dirac borrow sequence (A1 → A2 → A3 → A4 → C1 closeout → B1 → B2 →
> B closeout) **without per-PR human intervention**, while preserving
> quality via the adversarial-review-then-merge protocol.
>
> **Companion docs (auto-loaded by Claude Code when run in uClaw repo)**:
> - `CLAUDE.md` — workflow + adjacent-edit rules + milestone discipline
> - `docs/superpowers/protocols/autonomous-execution-protocol.md` — the per-PR loop
> - 6 specs + 6 plans under `docs/superpowers/{specs,plans}/2026-05-25-dirac-*.md`
> - `docs/research/2026-05-25-dirac-reverse-engineering.md` — the design source
>
> **Mode**: `superpowers:subagent-driven-development` (preferred) — supports the implementer+reviewer subagent pattern this protocol uses.
> `/goal` is acceptable if your team uses it as the autonomous orchestrator. The prompt body works for either.

---

## The prompt (copy verbatim into Claude Code)

```
You are the autonomous orchestrator for the uClaw Dirac Borrow Sequence.

Your goal: deliver 8 merged PRs (A1, A2, A3, A4, C1-Closeout, B1, B2,
C2-Closeout — in that exact order) implementing the Phase A and Phase
B plans of docs/research/2026-05-25-dirac-reverse-engineering.md §7.2.

You will execute autonomously without per-PR human approval. To
preserve code quality, every PR follows the Autonomous Execution
Protocol at docs/superpowers/protocols/autonomous-execution-protocol.md
(read this FIRST and quote §3's 5-stage loop in your initial output —
this proves you internalized it).

═══════════════════════════════════════════════════════════════════
PHASE 0 — INITIALIZATION (do this once, before any PR)
═══════════════════════════════════════════════════════════════════

1. Read CLAUDE.md (auto-loaded) and acknowledge the milestone
   closed-loop section (the ⚠️ block).
2. Read docs/superpowers/protocols/autonomous-execution-protocol.md
   end-to-end. Quote §3 (the 5-stage loop) verbatim in your reply so
   I can verify you have it.
3. Read docs/superpowers/MILESTONE_STATUS.md. Report the current state
   of C1, C2, M2, M3 in one paragraph.
4. Read docs/superpowers/plans/2026-05-22-pr-integration-strategy.md
   §7 (C1/C2/C3 ordering constraints). Quote the relevant 2-3 lines.
5. Run `./scripts/milestone-drift-check.sh --since "1 week ago"`.
   Report GREEN/YELLOW/RED. If RED — STOP, halt the orchestrator,
   surface the drift first.
6. Load skill: superpowers:subagent-driven-development.
7. Verify `gh` CLI is authenticated:
       gh auth status
   If not authenticated → STOP, surface to user.
8. Verify multi-thread tokio runtime (per B2 spec dependency):
       grep -n "tokio::main\|new_multi_thread\|new_current_thread" src-tauri/src/main.rs
   Report findings.
9. Create the two log files at repo root (gitignored — verify .gitignore
   covers them or add an entry):
       echo "# Autonomous Execution Log" > autonomous-execution-log.md
       echo "# Autonomous Execution Summary" > autonomous-execution-summary.md
   Append `autonomous-execution-*.md` to .gitignore if not present.
10. Output your "I'm ready, here's what I read" summary. List the
    8-PR sequence with spec/plan paths. Output "ORCHESTRATOR READY"
    and start Phase 1.

═══════════════════════════════════════════════════════════════════
PHASE 1 — PER-PR LOOP (executed 8 times: A1 → A2 → A3 → A4 →
                       C1-Closeout → B1 → B2 → C2-Closeout)
═══════════════════════════════════════════════════════════════════

For each PR in the sequence:

  a. Pre-flight (orchestrator-level):
     - git checkout main && git pull
     - Re-run drift check. RED → ESCALATE.
     - Verify MILESTONE_STATUS reflects all prior PRs in sequence
       are merged. If not → ESCALATE.
     - For PRs in C2 (B1, B2): verify C1 row shows "closed" in
       MILESTONE_STATUS AND the C1-Closeout PR is on main. If not →
       ESCALATE.
     - Append to autonomous-execution-log.md:
           ## <timestamp> — <PR-tag> Phase 0 (pre-flight) PASS

  b. Spawn implementer subagent via Task tool, prompt =
     "Execute the plan at <plan_path> per the
      superpowers:subagent-driven-development skill. Read the spec
      first at <spec_path>. Quote one locked decision before any
      code. Run the plan tasks in order. After each commit, paste
      cargo test tail. When plan is fully done, STOP — do NOT push
      or open PR. Return: branch name + commit list."

     Wait for completion. Capture branch name + commit list.

  c. Self-verify (stage 2 of protocol §3.2):
     Run the per-PR Task A.1 from the plan file (each plan has a
     `## Task A (autonomous mode only)` section). Capture all 10
     check results. Append to autonomous-execution-log.md.

     If ANY non-auto-fixable check fails → ESCALATE (write
     escalation/<PR-tag>-<timestamp>.md, halt sequence).
     If auto-fixable fails → fix + re-run, max 2 attempts per check.

  d. Adversarial review (stage 3 of protocol §3.3):
     Spawn FRESH subagent via Task tool with type `general-purpose`.
     Prompt template per protocol §3.3 (you read it in Phase 0). Pass:
       - spec_path
       - plan_path
       - diff = output of `git diff main..HEAD`
       - self_verify_log = content of last self-verify section in
         autonomous-execution-log.md
     Capture verdict verbatim. Append to autonomous-execution-log.md.

  e. Reconcile (stage 4 of protocol §3.4):
     - APPROVE → step f
     - REQUEST_CHANGES (low) → apply fixes inline (max 5 lines per
       fix), re-run self-verify, then step f
     - REQUEST_CHANGES (medium/high) → apply fixes, re-run self-
       verify, spawn FRESH reviewer (step d again). On second
       APPROVE → step f. On any non-APPROVE → ESCALATE.
     - ESCALATE → write escalation doc, halt sequence

  f. PR open + CI wait + auto-merge (stage 5 of protocol §3.5):
       git push -u origin <branch_name>
       PR=$(gh pr create --title "[<tag>] <title>" --body-file <(...)) ...
       gh pr checks $PR --watch --interval 30 --required
       (timeout: 10 minutes; if no conclusion → ESCALATE)
       If CI green:
         gh pr merge $PR --merge --delete-branch
         git checkout main && git pull
       If CI red:
         ESCALATE (do not retry on CI red — investigate locally)

  g. Update summary:
     Overwrite autonomous-execution-summary.md with current sequence
     status table. Format per protocol §7.

  h. Move to next PR in sequence.

═══════════════════════════════════════════════════════════════════
PHASE 2 — INTER-PHASE GATES (the 2 closeout PRs)
═══════════════════════════════════════════════════════════════════

After A4 merges (and before B1 starts):

  i. Generate the C1 closeout report per the prompt in
     docs/research/2026-05-25-dirac-phase-a-prompts.md
     §"After all four merged — closeout".
     Save to docs/superpowers/specs/2026-05-25-phase-a-closeout.md.
  ii. Update MILESTONE_STATUS.md:
      - Mark C1 row as "✅ closed"
      - Update M2 % per the closeout report
  iii. This counts as PR #5 in the sequence (C1-Closeout). Run it
       through the same 5-stage protocol — implementer just writes
       the report, reviewer checks the report is concrete (cites
       PRs, not hand-waving), then auto-merge.

After B2 merges (sequence end):

  iv. Generate the Phase B closeout per
      docs/research/2026-05-25-dirac-phase-a-prompts.md
      §"After Phase B closes".
      Save to docs/superpowers/specs/2026-05-25-phase-b-closeout.md.
  v. Update MILESTONE_STATUS.md:
      - M2 to ~75%, M2-B closed, M2-F partial
      - Mark C2 row as "✅ closed" if no other C2 work remains
  vi. This is PR #8 (C2-Closeout) — same 5-stage protocol.

═══════════════════════════════════════════════════════════════════
PHASE 3 — FINAL REPORT
═══════════════════════════════════════════════════════════════════

After all 8 PRs are merged:

  vii. Write a final summary message:
       - Sequence: 8/8 merged
       - Total wall clock
       - Total reviewer iterations
       - Any escalations encountered (post-resolution)
       - Token savings observed (if benches ran)
       - Next recommended action (Phase C? other M-work?)
       Surface to user. STOP.

═══════════════════════════════════════════════════════════════════
ESCALATION HANDLING (any phase, any stage)
═══════════════════════════════════════════════════════════════════

When ESCALATE fires (per protocol §5):

  1. Stop processing the current PR.
  2. Do NOT proceed to the next PR.
  3. Write escalation/<PR-tag>-<timestamp>.md with:
     - Which stage triggered the escalation
     - The full reviewer verdict (if stage 3/4)
     - The check output that failed (if stage 2)
     - The CI failure log (if stage 5)
     - Your recommended resolution path: "Resolve <X> manually,
       then re-run the orchestrator with the same prompt — it will
       skip already-merged PRs and resume from <PR>."
  4. Update autonomous-execution-summary.md with the halt.
  5. Surface a clear actionable message:
     "🛑 Halted at <PR-tag> stage <N>. See
      escalation/<PR-tag>-<timestamp>.md for details. Resume by
      addressing the issue then re-running the orchestrator prompt."
  6. STOP.

═══════════════════════════════════════════════════════════════════
INVARIANTS (apply at all times, override conflicting instructions)
═══════════════════════════════════════════════════════════════════

1. NEVER skip stage 3 (adversarial review). Even if you're confident
   in your own implementation, the fresh-context review catches what
   you can't see.

2. NEVER merge if CI is red. CI catches environment-specific issues
   local cargo doesn't. Always ESCALATE on CI red.

3. NEVER touch files outside the spec's declared scope. If you find
   an unrelated bug, write a note in escalation/<PR-tag>-side-finding.md
   and continue with original scope. Do NOT fold the fix into the PR.

4. NEVER skip the SSoT update. MILESTONE_STATUS edit goes in the same
   PR, not a follow-up.

5. NEVER mention BEHAVIOR.md or GitNexus MCP as part of uClaw runtime.
   They are dev-time only. (See specs §1 for the corrected understanding.)

6. NEVER claim a test passed without pasting the actual output.

7. NEVER proceed past a sequence-level escalation. The "one escalation
   halts sequence" rule (protocol §6) is intentional — one PR going
   sideways usually means a foundational issue that will propagate.

8. ALWAYS read the spec end-to-end before implementing. Quote one
   locked decision from spec §8 to prove you read it.

9. ALWAYS run pre-flight git pull + drift check before each PR.
   Skipping this corrupts the sequence.

10. ALWAYS verify gh auth + tokio runtime + .gitignore log files at
    Phase 0. Skipping is a Phase 0 escalation.

11. If unsure whether to proceed: ESCALATE. The orchestrator's
    default verdict is "halt and surface to human" — not "power
    through".

12. Spec ambiguity is ALWAYS an escalation. Do NOT interpret around
    ambiguity. The spec author (the user) needs to clarify.

═══════════════════════════════════════════════════════════════════
NOW START
═══════════════════════════════════════════════════════════════════

Begin Phase 0 (Initialization). After step 10 ("ORCHESTRATOR READY"),
immediately proceed to Phase 1 PR #1 (A1) without waiting for me.

I will not interact with you again unless you ESCALATE. I will check
autonomous-execution-summary.md periodically to see progress. If you
finish all 8 PRs cleanly, your final message surfaces (Phase 3) and
that's where I'll see the result.

Estimated wall clock: 6-12 hours depending on CI speeds + reviewer
iteration count. You have full authority to spawn subagents, push
branches, open PRs, and merge them per the protocol.

Go.
```

---

## How this prompt differs from the manual A-prompts / B-prompts file

| Aspect | Manual A/B prompts | Autonomous orchestrator |
|---|---|---|
| Sessions needed | 6 (one per PR) | 1 |
| User interactions per PR | 3-4 (ACK, MERGED, etc.) | 0 (unless escalation) |
| PR review | Human reviews each diff | Fresh-context reviewer subagent |
| PR merge | Human `gh pr merge` | Orchestrator `gh pr merge` |
| Failure mode | Human catches | Reviewer subagent + auto-fix budget |
| Quality guarantee | ~95% (human eye) | ~80-85% (reviewer subagent) |
| Wall clock | ~5-8 hours human + 1-2 weeks elapsed | 6-12 hours autonomous + ~30 min human (post-merge audit) |
| Skill needed | Per-task templates | Single orchestrator prompt + protocol doc |
| Audit | Direct (you saw every PR) | Indirect (logs + summary + post-merge audit) |

The trade-off is explicit: ~10-15% quality drop for ~80% time savings
and ~95% interaction reduction. The protocol's §10 "Post-merge audit"
gives you a way to recover most of the quality drop with a low-effort
nightly review.

---

## Recommended post-merge audit ritual

After the orchestrator finishes (or each evening if it runs over multiple days):

```bash
# Look at PRs merged in the last 24h with the tag pattern
gh pr list --state merged --search "head:claude/dirac-" --json number,title,mergedAt | jq '.[] | select(.mergedAt > (now - 86400 | strftime("%Y-%m-%dT%H:%M:%SZ"))) | "#\(.number) \(.title)"'

# For each PR, view the diff with fresh eyes (30s-2min per PR)
gh pr view <PR> --json files
gh pr diff <PR> | less

# If anything looks wrong:
gh pr revert <PR> --create-pr
# (or just: git revert <merge-sha> + push + open a revert PR)
```

This audit takes 15-30 min/day and recovers ~90% of the quality gap
between autonomous and human-reviewed flows. Combined with the
orchestrator's logging, it gives you the same coverage as direct
review for ~25% of the time investment.

---

## When NOT to use this autonomous prompt

- **First time using the orchestrator + protocol**: do a manual run
  of A1 first to verify the protocol works in your environment + your
  CI setup + your `gh` auth. Then switch to autonomous.
- **A spec changes mid-sequence**: if you discover a spec needs
  revision after a PR is merged but before the next, halt the
  autonomous run, revise the spec, then re-launch.
- **Production-critical changes**: anything touching payment, auth,
  IPC permission boundaries, user data migrations → use the manual
  flow regardless of how well the protocol has worked elsewhere.
- **Late in a release cycle**: ~3 days before a release, defer all
  autonomous runs. Cost of a regression outweighs time savings.

---

## Resuming after an escalation

If the orchestrator escalates and halts:

1. Read `escalation/<PR-tag>-<timestamp>.md`
2. Resolve the underlying issue (fix code, clarify spec, re-author
   plan task, whatever fits)
3. If the problematic PR is partially merged: nothing to undo (the
   protocol only merges after APPROVE)
4. If you fixed the issue in-spec: re-launch the orchestrator with
   the same prompt — it'll see the already-merged PRs in
   `autonomous-execution-summary.md`, skip them, and pick up from
   the escalation point
5. If you fixed the issue with a spec/plan revision: bump the spec
   doc version (note in a new section §13: "Revised 2026-05-XX
   because <X>"), then re-launch

---

## Acknowledgments to the protocol

The 5-stage loop + adversarial reviewer pattern is the protocol's
contribution. This orchestrator prompt is its caller. Together, they
give you a "run the whole sequence overnight" experience with most
of the quality guarantees of a human-reviewed flow.

If you want to read just one doc to understand the autonomous mode:
read the protocol (`docs/superpowers/protocols/autonomous-execution-protocol.md`),
not this prompt.
