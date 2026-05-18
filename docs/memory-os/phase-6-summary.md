# Memory OS Foundation — Phase 6 Summary

**Status:** in progress (this PR).
**Builds on:** Phase 1 (EntityPage CRUD + V35 schema), Phase 2 (auto-link), Phase 3 (`WikiSynthesizer` trait + StubSynthesizer), Phase 4 (`memory_health` zero-LLM checks), Phase 5 (`LintAnalyzer` trait + StubAnalyzer + cost guard + `cost_records` rollup).
**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` § Tier-B B3 (Tier-escalating enrichment) + § Phase 6 plan.
**Plan:** `docs/superpowers/plans/agent-memory-os.md` § Phase 6.

## What this PR adds

Phase 6 closes the LLM seam left open in Phase 3 + 5 and ships
mention-count-driven tier escalation. Three high-level changes:

1. **Shared LLM adapter** (`MemoryOsLlmClient`) — one façade for
   Memory OS scenarios that need a single-shot completion. Phase 6b/6c
   route through this; Phase 6.2 EntitySynthesizer does too.
2. **Stub → Real swap behind opt-in flags.** `RealWikiSynthesizer` and
   `RealLintAnalyzer` exist; both default OFF so users without a
   provider configured keep getting deterministic stub output.
3. **Tier escalator + EntitySynthesizer.** Background scenario maps
   backlink count to `enrichment_tier`; manual button + IPC lets the
   user re-synthesize an EntityPage's compiled_truth on demand.

| # | Commit | What |
|---|---|---|
| 1 | `feat(llm): MemoryOsLlmClient — shared real-LLM adapter` | New `memory_graph/memory_os_llm.rs`. Trait `MemoryOsLlm` + production `MemoryOsLlmClient` (wraps `ProviderService::get_chat_llm_config` → `llm::create_provider`) + cost recording with per-feature `cost_tag` prefix + `MockMemoryOsLlm` for tests. 4 unit tests. |
| 2 | `feat(memory-os): RealWikiSynthesizer (Phase 6b)` | wiki_synth.rs gains `RealWikiSynthesizer` impl. Constrained narrative prompt ≤350 words. `MemoryOsConfig.wiki_real_synthesizer_enabled` flag (default OFF). AppState picks Stub vs Real at boot. 6 unit tests + 3 forward-compat config tests. |
| 3 | `feat(memory-os): RealLintAnalyzer (Phase 6c)` | memory_lint.rs gains `RealLintAnalyzer` impl. JSON-only response schema (`{verdict, severity, message, contradiction?}`), parser tolerates surrounding prose, falls back to raw-text warn on malformed output. `MemoryOsConfig.lint_real_analyzer_enabled` flag (default OFF). Phase 5's `memory_lint%` cost guard auto-applies because the new prefix `memory_lint:` matches. 11 unit tests + 3 forward-compat config tests. |
| 4 | `feat(memory-os): tier_escalator scenario` | New `proactive/scenarios/tier_escalator.rs`. Pure threshold function (0-2 → Tier 3, 3-7 → Tier 2, ≥8 → Tier 1), transition classifier (Upgrade/Downgrade/NoChange), orchestrator that scans EntityPages and writes `metadata.enrichment_tier` + `last_escalated_at`. Daily upgrade cap = 10 (downgrades uncapped). Cap state reconstructed via `cost_records.model LIKE 'memory_tier:upgrade%'`. Wired into ProactiveService tick at every 240 ticks (~2h). 10 unit tests + 3 forward-compat config tests. |
| 5 | `feat(memory-os): EntitySynthesizer (Phase 6.2)` | New `proactive/scenarios/entity_synthesizer.rs`. Trait + StubEntitySynthesizer + RealEntitySynthesizer. Structured JSON output schema `{compiled_truth, aliases[]}`. `persist_synthesis` deprecates the previous active version and writes a new one via `create_version` (auto-link runs). `synthesize_entity_now` end-to-end facade. AppState gains `entity_synthesizer` trait-object field. 14 unit tests + 2 forward-compat config tests. |
| 6 | `feat(ui): Phase 6.3 — WikiView Tier badge + Synthesize button` | New IPC `memory_entity_page_synthesize_now`. WikiView reflowed header with colour-coded `TierBadge` (tier 1/2/3 with tooltips) + `Synthesize` button + post-synth descriptor badge ('real:claude-sonnet · 480t' or 'stub synth'). Toast feedback via sonner. `last_escalated_at` surfaced in the metadata strip. |
| 7 | `docs(memory-os): Phase 6 summary` | This document. |

## Real LLM seam — how the swap works

```
              ┌─────────────────────────────────────────┐
              │            AppState (bootstrap)         │
              │                                         │
              │   if wiki_real_synthesizer_enabled:     │
              │     wiki_synthesizer = Real             │
              │   else:                                 │
              │     wiki_synthesizer = Stub             │
              │                                         │
              │   if lint_real_analyzer_enabled:        │
              │     lint_analyzer = Real                │
              │   else:                                 │
              │     lint_analyzer = Stub                │
              │                                         │
              │   if entity_synthesizer_enabled:        │
              │     entity_synthesizer = Real           │
              │   else:                                 │
              │     entity_synthesizer = Stub           │
              └─────────────────────────────────────────┘
                              ▲
                              │  All three Real impls receive
                              │  the same Arc<MemoryOsLlmClient>
                              │
              ┌─────────────────────────────────────────┐
              │        MemoryOsLlmClient                │
              │                                         │
              │   provider_service: ProviderService     │
              │   db: cost_records persistence          │
              │                                         │
              │   complete_text(cost_tag, sys, usr,     │
              │                 max_tokens)             │
              │     1. ProviderService.get_chat_llm     │
              │        _config() → active model         │
              │     2. llm::create_provider →           │
              │        Anthropic | OpenAI-compatible    │
              │     3. provider.complete(...)           │
              │     4. record cost_records.model =      │
              │        '{cost_tag}:{actual_model}'      │
              └─────────────────────────────────────────┘
```

`cost_tag` values shipped:
- `memory_wiki` — used by RealWikiSynthesizer (Phase 6b)
- `memory_lint` — used by RealLintAnalyzer (Phase 6c, Phase 5 cost guard already sums by `LIKE 'memory_lint%'`)
- `memory_entity_synth` — used by RealEntitySynthesizer (Phase 6.2)
- `memory_tier:upgrade_{from}_to_{to}` — used by tier_escalator (zero-cost rows for daily cap accounting)

## Tiers (Phase 6.1)

| Tier | Backlinks | Treatment |
|------|-----------|-----------|
| 1 (full)  | ≥ 8 | LLM writes a full profile + cross-source synthesis; eligible for the most-aggressive re-synthesis cadence |
| 2 (rich)  | 3-7 | LLM writes 200-500 char compiled_truth |
| 3 (stub)  | 1-2 | One-sentence stub; default on creation |

Lower tier number = more important. Upgrades are capped at 10/day
because each may eventually trigger a Phase 6.2 LLM resynth.
Downgrades bypass the cap (they SAVE token spend by demoting irrelevant pages).

## What this PR does NOT add

- **Auto-synth on tier upgrade.** Phase 6.2 stays manual — the Synthesize
  button on each page is the only trigger. A future tick block can pick
  up tier-up'd pages and synthesize them under their own daily cap. This
  preserves the "one button = one decision = one cost surface" property
  while the seam is fresh.
- **Per-role models.** All three real implementations route through
  `get_chat_llm_config()` (the chat-role active model). Per-feature role
  overrides (`role_models["memory_synth"]` = Sonnet, `["memory_lint"]` =
  Haiku, …) is a settings-UI follow-up.
- **Streaming.** The trait method is `complete_text` — Memory OS callers
  always need a full completion, never deltas. The provider's `stream`
  method is intentionally not exposed.
- **Wiki regen cost cap.** Phase 6b writes `memory_wiki:` cost rows but
  no per-day cap is enforced today. The tick cadence (every 60 ticks ~
  5 min) + the 20-snapshot ceiling fed into the prompt is the rate
  limiter. A uniform Memory OS spend dashboard can land later.
- **Tier-1 priority synth queue.** The plan mentions ranking
  re-synthesis attempts by tier — that's Cognitive Phase 10's
  `wiki_compile` job.

## How to verify locally

```bash
cd ~/Documents/uclaw
git fetch && git checkout claude/p6-memory-os-tier-escalation

# 1. Rust build
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head

# 2. Rust tests (Phase 6 additions, by module)
cargo test --lib memory_graph::memory_os_llm 2>&1 | tail                   # 4 cases (6a)
cargo test --lib memory_graph::wiki_synth 2>&1 | tail                      # +6 cases for 6b
cargo test --lib proactive::scenarios::memory_lint 2>&1 | tail             # +11 cases for 6c
cargo test --lib proactive::scenarios::tier_escalator 2>&1 | tail          # 10 cases (6.1)
cargo test --lib proactive::scenarios::entity_synthesizer 2>&1 | tail      # 14 cases (6.2)
cargo test --lib memubot_config::tests::memory_os 2>&1 | tail              # +8 cases for new flags
# Phase 1-5 should still pass:
cargo test --lib memory_graph::store::tests::auto_link 2>&1 | tail
cargo test --lib proactive::scenarios::memory_health 2>&1 | tail

# 3. TS check
cd ../ui && npx tsc --noEmit 2>&1 | head

# 4. Smoke in `cargo tauri dev`:
#
# // Create an EntityPage with no provider configured — Stub path:
# const a = await __TAURI__.core.invoke('memory_entity_page_create', {
#   input: { slug: 'alice', title: 'Alice', compiledTruth: 'Senior eng.',
#            metadata: { subkind: 'person' } }
# });
# const outcome = await __TAURI__.core.invoke('memory_entity_page_synthesize_now', {
#   input: { nodeId: a.node.id }
# });
# // -> { synthesizer_descriptor: 'stub:no-llm', token_cost: 0,
# //      new_compiled_truth: '[stub synthesis] Alice ...' }
#
# // Flip the Real flag in ~/.uclaw/memubot_config.json:
# //   "memory_os": { "entity_synthesizer_enabled": true,
# //                  "wiki_real_synthesizer_enabled": true,
# //                  "lint_real_analyzer_enabled": true }
# // and configure an active provider in Settings.  Restart.
#
# // Same call now goes through the LLM:
# const real = await __TAURI__.core.invoke('memory_entity_page_synthesize_now', {
#   input: { nodeId: a.node.id }
# });
# // -> { synthesizer_descriptor: 'real:memory_os_llm',
# //      token_cost: <real>, llm_model: 'claude-...', new_compiled_truth: '...' }
#
# 5. UI: open Kaleidoscope Memory → Wiki tab → click an EntityPage.
#    Tier badge appears next to slug/subkind (colour-coded). Click
#    "Synthesize" — see Toast on completion + descriptor badge
#    appearing in the header.
```

## How to disable / roll back

### Roll all Phase 6 LLM features back to stubs

```jsonc
{
  "memory_os": {
    "wiki_real_synthesizer_enabled": false,
    "lint_real_analyzer_enabled": false,
    "entity_synthesizer_enabled": false
  }
}
```

Restart. All three trait objects revert to their Stub impls — zero LLM
calls, deterministic placeholder output. Existing cost_records rows on
disk are untouched.

### Disable tier escalation entirely

```jsonc
{
  "memory_os": {
    "tier_escalator_enabled": false
  }
}
```

ProactiveService stops the periodic scan. Existing `enrichment_tier`
values on disk stay. The Synthesize button on each page still works
(it doesn't depend on tier).

### Cap upgrade rate

```jsonc
{
  "memory_os": {
    "tier_escalator_daily_cap": 3
  }
}
```

Tighter than the default 10 — useful while soaking real-LLM behaviour
on a busy graph.

## Adjacent edits called out per CLAUDE.md

- `src-tauri/src/memory_graph/mod.rs` — registered `memory_os_llm` module.
- `src-tauri/src/memory_graph/memory_os_llm.rs` — new file (Phase 6a).
- `src-tauri/src/memory_graph/wiki_synth.rs` — `RealWikiSynthesizer` added (Phase 6b).
- `src-tauri/src/memory_graph/entity_page.rs` — `EntityPageMetadata.last_escalated_at` added (Phase 6.1).
- `src-tauri/src/proactive/scenarios/mod.rs` — registered `tier_escalator` + `entity_synthesizer` modules.
- `src-tauri/src/proactive/scenarios/tier_escalator.rs` — new file (Phase 6.1).
- `src-tauri/src/proactive/scenarios/memory_lint.rs` — `RealLintAnalyzer` + `parse_lint_response` helper + `truncate` helper added (Phase 6c).
- `src-tauri/src/proactive/scenarios/entity_synthesizer.rs` — new file (Phase 6.2).
- `src-tauri/src/proactive/service.rs` — `MemoryOsRuntimeConfig` gains two fields (tier flag + cap); tick_inner gains the Phase 6.1 scan block.
- `src-tauri/src/memubot_config.rs` — four new flags
  (`wiki_real_synthesizer_enabled`, `lint_real_analyzer_enabled`,
  `tier_escalator_enabled`, `tier_escalator_daily_cap`,
  `entity_synthesizer_enabled`); 11 forward-compat tests covering each
  flag's default + explicit override.
- `src-tauri/src/app.rs` — pre-builds three trait-object fields
  (wiki_synthesizer, lint_analyzer, entity_synthesizer) based on the
  per-feature flag. AppState struct grows `entity_synthesizer` field.
- `src-tauri/src/ipc.rs` — `EntityPageSynthesizeNowInput` DTO.
- `src-tauri/src/tauri_commands.rs` — `memory_entity_page_synthesize_now` command.
- `src-tauri/src/main.rs` — `invoke_handler!` registration.
- `ui/src/lib/types.ts` — `EntityPageSynthesizeNowInput` + `EntitySynthesisOutcome` + `last_escalated_at` on `EntityPageMetadata`.
- `ui/src/lib/tauri-bridge.ts` — `memoryEntityPageSynthesizeNow` invoke wrapper.
- `ui/src/components/memory/WikiView.tsx` — `TierBadge` component +
  Synthesize button + post-synth descriptor badge + metadata strip
  reflow. Removed redundant bottom aliases block.

No new V-migration. Phase 6 uses existing `memory_nodes.metadata_json`,
`memory_versions`, and `cost_records` (V13).

## Performance notes

- **MemoryOsLlmClient** has no per-call setup beyond the existing
  `ProviderService::get_chat_llm_config` (read lock on configs) and
  `create_provider` (Arc construction). Negligible compared with the
  LLM call itself.
- **RealWikiSynthesizer**: one LLM call per regenerate. The snapshot
  fed to the prompt is bounded at 20 pages × 200 chars ≈ 4 KB. Output
  capped at 1500 tokens.
- **RealLintAnalyzer**: one LLM call per candidate. Phase 5 already
  caps candidates at 8 per run + 50_000 tokens/day. Both bounds carry
  over unchanged.
- **tier_escalator**: one indexed SELECT + a per-page UPDATE for the
  delta set. On a 10k-node DB the scan completes in well under 50ms.
  Cap reconstruction is a single COUNT(*) on a `model`-indexed table.
- **EntitySynthesizer**: one LLM call + one version write + one
  metadata UPDATE. The version write inherits Phase 2's auto-link
  hook so any new `[[entity:slug]]` references in the regenerated
  text auto-create edges with no extra calls.
