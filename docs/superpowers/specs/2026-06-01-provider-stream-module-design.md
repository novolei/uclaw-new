# ProviderStream Module Design

**Date:** 2026-06-01
**Status:** ProviderStream Module implemented; hot-path adapter continuation in progress
**Parent spec:** `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
**Pi references:** `/Users/ryanliu/Documents/pi_agent_rust/src/model.rs`, `/Users/ryanliu/Documents/pi_agent_rust/src/provider.rs`, `/Users/ryanliu/Documents/pi/packages/ai/src/utils/event-stream.ts`

## Problem

uClaw provider implementations already normalize provider SSE chunks into
`StreamDelta`, and `agent::llm_stream` assembles those deltas into a final
`RespondOutput`. The seam is still shallow for replay, diagnostics, and future
cross-provider QA because `StreamDelta` has no explicit lifecycle around text,
thinking, or tool-call content blocks.

Callers that need event evidence must infer starts and ends from adjacent
deltas. That spreads stream assembly knowledge outside the provider seam.

## Goal

Add a `ProviderStream` Module that converts existing `StreamDelta` values into
a lifecycle event sequence:

```text
StreamDelta
  -> ProviderStreamAssembler
       -> Start
       -> TextStart/TextDelta/TextEnd
       -> ThinkingStart/ThinkingDelta/ThinkingEnd
       -> ToolCallStart/ToolCallDelta/ToolCallEnd
       -> Done
```

The first slice is additive. It does not replace provider streaming or the
agent hot path; it gives tests, eval, and later adapters a stable normalized
event Interface.

## Current uClaw Truth

- `LlmProvider::stream` returns `Stream<Item = Result<StreamDelta, Error>>`.
- `OpenAIProvider` and `AnthropicProvider` both emit `StreamDelta` variants for
  text, thinking, signatures, tool calls, and done.
- `agent::llm_stream::stream_completion` currently owns accumulation and final
  output construction.
- `llm::stream_error` already classifies stream failures, but event lifecycle
  evidence is not represented as a reusable Module.

GitNexus impact:

- `StreamDelta`: LOW, no direct affected processes.
- `LlmProvider`: LOW, four direct implementers. This slice does not change the
  trait.
- `classify_stream_error`: LOW. This slice does not change error
  classification.

## Pi Reference Truth

Pi providers emit a richer `StreamEvent` protocol:

- `Start` and `Done` bracket the stream.
- Text, thinking, and tool calls each have start, delta, and end events.
- The TypeScript `AssistantMessageEventStream` resolves final results from
  terminal events.
- The Rust agent can forward raw provider stream events and also project them
  into assistant-message events.

The transferable design is the explicit lifecycle, not a wholesale provider
rewrite.

## uClaw Adaptation

Add `src-tauri/src/llm/provider_stream.rs` with:

- `ProviderStreamEvent` enum.
- `ProviderStreamAssembler` state machine.
- `push_delta(StreamDelta) -> Vec<ProviderStreamEvent>`.
- `finish() -> Vec<ProviderStreamEvent>` for stream-ended-without-done
  diagnostics.
- Stable content indexes for text/thinking/tool blocks.

This Module can later feed eval evidence, diagnostics, or a replacement
`llm_stream` implementation.

## Interface

```rust
let mut assembler = ProviderStreamAssembler::new();
let events = assembler.push_delta(StreamDelta::TextDelta { text: "hi".into() });
let done = assembler.push_delta(StreamDelta::Done { finish_reason: Some("stop".into()), usage: None });
```

Rules:

- The first pushed delta emits `Start` once.
- A text delta opens text if needed, emits `TextDelta`, and closes when another
  content type starts or done arrives.
- Thinking and tool-call deltas follow the same start/delta/end pattern.
- Signature deltas emit `ThinkingSignature` without forcing a block end.
- `Done` closes active blocks before emitting terminal `Done`.
- `finish()` closes active blocks and emits no terminal `Done`.

## Acceptance Evidence

- Tests prove text deltas are bracketed by start/end and terminal done.
- Tests prove thinking switches to text with the thinking block closed first.
- Tests prove tool call name/argument deltas produce start/delta/end.
- Tests prove duplicate `Done` does not emit another terminal event.
- Tests prove `stream_completion` emits normalized `ProviderStreamEvent`
  values through `StreamSink` for an end-to-end provider path.
- Tests prove provider stream errors emit a normalized error event before retry
  or fatal return.
- Tests prove `stream_completion` assembles `RespondOutput` through the
  ProviderStream collector rather than duplicating text/tool/thinking state in
  the caller.
- `cargo test --lib llm::provider_stream -- --nocapture` passes.
- `cargo test --lib agent::llm_stream -- --nocapture` passes.
- `git diff --check` passes.
- GitNexus `detect_changes` is recorded before commit.

## Non-Goals

- Do not change `LlmProvider::stream`.
- Do not edit OpenAI or Anthropic provider parsers in this slice.
- Do not replace `agent::llm_stream::stream_completion`; route its provider
  delta handling through the ProviderStream Module.
- Do not add UI streaming events.

## Rollback

Revert the ProviderStream commits. Since the Module is additive and the
provider trait is unchanged, rollback leaves the current stream hot path
untouched.
