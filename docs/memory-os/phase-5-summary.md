# Memory OS Foundation — Phase 5 Summary

**Status:** in progress (this PR).
**Builds on:** Phase 1 (V34 tables + EntityPage.metadata.contradictions schema slot), Phase 2 (auto-link edges that feed backlink counts), Phase 3 (WikiSynthesizer LLM-seam pattern), Phase 4 (memory_health_findings table + Health tab).
**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` § Tier-A A4 + Tier-B B2.
**Plan:** `docs/superpowers/plans/agent-memory-os.md` § Phase 5.

## What this PR adds

Phase 5 closes the Foundation layer's last two big gaps:

1. **Recall ranking gets signal-aware.** Two new config knobs let you
   prefer EntityPage hits over Episode fragments (`entity_page_boost`)
   and well-connected entities over isolated ones
   (`backlink_boost_weight`). Both default neutral so the upgrade is a
   no-op until you opt in.

2. **LLM-driven lint joins zero-LLM health.** `memory_lint` runs the
   semantic-tier checks (hub stub, phantom hub, stale summary,
   contradiction) that complement Phase 4's structural-integrity scans.
   Writes into the SAME `memory_health_findings` table with `is_lint=1`
   so the Health tab in `MemoryModule` automatically surfaces lint
   findings alongside health findings, with the existing "lint" badge.

| # | Commit | What |
|---|---|---|
| 1 | `feat(memory): compiled_truth + backlink boost in recall ranking` | Two new `MemoryRecallConfig` fields (`entity_page_boost`, `backlink_boost_weight`), batch `fetch_boost_signals` SQL helper, boost applied inside the existing fusion loop. RRF fusion was already implemented at `recall.rs:1078` — this commit notes that and layers boost on top. 6 unit tests. |
| 2 | `feat(memory-os): memory_lint scenario (4 LLM checks with budget hook)` | `proactive/scenarios/memory_lint.rs` — LintAnalyzer trait + StubAnalyzer + 4 finders + `run_lint_checks` orchestrator. Phantom-hub variant reserved; real detection lands in Engines Phase 15. 10 unit tests. |
| 3 | `feat(memory-os): contradictions persist into EntityPage metadata + UI` | WikiView renders a new "Contradictions" section when `metadata.contradictions` is non-empty. Fixes a snake_case ↔ camelCase wire mismatch in TS types that would have caused silent undefined reads at runtime. |
| 4 | `feat(memory-os): wire memory_lint into ProactiveService + cost guard + IPC` | Periodic lint scan every 120 ticks (~60 min), cost guard reads today's `cost_records WHERE model LIKE 'memory_lint%'`, AppState carries `lint_analyzer`. New `memory_lint_run_now` IPC. |
| 5 | `docs(memory-os): Phase 5 summary` | This document. |

## What this PR does NOT add

- **A real LLM client for lint.** `StubAnalyzer` produces deterministic
  `[stub] ...` findings so the full pipeline (orchestrator + persist
  + Wiki contradictions UI + Health panel rendering) can be tested
  end-to-end without LLM credentials. The seam (LintAnalyzer trait on
  AppState) matches the Phase 3 WikiSynthesizer pattern exactly.
- **Phantom-hub detection.** The variant is in `LintCheckKind` so the
  analyzer signature stays stable across phases, but `fetch_candidates`
  emits zero phantom_hub candidates in Phase 5. Real detection needs
  NER (slug references in free-text without explicit `[[entity:slug]]`
  markup), which is Engines Phase 15.
- **Bidirectional `contradicted_by` field.** Phase 5 writes
  `metadata.contradictions[]` (one-sided). Phase 9 (Cognitive layer)
  adds the bidirectional sync where `page_a.contradictions` and
  `page_b.contradicted_by` mirror each other.
- **Lint cadence based on EntityPage write count.** The original plan
  said "every 15 EntityPage writes". Phase 5 uses tick frequency (every
  120 ticks ~ 60 min) plus the cost cap instead — simpler plumbing,
  same effective rate since the cost cap is the true safety mechanism.
- **Recall boost UI.** The two new knobs (`entity_page_boost`,
  `backlink_boost_weight`) are exposed via the existing
  `memory_recall_config` IPC + DTO — anyone with that settings tab can
  already tune them. A dedicated UI affordance can land later if
  needed.

## How to verify locally

```bash
cd ~/Documents/uclaw
git fetch && git checkout claude/p5-memory-os-lint-and-boost

# 1. Rust build
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head

# 2. Rust tests (Phase 5 additions)
cargo test --lib memory_graph::recall::phase5_boost_tests 2>&1 | tail   # 6 cases
cargo test --lib proactive::scenarios::memory_lint 2>&1 | tail          # 10 cases
cargo test --lib memubot_config::tests::memory_os 2>&1 | tail            # +2 Phase 5 cases

# Earlier-phase tests should all still pass:
cargo test --lib memory_graph::store::tests::auto_link 2>&1 | tail
cargo test --lib proactive::scenarios::memory_health 2>&1 | tail

# 3. TS check
cd ../ui && npx tsc --noEmit 2>&1 | head

# 4. IPC smoke in cargo tauri dev:
#
# // Set up: create two EntityPages so contradiction finder has candidates
# const a = await __TAURI__.core.invoke('memory_entity_page_create', {
#   input: { slug: 'alice', title: 'Alice',
#            compiledTruth: 'Senior engineer at Acme.', metadata: { subkind: 'entity' } }
# });
# // Append two timeline entries that disagree (stub doesn't detect — but the
# // candidate fetcher emits a contradiction CANDIDATE for any page with >=2 entries)
# await __TAURI__.core.invoke('memory_entity_page_append_timeline', {
#   input: { nodeId: a.node.id, date: '2026-05-01', text: 'works at Acme' }
# });
# await __TAURI__.core.invoke('memory_entity_page_append_timeline', {
#   input: { nodeId: a.node.id, date: '2026-05-15', text: 'works at Beta' }
# });
# // Also a hub-stub candidate
# const stub = await __TAURI__.core.invoke('memory_entity_page_create', {
#   input: { slug: 'stub-hub', title: 'Stub Hub',
#            compiledTruth: 'x', metadata: { subkind: 'entity' } }
# });
# // (Backlinks must be created via referencing Episodes; skipped here for brevity.)
#
# const outcome = await __TAURI__.core.invoke('memory_lint_run_now', { input: {} });
# // -> { contradiction: 1, hub_stub: 0..1, ..., total_inserted: 1+,
# //      total_tokens: 0, analyzer_descriptor: 'stub:no-llm' }
#
# const findings = await __TAURI__.core.invoke('memory_health_list_findings', {
#   input: { checkKind: 'contradiction' }
# });
# // -> at least one HealthFindingDto with isLint=true
#
# 5. UI: open Kaleidoscope memory → Wiki tab, click the Alice page.
#    The Contradictions section is empty (stub doesn't write to
#    metadata.contradictions[]; only real LLM does). To exercise the UI:
#    use the dev console to insert a synthetic contradiction directly:
#
# // (dev-only — appends to metadata)
# // The Lint section in Health tab should also show the new finding.
```

## How to disable / roll back

### Disable Phase 5 entirely

```jsonc
{
  "memory_os": {
    "memory_lint_enabled": false
  }
}
```

ProactiveService stops the periodic scan; `memory_lint_run_now` IPC
returns a structured error. Health-tab list/dismiss for existing lint
rows keeps working.

### Cap the cost

```jsonc
{
  "memory_os": {
    "memory_lint_daily_token_budget": 0
  }
}
```

Even with `memory_lint_enabled: true`, a budget of 0 means
`remaining_budget < 4096` always — every candidate is skipped. The cap
is the real safety mechanism; the flag is the kill switch.

### Tune the recall boosts back to neutral

Defaults are already neutral. If you've turned them up via
`memory_recall_config` IPC and want to revert:

```js
await __TAURI__.core.invoke('patch_memory_recall_config', {
  input: { entityPageBoost: 1.0, backlinkBoostWeight: 0.0 }
});
```

## Adjacent edits called out per CLAUDE.md

- `src-tauri/src/memory_graph/recall.rs` — two new `MemoryRecallConfig`
  fields + DTO field + From impl + `fetch_boost_signals` helper. The
  existing `From<MemoryRecallConfigDto>` + `From<MemoryRecallConfig>`
  pairs both thread the new fields.
- `src-tauri/src/proactive/scenarios/mod.rs` — registered the new
  `memory_lint` module.
- `src-tauri/src/proactive/scenarios/memory_lint.rs` — new file.
- `src-tauri/src/memubot_config.rs` — two new Phase 5 fields with
  forward-compat tests.
- `src-tauri/src/app.rs` — `AppState.lint_analyzer` field +
  StubAnalyzer install.
- `src-tauri/src/proactive/service.rs` — three new fields + three new
  constructor params + clone_state_refs propagation + tick block + the
  `today_start_ms_utc` helper at module scope. **The constructor
  signature change is the third in a row — Phase 6 should probably
  introduce a `ProactiveServiceConfig` struct to avoid further
  positional-arg drift.**
- `src-tauri/src/main.rs` — constructor call updated to pass the new
  params and read `lint_analyzer` from AppState.
- `src-tauri/src/ipc.rs` — `LintRunNowInput`.
- `src-tauri/src/tauri_commands.rs` — `memory_lint_run_now` command.
- `ui/src/lib/types.ts` — `LintRunOutcome` + `LintRunNowInput`. Also
  fixed three snake_case ↔ camelCase mismatches in EntityPageMetadata.
- `ui/src/lib/tauri-bridge.ts` — `memoryLintRunNow` invoke wrapper.
- `ui/src/components/memory/WikiView.tsx` — new Contradictions section
  in EntityPageDetail.

No new V-migration. Phase 5 reuses `memory_health_findings` (V34) with
`is_lint=1` and `cost_records` (V13) for the daily-spend query.

## Performance notes

- Recall boost: short-circuits the SQL roundtrip entirely when both
  knobs are at default (1.0 / 0.0). When enabled, one extra
  `IN (?, ?, ...)` LEFT JOIN per recall — negligible.
- Lint scenario: candidate fetch is three SQL queries, plus zero or
  more analyzer calls (StubAnalyzer is in-process so even with N
  candidates the wall time is ~ms; a real LLM client will dominate the
  budget, which is why the cost guard exists).
- Tick cadence: 120 ticks at default 30s = ~60min between lint scans.
  Tight enough to catch issues, loose enough that even uncapped a stub
  run is sub-second.
- The daily-spend query is one `COALESCE(SUM(...))` against a small
  indexed table — sub-ms in practice.
