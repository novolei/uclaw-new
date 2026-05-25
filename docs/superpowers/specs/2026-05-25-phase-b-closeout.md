# Dirac Phase B / C2-slice Closeout + C2 Readiness Report

> **Slot**: spec (summarizes shipped implementation decisions), per
> `docs/research/2026-05-25-dirac-phase-a-prompts.md` §"After Phase B closes".
> **Date**: 2026-05-25. **Author**: autonomous orchestrator (claude-opus-4-7).
> **Scope**: the Dirac borrow **Phase B** (B1, B2) — the C2-slice track per
> `docs/research/2026-05-25-dirac-reverse-engineering.md` §7.2. This is the
> 8th and final PR of the autonomous Dirac Borrow Sequence.

---

## 0. Full sequence outcome (8/8)

| # | PR | Tag | Merge SHA | Reviewer iterations | Risk |
|---|---|---|---|---|---|
| 1 | #496 | C1-Dirac-A1 | 17ffe1c6 | 0 (APPROVE) | LOW |
| 2 | #498 | C1-Dirac-A2 | 52ba4833 | 1 (low → fix) | MEDIUM |
| 3 | #505 | C1-Dirac-A3 | 35187bf3 | 0 (APPROVE + doc nit) | LOW-MED |
| 4 | #508 | C1-Dirac-A4 | f70f6fc4 | 0 (APPROVE) | LOW |
| 5 | #509 | C1-Closeout | 142bbc09 | 0 (APPROVE) | doc |
| 6 | #517 | C2-Dirac-B1 | 30008c4b | 1 (medium → fix → re-review) | HIGH |
| 7 | #522 | C2-Dirac-B2 | e4b1af4d | 0 (APPROVE) | MED-HIGH |
| 8 | this | C2-Closeout | — | — | doc |

- **Cumulative reviewer rejections: 2** (A2 low, B1 medium) — under the
  protocol §6 budget of 4.
- **Escalations: 1** (Phase 0 — no-CI/drift-RED/concurrency), resolved by the
  user ("B": local-cargo gate). 0 escalations during PR execution.
- Worktree-isolated (`uclaw-worktrees/dirac-seq`) from A3 onward after a
  concurrent process was detected editing the shared main dir.

Phase B delivered:
- **B1** — word-anchor upgrade: `AnchorStateManager` stores tokens with
  Myers cross-read alignment; format pivot `Apple§<literal>`; `ReadFileTool`
  emits anchored lines; `EditTool` anchored path with 4-step byte-equal
  validator + hard stale-file reject (`PreconditionFailed`).
- **B2** — ContextManager wire-up: `ChatDelegate` holds a per-session
  `ContextManager`; `effective_system_prompt` routes baseline through A4's
  `InjectionContext` while preserving byte-stability; fragments inject into
  the per-turn dynamic block; `context.search`/`context.read` registered;
  `get_compose_stats` Tauri command.

---

## 1. Cumulative token savings (A1–A4 + B1–B2)

**Status: MODELED, not measured.** No rollout JSONL capture and no live-LLM
session were available in autonomous mode, so neither the A2/A3 benches nor
the B1/B2 50-turn fixtures were run with real traffic. Modeled contributions:

- **A2** multi-file batch: N tool calls → 1 (the dominant Dirac cost lever).
- **A3** read-hash short-circuit: ~79–99% on repeated reads of unchanged files.
- **B1** anchored edits: ~0 `old_text`-mismatch errors (token can't drift) +
  fewer re-read/recompute cycles; modeled −15–25% tokens/task on refactors.
- **B2** context-on-demand: positions fragment selection; modest direct token
  win today (fragment sets are empty until M2-D), larger once fragments land.

**Action (unchanged from C1 closeout)**: the empirical numbers must come from
the integration-strategy §7 **C1.5 50-turn benchmark** + `cached_input_tokens`
measurement. Do NOT cite the modeled figures as measured results.

---

## 2. M2 progress

- Pre-sequence: ~55–58%. Post-A (C1 closeout): ~63%. **Post-B: ~75%** (per
  MILESTONE_STATUS). B2 **closes M2-B** (ContextManager: "pilot, wire-up
  missing" → wired) and partially closes **M2-F** (2 of 7 context tools wired;
  the other 5 are unimplemented stubs deferred to dedicated PRs).
- The four M2 **quantitative DoD gates** (50-turn token −60–75%, cache-hit
  ≥50%, monthly cost ↓60%, format consistency +1.5/5) remain **unmeasured**.
  M2 is ~75% by slice/wire-up count, NOT by measured DoD.

---

## 3. M3 progress

B1 contributes M3 (Capability Mesh) **foundations**, not wire-up: the anchored
`EditTool` + `AnchorStateManager` now carry reliability semantics (byte-equal
validation, stale-file rejection) that a future `CapabilityCard` can advertise.
This is foundational only — B1 did NOT register tools into the M3
`ToolRegistry` (that is M3-T2 / C2.1). Estimate: M3 ~22% → ~24% (foundations
nudge, not a wire-up). The bulk of M3 remains untouched.

---

## 4. C2 close criteria (integration-strategy §7) — what else must land

**The Dirac Phase B / C2-slice track: CLOSEABLE — closed by this report.**
Per the governing definition (orchestrator + phase-a-prompts: "Phase B closes
= B1 + B2 merged + Phase B closeout shipped"), the condition is met.

**The broader integration-strategy §7 C2 (M3 Capability Mesh advancement): NOT
closeable.** Dirac Phase B is an M2-closeout slice track; §7 C2 is M3 work.
The real C2 (C2.1–C2.6) is essentially untouched:
- **C2.1** M3-T2: register existing tools (builtin/MCP/memU/skill-as-tool) into `ToolRegistry`
- **C2.2** M3-T3: ProviderRegistry for MCP/gbrain/memU + health TTL
- **C2.3** M3-T4: missing tools (mcp_resource / request_permissions / view_image / tool_search / unified_exec) + V47 migration
- **C2.4** M3-T5: Skill scope ENUM + per-turn injection
- **C2.5** M3-T6: PluginRegistry + manifest (4-source / 5-kind)
- **C2.6** M3 closeout report

So this report **closes the Dirac C2-slice track** and updates MILESTONE_STATUS
accordingly, while explicitly leaving the broad §7 C2 (M3) open. Note that
B2's `context.search`/`context.read` registration is itself a small down-payment
on C2.1 (registering working tools), but the bulk of M3-T2 remains.

---

## 5. C3 prep — no B-phase architectural drift invalidating C-phase

Phase C is Phase C2 (AST tools: `replace_symbol`, `get_function`) and Phase C3
(forcing-function audit). Checked B-phase against the C-phase spec assumptions:

- **AST tools (C2)** layer on B1's byte-range + validation pattern — B1's
  anchored-edit splice + 4-step validator is exactly the substrate
  `replace_symbol` was designed to reuse. No drift; B1 strengthens C2's footing.
- **Forcing-function (C3)** — B1's hard stale-file reject (`PreconditionFailed`)
  is the first forcing-function primitive; C3 generalizes it. No drift.
- **No spec-invalidating change** shipped in B1/B2. `EditArg` stayed a struct
  (A2 shape preserved), `ContextManager`/`ContextToolSet` APIs are additive.

**Known residuals carried forward (documented, benign):**
- `get_file_skeleton` anchor display changed (token vs `Apple§hash`) — B1
  side-finding; no test regression. (escalation/C2-Dirac-B1-side-finding.md)
- B2's registered `context.search`/`context.read` operate on an empty
  `ContextToolSet` separate from the delegate's `ContextManager` until **M2-D**
  unifies the fragment sets. Wire-up is live (bench proves selection); fragment
  *population* is the M2-D follow-up.
- `is_first_act_turn` / `context_pressure_ratio` are inert today (all 10
  baseline blocks are `Always`-policy); A4's channel is wired but dormant until
  a non-`Always` block ships (Phase C verbose-spec blocks).

---

## 6. Governance findings from the autonomous run (for the post-merge audit)

1. **No CI at start; CI appeared mid-sequence.** Phase 0 found no
   `.github/workflows/`; the user authorized a local-cargo merge gate ("B").
   A concurrent commit (`f4c2d71b` "ci: add review and debug workflows") added
   CI between B1 (#517) and B2 (#522). B2 onward were gated by the real CI
   (all of #522's 7 checks passed). **Recommendation**: now that CI exists,
   future autonomous runs use the protocol's real stage-5 CI gate; the
   local-cargo substitution is no longer needed.
2. **Pre-existing repo-wide clippy `-D warnings` debt** persists (provider-core,
   gep, browser modules). Each Dirac PR's changed code is clippy-clean, but a
   real CI clippy `-D warnings` gate still can't pass repo-wide. Burn-down is a
   separate `[Backlog]` PR.
3. **Concurrent-process collision** in the shared working dir forced a worktree
   isolation mid-run (A3). Future autonomous sequences should start in a
   dedicated worktree.
4. **Modeled-not-measured token savings** across the whole sequence — the
   C1.5 50-turn bench is the outstanding empirical validation.

---

## 7. Verdict

**Dirac Phase B / C2-slice track: CLOSED.** All 8 sequence PRs merged
(7 code/doc + this closeout). M2 advanced to ~75% (M2-B closed, M2-F partial).
M3 foundations nudged (~24%). The broad integration-strategy §7 C2 (M3
Capability Mesh, C2.1–C2.6) remains OPEN and is the natural next track.

**Recommended next action**: begin §7 **C2.1 (M3-T2 ToolRegistry registration)**
— the strict C1→C2→C3 ordering is satisfied (C1-slice + C2-slice closed; M2 at
~75%). Before relying on Dirac token-saving claims, run the C1.5 50-turn bench.
