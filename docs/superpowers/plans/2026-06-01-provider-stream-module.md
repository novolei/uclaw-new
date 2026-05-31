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

## Continuation Slice: Hot-Path ProviderStream Adapter

The parent program spec requires one provider path to use `ProviderStream`
end-to-end. The first slice added the Module but left
`agent::llm_stream::stream_completion` duplicating stream state. This
continuation wires the hot path through the Module while preserving the
existing `LlmProvider::stream` Interface.

- [x] **Step 1: Update spec and run GitNexus impact**

Impact evidence before editing existing symbols:

- `stream_completion` in `src-tauri/src/agent/llm_stream.rs`: LOW, 3 direct
  test callers, no process flows.
- `StreamSink` in `src-tauri/src/agent/llm_stream.rs`: LOW, direct implementers
  `ChatDelegate`, `NoopSink`, and test `RecordingSink`.
- `ProviderStreamAssembler` was not indexed yet; GitNexus returned UNKNOWN for
  the newly added symbol.

- [x] **Step 2: Add failing hot-path tests**

Add tests proving:

- `stream_completion` emits `ProviderStreamEventKind` values through the sink
  for a real provider stream.
- fatal provider errors emit a normalized ProviderStream error event.

Observed RED:

```text
error[E0407]: method `on_provider_stream_event` is not a member of trait `StreamSink`
error[E0599]: no variant or associated item named `Error` found for enum `ProviderStreamEventKind`
```

- [x] **Step 3: Add ProviderStream collector**

Add a collector in `llm::provider_stream` that consumes normalized events and
builds `RespondOutput`, including text, thinking, thinking signatures, tool
calls, finish reason, and token usage.

- [x] **Step 4: Route `stream_completion` through ProviderStream**

Use `ProviderStreamAssembler` for every provider delta. Send each event to
`StreamSink`, let the collector assemble output, and keep retry/cancellation
semantics unchanged.

- [x] **Step 5: Verify and commit**

Run:

```bash
cargo test --lib llm::provider_stream -- --nocapture
cargo test --lib agent::llm_stream -- --nocapture
git diff --check
```

Then run GitNexus `detect_changes(scope: "staged")` and commit.

Observed GREEN before commit:

- `cargo test --lib llm::provider_stream -- --nocapture`: 4 passed.
- `cargo test --lib agent::llm_stream -- --nocapture`: 6 passed.
- `git diff --check`: no output.
