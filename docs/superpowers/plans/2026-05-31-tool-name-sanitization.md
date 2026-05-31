# Tool-Name Sanitization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Stop LLM providers rejecting requests when a tool name contains characters outside `^[a-zA-Z0-9_-]+$` (the OpenAI/Anthropic constraint). Sanitize tool names at the single registration chokepoint in `ToolRegistry`, with collision-safe suffix-dedupe.

**Architecture:** One file (`agent/tools/tool.rs`): add `sanitize_tool_name()`; route `register`/`register_boxed` through a shared `insert_tool()` that sanitizes the key + suffix-dedupes collisions with a `tracing::warn`. Because `list_definitions` emits the registry KEY (not `tool.name()`) and dispatch resolves by that key, the sanitized name propagates automatically to both providers and back through dispatch. MCP execution uses the proxy's stored fields and is unaffected.

**Tech Stack:** Rust. No new deps. Spec: `docs/superpowers/specs/2026-05-31-tool-name-sanitization-design.md`.

---

## Source-of-truth references (verified)

- `agent/tools/tool.rs`:
  - `ToolRegistry { tools: HashMap<String, Box<dyn Tool>> }` (tool.rs:328).
  - `register<T: Tool + 'static>(&mut self, tool: T) { self.tools.insert(tool.name().to_string(), Box::new(tool)); }` (337).
  - `register_boxed(&mut self, tool: Box<dyn Tool>) { self.tools.insert(tool.name().to_string(), tool); }` (344).
  - `get(&self, name) -> Option<&dyn Tool>` (348); `list_definitions` emits `name: name.clone()` from the HashMap key (353); `len()` (368).
  - Test module is `#[path = "tool_tests.rs"]` (bottom of tool.rs); `tool_tests.rs` has a stub `EchoTool` (hardcoded name) at line 88 — NOT configurable, so the new tests add a `NamedStub`.
- `Tool` trait (tool.rs:219): `fn name(&self) -> &str`; `async fn execute(&self, params) -> Result<ToolOutput, ToolError>` (+ other methods with defaults). The `NamedStub` only needs `name` + `execute` (+ `description`/`parameters_schema` if they lack defaults — check the trait; EchoTool implements `name`, `description`, `parameters_schema`, `execute`, so mirror EchoTool).
- Consumers that need no change (verified): `OpenAiProvider::convert_tools` (openai.rs:231) + `AnthropicProvider::convert_tools` (anthropic.rs:186) both read `ToolDefinition.name` (= sanitized key); dispatch `self.tools.get(&tc.name)` (tool_dispatch/mod.rs:335).

---

## CRITICAL facts

1. **Single chokepoint = the registry key.** Sanitize where the key is set (`register`/`register_boxed`). Do NOT touch the providers or dispatch — they inherit the sanitized key automatically.
2. **Sanitize is identity on already-valid names** — the 99% of tools (`read_file`, `mcp__gitnexus__api_impact`, etc.) get byte-identical keys, zero behavior change.
3. **Collision dedupe must terminate + warn** — after sanitize, two distinct names can collide; append `_2`, `_3`, … until free, and `tracing::warn`. (Today `insert` silently overwrites — this is a strict improvement.)
4. **MCP unaffected** — proxy executes via stored `server_id`/`tool_name`; `parse_mcp_tool_name` is not on the dispatch path.
5. **Pre-commit hooks** — no `--no-verify`.

---

## File Structure

| File | Change | LoC |
|---|---|---|
| `agent/tools/tool.rs` | `sanitize_tool_name` + private `insert_tool` (sanitize+dedupe) + `register`/`register_boxed` delegate to it | ~+30 |
| `agent/tools/tool_tests.rs` | `NamedStub` + sanitize unit tests + collision + regression tests | ~+90 |

---

## Tasks

### Task 1: `sanitize_tool_name` + unit tests

**Files:** `src-tauri/src/agent/tools/tool.rs`, `src-tauri/src/agent/tools/tool_tests.rs`.

- [ ] **Step 1: Write failing tests** in `tool_tests.rs` (append; `use super::*;` is already in scope for the path-included module — confirm and add `use crate::agent::tools::tool::sanitize_tool_name;` if needed):
```rust
#[test]
fn sanitize_passthrough_valid() {
    assert_eq!(sanitize_tool_name("read_file"), "read_file");
    assert_eq!(sanitize_tool_name("mcp__gitnexus__api_impact"), "mcp__gitnexus__api_impact");
    assert_eq!(sanitize_tool_name("a-b_c9"), "a-b_c9");
}
#[test]
fn sanitize_replaces_invalid_chars() {
    assert_eq!(sanitize_tool_name("mcp__srv__foo.bar"), "mcp__srv__foo_bar");
    assert_eq!(sanitize_tool_name("a b/c:d"), "a_b_c_d");
    assert_eq!(sanitize_tool_name("工具名"), "___"); // 3 CJK chars → 3 underscores
}
#[test]
fn sanitize_empty_falls_back() {
    assert_eq!(sanitize_tool_name(""), "unnamed_tool");
}
#[test]
fn sanitize_truncates_to_64() {
    let long = "a".repeat(100);
    assert_eq!(sanitize_tool_name(&long).len(), 64);
}
#[test]
fn sanitize_result_always_matches_pattern() {
    for raw in ["read_file", "mcp__s__a.b", "工具", "", "x/y z:1", &"q".repeat(80)] {
        let s = sanitize_tool_name(raw);
        assert!(!s.is_empty() && s.len() <= 64);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "sanitized {raw:?} → {s:?} still has invalid chars");
    }
}
```

- [ ] **Step 2: Run → red.** `cd src-tauri && cargo test --lib agent::tools::tool 2>&1 | tail` (fails: `sanitize_tool_name` undefined).

- [ ] **Step 3: Implement** in `tool.rs` (above `impl ToolRegistry`, as a free `pub fn`):
```rust
/// Regularize an arbitrary tool name to the `^[a-zA-Z0-9_-]+$` shape that both
/// OpenAI and Anthropic require for `function.name`. Invalid chars → '_';
/// empty → "unnamed_tool"; truncated to 64 (Anthropic's upper bound).
pub fn sanitize_tool_name(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if s.is_empty() {
        s = "unnamed_tool".to_string();
    }
    if s.len() > 64 {
        s.truncate(64); // all chars are ASCII after the map, so 64 is a char boundary
    }
    s
}
```

- [ ] **Step 4: Run → green.** `cd src-tauri && cargo test --lib agent::tools::tool 2>&1 | tail`.

- [ ] **Step 5: Commit.**
```bash
git add src-tauri/src/agent/tools/tool.rs src-tauri/src/agent/tools/tool_tests.rs
git commit -m "feat(agent): sanitize_tool_name — regularize tool names to provider pattern (tns.1)"
```

### Task 2: sanitize + collision-dedupe at registration

**Files:** `src-tauri/src/agent/tools/tool.rs`, `src-tauri/src/agent/tools/tool_tests.rs`.

- [ ] **Step 1: Write failing tests** in `tool_tests.rs`. First add a configurable-name stub (mirror `EchoTool`'s trait methods — copy its `description`/`parameters_schema`/`execute` bodies, just make `name` return the stored string):
```rust
struct NamedStub(String);
#[async_trait::async_trait]
impl Tool for NamedStub {
    fn name(&self) -> &str { &self.0 }
    fn description(&self) -> &str { "stub" }
    fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({"type":"object"}) }
    async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::success("ok", 0))
    }
}
```
(Check `EchoTool`'s exact trait impl + imports — match the `#[async_trait]` attribute style and `ToolOutput::success` signature it uses. If `Tool` has more required methods than these four, copy them from EchoTool.)
Then the behavior tests:
```rust
#[test]
fn register_sanitizes_invalid_name_into_key() {
    let mut reg = ToolRegistry::new();
    reg.register(NamedStub("mcp__srv__foo.bar".into()));
    // exposed via list_definitions (what providers send) is valid + is the key
    let defs = reg.list_definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "mcp__srv__foo_bar");
    assert!(defs[0].name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    // dispatch resolves by the sanitized key
    assert!(reg.get("mcp__srv__foo_bar").is_some());
}
#[test]
fn register_collision_suffix_dedupes() {
    let mut reg = ToolRegistry::new();
    reg.register(NamedStub("foo.bar".into())); // → foo_bar
    reg.register(NamedStub("foo_bar".into())); // collides → foo_bar_2
    assert_eq!(reg.len(), 2);
    assert!(reg.get("foo_bar").is_some());
    assert!(reg.get("foo_bar_2").is_some());
}
#[test]
fn register_boxed_also_sanitizes() {
    let mut reg = ToolRegistry::new();
    reg.register_boxed(Box::new(NamedStub("a:b".into())));
    assert!(reg.get("a_b").is_some());
}
```

- [ ] **Step 2: Run → red.** `cd src-tauri && cargo test --lib agent::tools::tool 2>&1 | tail` (register still inserts raw `tool.name()`, so `get("mcp__srv__foo_bar")` is None).

- [ ] **Step 3: Implement** — replace `register` + `register_boxed` bodies with a shared private helper:
```rust
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.insert_tool(Box::new(tool));
    }

    /// Register a pre-boxed `Tool` instance. Used by `AgentApi.build_session_registry`
    /// where descriptor builders return `Box<dyn Tool>` (the concrete type is
    /// erased at registration time).
    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        self.insert_tool(tool);
    }

    /// Insert a tool under a provider-safe, collision-free key. The key is
    /// `sanitize_tool_name(tool.name())`; on collision (exact dup OR post-sanitize
    /// clash) a numeric suffix is appended. This key is what `list_definitions`
    /// exposes to providers and what `dispatch` resolves against, so the model
    /// only ever sees + echoes valid names.
    fn insert_tool(&mut self, tool: Box<dyn Tool>) {
        let mut key = sanitize_tool_name(tool.name());
        if self.tools.contains_key(&key) {
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
        self.tools.insert(key, tool);
    }
```
(Confirm `tracing` is already imported/available in `tool.rs`; the crate uses `tracing` widely. If not in scope, use `tracing::warn!` fully-qualified as written — no `use` needed.)

- [ ] **Step 4: Run → green.** `cd src-tauri && cargo test --lib agent::tools::tool 2>&1 | tail` (all new + existing pass).

- [ ] **Step 5: Commit.**
```bash
git add src-tauri/src/agent/tools/tool.rs src-tauri/src/agent/tools/tool_tests.rs
git commit -m "feat(agent): sanitize + collision-dedupe tool names at registry chokepoint (tns.2)"
```

### Task 3: Verification

- [ ] `cd src-tauri && cargo test --lib agent::tools::tool 2>&1 | tail` — sanitize + register + regression tests pass; existing `EchoTool` tests still pass.
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo test --lib agent 2>&1 | tail -6` — net green; only the 2 known pre-existing failures (`shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long`).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "tools/tool\.rs|tool_tests" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **No-regression:** a valid name (`read_file`) → identical key (sanitize is identity); `list_definitions` for a normal registry is unchanged in names.
- [ ] **Bug fixed:** the regression test (`register_sanitizes_invalid_name_into_key`) proves an invalid name now surfaces to providers as a pattern-valid name resolvable by dispatch.

---

## Self-Review

- ✅ **Spec coverage:** `sanitize_tool_name` (Task 1); registry chokepoint + collision dedupe + warn (Task 2); regression mirroring the bug + no-regression + both-provider coverage-by-construction (Task 3). Out-of-scope items (provider changes, MCP-source sanitize) explicitly not touched.
- ✅ **Placeholder scan:** full code in every step; the only verify-and-match instructions (NamedStub mirroring EchoTool; tracing import) have concrete fallbacks.
- ✅ **Type consistency:** `sanitize_tool_name(&str) -> String`; `insert_tool(&mut self, Box<dyn Tool>)`; `register`/`register_boxed` delegate to it; registry key type `String` unchanged.
- ✅ **Risk-scaled:** single file, single chokepoint, identity on valid names; strict improvement over today's silent-overwrite. The bug is provider-agnostic and this fixes both via the shared `list_definitions` key.
- Decisions: chokepoint = registry key (not providers, not MCP source); collision = suffix-dedupe + warn; truncate 64; empty → "unnamed_tool".
