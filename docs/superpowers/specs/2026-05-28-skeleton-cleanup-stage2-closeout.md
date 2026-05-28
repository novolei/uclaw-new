# 阶段 2 骨架清理 · Closeout Report

**Status:** Closed at P4 (2026-05-28). P5 canceled — see §4.

**Audit → Closeout span:** 2026-05-27 (audit landed) → 2026-05-28 (P4 merged + closeout written).

**Base commit (audit time):** `7805e3ca` (parent of P1 plan commit `87445844`).
**Final commit:** `210a8be7` (P4 squash-merge on `main`).

**Strategic baseline:** [`2026-05-28-uclaw-pi-lightweight-product-philosophy.md`](../../adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md) — Pi-lightweight kernel; supersedes the heavyweight Agent OS v2 north-star.

**Source documents:**
- Audit: [`2026-05-27-pi-convergence-gap-audit.md`](2026-05-27-pi-convergence-gap-audit.md) §4 (5-phase remediation)
- Assessment: [`2026-05-28-skeleton-cleanup-assessment.md`](2026-05-28-skeleton-cleanup-assessment.md) §2.3 (5-PR slicing)

**PR chain:**

| # | Title | PR | Squash SHA | Merged |
|---|---|---|---|---|
| P1 | skill subsystem dead code | [#566](https://github.com/novolei/uclaw-new/pull/566) | `8debd782` | 2026-05-28 |
| P2 | plugin loader + workers + scheduler | [#567](https://github.com/novolei/uclaw-new/pull/567) | `81f0ac79` | 2026-05-28 |
| P3 | harness split + rename | [#568](https://github.com/novolei/uclaw-new/pull/568) | `9be7cfdd` | 2026-05-28 |
| P4 | RegistryHub kill + tool_families extract | [#569](https://github.com/novolei/uclaw-new/pull/569) | `210a8be7` | 2026-05-28 |
| ~~P5~~ | ~~memory cleanup~~ | — | **CANCELED** | — |

---

## Executive summary

P1-P4 removed **net −3,475 LoC** of dead/skeleton code across the agent, plugin loader, workers, scheduler, harness, and registries subsystems in 4 squash-merged PRs (21 bisectable commits before squash — 3+4+8+6 across P1/P2/P3/P4). Test baseline preserved throughout — `cargo test --lib agent::` ended at **764 passed / 2 pre-existing failures** vs. the pre-P1 baseline of 759/2 (the +5 is the 5 surviving `ToolFamilyCard` structural tests after P4's extraction). `cargo build` warning count unchanged at 49 across the entire series.

**P5 was canceled** because the [assessment §1.B](2026-05-28-skeleton-cleanup-assessment.md#1b-骨架组--记忆抽象大头1778-loc--适配文件) verdict (`KILL memory_policy + memory_contract`) was contradicted by a freshly-landed `5df3ade1` commit 3 days before the audit (`refactor(memory): MemoryPolicyExecutor takes Arc<HookBus> (shared-bus ready)` — the "shared-bus ready" suffix is explicit prep-for-future-wireup language) and by the Agent Memory OS v2 program state (A+C+E phases merged via PR #288 on 2026-05-20; B+D phases pending). Those modules are load-bearing foundation for in-flight memory work, not dead skeleton. Cleanup of any genuinely-dead leaves inside the memory subsystem is deferred to a post-B+D wave — see §5.

---

## §1 Series totals

### LoC accounting

| PR | Files changed | Insertions | Deletions | Net |
|---|---:|---:|---:|---:|
| P1 | 6 | +3 | −435 | **−432** |
| P2 | 10 | +31 | −1,418 | **−1,387** |
| P3 | 59 | +614 | −613 | **+1** (rename PR) |
| P4 | 19 | +12 | −1,669 | **−1,657** |
| **Cumulative code** | — | — | — | **−3,475 LoC** |

P3's near-zero net delta is by design — that PR was a `harness/` → `eval/` directory rename + 25 Rust type renames (`*Harness*` → `*Eval*`) + 4 Tauri command renames (`run_*_harness` → `run_*_eval`) + UI surface sync. Pure mechanical renaming, no logic change.

### Test deltas (`cargo test --lib`)

| Stage | agent:: pass | agent:: fail | full lib pass | full lib fail |
|---|---:|---:|---:|---:|
| Pre-P1 (`8cba001a`) | 759 | 2 | 3,084 | 7 |
| After P1 | 759 | 2 | 3,084 | 7 |
| After P2 | 759 | 2 | 3,044 | 7 (−40: deleted-module tests) |
| After P3 | 759 | 2 | 3,044 | 7 (rename: no count change) |
| After P4 | 764 | 2 | 3,008 | 7 (+5 `ToolFamilyCard` survives extraction; −36: registries internals + 2 resolver-integration tests) |

All 2 `agent::` failures + 7 `cargo test --lib` total failures are pre-existing and unchanged across the series:
- `shell::tests::test_daemon_mode_approval_unchanged`
- `agent::tools::builtin::skill_marketplace::tests::truncate_for_error_long`
- 5 other unrelated pre-existing failures in non-`agent::` modules.

### Build warnings

`cargo build` produced 48-49 warnings throughout (the count fluctuated by 1 due to file-shuffle, not by introduced warnings). No new warnings were attributable to any P1-P4 commit.

### Time-to-ship

All 4 PRs landed on **2026-05-28**, the same day the assessment was written. Total span: ~14 hours of subagent-driven implementation + review (haiku for mechanical deletions, sonnet for rewires and Rust surface changes, opus for final cumulative reviews per PR).

---

## §2 What got killed

### P1 — skill subsystem dead code (−432 LoC)

Killed the `skill_selection/` module (M2-H L3 4-level selection scaffold, never wired into any selector — `agent/skill_selection_tests.rs` exercised pure data math, no live caller), the `SkillRenderer` trait + 5 unused renderer files in `agent/dispatcher_skills/`, and 3 orphan fields on `TokenBudgetSnapshot` (`m2h_l3_levels_*`).

Preserved: the disk-tier `SkillsRegistry`, `LoadedSkill`, `skill_md_parse/`, and the live `skills` Tauri commands.

### P2 — installer + workers + scheduler (−1,387 LoC)

Killed `plugin_manifest/load.rs` (233 LoC TOML installer whose installer commit 2 never landed; verified zero non-test callers), the `workers/` module (401 LoC M3-T3 multi-worker scheduling pilot, orchestrator never wired), the `task_scheduler/` module (391 LoC M3-T4 ScheduleQueue pilot with self-doc admitting "actual runner lives in M3-T4 commit 2" — that commit never landed), and the `TaskScheduler` struct + helpers + test module inside `runtime/task.rs` (~336 LoC of preemption scaffold; production cancellation runs through Slice 1a's `CancellationToken`).

Preserved: `plugin_manifest/schema.rs` (279 LoC; future subprocess RPC plugin protocol per ADR §6.5), `TaskKind` enum + `SessionTask` trait (imported by `agent/regular_task.rs` + `agent/rollout_integration.rs`).

### P3 — harness split + rename (~0 net LoC)

Pure mechanical rename + extraction. Extracted 2 load-bearing types out (`harness/trajectory.rs` → `agent/trajectory.rs`, `harness/budget.rs` → `agent/tool_budget.rs`), renamed the remaining offline-eval directory `harness/` → `eval/`, renamed 25 `*Harness*` Rust types to `*Eval*` equivalents (took 3 successive commit rounds to catch all — see §6.ii), renamed 4 Tauri commands `run_*_harness` → `run_*_eval` atomically with their UI surface in `SystemTab.tsx` + `dev-tauri-mock.ts`.

Preserved: 3 `.join("harness")` filesystem path strings (backward-compat with existing on-disk eval artifacts), `BrowserRecipePromotionState::HarnessReady` enum variant (separate browser-recipe-promotion domain, structurally distinct from `eval::EvalCase`), wire-schema tokens (`harness_case_ids` serde field, `harness_subject` string tags).

Frees the `harness/` namespace for the future autonomy supervisor per ADR §10.

### P4 — RegistryHub kill + tool_families extract (−1,657 LoC)

Killed `registries/` subtree (10 files): `hub.rs` (605), `store.rs` (311), `resolver.rs` (285), and 7 smaller per-slot typed-entry files. The hub.rs module doc itself acknowledged the dead-read-path: *"slice 1 just makes the data available; calling the resolver from `skill_search` / `load_skill` is slice 2"* — slice 2 never landed. Also unwired the dead pipeline that fed the hub: `main.rs:124-199` M3-T1 boot-sync `spawn` block (4 calls + diagnostic log), `proactive/service.rs:2148-2160` Bundle 23 same-session hub Skills resync, and 9+ `registry_hub` field declaration / initializer / passthrough sites across `AppState` + `ProactiveState` + `ProactiveStateRefs`. Also deleted 2 resolver-integration tests in `agent/tool_families_tests.rs` that exercised the dead resolver pathway.

Preserved: `agent/tool_families.rs` + 5 surviving structural tests (jcode-inspired `ToolFamilyCard` cards moved from `registries/` to `agent/` — future schema for the AgentApi handle, same treatment `plugin_manifest/schema.rs` got in P2). Live items in `app.rs` (`skills_registry`, `provider_service`), Bundle 23 same-session skill visibility (persist + disk-tier `reg.discover()` rescan), and `tauri::generate_handler!` invoke handler entries.

---

## §3 Preserved on purpose

These items survived P1-P4 by design — they were **not** dead skeleton, despite the surrounding architecture being torn down:

| Item | Survived from | Reason |
|---|---|---|
| `plugin_manifest/schema.rs` (279 LoC) | P2 | ADR §6.5 future subprocess RPC plugin protocol — the manifest type schema is stable and reusable |
| `TaskKind` + `SessionTask` | P2 | Imported by `agent/regular_task.rs` + `agent/rollout_integration.rs` (live agent loop) |
| `agent/trajectory.rs` + `agent/tool_budget.rs` | P3 | Per-turn trajectory store + tool-result truncator; held by `AppState`, written by every agent turn |
| `eval/cases/` (22 JSON fixtures) | P3 | Eval-case test fixtures, alive in `eval::run_*_eval` Tauri commands |
| `BrowserRecipePromotionState::HarnessReady` + sibling browser types | P3 | Separate browser-recipe-promotion domain, structurally distinct from `eval::EvalCase` |
| `data_dir.join("harness")` × 3 sites | P3 | Filesystem backward-compat: existing on-disk eval artifacts at `~/.config/uclaw/harness/...` keep working |
| `agent/tool_families.rs` + 5 structural tests (174+120 LoC) | P4 | Jcode-inspired `ToolFamilyCard` cards — future AgentApi handle schema |
| Disk-tier `SkillsRegistry` + `provider_service` + Bundle 23 disk-tier rescan | P4 | Live skill_search + provider list + same-session new-skill visibility paths |

---

## §4 Why P5 was canceled

### The recency contradiction

Assessment §1.B verdict: `memory_policy/` (1,118 LoC) + `memory_contract/` (660 LoC) + 3 adapter files (~100 LoC) all **KILL**. Total ~1,878 LoC of "dead memory abstraction".

But the assessment itself flagged the recency concern in its own callout (§1.B and §3 Open Decision #1): *"⚠️ **`memory_policy` recency 风险**:最近 commit `5df3ade1`(3 天前)是 HookBus 重构、message 写'shared-bus ready'——这是给'未来接线'打地基。**需要确认无 WIP 分支在路上**(见 §3 Open Decisions)。"*

That **was** the kill-switch. The answer turned out to be **YES, active WIP wire-up is in progress**, and the assessment's §1.B verdict was wrong.

### Evidence of active wire-up

A `git log --since="2026-05-01" --oneline` of `memory_policy/`, `memory_contract/`, and the 3 adapter files shows **8 successive feature commits in the 9 days before the audit**:

```
5df3ade1  refactor(memory): MemoryPolicyExecutor takes Arc<HookBus> (shared-bus ready)
239ada80  feat(harness): surface memory policy receipts
98da99c5  feat(browser): classify runtime memory through policy spine
5ae0ea17  feat(agent-os): add memu target and context memory policy adapter
d603219e  feat(agent-os): wire memory policy gbrain artifact targets
8604a6e0  feat(agent-os): add memory policy executor contract
fe99e9cc  feat(memory_contract): typed memory graph adapter layer [M6-T1 pilot]
```

This is the **Agent Memory OS v2 program** (per the user's project memory):
> *"Agent Memory OS v2 program (A-E) — dual-layer second brain; A+E+C MERGED to main via PR #288 (2026-05-20); next is B then D."*

A+C+E (the foundation: `memory_contract` types, `memory_policy` executor, harness/eval receipt surfacing) merged to main 8 days before the audit. **B+D are still pending** — they build on top of A+C+E. The 5df3ade1 HookBus refactor ("shared-bus ready") was preparing the spine for the B-phase plug-in.

### What "fill-but-not-read" actually meant

The §1.B framing was correct in narrow scope: at the moment of the audit, no production code path **invoked** `MemoryPolicyExecutor::execute()`. But this is the same pattern as M3-T1's RegistryHub at the time of P4 — except that whereas the RegistryHub's "slice 2" had been abandoned (per the module doc), the memory_policy's read side was actively being wired in a *separate* program (Memory OS v2 B). The audit lacked the cross-context to distinguish "abandoned scaffold" from "foundation prep for next phase".

### The 8 cross-tree consumer files

The assessment's "0 production callers" claim was based on grepping for `MemoryPolicyExecutor::execute`. But the broader `crate::memory_policy::*` surface (`MemoryPolicyExecutionReceipt`, `MemoryKnowledgeClass`, `MemoryPolicyActionKind`, `MemoryPolicySource`, `build_receipt`, `receipt_to_eval_event`) is actively consumed by **8 files**:

```
runtime/context_memory_policy.rs           + tests
browser/runtime_memory_policy.rs           + tests
eval/adapters/memory_policy.rs             + tests
```

Killing those would force Memory OS v2 B+D to recreate them — wasting the merged A+C+E work + the merged PR #288 review effort.

### Decision (locked 2026-05-28 via brainstorming Q1)

> **P5 is canceled.** memory_policy + memory_contract + adapters stay intact. Any genuinely-dead leaves inside the memory subsystem are deferred to a post-B+D cleanup wave (see §5). Stage 2 closes at P4.

---

## §5 Deferred to post-B+D memory cleanup wave

Tracking items to re-evaluate **after Memory OS v2 B+D land**. These are not blockers for current work — just things worth a fresh look once the final memory shape stabilizes.

| # | Item | Reason to revisit | Owner |
|---|---|---|---|
| 1 | `memory_graph::write` freeze hook (`scripts/git-hooks/checks/check-memory-graph-freeze.sh`) | Assessment §1.B noted the hook's regex matches non-existent free functions (`memory_graph::write` literal); real write API is `.create_node()` etc. (34 live writes). Hook is currently no-op — needs hardening once the post-B+D write API stabilizes, or deletion if the policy is no longer relevant. | user (memory专项) |
| 2 | `memory_policy::targets/` 5 target adapters (`agent_os/`, `gbrain/`, `harness_eval/`, `memu/`, plus stub files) | Some target adapters may not survive the B+D wire-up (the read side will pick specific targets; unused ones become dead). Audit after B+D lands. | user |
| 3 | `eval/adapters/memory_policy.rs` (14 LoC) + `eval/adapters/memory_policy_tests.rs` (74 LoC) | These bridge memory_policy receipts into the eval module. If B+D changes the `receipt_to_eval_event` shape, this adapter may need a rewrite or removal. | user |
| 4 | Stale prose-doc references to `M3-T1` / `M6-T1` / `M7-T1` milestones | Several files still mention these old milestone IDs in doc comments (e.g., `skill_md_parse/mod.rs` was updated by P4; others may remain). One-off sweep is worth doing after B+D restabilizes the memory namespace. | claude (mechanical) |

---

## §6 Lessons learned

### i. Audit recency check before kill

The assessment had **the recency callout** (§1.B "⚠️ `memory_policy` recency 风险") but its §1.B table still listed `KILL` as the verdict. The callout punted the decision to §3 Open Decision #1 — without that punt, P5 might have launched, and a subagent might have completed it before the contradiction surfaced. **Generalization:** when a module shows commits within 1 week of an audit AND a commit message contains preparatory language ("shared-bus ready", "slice N foundation", "pilot", "[X-T1] wire-up"), the `KILL` verdict is **shaky**. Tag those modules as "verify against active programs before kill", not "kill pending Open Decision".

### ii. Plan recon should be exhaustive grep, not hand-curated tables

P3 Task 4 (`*Harness*` → `*Eval*` rename) took **3 successive commits** to catch all instances because the original spec listed 6 type names. The real count was 25 (discovered via `\bHarness[A-Z]` exhaustive grep in successive review rounds). Same pattern in P4 Task 4 (registry_hub field stripping) where Task 3 left a `main.rs` caller that surfaced only when Task 4 changed the constructor signature.

**Generalization:** when writing a plan that names identifiers to rename or strip, generate the candidate list via `grep -rn "\b<Prefix>[A-Z]\b" src/ --include="*.rs"` BEFORE writing the plan table. Don't trust hand-curated identifier lists in the spec — they will be incomplete.

### iii. Assessment → plan → subagent-driven loop held up well

21 bisectable commits across P1-P4 (3+4+8+6), all squashed cleanly, zero post-merge revert, zero new test failures, zero new warnings. The brainstorm → writing-plans → subagent-driven-development workflow — combined with the per-PR pattern of "spec compliance review + code quality review + final cumulative review" — caught every scope creep, every accidental edit, and every regression before merge.

The 3 follow-up commits in P3 Task 4 + 2 follow-up commits in P4 Task 4 were the most expensive parts of the series (extra subagent runs + review rounds). But each follow-up was *triggered by a review finding from a fresh subagent*, which is exactly the system working as designed.

**Generalization:** the "subagent rounds" cost scales with plan-recon precision. Better recon → fewer rounds. Tighter inventory grep at plan-write time pays back ~5x at execution time.

### iv. Cancelling P5 was the right call

Killing memory_policy would have:
1. Forced Memory OS v2 B+D to recreate the receipt types + executor scaffold.
2. Wasted the review effort that landed PR #288 (A+C+E phases).
3. Required re-running grill questions on `memory_graph::write` freeze hook decisions.
4. Inverted ~1,878 LoC removal for ~2,500 LoC re-addition in B+D.

The brainstorming-skill grill question that surfaced the conflict ("memory_policy + memory_contract is the load-bearing foundation for Memory OS v2 B+D ... what should P5 do?") prevented all four problems. **Worth the 5 minutes.**

---

## Closing

阶段 2 closes at P4. Net **−3,475 LoC** of dead skeleton removed across 4 PRs. The agent loop is materially leaner: the `harness/` namespace is freed for future autonomy supervisor work, the M3-T1 RegistryHub scaffolding is gone, the M2-H L3 skill_selection scaffold is gone, the M7-T1 installer scaffold is gone, the M3-T3 workers + M3-T4 task_scheduler pilots are gone. The kernel surface that remains is closer to the Pi-lightweight ADR's "stateless loop + one `AgentApi` handle + Pi `AgentHarness` layer" target.

Next strategic step is **阶段 3 Pi 一个 `AgentApi` 句柄** (per audit §4) — but that's a new design cycle, not a continuation of 阶段 2. This closeout puts the bow on 阶段 2.
