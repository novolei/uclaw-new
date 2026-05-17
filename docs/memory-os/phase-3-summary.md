# Memory OS Foundation — Phase 3 Summary

**Status:** in progress (this PR).
**Builds on:** Phase 1 (EntityPage CRUD + V34 tables), Phase 2 (auto-link).
**Spec:** `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`
**Plan:** `docs/superpowers/plans/agent-memory-os.md` § Phase 3.

## What this PR adds

Phase 3 stands up the **AI Wiki view** — a new Kaleidoscope memory
tab that renders `wiki_artifacts(kind=index|overview)` and lets the
user create EntityPages from the QuickCapture dialog. The wiki is now
end-to-end functional: every ~5 minutes the ProactiveService refreshes
the index, and a "Regenerate Overview" button calls the configured
`WikiSynthesizer`. Phase 3 ships a stub synthesizer that produces
deterministic placeholder markdown clearly labelled "stub"; a
follow-up PR (or Cognitive Phase 10's `wiki_compile`) swaps in a real
LLM client without touching IPC or UI code paths.

| # | Commit | What |
|---|---|---|
| 1 | `feat(memory): wiki_synth module (index SQL-only + overview LLM-injectable)` | `memory_graph/wiki_synth.rs`. SQL-only `regenerate_index`, async `regenerate_overview` through the new `WikiSynthesizer` trait. Ships `StubSynthesizer` (deterministic, no LLM credentials needed). 12 unit + async tests. |
| 2 | `feat(memory-os): wire wiki_synth into ProactiveService tick + AppState` | ProactiveService periodically (every 10 ticks ~5 min) calls `regenerate_index` on `tokio::spawn_blocking`. `AppState.wiki_synthesizer` holds the trait object; defaults to `StubSynthesizer`. New `MemoryOsConfig.wiki_view_enabled` flag. |
| 3 | `feat(ipc): wiki get/regenerate tauri commands + frontend wrappers` | Three new IPC commands: `memory_wiki_get_overview` / `memory_wiki_get_index` / `memory_wiki_regenerate`. `WikiArtifactDto` and `WikiRegenerateOutcome` types on both sides. Registered in `main.rs::invoke_handler!`. |
| 4 | `feat(ui): WikiView component with EntityPage editor + overview panel` | New `components/memory/WikiView.tsx`. Three-region layout (header / collapsible overview / two-column index+detail). Renders compiled_truth + timeline + aliases for selected page. Theme tokens only. Reads via the new IPC wrappers. |
| 5 | `fix(ui): wire WikiView into MemoryModule (correct surface, not MemoryPanel)` | Fix-up: MemoryPanel is an orphan; live surface is `MemoryModule.tsx` (KaleidoscopeShell). Wiki tab added there, MemoryPanel reverted. |
| 6 | `feat(ui): QuickCapture supports creating EntityPage` | `QuickCaptureDialog` grows a fragment / entity_page mode toggle. EntityPage mode collects slug + title + subkind + compiled_truth and calls `memory_entity_page_create`. Slug validation, kebab-case enforced. |
| 7 | `docs(memory-os): Phase 3 summary` | This document. |

## What this PR does NOT add

- **A real LLM synthesizer.** `StubSynthesizer` is the only
  implementation shipped. Replacing it with an Anthropic / OpenAI
  client is a one-line `AppState::new` change — the seam is
  intentionally narrow so Phase 3 ships without dragging in
  provider plumbing / cost telemetry / API-key wiring. The
  WikiView shows a "stub LLM" badge whenever the overview was
  produced by the stub so users can tell at a glance.
- **Inline editing of compiled_truth.** Phase 9 (Cognitive
  provenance) adds the editor with inferredParagraphs visual markers
  on top of the existing detail render path.
- **Hot/Purpose/Log control-plane files.** Those are Cognitive
  Phase 12 (`hot.md` / `purpose.md` / `log.md`).
- **Vitest coverage of WikiView.** The component is built on top of
  patterns already covered by the existing memory test suite
  (Dialog + ScrollArea + ReactMarkdown). A dedicated test file for
  WikiView lands once the markdown editor support arrives in
  Phase 9 — testing the read-only render in Phase 3 would mostly
  retest the existing primitives.

## How to verify locally

```bash
cd ~/Documents/uclaw
git fetch && git checkout claude/p3-memory-os-wiki

# 1. Rust build
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head

# 2. Rust tests (Phase 3 additions)
cargo test --lib memory_graph::wiki_synth 2>&1 | tail
cargo test --lib memubot_config::tests::memory_os 2>&1 | tail
# Existing Phase 1+2 tests should still pass:
cargo test --lib memory_graph::store::tests::auto_link 2>&1 | tail
cargo test --lib memory_graph::auto_link 2>&1 | tail

# 3. TS check
cd ../ui && npx tsc --noEmit 2>&1 | head

# 4. IPC smoke in cargo tauri dev:
#
# const a = await __TAURI__.core.invoke('memory_entity_page_create', {
#   input: { slug: 'acme', title: 'Acme', compiledTruth: 'A startup.',
#            metadata: { subkind: 'entity' } }
# });
# const b = await __TAURI__.core.invoke('memory_entity_page_create', {
#   input: { slug: 'rag', title: 'RAG', compiledTruth: 'Retrieval-augmented generation.',
#            metadata: { subkind: 'concept' } }
# });
# await __TAURI__.core.invoke('memory_wiki_regenerate', {
#   input: { kind: 'index' }
# });
# await __TAURI__.core.invoke('memory_wiki_get_index', { input: {} });
# -> should return a WikiArtifactDto with content containing "Entity (1)" + "Concept (1)"
#
# await __TAURI__.core.invoke('memory_wiki_regenerate', {
#   input: { kind: 'overview' }
# });
# -> { kind: 'overview', synthesizerDescriptor: 'stub:no-llm', ... }
#
# 5. UI: open the memory module (kaleidoscope) and switch to the
#    new "Wiki" pill. Header should show "stub LLM" badge after the
#    overview regenerate.
```

## How to disable / roll back

```jsonc
// ~/.uclaw/memubot_config.json
{
  "memory_os": {
    "wiki_view_enabled": false
  }
}
```

Effects when off:
- ProactiveService stops the periodic index regen.
- All three `memory_wiki_*` IPC commands return a structured error.
- The WikiView shows the error inline (and the Wiki tab is still
  visible — hide-tab gating would require a separate frontend flag
  query which is overkill for Phase 3; the user-facing error
  message is clear enough).

Existing `wiki_artifacts` rows on disk are untouched and reappear
whenever the flag flips back on.

## Adjacent edits called out per CLAUDE.md

- `src-tauri/src/memory_graph/mod.rs` — registered the new
  `wiki_synth` module.
- `src-tauri/src/app.rs` — `AppState` gains
  `wiki_synthesizer: Arc<dyn WikiSynthesizer>`; bootstrap installs
  `StubSynthesizer`.
- `src-tauri/src/main.rs` — `invoke_handler!` registers three new
  commands; `ProactiveService::new` call passes
  `memubot_config.memory_os.wiki_view_enabled`.
- `src-tauri/src/proactive/service.rs` — `ProactiveStateRefs` and
  `ProactiveService` carry a new `wiki_view_enabled: bool`;
  `clone_state_refs` propagates it. Test helper at the bottom of the
  file passes `true` to match `MemoryOsConfig::default()`.
- `src-tauri/src/memubot_config.rs` — `MemoryOsConfig.wiki_view_enabled`
  (default true).
- `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx` — Wiki
  tab in the live module surface (NOT `MemoryPanel.tsx`, which is an
  orphan; see commit `a17fa70`).
- `ui/src/lib/types.ts` + `ui/src/lib/tauri-bridge.ts` — new IPC
  wire types + invoke wrappers.

No new V-migration. Phase 3 reuses the `wiki_artifacts` table created
in Phase 1's V34.

## Performance notes

- `regenerate_index` is one prepare + one query_map over
  `memory_nodes WHERE kind='entity_page'` plus a markdown string
  concatenation. On the order of low-ms for thousands of pages.
- `regenerate_overview` projects 20 page snapshots + corpus counts
  into owned memory before dropping the conn lock — so the
  synthesizer (potentially making LLM network calls) does NOT block
  any other database access. A fresh lock is taken only for the final
  INSERT.
- Tick-side regen runs on `tokio::spawn_blocking` so the rusqlite
  mutex acquisition never stalls the runtime.
- Token cost: zero for index regen, zero for stub overview, real LLM
  cost once a non-stub synthesizer is plugged in (Phase 5 cost
  dashboard already has the `token_cost` column ready in
  `wiki_artifacts`).
