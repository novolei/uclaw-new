# Dirac Phase A / C1 Closeout Report

> **Slot**: spec (summarizes shipped implementation decisions, per
> `docs/research/2026-05-25-dirac-phase-a-prompts.md` §"After all four merged — closeout").
> **Date**: 2026-05-25. **Author**: autonomous orchestrator (claude-opus-4-7).
> **Scope**: the Dirac borrow **Phase A** (A1–A4), which is the M2-closeout
> *slice track* of C1 per `docs/research/2026-05-25-dirac-reverse-engineering.md` §7.2.

---

## 0. Sequence outcome

| PR | Tag | Merge SHA | Reviewer iterations | Risk |
|---|---|---|---|---|
| #496 | C1-Dirac-A1 | 17ffe1c6 | 0 (APPROVE 1st) | LOW |
| #498 | C1-Dirac-A2 | 52ba4833 | 1 review, REQUEST_CHANGES(low)→fixed inline | MEDIUM |
| #505 | C1-Dirac-A3 | 35187bf3 | 0 (APPROVE + 1 non-blocking doc advisory, fixed) | LOW-MEDIUM |
| #508 | C1-Dirac-A4 | f70f6fc4 | 0 (APPROVE) | LOW |

4/4 merged. 1 reviewer change-request across the phase (A2 low), well under
the protocol §6 sequence-level reject budget (4). Zero escalations after the
Phase-0 environment escalation was resolved (see §3).

What each shipped:
- **A1** — symmetric `tool_use`/`tool_result` pair repair in
  `agentic_loop::purge_orphaned_tool_results` (inserts placeholder
  `ToolResult` for orphaned `ToolUse`; signature widened to `&mut Vec`).
  Eliminates intermittent Anthropic "tool_use without matching tool_result"
  rejections after compaction.
- **A2** — `EditTool` batch form `{files:[{path,edits}]}` with two-phase
  validate-all-then-apply atomicity + first-failure-skips-rest. N-file
  refactor in 1 tool call. Loose schema (provider-agnostic).
- **A3** — `[File Hash: 0x<fnv1a32>]` header on every `read_file` + optional
  `assume_hash` short-circuit (reuses `anchor_state::fnv1a_32`). Content-
  addressed memoization for repeated reads.
- **A4** — JIT injection-policy channel for `BaselineBlock`
  (`InjectionPolicy` / `InjectionContext` / module-level
  `render_with_context`). Channel only — no payload, no
  `compose_system_prompt` wiring.

---

## 1. Token savings observed

**Status: MODELED, not measured.** The autonomous run had no bench harness
and no live-LLM session available, so the A2 round-trip bench (spec §6.3) and
A3 manual integration smoke (spec §6.2/§6.3) were **not run**.

Modeled (from the specs, not empirical):
- **A2**: an N-file refactor collapses N tool calls → 1 (spec §1 cost table:
  8→1 for `refactor_DynamicCache`). Dirac's published eval attributes the
  bulk of its 2.8× cost reduction over Cline to this batching change.
- **A3**: re-reading an unchanged file returns ~20 tokens instead of the full
  body — modeled ~79% saving on a 200-line file re-read 5×, approaching
  95–99% for 50 KB files (spec §3.5).

**Action**: the empirical numbers must come from the **M2-closeout 50-turn
bench** (integration-strategy §7 C1.5) — see §4. Do NOT cite the modeled
figures as measured results.

---

## 2. M2 progress: before vs after Phase A

- **Before** (MILESTONE_STATUS pre-Phase-A): M2 ≈ 55–58%.
- **Phase A delivered** 4 M-Wireup slices. Per-spec self-estimates: A1 ~+3%,
  A2 ~+5%, A3 ~+3–4%, A4 ~+2%. These are wire-up/architecture slices, not DoD
  gate completions.
- **After**: M2 ≈ **63–65%** (estimate). The four DoD *quantitative* gates
  (50-turn token −60–75%, cache-hit ≥50%, monthly cost ↓60%, format
  consistency +1.5/5) remain **unmeasured** — Phase A did not close them.

So Phase A moved M2 from "~58% pilots+slices" to "~63–65% with 4 more
production slices," but the measured-DoD bar is still open.

---

## 3. Spec ambiguities hit + resolutions (input for Phase B/C authoring)

| # | Where | Ambiguity | Resolution |
|---|---|---|---|
| 1 | A1 §4 vs §3/§12 | §4 said "keep `&mut [ChatMessage]` signature, 4 call sites untouched", but §3's algorithm inserts messages (impossible on a slice) and §12 reviewer-focus explicitly expected a `&mut Vec` change | §4 stale; implemented `&mut Vec<ChatMessage>`, propagated to 3 prod call sites. Reviewer verified. |
| 2 | A2 plan Step 3.1 vs spec §3.2 | Plan sketch did interleaved per-file validate→apply (would write file a before discovering file b's validation failure); spec §3.2 + test 5.8 require two-phase validate-ALL-then-apply | Spec governs; implemented true two-phase. test 5.8 reads file back from disk to prove no partial write. |
| 3 | A2 §4.1/§8.3 oneOf | Spec preferred `oneOf` schema but required per-provider live testing (Anthropic/OpenAI/Gemini) which autonomous mode can't run | Shipped the spec-sanctioned loose-schema fallback (both `files`+`path` optional) — provider-agnostic by construction. |
| 4 | A2 review (low) | Pre-failure files (i<fail_idx) were labeled "Skipped due to failure on prior file" — false for files before the failure | Fixed: i<fail_idx → "Not applied — batch aborted: validation failed on a later file (<path>)". |
| 5 | A3 §3.1/§4 | Spec sketched a fresh `compute_file_hash`; `anchor_state::fnv1a_32` already existed | Reused it (no duplication). FNV known-vectors test locks the algorithm. |
| 6 | A3 plan | Plan test used `result.output_text()`; no such method exists | Used real accessor `result.result["content"].as_str()`. |
| 7 | A4 §3.4 | Spec assumed a `BaselineBlockRegistry` struct with `.iter()`; reality is module-level `registry()`/`render_all()` fns | Adapted `render_with_context` to a module-level `pub fn`. |

**Environment-level findings (resolved, but should inform any future autonomous run):**
- **No CI** on the repo (`.github/workflows/` absent) — the protocol's stage-5
  CI gate is inoperable. Escalated at Phase 0; user authorized "local cargo
  (build+test+clippy) green + adversarial review" as the merge gate in lieu
  of CI. **Residual risk is higher than the protocol's ~80–85% estimate**
  because there is no out-of-band CI catch.
- **Repo-wide clippy `-D warnings` debt** (pre-existing: provider-core
  `derivable_impls`, `agentic_loop::build_compression_summary_refs` dead code,
  many unused imports). The clippy gate was narrowed to "changed code
  introduces no new lints." Implementer clippy checks false-negative (halt at
  the first pre-existing error before compiling the target crate) — orchestrator
  must re-run with `-A clippy::derivable_impls` and grep the changed file.
- **Concurrent process** edited the shared main working dir (`edit.rs`,
  `dispatcher.rs`) mid-A3. User directed isolation; A3/A4 + this closeout ran
  in a dedicated worktree (`uclaw-worktrees/dirac-seq`). **Recommendation:
  future autonomous sequences should start in a dedicated worktree.**
- **`origin/main` advances** during the run via concurrent codex PRs
  (#495/#497/#499/...). Scope checks must use `git merge-base origin/main HEAD`,
  not the moving `origin/main` ref.

---

## 4. Is C1 closeable?

**The Dirac Phase A track of C1: YES — closed by this report.** Per the
governing definition in the orchestrator prompt + phase-a-prompts §"After
everything" ("C1 closed = A1–A4 merged + M2 closeout report shipped"), the
condition is met: all four merged, this report ships.

**The broader integration-strategy §7 C1 (full M2 closeout): NO — still open.**
Phase A is one slice track. The other C1 work items remain:
- **C1.1** Bundle 17-B/C dispatcher fold-delta wire-up
- **C1.2** M2-J Token Usage UI → Settings
- **C1.3** M2-H L3 skills top-K wire-up
- **C1.4** M2-B ContextManager + M2-F context-tools wire-up (B2 addresses
  part of this in Phase B)
- **C1.5** **50-turn benchmark + cached_input_tokens measurement** — the
  empirical data that would validate §1's modeled token savings and the M2
  quantitative DoD gates.

So this report **closes the Dirac C1 sub-track** and updates MILESTONE_STATUS
accordingly, while explicitly leaving the broader M2-closeout (C1.1–C1.5 +
formal bench) open and tracked.

---

## 5. C1 (Dirac track) closeout — declaration

Per integration-strategy §6 cutoff criteria, for the **Dirac Phase A track**:

1. ADR/spec scope for A1–A4: ✅ all four specs' in-scope items delivered
   (verified per-PR by fresh-context adversarial review).
2. plan-doc tasks: ✅ each plan executed; commits bisectable.
3. Benchmark data: ⚠️ **MODELED only** — empirical 50-turn bench deferred to
   C1.5 (broad M2 closeout). Flagged, not hidden.
4. "Closes" evidence in PR descriptions: ✅ each PR cites spec + research §7.2.
5. This closeout report: ✅ (this document).

**Verdict**: the Dirac Phase A / C1-slice track is **closed**. Broad M2
closeout continues via C1.1–C1.5. Phase B (B1, B2 — C2 scope) may now proceed
per the phase-a-prompts Phase B hard gate (A1–A4 merged + this report shipped).

---

## 6. Recommended next actions

1. **Proceed to Phase B** (B1 word-anchor upgrade, B2 ContextManager wire-up)
   — the hard gate is satisfied.
2. **Schedule the C1.5 50-turn bench** as the empirical validation of A2/A3
   token savings + M2 quantitative DoD. Until then, do not report Dirac token
   savings as measured.
3. **Burn down the clippy `-D warnings` debt** in a separate `[Backlog]` PR so
   a real CI clippy gate can be enabled (prerequisite for restoring the
   protocol's full stage-5 CI guarantee).
4. **Add a minimal CI workflow** (cargo build+test+clippy + tsc) so future
   autonomous sequences regain the out-of-band catch the protocol assumes.
