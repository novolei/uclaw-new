# 2026-05-25 Tool Result OpenAI Order Fix

## Goal

Prevent OpenAI-compatible chat requests from failing when compacted or
interrupted history leaves `ToolResult` blocks without the matching assistant
`ToolUse`, and keep memory inventory calls from flooding the next LLM request.

## Evidence

- Failed session `84293774-2317-4166-9c39-764b3cdfdfde` persisted the user turn
  `你记得关于我的什么事情啊` but no assistant reply for that turn.
- Tool artifacts showed `memu_memory` returned 922 memories twice at about
  220 KB each.
- `OpenAIProvider::convert_messages` emitted every `ToolResult` as
  `role="tool"` without checking that a preceding assistant `tool_calls` entry
  was still active.

## Scope

- Add an OpenAI provider-side guard for orphan `ToolResult` blocks.
- Clamp `memu_memory` list/retrieve limits before results are returned to the
  agent loop.
- Add focused unit coverage for both guards.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml llm::providers::openai --lib`
- `cargo test --manifest-path src-tauri/Cargo.toml agent::tools::memu_tools --lib`
