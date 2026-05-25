# Phase A + B — Claude Code Prompt Templates (A1-A4, B1-B2)

> **Filename note**: this file is `2026-05-25-dirac-phase-a-prompts.md`
> for historical reasons (Phase A was written first). It now covers
> Phase A (C1) AND Phase B (C2). Phase C templates will land in
> a separate file when those specs are written.
>
> **Companion to**: `docs/research/2026-05-25-dirac-reverse-engineering.md`
> + spec/plan files at:
> - Phase A (C1): `docs/superpowers/specs/2026-05-25-dirac-a{1,2,3,4}-*-design.md`
> - Phase A (C1): `docs/superpowers/plans/2026-05-25-dirac-a{1,2,3,4}-*.md`
> - Phase B (C2): `docs/superpowers/specs/2026-05-25-dirac-b{1,2}-*-design.md`
> - Phase B (C2): `docs/superpowers/plans/2026-05-25-dirac-b{1,2}-*.md`
>
> **Usage**: open Claude Code in `/Users/ryanliu/Documents/uclaw`. The
> `CLAUDE.md` entry file auto-loads `BEHAVIOR.md`, `CONTEXT.md`, North
> Star ADR, and `MILESTONE_STATUS.md`. **Don't re-paste any of those
> in the prompt** — let context auto-load. The prompts below assume
> that loaded context.
>
> **Order**: run one A task per Claude Code session. Wait for the PR
> to merge before starting the next. Phase A is parallelizable in
> theory (no file overlap), but sequential execution keeps the M2
> closeout SSoT clean and reviewable.

---

## Universal preamble (paste at the top of each session)

```
This is Claude Code in the uClaw repo. Before doing anything else:

1. Acknowledge that you've read CLAUDE.md (already auto-loaded).
2. Read docs/superpowers/MILESTONE_STATUS.md — report current C1 state in 1 line.
3. Run `./scripts/milestone-drift-check.sh --since "1 week ago"` and report GREEN/YELLOW/RED.
4. Load the superpowers:subagent-driven-development skill OR the
   superpowers:executing-plans skill. State which.
5. STOP. Wait for me to confirm the task before reading any code.

After I confirm, I'll point you to the specific spec + plan file pair
for the task. You then:
- Read the spec FIRST (it's the design, locked decisions, rollback,
  test plan)
- Read the plan SECOND (the executable task list)
- Before writing ANY code, output the commit list you intend to ship
  and wait for my ack. The plan has commit checkpoints — confirm you
  understand the bisectability requirement.
- Execute task-by-task per the plan's `- [ ]` checkboxes.
- After each commit, paste the actual diff of what changed and the
  output of the verification commands listed in the plan.
- When the plan's final task completes, push the branch + open PR
  per the plan's PR template.

PR tag must be one of [C1-Dirac-A1], [C1-Dirac-A2], [C1-Dirac-A3],
[C1-Dirac-A4] — exactly matching the task. Allocate the concrete
C1-T<N> number against MILESTONE_STATUS.md before opening the PR.

SSoT update (MILESTONE_STATUS.md one-line edit) is part of the PR,
not a follow-up.

If the drift check returns RED, stop and raise it before anything else.
```

After Claude Code completes Step 5 and waits, paste the task-specific
prompt below.

---

## A1 — Tool-Use/Result Pair Repair

```
Task: C1-Dirac-A1 — tool_use/tool_result pair repair on compaction.

Spec:  docs/superpowers/specs/2026-05-25-dirac-a1-tool-pairing-repair-design.md
Plan:  docs/superpowers/plans/2026-05-25-dirac-a1-tool-pairing-repair.md

Branch: claude/dirac-a1-tool-pairing-repair
PR tag: [C1-Dirac-A1]

Read the spec end-to-end (sections 1-11). Then read the plan's task
list. Output:

1. A summary of what you're about to do (2-3 sentences)
2. The 3-commit sequence you'll ship (per plan §"Concrete commit plan"
   in the spec / Task 1-5 in the plan)
3. Any clarifying question, or "READY"

Wait for me to ack before touching code.

Critical context from the spec to internalize:
- The fix is symmetric: existing code deletes orphan tool_result; new
  Step C inserts placeholder tool_result for orphan tool_use
- `purge_orphaned_tool_results` signature changes from `&mut [..]` to
  `&mut Vec<..>` because we may need to insert messages — verify 4
  call sites compile
- The placeholder string is COMPACTED_TOOL_RESULT_PLACEHOLDER constant
- Repair must be idempotent (test 3.3 verifies this)
- Inspired by Dirac ContextManager.ensureToolResultsFollowToolUse
  (line 287-393 of dirac source); NOT just copying — adapting to
  uClaw's ChatMessage/ContentBlock shapes
```

---

## A2 — Multi-File Edit Schema

```
Task: C1-Dirac-A2 — EditTool batch form ({files: [{path, edits}]}).

Spec:  docs/superpowers/specs/2026-05-25-dirac-a2-multifile-edit-schema-design.md
Plan:  docs/superpowers/plans/2026-05-25-dirac-a2-multifile-edit-schema.md

Branch: claude/dirac-a2-multifile-edit-schema
PR tag: [C1-Dirac-A2]

This is the highest-ROI item in Phase A. Predicted 30-50% reduction
in LLM round-trips on refactor tasks (per research doc §7.4).

Read the spec end-to-end. Pay special attention to:
- §3.1 detect-and-dispatch shape
- §3.2 two-phase (validate-then-apply) semantics
- §3.3 approval-flow decision (Option X = whole-batch approval)
- §3.4 output formatting (per-file summary + diffs)
- §4.1 schema oneOf — and the fallback path if any provider rejects oneOf
- §8.4 sequential-dependency: first failure halts, remaining files
  report "Skipped due to failure on prior file in batch"

Then read the plan. Output:

1. Summary (2-3 sentences)
2. 5-commit sequence
3. Provider-compat check: which providers will you test the oneOf
   schema against, and what's your fallback plan if one rejects?
4. Any clarifying question, or "READY"

Wait for ack.

Implementation note: Task 1 (refactor execute → execute_single_file)
is a pure refactor. The existing test suite must pass byte-identically
after that commit. Verify before moving on. The validation/apply
split inside execute_single_file (Task 3 §3.1) IS a behavior-affecting
refactor — must split the function so validate-then-apply can be
called from execute_batch.
```

---

## A3 — ReadFile Hash Short-Circuit

```
Task: C1-Dirac-A3 — read_file [File Hash] header + assume_hash short-circuit.

Spec:  docs/superpowers/specs/2026-05-25-dirac-a3-read-hash-shortcircuit-design.md
Plan:  docs/superpowers/plans/2026-05-25-dirac-a3-read-hash-shortcircuit.md

Branch: claude/dirac-a3-read-hash-shortcircuit
PR tag: [C1-Dirac-A3]

Read the spec end-to-end. Pay special attention to:
- §3.1 hash function choice — FNV-1a 32-bit
- §3.3 execute flow
- §8.4 malformed assume_hash → InvalidParams (not silent ignore)
- §7 the **header IS a behavior change** for downstream parsers — your
  pre-flight Step 0.3 in the plan must audit frontend code for any
  parser expecting raw file content at byte 0

Then read the plan. The reuse audit (plan Step 0.2) is critical —
if anchor_state.rs already has fnv1a_32, reuse it rather than
duplicating the constant.

Output:

1. Summary (2-3 sentences)
2. Result of pre-flight Step 0.2 (fnv1a_32 already present?) and 0.3
   (frontend parsers found?)
3. Adapted commit plan based on reuse audit
4. Any clarifying question, or "READY"

Wait for ack.

Critical: the FNV test vectors (Step 4.1) are non-negotiable. If
empty string hash ≠ 0x811c9dc5, your impl is wrong — fix before
proceeding.
```

---

## A4 — JIT Injection Channel

```
Task: C1-Dirac-A4 — BaselineBlock injection_policy channel.

Spec:  docs/superpowers/specs/2026-05-25-dirac-a4-jit-injection-channel-design.md
Plan:  docs/superpowers/plans/2026-05-25-dirac-a4-jit-injection-channel.md

Branch: claude/dirac-a4-jit-injection-channel
PR tag: [C1-Dirac-A4]

This is the smallest A task (0.5 day). It's an ARCHITECTURAL CHANNEL
ONLY — no new block content, no live wire-up to compose_system_prompt.
If you find yourself touching mode_prompts.rs or KARPATHY_BASELINE,
STOP — that's out of scope.

Read the spec end-to-end. Pay special attention to:
- §1.1 — the v1.0 misreading: BEHAVIOR.md is NOT in uClaw runtime.
  uClaw runtime system prompt = baseline.md + M2-A BaselineBlocks.
- §2.2 out of scope — be strict
- §8.5 — explicitly no block content changes
- §6.2 — the regression test (#7) verifying production block set
  unaffected

Then read the plan. Step 0.2 (M2-A pilot is the latest reference) and
Step 0.3 (locate registry type / iteration method) are critical
discovery steps before code.

Output:

1. Summary (2-3 sentences)
2. Step 0.3 results: registry type name, iteration method, acquisition method
3. 4-commit sequence
4. Any clarifying question, or "READY"

Wait for ack.

Critical: the regression test (plan Step 3.7) proves no production
block accidentally gained a non-Always policy. If it fails post-impl,
A4 changed production behavior — bug.
```

---

## Order-of-operations runner

If you want to run all four sequentially in **a single Claude Code
session** (not recommended — but possible if scope is tight):

```
Run A1 first. When A1's PR is merged on main, return here.
Then run A2. When A2's PR is merged on main, return here.
Then run A3. When A3's PR is merged on main, return here.
Then run A4.

Between each: pull main, drift check, status report. Do not start
the next A task while the previous one is still under review — the
PR-integration-strategy §7 C1 cutoff criteria require closed PRs to
count toward C1 progress.
```

I recommend **one Claude Code session per A task**. Reasons:

1. Each session keeps a clean conversation scope — review the diff
   thoroughly without context dilution
2. PR review feedback can flow into the same session that opened the PR
3. If one A task hits a blocker, others stay clean
4. Session-scoped MEMORY captures task-local feedback better than a
   monster mega-session

---

## After all four merged — closeout

```
All four A tasks (A1, A2, A3, A4) merged. Write a closeout report:

1. Total token savings observed across A1-A4 (per A2 round-trip bench
   + A3 manual integration smoke). If benches weren't run, flag for
   M2 closeout bench (50-turn fixture session).
2. M2 progress estimate: before vs after Phase A. Pull from
   MILESTONE_STATUS.md.
3. Any spec ambiguities you hit + how you resolved them. These should
   inform Phase B/C spec authoring.
4. Whether C1 is closeable (per pr-integration-strategy.md §7
   criteria) given Phase A is now done.
5. If yes — output the C1 closeout report draft per the integration
   strategy. If no — list what's still blocking C1.

Save to docs/superpowers/specs/2026-05-25-phase-a-closeout.md (spec
slot because it summarizes implementation decisions, not future plans).
```

---

## Universal failure-mode handlers

If Claude Code does any of these during a session, **stop and correct
immediately**:

| Symptom | Likely cause | Correction |
|---|---|---|
| Starts editing code before outputting commit plan | Skipped the alignment gate | "Stop. Output the commit plan first, wait for ack." |
| Mentions BEHAVIOR.md as part of uClaw runtime | Confused dev docs with runtime (the v1.0 bug) | Re-read spec §1.1 / §1.2; do not import dev docs into runtime |
| Tries to call GitNexus MCP from Rust runtime code | Confused dev tools with runtime | GitNexus is dev-only; AST work goes via in-process tree-sitter |
| Combines multiple A tasks into one PR | Bisectability violation | "Each A task is its own PR. Revert and rebase." |
| Skips the SSoT update commit | Treats it as follow-up | "MILESTONE_STATUS update is in this PR, not separate." |
| Skips drift check | Closed-loop discipline violation | "Run drift check before continuing." |
| Reports tests passing without pasting output | Trust-but-verify violation | "Paste cargo test tail output." |
| Asserts an unfamiliar function/file exists | Memory drift | "Verify with grep before asserting; cite line numbers." |

---

## After everything (Phase A)

When Phase A ships, the research doc `2026-05-25-dirac-reverse-engineering.md`
§7.2 Phase A is fully implemented. Phase B (word-anchor upgrade,
ContextManager wire-up) is the natural next step but lives in C2 per
the integration strategy. **Don't start B until C1 closes**.

---

# Phase B — Prompts (C2 work — only run AFTER C1 closes)

> **Hard gate**: Phase B is C2 scope per
> [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](../superpowers/plans/2026-05-22-pr-integration-strategy.md) §7.
> Before any B prompt, confirm MILESTONE_STATUS.md shows **C1 closed**
> (all of A1, A2, A3, A4 merged + M2 closeout report shipped).
> If C1 isn't closed, Claude Code must report it and stop in the
> universal preamble step 2.

**B-tasks**: B1 (word-anchor upgrade — 3 days) and B2 (ContextManager
wire-up + context tools — 2 days).

**Parallelism**: B1 and B2 touch disjoint files
(`anchor_state.rs` + `tools/builtin/{file,edit}.rs` vs
`dispatcher.rs` + `context_manager/manager.rs` + `runtime/context_tools.rs`).
They CAN run in parallel. **Recommended order**: B1 first (larger
token-saving impact, more self-contained). B2 second (depends on A4's
`InjectionContext`, which is already merged when C1 closes).

**Same universal preamble as Phase A** — paste the top "Universal
preamble" block at the start of every B session.

---

## B1 — Word-Anchor Upgrade

```
Task: C2-Dirac-B1 — word-anchor upgrade.

Spec:  docs/superpowers/specs/2026-05-25-dirac-b1-word-anchor-upgrade-design.md
Plan:  docs/superpowers/plans/2026-05-25-dirac-b1-word-anchor-upgrade.md

Branch: claude/dirac-b1-word-anchor-upgrade
PR tag: [C2-Dirac-B1]

This is the largest Phase A/B item (3 days, 6 commits, ~800 lines diff).
Pay attention to scope boundaries — Out of scope (spec §2.2) includes
AST work (Phase C), persistent anchor state, and Sword dictionary file
externalization. If you find yourself touching tree-sitter or adding an
asset file, STOP.

Read the spec end-to-end. Pay special attention to:
- §1.1 — uClaw ALREADY has word-anchor machinery in anchor_state.rs
  (CURATED_WORDS dictionary, align_anchors Myers diff). B1 is wiring
  it through and pivoting the FORMAT, not building from scratch.
- §1.2 — the 4 actual gaps (register_file_lines bug, ReadTool doesn't
  emit anchors, EditTool doesn't validate anchors, format is opaque)
- §3.1 — FileAnchorState pattern (lines + anchors stored together)
- §3.2 — the format pivot (Apple§<literal content>, NOT Apple§<hash>)
- §3.5 — 4-step validator (matches Dirac EditExecutor.resolveAnchor)
- §3.6 — stale-file hard reject (the 'environment as forcing function'
  pattern)
- §8.4 — stale-file rejection is HARD, not soft warning

Then read the plan. Pre-flight Step 0.3 (downstream parser audit) is
critical — the output format change (anchored lines) affects anyone
who parses read_file output. Pre-flight Step 0.4 (verify
PreconditionFailed variant) gates Task 4.

Output:

1. Summary (2-3 sentences) — focus on "we have the machinery; B1 is
   integration + format pivot"
2. Step 0.3 + 0.4 results
3. 6-commit sequence
4. Any clarifying question, or "READY"

Wait for ack.

Critical: the format pivot (Apple§a1f89c → Apple§<literal content>)
must preserve the diff-carry-forward property. Test 5.3 (carries
across inserted lines) and 5.4 (freshens for changed lines) are the
critical regression checks — if those fail, the Myers diff layer
broke.
```

---

## B2 — ContextManager Wire-Up

```
Task: C2-Dirac-B2 — ContextManager wire-up + context.* tools + get_compose_stats.

Spec:  docs/superpowers/specs/2026-05-25-dirac-b2-context-manager-wireup-design.md
Plan:  docs/superpowers/plans/2026-05-25-dirac-b2-context-manager-wireup.md

Branch: claude/dirac-b2-context-manager-wireup
PR tag: [C2-Dirac-B2]

Depends on: C1-Dirac-A4 merged (uses InjectionContext +
BaselineBlockRegistry::render_with_context). Verify before starting.

Read the spec end-to-end. Pay special attention to:
- §1.1 — both ContextManager AND ContextToolSet are 'built but not
  wired'. B2 is plumbing; the hard algorithm work is done.
- §3.2 — the cache-discipline trade-off (turn 1 differs from turn 2+;
  byte-stable from turn 2 onward). This is intentional per A4 spec §8.6.
- §3.3 — the sync→async bridge via tokio::task::block_in_place.
  Pre-flight Step 0.3 verifies multi-thread runtime.
- §3.5 — fragments inject into build_dynamic_context (per-turn block),
  NOT system prompt. Cache discipline is sacred.
- §8.5 — only context.search and context.read are wrapped. The other
  5 are Err(unimplemented) stubs; registering stubs confuses the LLM.

Then read the plan. Pre-flight Steps 0.3-0.5 (runtime check, struct
field discovery, tool registration site discovery) all gate later
tasks. Don't skip them.

Output:

1. Summary (2-3 sentences) — focus on 'plumbing, not algorithm'
2. Pre-flight Step 0.3-0.5 results: tokio runtime kind, ContextArtifact
   field names, tool registration callsite
3. 7-commit sequence
4. Any clarifying question, or "READY"

Wait for ack.

Critical: cache-discipline check at §6.2 — turns 2-N must have
byte-stable system prompt. If post-PR rollout shows system prompt
bytes drifting between turns 2 and N, dynamic content is leaking
into the system prompt. Investigate via the dispatcher.rs:641-644
discipline note.
```

---

## Order-of-operations for Phase B

**Strongly recommended**: B1 first → wait for merge → B2 second.

Reasons:
- B1's token-savings impact is larger and user-visible. Ship the
  measurable win first.
- B2's tests assume ContextManager + builtin tools coexist. If B1's
  EditTool refactor lands in B2's middle, merge conflicts on
  `agent/tools/builtin/edit.rs` and `file.rs` are easy.
- C2 progress is more legible if B1 closes M3 prep work cleanly
  before B2 closes M2 wire-up.

If you must parallelize (e.g., two devs / two agents working
simultaneously):
- B1 owns `anchor_state.rs` + `tools/builtin/{file,edit}.rs`
- B2 owns `dispatcher.rs` + `context_manager/manager.rs` +
  `runtime/context_tools.rs` + `tauri_commands.rs` + `main.rs`
- Coordinate on the M2-A finalization PR (uses A4's
  `render_with_context` — if M2-A finalization lands during B2's
  development, B2 needs to rebase)

---

## After Phase B closes

```
Both B1 and B2 merged. Write a Phase B closeout + C2 readiness check:

1. Cumulative token savings A1-A4 + B1-B2: pull rollout JSONL data
   if available, otherwise structured estimates from the 50-turn
   bench fixture
2. M2 progress: should be ≥75% now (B2 closes M2-B, partial M2-F).
   Pull from MILESTONE_STATUS.md.
3. M3 progress: B1 contributes to M3 Capability Mesh foundations
   (the anchored EditTool + AnchorStateManager have reliability
   metadata now). Estimate.
4. C2 close criteria per pr-integration-strategy.md §7 — what else
   needs to land for C2 to formally close (besides B1+B2)? List
   blockers if any.
5. C3 prep — Phase C2 (AST tools) and Phase C3 (forcing-function
   audit) are next. Confirm no architectural drift in B-phase that
   would invalidate C-phase specs.

Save to docs/superpowers/specs/2026-05-25-phase-b-closeout.md.

Then: if C2 is closeable, write the C2 closeout report draft. If
not, list the residuals.
```

---

## Phase B failure-mode handlers (in addition to Phase A's)

| Symptom | Likely cause | Correction |
|---|---|---|
| B1 anchor stability test fails after format pivot | Myers diff carry-forward broke during refactor | Re-read `align_anchors` lines 70-113; ensure new tokens only assigned for inserted lines, unchanged carry old |
| B1 token capacity test fails (3000 distinct lines) | `generate_anchor_token` salt escalation didn't trigger | Verify the seen-set check + salt increment in `initialize_anchors` |
| B1 ReadFileTool test fails after wiring | A3 tests need re-adjustment for the new anchored output format | Document A3 test adjustments in B1's PR description |
| B2 cache_read_input_tokens drops to 0 across turns | Dynamic content leaking into system_prompt | Check `composed.system_prompt` doesn't include any per-turn varying fields besides the A4 InjectionContext trickle |
| B2 single-thread runtime hits panic on block_in_place | Wrong tokio runtime kind | Switch to channel-based oneshot pattern per spec §3.3 fallback |
| Anyone touches `compose_system_prompt` removal | Out of scope for both B1 and B2 | Spec §2.2 — that's M2-A finalization PR's job |
| Anyone adds the 5 unimplemented ContextToolSet stubs as tools | Spec §8.5 violation | Stubs confuse LLM; ship only working 2 in B2 |

---

## After everything (Phase A + B)

When B1 + B2 ship, the research doc §7.2 Phase A + Phase B are
fully implemented. M2 is closeable, M3 has foundations. Next:

- **C2 closeout** (other than B): M3-T2 ToolRegistry registration of
  existing tools, M2-J Token Usage Settings UI, anything left in M2
  per MILESTONE_STATUS. NOT in research doc — those are existing
  C2 work items.
- **Phase C1** (research doc §7.2 C1): Shadow Git Checkpoint Store
- **Phase C2** (research doc §7.2 C2): AST-Native Editing Tools
  (in-process tree-sitter, ~12-20 days)
- **Phase C3** (research doc §7.2 C3): Forcing-Function Tool Audit

Phase C specs/plans are TBD — write them when C2 closes (not now;
they depend on B-phase landing decisions that may shift the
architecture).
