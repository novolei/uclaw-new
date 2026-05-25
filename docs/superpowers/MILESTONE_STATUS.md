# Milestone Status — Single Source of Truth

> **Live state** of `uclaw-upgrade-implementation-plan.md` M0-M9 milestones.
> Updated manually after each PR merge per closed-loop process §5.1 (see
> [`plans/2026-05-22-pr-integration-strategy.md`](plans/2026-05-22-pr-integration-strategy.md)).
>
> **Last updated**: 2026-05-25 by Ryan + Cowork (claude-sonnet-4-6)
> **After PR**: C2-Closeout (Dirac Phase B / C2-slice track closed — see `specs/2026-05-25-phase-b-closeout.md`; full 8-PR Dirac Borrow Sequence complete: A1 #496 / A2 #498 / A3 #505 / A4 #508 / C1-Closeout #509 / B1 #517 / B2 #522). Broad §7 C2 (M3) remains open — next: C2.1 M3-T2 ToolRegistry registration)

---

## Quick view

| M | 名称 | % | 状态 | Next action |
|---|---|---|---|---|
| **Phase 0.5** | Infrastructure (LICENSE / hooks / skills / crate 复制) | **100%** | ✅ closed | — |
| **M0** | ADR Lock + License + Workspace | **100%** | ✅ closed | — |
| **M1** | Runtime Contracts (2-3 weeks) | **100%** | ✅ closed | task #57 closes; retrospective #321 |
| **M2** | Context Fabric (5-7 weeks) | **~75%** | 🟡 in-progress | Dirac C1-slice track closed (#496/#498/#505/#508); B1 merged #517; B2 wires M2-B (ContextManager) + M2-F partial (2/7 tools); broad M2 closeout = C1.1-C1.5 + 50-turn bench |
| **M3** | Capability Mesh (6-8 weeks) | **~22%** | 🟡 early | **C2.1 — M3-T2 ToolRegistry registration** |
| **M4** | World Projection (3-4 weeks) | **~24% (pilots)** | 🟡 pilots only | **C3.1 — M4-T1 wire-up after C2** |
| **M5** | Policy Hooks + Isolation (4-5 weeks) | **~10%** | 🟠 pilot-only | Wait for M3 close (T1 contract patch in #338) |
| **M6** | Browser Provider 抽象 (3-4 weeks) | **0%** | ⚪ not started | Wait for M3 close |
| **M7** | Evolution Factory (6-8 weeks) | **0%** | ⚪ not started | Wait for M2 + M3 close |
| **M8** | Teams v1 (5-7 weeks) | **0%** | ⚪ not started | Wait for M5 + M7 close |
| **M9** | Cluster v1 (12-16 weeks) | **0%** | ⚪ not started | Long-term, after M8 |

---

## Detailed status

### Phase 0.5 — Foundation infra (T1-T10) ✅ 100%

All sub-tasks merged:

| Task | Description | PR |
|---|---|---|
| T1 | LICENSE + NOTICE + THIRD_PARTY procedure | #289 |
| T2 | Cargo workspace refactor | #297 |
| T3 | First 3 utility crates (path / image / home) | #299 |
| T4 | Second 9 utility crates | #300 |
| T5a | path-utils + image + uclaw home sweep | #301 |
| T6 | uclaw_utils_home full-repo sweep | #302 |
| T7 | memory_graph runtime panic guard | (sub-PR) |
| T8 | BEHAVIOR.md + CLAUDE.md lightweight refactor | #292 |
| T9 | git pre-commit hooks (memory_graph::write / dirs::home_dir / SPDX) | #291 |
| T9b | Un-ignore `.claude/` + PreToolUse hooks | #294 |
| T10 | 7 uClaw-specific skills under `.claude/skills/uclaw-*/` | #295 |
| #10 DRI naming | BEHAVIOR.md §10 | #293 |

---

### M1 — Runtime Contracts ✅ 100% (closes at #320, retro #321)

| Sub-task | Description | PR |
|---|---|---|
| M1-T1 | runtime contracts (SessionTask trait skeleton) | #304 |
| M1-T1 patch | HookDecision + BoundaryRef + WorkerId | #338 |
| M1-T2a | SessionTask trait | #305 |
| M1-T2b | agentic_loop state audit spec | #306 |
| M1-T2c | RegularTask wrap + R-1 fix | #307 |
| M1-T2d | R-6 cancellation token | #314 |
| M1-T3 | HarnessSubject bridge | #308 |
| M1-T4a | RegularTask intermediate events | #309 |
| M1-T4b | dispatcher rollout bridge | #311 |
| M1-T4c | browser rollout bridge | #316 |
| M1-T4d | browser rollout wire-up | #317 |
| M1-T4e | automation rollout bridge | #319 |
| M1-T4f | automation rollout wire **(closes M1)** | #320 |
| M1-T5 | rollout JSONL writer + V48 migration | #310 |
| M1-T6 | TokenUsage 6-D + V49 migration | #313 |
| M1-T7 | Prewarm LLM | #315 |
| M1 retro | retrospective doc | #321 |

**Exit criteria met**: agent loop drives RegularTask via TaskEvent stream;
rollout writes to JSONL; HarnessSubject bridges to harness eval.

---

### M2 — Context Fabric 🟡 ~75%

> Plan §4.3 DoD: 10 sub-tasks + bench(50-turn token -60-75%)+ cache hit ≥
> 50% + cost ↓ 60% + format consistency +1.5/5.

| Sub-task | Pilot | Wire-up | Status |
|---|---|---|---|
| M2-A baseline 10 block | #326 + #327 | #328 (compose_system_prompt 用 registry) | ✅ done |
| M2-B ContextManager skeleton | #339 | C2-Dirac-B2 (effective_system_prompt → for_prompt_with_injection; fragments → build_dynamic_context) | ✅ wired |
| M2-C 30+ Context Fragments | #329 (3 samples) | — | 🔴 only 3/20+, wire-up missing |
| M2-D Diff-based re-injection | #340 (pilot) | Bundle 16 + 17 (#384-385); **Bundle 17-B C1.1 PR-1 prep branch ready (V52 + delta path)**; 17-C pending | 🟡 ~85% |
| M2-E Template engine | #324 + #325 | — | 🟡 引入了但还没全勺 |
| M2-F 7 Context Tools | #330 (search + read + pin/release) | C2-Dirac-B2 (context.search + context.read registered) | 🟡 partial — 2/7 wired (search+read); fold/cite/compare/pin/release deferred |
| M2-G 8-field Structured Fold | #331 | #367 Slice 3-A (real /compact) | ✅ done |
| M2-H L1-L7 Token defense | #332-#337 | #365 Slice 2 (L2/L5/L6) + #379 Slice 3-C (L4/I) | 🟡 ~85% (L3 wire-up missing) |
| M2-I Cache placement | #341 | #379 Slice 3-C | ✅ done |
| M2-J Token Budget UI | #342 | — | 🔴 wire-up missing |

**Dirac borrow slices** (C1 = Phase A small repairs feeding M2 closeout; C2 = Phase B wire-ups) — see `docs/research/2026-05-25-dirac-reverse-engineering.md` §7.2:

| Dirac PR | Scope | Status |
|---|---|---|
| C1-Dirac-A1 | tool_use/tool_result pair repair on compaction (agentic_loop, M-Wireup, ~+3% M2) | ✅ merged #496 |
| C1-Dirac-A2 | EditTool batch form ({files: [...]}) (M-Wireup, ~+5% M2) | ✅ merged #498 |
| C1-Dirac-A3 | ReadFile [File Hash] header + assume_hash short-circuit (M-Wireup, ~+3% M2) | ✅ merged #505 |
| C1-Dirac-A4 | JIT injection channel for BaselineBlock (InjectionPolicy/Context, M-Wireup, ~+2% M2) | ✅ merged #508 |
| C2-Dirac-B1 | word-anchor upgrade (record_read align + Apple§literal format + anchored EditTool + stale reject) | ✅ merged #517 |
| C2-Dirac-B2 | ContextManager wire-up + 2 context tools + get_compose_stats (closes M2-B, M2-F partial) | ✅ merged #522 |

**Dirac Phase A / C1-slice track: ✅ CLOSED** 2026-05-25 via closeout report `specs/2026-05-25-phase-a-closeout.md` (4/4 merged, 1 reviewer low-fix, 0 escalations post-Phase-0). NOTE: this closes the *Dirac slice track* of C1; the broader integration-strategy §7 C1 (C1.1-C1.5 below + formal 50-turn bench) remains open. Token savings are MODELED, not yet measured — pending C1.5 bench.

**Dirac Phase B / C2-slice track: ✅ CLOSED** 2026-05-25 via closeout report `specs/2026-05-25-phase-b-closeout.md` (B1 #517 + B2 #522 merged; B2 closes M2-B + M2-F partial). NOTE: this closes the *Dirac slice track* of C2; the broader integration-strategy §7 C2 (M3 Capability Mesh — C2.1 ToolRegistry registration … C2.6 M3 closeout) remains OPEN and is the next track. Full 8-PR Dirac Borrow Sequence (A1-A4 + B1-B2 + 2 closeouts) complete.

**Outstanding for M2 closure**:

1. C1.1 Bundle 17-B/C wire-up (task #146)
2. C1.2 M2-J Token Usage 页接入 Settings
3. C1.3 M2-H L3 skills top-K wire-up
4. C1.4 M2-F remaining context tools wire-up (5 of 7 stubs: fold/cite/compare/pin/release) — M2-B CLOSED by #522, M2-F search+read wired
5. C1.5 50-turn benchmark + cached_input_tokens measurement
6. C1.6 closeout report

---

### M3 — Capability Mesh 🟡 ~22%

> Plan §5.x DoD: 5 registries + plugin manifest + 4-source discovery + 5-kind
> impl + capability profile.

| Sub-task (plan) | Pilot (task list) | Wire-up | Status |
|---|---|---|---|
| M3-T1 5 Registry skeleton | M3-T1 pilot #343 | #390 slice 1 + #391 slice 2 (Tools + Models) | 🟡 ~60% |
| M3-T2 ToolRegistry registration of existing tools | (no pilot) | — | 🔴 not started |
| M3-T3 ProviderRegistry for MCP/gbrain/memU | M3-T9 #349 (MCP types) | — | 🔴 not started |
| M3-T4 缺失工具 (mcp_resource / view_image / etc) | — | — | 🔴 not started |
| M3-T5 Skill scope + per-turn injection | #357 (SKILL.md parser) | (V43 migration exists) | 🟡 partial |
| M3-T6 PluginRegistry + manifest | #358 (M7-T1 in task list, but maps to M3-T6 in plan!) | — | 🔴 not started — **编号有冲突,见 strategy §3.1 注** |
| (extras pilots, not in plan) | M3-T3 #354 worker; T4 #352 scheduler; T7 #353 IM; T8 #357 skill md | — | pilots only |

**Outstanding for M3 closure**: 6 sub-task wire-up (T2-T6). ~6-8 weeks per plan.

---

### M4 — World Projection 🟡 ~24% (all pilots, 0 wire-up)

| Sub-task (plan) | Pilot (task list) | Wire-up | Status |
|---|---|---|---|
| M4-T1 WorldProjection + apply_event | M4-T1 #346 | — | 🔴 wire-up missing |
| M4-T2 projection subscriber + diff_since + V53 | M4-T2 (in batch) | — | 🔴 wire-up missing |
| M4-T3 `useWorldProjection` frontend hook | — (plan-only target) | — | 🔴 not started |
| M4-T4 panel consumer migration | — | — | 🔴 not started |
| (extras) | M4-T3 FS #355; T4 git; T5 browser tab; T6 Slack; T7 mail/cal; T8 doc/dataset | — | pilots in `world/` module, unused |

**Outstanding**: 4 sub-task wire-up. ~3-4 weeks per plan.

---

### M5 — Policy Hooks + Isolation 🟠 ~10% (foundation patch only)

- HookDecision + BoundaryRef contract types in #338 (M1-T1 patch)
- 13 hook events: not implemented
- Isolation profiles: not implemented

**Outstanding**: Full milestone. Should follow M3.

---

### M6-M9 — Not started

- **M6** Browser Provider: design clear in codex-comparison §14; no pilot
- **M7** Evolution Factory: design in codex-comparison §11; **Note: 注意 task
  list "M7-T1 pilot — Plugin manifest" = #358 实际上属于 plan §5.x 的 M3-T6,
  不属于 plan §9 的 M7 Evolution。需要在下次 task list cleanup 时修正**
- **M8** Teams v1: design in codex-comparison §12; no pilot
- **M9** Cluster v1: 远期 12-16w; no pilot

---

## Closed-loop process attached

- **Per-PR**: 合并后 < 2min,编辑本表 + 跑 `scripts/milestone-drift-check.sh`
- **Per-week**: 周一上午跑 drift check + 审 alarm + 留 NOTE
- **Per-month**: 月底回写 `uclaw-upgrade-implementation-plan.md` §34 进度快照,
  升 plan 版本号 v2.X+1

参考: [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](plans/2026-05-22-pr-integration-strategy.md) §5

---

## Drift log (notes when alarms fire)

> 当 drift check 触发红色 alarm 时,在这里追加一行;关 window 后划掉。

- **2026-05-22 RED — consecutive Bundle run = 20 > 7 threshold** (PRs #370-#389 strip before C1.1 resumed). Tactical ratio itself is healthy (25/200 = 12%). PR #397 ([M2-D wire-up]) breaks the run; closing C1.1 resets the counter to 0. **No action required** — alarm is informational; the counter was already broken by #391/#390/#367/#366/#365/#364/#328 (5 M-Wireup) before #397 landed.

---

## Cutoff hall of fame (closed milestones)

> 每关一个 milestone,从 "Quick view" 移到这里 + 链 closeout report。

- **Phase 0.5** closed 2026-05-20 ([report TBD if any retro]). 14 PRs.
- **M0** closed 2026-05-20. ADR Lock + License via Phase 0.5-T1.
- **M1** closed 2026-05-21 via PR #320. Retrospective: PR #321. 17 PRs.
