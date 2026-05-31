# ProviderStream Module TDD Plan

> **For agentic workers:** Execute this plan step by step. Keep the first slice
> additive and away from provider hot-path rewrites.

**Goal:** Add a Deep Module that projects `StreamDelta` into explicit provider
stream lifecycle events.

**Spec:** `docs/superpowers/specs/2026-06-01-provider-stream-module-design.md`

## Recon Notes

- uClaw `StreamDelta` is a compact provider-normalized delta type.
- `agent::llm_stream` owns final response assembly today.
- Pi Rust `StreamEvent` exposes start/delta/end/done lifecycle events.
- Pi TypeScript `AssistantMessageEventStream` resolves final results from done
  or error terminal events.
- GitNexus impact for `StreamDelta`, `LlmProvider`, and
  `classify_stream_error` was LOW. This slice adds a new Module and exports it
  without changing those existing symbols.

## Files

- Create: `src-tauri/src/llm/provider_stream.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Update: `docs/superpowers/plans/2026-05-31-pi-modernization-six-modules.md`

## Steps

- [x] **Step 1: Add failing ProviderStream tests**

Add tests proving:

1. text deltas produce `Start`, `TextStart`, `TextDelta`, `TextEnd`, `Done`;
2. thinking closes before text starts;
3. tool call deltas produce tool lifecycle events;
4. duplicate done is ignored.

Run:

```bash
cargo test --lib llm::provider_stream -- --nocapture
```

Observed before implementation: compilation failed because the Module types did
not exist.

- [x] **Step 2: Implement ProviderStream event types**

Add `ProviderStreamEvent`, `ProviderStreamEventKind`, active-block state, and
stable content indexes.

- [x] **Step 3: Implement ProviderStreamAssembler**

Add `push_delta`, `finish`, block close helpers, and duplicate terminal guard.

- [x] **Step 4: Export and verify**

Export the Module from `llm::mod`, then run:

```bash
cargo test --lib llm::provider_stream -- --nocapture
git diff --check
```

Observed: all ProviderStream tests passed and whitespace check had no output.

- [x] **Step 5: Run GitNexus detect-changes and commit**

Stage only ProviderStream files and run GitNexus `detect_changes` on staged
changes. Commit with verification output in the commit body.
