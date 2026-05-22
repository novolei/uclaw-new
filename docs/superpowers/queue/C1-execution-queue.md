# C1 Execution Queue — M2 Closeout

> **Active queue** for C1 (finish M2). Drives sequential PR execution
> per [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](../plans/2026-05-22-pr-integration-strategy.md) §7.
> Each item is a self-contained PR; agent picks the first unchecked
> and executes per the linked spec.
>
> **Last updated**: 2026-05-22 (queue created from strategy doc §7)
> **Status**: 0/7 items done

---

## How agents use this file

1. Read this file at session start (also enforced by skill
   `uclaw-milestone-closed-loop`).
2. Pick the **first item with `[ ]`** (unchecked).
3. Execute per the linked spec.
4. After PR merge + SSoT update, **edit this file**: change `[ ]` → `[x]`
   and append PR # to "actual PR" cell.
5. Continue or stop based on context budget — don't try to do all 7 in
   one session (the closed-loop discipline lives in each PR's review).

---

## Queue items

### [ ] C1.1 PR-1 — Bundle 17-B /compact fold-delta wire-up

- **Spec**: [`specs/2026-05-22-bundle-17bc-wireup-design.md`](../specs/2026-05-22-bundle-17bc-wireup-design.md) §4 PR-1 + §6.1-6.3 + **§9 reconciliation addendum (binding where it conflicts with §4)**
- **Branch**: `prep/bundle-17b-dispatcher-wireup` *(branch name kept for traceability; actual edits are in tauri_commands.rs not dispatcher.rs — see spec §9.1)*
- **Files touched** (revised per §9.4): `db/migrations.rs` (V52), `agent/compact/{mod.rs, baseline.rs (new), render.rs}`, `tauri_commands.rs` (/compact intercept), `main.rs` (invoke_handler register), `memubot_config.rs`, `CONTEXT.md` (V52 registry row)
- **Commits planned**: 4 (spec addendum + V52 migration & helpers + /compact delta branch & threshold + tests, per spec §9.5)
- **Done means**: PR merged + MILESTONE_STATUS M2 row updated + threshold setting wired (default 5) + agent_fold_baselines table live
- **Unblocks**: C1.1 PR-2, task #146 closure
- **Actual PR**: _(fill on merge)_

### [ ] C1.1 PR-2 — Bundle 17-C FoldDeltaStats telemetry

- **Spec**: same doc §4 PR-2 + §7 (FoldDeltaStats shape)
- **Branch**: `prep/bundle-17c-telemetry`
- **Depends on**: C1.1 PR-1 merged
- **Files touched**: `agent/token_budget/snapshot.rs`, `agent/dispatcher.rs`, `tauri_commands.rs`
- **Commits planned**: 3 (per spec §4 PR-2)
- **Done means**: PR merged + 50-turn fixture test shows `delta_applied` count > 0 + MILESTONE_STATUS M2 row to ~58%
- **Unblocks**: C1.2 (M2-J UI needs this telemetry to display)
- **Actual PR**: _(fill on merge)_

### [ ] C1.2 — M2-J Token Usage UI in Settings

- **Spec**: needs to be written first — `specs/<date>-m2j-token-usage-ui-design.md`
- **Branch**: `prep/m2j-token-usage-ui`
- **Depends on**: C1.1 PR-2 (FoldDeltaStats shape stable)
- **Files touched**: New `ui/src/components/settings/TokenUsageSection.tsx`, mount in IntelligenceTab (or SystemTab), `tauri-bridge.ts` wrapper for `get_token_budget_snapshot`
- **Commits planned**: 2 (backend wrapper if missing, then frontend section)
- **Done means**: UI shows context-window % progress bar, top-10 token tools, FoldDeltaStats breakdown
- **Pattern reference**: mimic `StreamSkillThresholdsSection.tsx` from PR #396
- **Actual PR**: _(fill on merge)_

### [ ] C1.3 — M2-H L3 skills top-K wire-up

- **Spec**: needs to be written — `specs/<date>-m2h-l3-skills-topk-wireup-design.md`
- **Branch**: `prep/m2h-l3-skills-topk-wireup`
- **Depends on**: nothing (independent of C1.1/C1.2)
- **Pilot ref**: PR #336 (`agent/skill_selection`) — top-K + budget types already exist
- **Files touched**: `agent/dispatcher.rs` (replace current skill manifest assembly), possibly `agent/skill_selection.rs` (extend if budget plumbing needed)
- **Per-turn skill manifest budget**: default 1500 tokens, settings-exposed (extend StreamSkillThresholdsSection or new TokenUsage section)
- **Done means**: per-turn skill manifest token measured + capped at budget + telemetry on FoldDeltaStats-sibling stat (or simpler: `skill_manifest_tokens` in TokenBudgetSnapshot)
- **Actual PR**: _(fill on merge)_

### [ ] C1.4 — M2-B ContextManager + M2-F context tools wire-up

- **Spec**: needs to be written — `specs/<date>-m2bf-context-manager-tools-wireup-design.md`
- **Branch**: `prep/m2bf-context-manager-tools-wireup`
- **Depends on**: nothing strictly, but easier after C1.1-C1.3 since they touch dispatcher
- **Pilot refs**: PR #339 (M2-B skeleton), PR #330 (M2-F: search + read + pin/release)
- **Files touched**: `runtime/context_tools.rs` (replace `Storage("M2-F")` stubs), `agent/context_manager.rs`, dispatcher
- **Done means**: agent can call `context.search` / `context.read` / `context.fold` etc. as real tools; pilot `Storage` stubs removed
- **Note**: per plan §4.2 M2-F = 7 context tools. Pilot only has 4. Either implement remaining 3 (`fold`, `cite`, `compare`) here or split into C1.4a/b.
- **Actual PR**: _(fill on merge)_

### [ ] C1.5 — 50-turn benchmark + cached_input_tokens measurement

- **Spec**: `specs/<date>-m2-benchmark-plan.md` (write the methodology first)
- **Branch**: `prep/m2-50turn-benchmark`
- **Depends on**: C1.1-C1.4 merged (need wire-up done to measure savings)
- **Done means**:
  - Fixture: 50-turn session script under `scripts/benchmark/`
  - Run baseline (M2 pilots disabled / pre-Slice-2 head) — recorded token cost
  - Run with M2 fully wired — recorded token cost
  - Compare: target -60-75% per plan §4.3 DoD
  - Cached token hit rate measurement — target ≥ 50%
  - Result document: `docs/superpowers/reports/<date>-m2-benchmark.md`
- **Actual PR**: _(may be doc-only or include the benchmark script)_

### [ ] C1.6 — M2 closeout report + tagged release

- **Spec**: no spec needed; follows M1 retrospective doc pattern (PR #321)
- **Branch**: `prep/m2-closeout`
- **Depends on**: C1.1-C1.5 all merged + bench data exists
- **Done means**:
  - `docs/superpowers/reports/<date>-m2-closeout.md` with: actual vs estimated effort, bench results, sub-task that drifted as Bundles + root cause, advice for M3 startup
  - PR description includes "Closes M2" + links to: ADR §16 M2 exit criteria check, plan §4.3 DoD check, bench data, retro doc
  - MILESTONE_STATUS.md: M2 row → ✅ 100% + move to "Hall of fame"
  - `uclaw-upgrade-implementation-plan.md` §34.4 written (v2.5 snapshot)
- **Actual PR**: _(fill on merge)_

---

## Post-queue

After all 7 items checked, queue file is archived:

```bash
git mv docs/superpowers/queue/C1-execution-queue.md \
       docs/superpowers/queue/archive/2026-XX-XX-C1-completed.md
```

Then start C2 queue (M3 wire-up — 6 sub-tasks per plan §5.2).

---

## Reference

- Queue pattern: [`README.md`](README.md) in this directory
- Closed-loop discipline: [`../plans/2026-05-22-pr-integration-strategy.md`](../plans/2026-05-22-pr-integration-strategy.md)
- SSoT: [`../MILESTONE_STATUS.md`](../MILESTONE_STATUS.md)
- Skill: `.claude/skills/uclaw-milestone-closed-loop/`
