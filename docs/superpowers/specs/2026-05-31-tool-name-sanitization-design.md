# Tool-Name Sanitization Design

**Date:** 2026-05-31
**Status:** Design (approved in brainstorming; pending spec review)
**Origin:** Production bug — `OpenAI API error: Invalid 'tools[5].function.name': string does not match pattern '^[a-zA-Z0-9_-]+$'`.

## Problem

Both LLM providers serialize a tool's name straight into the request without validation:
- `OpenAiProvider::convert_tools` ([openai.rs:231](../../src-tauri/src/llm/providers/openai.rs)) → `"name": t.name`
- `AnthropicProvider::convert_tools` ([anthropic.rs:186](../../src-tauri/src/llm/providers/anthropic.rs)) → `"name": t.name`

OpenAI requires tool names to match `^[a-zA-Z0-9_-]+$`; Anthropic requires `^[a-zA-Z0-9_-]{1,64}$`. Any tool whose name contains a character outside `[A-Za-z0-9_-]` (a `.`, `:`, `/`, space, or non-ASCII) produces an invalid request and the provider rejects the whole turn. There is **no name sanitization anywhere** in the codebase.

Likely sources of invalid names:
- **MCP tools** — named `mcp__{server_id}__{tool_name}` ([mcp.rs:1750](../../src-tauri/src/mcp.rs)). The prefix/separators are valid, but a `server_id` or an MCP-advertised `tool_name` containing a `.`/`:`/`/` makes the whole name invalid (many MCP servers use dotted tool names).
- **Skill / command / plugin tools** — names can be arbitrary (a skill titled with a space, `.`, or CJK).

This is **provider-agnostic** — it currently surfaces on the OpenAI-compatible path but the Anthropic path has the same latent defect.

## Key structural facts (verified)

1. **`ToolRegistry::list_definitions` emits the registry KEY**, not `tool.name()`: `self.tools.iter().map(|(name, tool)| ToolDefinition { name: name.clone(), ... })` ([tool.rs:353](../../src-tauri/src/agent/tools/tool.rs)). Both providers build their request `tools` array from `ToolDefinition.name`.
2. **Dispatch resolves tool calls by registry key**: `self.tools.get(&tc.name)` ([tool_dispatch/mod.rs:335](../../src-tauri/src/agent/tool_dispatch/mod.rs)). The model echoes back exactly the name we sent.
3. **MCP execution does not depend on the exposed name**: `McpToolProxy` stores `server_id` + `tool_name` as fields and routes the JSON-RPC call through them. `parse_mcp_tool_name`/`from_prefixed_tool_name` are used **nowhere outside `mcp.rs`** (verified) — not on the dispatch/approval/audit path.
4. **`register`/`register_boxed` key on `tool.name()`** and `HashMap::insert` silently overwrites duplicates today ([tool.rs:337/344](../../src-tauri/src/agent/tools/tool.rs)).
5. Nothing cross-references `tool.name()` against the registry key besides registration.

Consequence: if the **registry key** is sanitized at registration, the sanitized name propagates automatically through `list_definitions` → both providers → the model's echoed tool-call name → `dispatch.get(key)`. Everything stays consistent with a single chokepoint, and MCP execution is unaffected.

## Design

Single chokepoint in `src-tauri/src/agent/tools/tool.rs`.

### `sanitize_tool_name`

```rust
/// Regularize an arbitrary tool name to the `^[a-zA-Z0-9_-]+$` shape that both
/// OpenAI and Anthropic require. Invalid chars → '_'; empty → "unnamed_tool";
/// truncated to 64 (Anthropic's upper bound).
pub fn sanitize_tool_name(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if s.is_empty() {
        s = "unnamed_tool".to_string();
    }
    if s.len() > 64 {
        s.truncate(64); // safe: all chars are ASCII after the map
    }
    s
}
```

### `register` / `register_boxed` — sanitize the key + collision dedupe

```rust
let mut key = sanitize_tool_name(tool.name());
if self.tools.contains_key(&key) {
    // Exact duplicate OR post-sanitize collision (e.g. "foo.bar" and "foo_bar").
    let base = key.clone();
    let mut n = 2;
    while self.tools.contains_key(&key) {
        key = format!("{base}_{n}");
        n += 1;
    }
    tracing::warn!(
        original = %tool.name(),
        resolved = %key,
        "tool name collision after sanitize; suffix-deduped"
    );
}
self.tools.insert(key, /* the boxed tool */);
```

(`register<T>` boxes the tool first, then inserts; `register_boxed` inserts the existing box. Both share the sanitize+dedupe logic — factor a small private helper `insert_tool(&mut self, tool: Box<dyn Tool>)` to avoid duplicating it.)

### What does NOT change

- Neither provider's `convert_tools` — they consume `list_definitions` output, which is now sanitized.
- Dispatch — `self.tools.get(&tc.name)` resolves because the key is the sanitized name the model echoes.
- MCP routing — proxy uses stored `server_id`/`tool_name`.

## Data flow

```
tool.name() (possibly invalid)
   ↓  register/register_boxed  →  sanitize_tool_name + collision dedupe
registry key (valid)
   ↓  list_definitions (emits key)
ToolDefinition.name (valid)
   ↓  convert_tools (OpenAI / Anthropic)  →  request function.name (valid) ✓
   ↓  model echoes the name back
dispatch: self.tools.get(&tc.name) → hits the sanitized key ✓
   ↓  MCP proxy executes via stored server_id/tool_name (real dotted name) ✓
```

## Error handling

`sanitize_tool_name` is total (never fails). Collision dedupe terminates (the suffix counter always finds a free key). No new error paths.

## Edge: `tool.name()` ≠ registry key

For an invalid-named tool, the registry key (sanitized) differs from the tool's own `tool.name()`. This is harmless: verified that no code compares `tool.name()` against the key, and SafetyManager/approval receive the dispatched name (the sanitized key, via `tc.name`). Documented as a known, intentional divergence.

## Testing

1. **`sanitize_tool_name` unit tests:** dotted (`mcp__s__a.b` → `mcp__s__a_b`), space, `/`, `:`, CJK, empty (`""` → `"unnamed_tool"`), `>64` (truncated to 64), already-valid passthrough (`read_file` → `read_file`).
2. **Collision dedupe:** register two tools whose names sanitize to the same key (e.g. `foo.bar` and `foo_bar`) → second becomes `foo_bar_2`; both resolve via `get`; the `len()` is 2.
3. **Regression (mirrors the bug):** register a tool named `mcp__srv__foo.bar`; assert `list_definitions()` reports a name matching `^[a-zA-Z0-9_-]+$` and `get("mcp__srv__foo_bar")` resolves it.
4. `cargo test --lib agent` net green (only the 2 known pre-existing failures: `shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long`); clippy clean on `tool.rs`; `Cargo.toml` unchanged.

(Tests use a tiny `#[cfg(test)]` stub `Tool` impl with a configurable `name()`; check `tool_tests.rs` — the existing test module path — for an existing stub to reuse.)

## Scope / files

| File | Change |
|---|---|
| `agent/tools/tool.rs` | `sanitize_tool_name` (pub fn) + sanitize/dedupe in `register`/`register_boxed` (via a shared private `insert_tool`) + unit tests |

Out of scope: per-provider changes (unnecessary — the chokepoint covers both); sanitizing at the MCP name source / `prefixed_tool_name` (unnecessary — the registry chokepoint covers all sources); changing `tool.name()` itself.

## Risk

Low. Single file, single chokepoint at the registration entry. Default-preserving: the 99% of tools with already-valid names get a byte-identical key (sanitize is identity on valid names; no collision). Fixes both providers and all tool sources at once.
