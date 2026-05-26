# Milestone Status — Single Source of Truth

> **Live state** of `uclaw-upgrade-implementation-plan.md` M0-M9 milestones.
> Updated manually after each PR merge per closed-loop process §5.1 (see
> [`plans/2026-05-22-pr-integration-strategy.md`](plans/2026-05-22-pr-integration-strategy.md)).
>
> **Last updated**: 2026-05-25 by Ryan + Cowork (claude-opus-4-7) — **full-codebase audit reconciliation**
> **After**: Whole-codebase audit vs `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` (M0-M9). The trackers had drifted hard from reality: SSoT/strategy scope froze at PR #396, but #397-#525 (~129 PRs, ~99 of them M6 browser-runtime) landed since. Authority rule applied: **status/% facts → code is authoritative** (corrected below); **design intent / deliverables → ADR is authoritative** (divergences logged as reconciliation debt in §"Design-debt"). See drift log + §"Audit 2026-05-25" for evidence.
> **Headline corrections**: M6 0% → **~65%** (massively under-reported); M5 10% → **~35%**; M2 75% → **~45%** (over-reported — broad DoD far from met); M7 0% → ~15% ADR-scoped; M8 0% → ~30% (non-compliant scaffold).
> **Next action** (per owner decision 2026-05-25): **C0 — M6 closeout first** (closest to done + user-visible), then C1 M2 closeout, then C2 M3, then C3 M4.

---

## Quick view

> ⚠️ %s below were re-baselined 2026-05-25 against the actual codebase. "WIRED" = in the real production path; pilots/types-only/dead stubs are discounted. Where code diverged from ADR design intent, the cell carries an ADR-reconciliation note (see §"Design-debt").

| M | 名称 | % | 状态 | Next action |
|---|---|---|---|---|
| **Phase 0.5** | Infrastructure (LICENSE / hooks / skills / crate 复制) | **100%** | ✅ closed | — |
| **M0** | ADR Lock + License + Workspace | **100%** | ✅ closed | — |
| **M1** | Runtime Contracts (2-3 weeks) | **~100%** | ✅ closed | contracts + adapters + rollout JSONL all wired; retro #321 |
| **M2** | Context Fabric (5-7 weeks) | **~45%** ⬇️ | 🟡 in-progress | was reported 75% — over-reported. Core scaffold + Dirac wire-ups real (ContextManager wired, 2/7 tools, cache placement, M2-G fold in /compact). DoD gap: 5/7 tools, 0/30 fragments populated, L1-L7 mostly unwired, no 50-turn bench/cache-hit/cost data, template engine + Token Budget UI missing. → C1 closeout |
| **M3** | Capability Mesh (6-8 weeks) | **~25%** | 🟡 early | %≈accurate but cells were wrong: ToolRegistry **is per-session WIRED** (dispatcher uses `list_definitions()`); plugin manifest schema+parser exist (unused). Missing: ProviderRegistry health/families, CapabilityProfileRegistry, WorkerRegistry, plugin discovery. 0/3 exit criteria met. → **C2** |
| **M4** | World Projection (3-4 weeks) | **~25%** | 🟡 pilots only | world/ ProjectionStore + subscriber + frontend lib exist; **0 UI consumers**, no `apply_event`, no `diff_since`, no `useWorldProjection` hook. → C3 |
| **M5** | Policy Hooks + Isolation (4-5 weeks) | **~35%** ⬆️ | 🟡 partial | was reported 10% — under-reported. **Full HookBus + all 13 hook events + decision aggregation exist; MemoryWrite hook wired in production.** Gap: agent loop fires only 1/13 events; no isolation profiles; no worktree policy |
| **M6** | Browser Provider 抽象 (3-4 weeks) | **~65%** ⬆️⬆️ | 🟡 in-progress | was reported 0% — **severely under-reported**. PlaywrightCli+MCP provider, runtime pack, Startup Splash/Doctor, Browser Identity settings, recovery surfaces, provider router all built (#414-520, ~99 PRs). Gap: no `BrowserProvider` trait (router instead), runtime-pack I/O not wired, no external provider stubs, cross-provider harness unwired → **exit criterion NOT met**. → **C0 (next)** |
| **M7** | Evolution Factory (6-8 weeks) | **~15%** ⬆️ | 🟠 scope-divergent | was 0%. ADR pipeline (reflect→candidate→gate→promote skill/SOP/script) ≈10%; promotion-gate schema exists unwired. ~35% of *adjacent* `learning/` infra exists but serves user-profile learning, not Evolution Factory |
| **M8** | Teams v1 (5-7 weeks) | **~30%** ⬆️ | 🟠 non-compliant | was 0%. teams/ orchestrator+channel+reviewer+supervisor scaffold (~45% built, wired as isolated Tauri cmd) but **violates ADR**: doesn't emit TaskEvent / use SessionTask / produce harness episode → must be re-wired to count |
| **M9** | Cluster v1 (12-16 weeks) | **~3%** | ⚪ not started | accurate. heartbeat.rs is session-local only; no WorkerNode/cluster infra |

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

### M2 — Context Fabric 🟡 ~45% (re-baselined 2026-05-25, was 75%)

> Plan §4.3 DoD: 10 sub-tasks + bench(50-turn token -60-75%)+ cache hit ≥
> 50% + cost ↓ 60% + format consistency +1.5/5.
>
> **Audit 2026-05-25**: the 75% figure counted pilots + Dirac closeouts as "done". Strict wired-into-production audit puts M2 at **~40-50%**. WIRED: ContextRef/ContextArtifact schema (`runtime/context.rs`), ContextManager in dispatcher hot path (`agent/context_manager/manager.rs`), `context.search`+`context.read` callable (2/7), budget accounting (ComposeStats), cache placement (ephemeral breakpoint), M2-G 8-field StructuredFold in `/compact` path, Dirac InjectionPolicy/InjectionContext. PILOT/MISSING: 5/7 context tools, fragment population (3 sample types, **0/30+ in production**), L1-L7 token defense (only partial L3 estimates), diff re-injection (infra only), template engine (missing), Token Budget UI (backend cmd only, no React). **DoD-gated closeout items (bench / cache-hit / cost / fragments / remaining tools) are the real gap.**

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

### M5 — Policy Hooks + Isolation 🟡 ~35% (re-baselined 2026-05-25, was 10%)

> **Audit 2026-05-25**: the 10% "foundation patch only" was wrong. A full HookBus is built.

| Deliverable | Evidence | Status |
|---|---|---|
| HookDecision + BoundaryRef types | `crates/uclaw-runtime-contracts/src/lib.rs:575,623` (#338) | ✅ WIRED |
| HookBus + decision aggregation (Deny>AskUser>Allow) | `agent/hook_bus/bus.rs:38` | ✅ WIRED (tests) |
| 13 hook events | `agent/hook_bus/event.rs:91` `ALL: [_;13]` | ✅ defined, full serde + tests |
| Hook fired in production | only `MemoryWrite` (`memory_policy/executor.rs:140`) | 🔴 **1/13 fire** — agentic_loop dispatches no hooks |
| Task isolation profiles | — | 🔴 MISSING |
| Dirty-worktree policy | — | 🔴 MISSING |

**Exit criterion** (a hook blocks an unsafe action + rejection appears in trace): **partially met** — MemoryWrite block works; other events don't fire; trace-visibility unverified.
**Outstanding**: wire agentic_loop to fire the remaining 12 events at the right points; isolation profiles; worktree policy.

---

### M6 — Browser Provider 🟡 ~65% (re-baselined 2026-05-25, was 0% "not started")

> **Audit 2026-05-25**: the single biggest tracker error. M6 received ~99 PRs (#414-520) during the 2026-05-21→25 window and is the most-built unclosed milestone. `src-tauri/src/browser/` now has 40+ files.

| Deliverable | Evidence | Status |
|---|---|---|
| BrowserProvider **trait** | — (router instead) | 🔴 MISSING — see Design-debt |
| Provider routing abstraction | `browser/provider.rs:448` `BrowserProviderRouter`; `runtime_contracts.rs:91` `BrowserProviderLane` | ✅ WIRED (struct/enum, not trait) |
| LocalChromiumProvider adapter | `provider.rs:16` id + BrowserContextManager | 🟡 PILOT (no formal wrapper) |
| PlaywrightCliProvider thin lane + worker protocol | `browser/playwright_cli.rs` + `resources/browser-runtime/worker/uclaw-playwright-worker.mjs` | ✅ WIRED (feature-gated OFF) |
| PlaywrightMcp provider | `browser/playwright_mcp.rs` + `_sidecar.rs` | ✅ WIRED (gated) |
| BrowserRuntimeSupervisor | `browser/runtime_supervisor.rs` | 🟡 PILOT (deadline model; reaping/liveness loop incomplete) |
| Runtime pack manager (manifest/paths/doctor) | `browser/runtime_pack.rs` + `_runner.rs` + `_ipc.rs` | 🟡 WIRED schema; **file I/O (download/install/cleanup/rollback) not wired** |
| Startup Splash / Doctor | `ui/src/components/startup/StartupSplash.tsx` + `lib/startup/startup-doctor.ts` + `browser/runtime_status.rs` | ✅ WIRED (front+back) |
| Browser Runtime / Identity settings | `ui/src/components/settings/BrowserRuntimeSettings.tsx` + `browser/identity/` | ✅ WIRED |
| Branded recovery/unavailable states | `StartupSplash.tsx` + `browser/recovery.rs` | ✅ WIRED |
| Guarded raw CDP escape hatch | `runtime_contracts.rs` `RawCdp` lane | 🟡 PILOT (policy only, no executor) |
| Action ladder policy | `provider.rs:331` `decide_browser_provider_route` | ✅ WIRED (cloud escalation stubbed) |
| Provider-independent browser harness | `harness/cases/browser/*.json` + `harness/adapters/browser_provider.rs` parity matrix | 🟡 PILOT (local only; cross-provider exec unwired) |
| Browser Use / Browserbase / Firecrawl stubs | `browser/hosted_provider.rs` (policy enum only) | 🔴 no adapters |
| Site workflow script + domain-skill contract | `browser/recipes.rs` | 🟡 WIRED contract (exec pipeline = M7) |

**Exit criterion** (same harness case runs vs local + mock external provider): **NOT met** — cases run local-only. This + the missing trait + runtime-pack I/O are the C0 closeout gap.

---

### M7 — Evolution Factory 🟠 ~15% ADR-scoped (re-baselined 2026-05-25, was 0%)

> **Audit 2026-05-25**: not 0%, but **scope-divergent**. Adjacent infra exists; the ADR §13 pipeline mostly doesn't.

- `learning/` (candidate, extractor, scheduler, stability_detector, cache, prompt_section) — ✅ WIRED but serves **user-profile facet learning** (style/identity/tooling/veto/goal), not skill/SOP/browser-script evolution. ~35% of adjacent infra.
- Harness promotion gate: `harness/self_improvement.rs` (SelfImprovementGateVerdict) + `harness/campaign.rs:57` `promotion_gate` field — 🔴 schema only, no wiring to block real promotions.
- Reflection generator / candidate builder for task traces → skill/SOP/script: 🔴 MISSING. `proactive/scenarios/gene_evolution.rs` is gene distillation, orthogonal.
- User review surface + rollback path: 🔴 MISSING.
- Browser domain-skill candidate gate (`browser/recipes.rs` + `fe3418b2`) is the closest real M7-shaped artifact.

**Outstanding**: the whole ADR Evolution Factory pipeline. Adjacent `learning/` infra may feed it but is not the milestone.

---

### M8 — Teams v1 🟠 ~30% (re-baselined 2026-05-25, was 0%) — **non-compliant scaffold**

> **Audit 2026-05-25**: a ~45%-built scaffold exists, but it **violates ADR Risk Register ("Teams duplicate runtime")** and so cannot count toward exit as-is.

- `agent/teams/`: orchestrator.rs (Coordinator), channel.rs (typed TeamChannel, persisted), reviewer.rs + runtime_policy.rs (ReviewGate), supervisor.rs, worker.rs — ✅ WIRED, but as an **isolated Tauri command** (`tauri_commands.rs:14790`).
- `workers/spec.rs`: WorkerRole enum (Researcher/Reviewer/Implementor/Synthesizer/Monitor/Custom) — role vocabulary, no registry.
- 🔴 No `TeamSpec`. 🔴 Teams **don't emit TaskEvent**, don't run through SessionTask/RegularTask, produce no harness episode → ADR §14.2 + §15 violation.

**Outstanding**: re-wire teams through TaskSpec/TaskEvent/SessionTask + add team harness episode before M8 progress counts. Add TeamSpec.

---

### M9 — Cluster v1 ⚪ ~3% (accurate)

- `agent/heartbeat.rs` is a **session-local** heartbeat, not cluster infra.
- 🔴 No WorkerNode, ClusterManager, capability routing, load-aware assignment, data-locality, checkpoint/failover, remote ingestion. Truly not started (远期 12-16w).

---

## Design-debt / code↔ADR reconciliation (logged 2026-05-25)

> Where code diverged from ADR design intent. Authority: **ADR wins on intent**; these are tracked as debt, not forced refactors now.

1. **Browser provider mesh duplicates ADR §9 ProviderRegistry** — code built `BrowserProviderRouter` + `BrowserProviderStatus` (browser-only mesh) ahead of M3's unified ProviderRegistry. **Decision (owner, 2026-05-25): reconcile toward ADR.** When M3-T3 lands the generic `ProviderRegistry`, browser providers must register **into** it rather than maintaining a parallel subsystem. Do not refactor preemptively.
2. **No `BrowserProvider` trait** — ADR §16 M6 names a trait; code uses an enum-dispatch router. Acceptable shape for now, but the M6 exit criterion (run a case against a **mock external provider**) needs a trait/interface seam. Add during C0 closeout.
3. **M8 teams bypass the runtime contract** — ADR §14.2/§15 require teams to run TaskSpec + emit TaskEvent. Current teams run isolated. Re-wire before counting M8 progress.
4. **M7 scope drift** — `learning/` (user-profile learning) is being conflated with the Evolution Factory. Keep them distinct in tracking; ADR M7 = task-trace→candidate→gate→promotion.
5. **task-list vs plan M-T numbering** (carried over) — task list splits M3 into T1-T9 pilots; plan §5.2 uses T1-T6. Reconcile at next task-list cleanup.

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
- **2026-05-25 RED (1-week window, 200 PRs, 134 "Backlog") — FALSE ALARM caused by a drift-script classification bug, not real drift.** The audit found those 134 "Backlog" PRs were mostly **real milestone work** mis-bucketed: ~99 are M6 browser-runtime (`feat(browser): …`), ~37 are M2 Dirac/memory, plus M3/M4 work. The drift script (`scripts/milestone-drift-check.sh`) only recognizes explicit `[M#-T#]` / `[Slice…]` / `[Bundle…]` tags in PR titles; the bulk of work used Conventional-Commit scopes (`feat(browser):`, `feat(agent):`) with no milestone tag, so it fell through to Backlog. **Action items**: (1) teach the drift script to map `feat(browser):`→M6, `feat(agent):`/context→M2, world/projection→M4, registry/plugin→M3 (spin off as a `[chore]` PR); (2) enforce per-PR `[M#-…]` tagging going forward per closed-loop §5.1 Rule 1. Real tactical ratio for the window is low; the milestone engine has been running hot on M6, not idle.

---

## Cutoff hall of fame (closed milestones)

> 每关一个 milestone,从 "Quick view" 移到这里 + 链 closeout report。

- **Phase 0.5** closed 2026-05-20 ([report TBD if any retro]). 14 PRs.
- **M0** closed 2026-05-20. ADR Lock + License via Phase 0.5-T1.
- **M1** closed 2026-05-21 via PR #320. Retrospective: PR #321. 17 PRs.
