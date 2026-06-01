# P2a-2 LLM `mcp__gbrain__put_page` Write Intercept Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the LLM calls the `mcp__gbrain__put_page` tool and gbrain accepts it, also best-effort shadow-write the page into the adapter `"pages"` namespace (gated, reusing P2a-1) — covering the dominant write path so P2c (read repoint) is unblocked.

**Architecture:** Intercept at the single dispatch chokepoint `McpToolProxy::execute` (mcp.rs). The `(gbrain, put_page)` proxy gets an armed `dual_write_pages: Option<Arc<dyn MemoryAdapter>>` field (`Some` only when the flag is on); after a successful gbrain call, `page_dual_write::shadow_write_page` runs. gbrain stays primary (its result is returned unchanged); every other MCP tool's `execute` is byte-identical (field `None`).

**Tech Stack:** Rust, Tauri, the `MemoryAdapter` trait + `memory_adapter::page_dual_write` (P2a-1), `serde_json`, the MCP `McpToolProxy` / `create_tool_proxies` machinery.

---

## Recon findings (complete — ground truth)

- Chokepoint `McpToolProxy::execute(&self, params: serde_json::Value)` (mcp.rs:1830). `params` is the LLM's arguments object `{slug, content}`. It is **moved** into `JsonRpcRequest::call_tool(req_id, &self.tool_name, params)` (~mcp.rs:1845), so args must be captured **before** that line. The genuine-success arm is `Ok(call_result)` with `!call_result.is_error` → `Ok(ToolOutput::success(&text, duration_ms))` (~mcp.rs:1893).
- `McpToolProxy` struct (mcp.rs:1771) fields: `server_id, tool_name, prefixed_name, description, input_schema, manager, auto_approve`. Second constructor is **`McpToolProxy::for_plugin(plugin_id, tool_name, mcp_manager)`** (mcp.rs:1925) — builds the struct literal with those 7 fields.
- **`create_tool_proxies(manager: &SharedMcpManager, locked: &McpManager) -> Vec<McpToolProxy>`** (mcp.rs:2654); the struct literal is at mcp.rs:2682. **Callers (4):**
  - `src-tauri/src/agent/tools/registry_build.rs:~222` — the agent-loop registry (executes). **Arm.**
  - `src-tauri/src/tauri_commands.rs:15001` — the **agent-teams run** registry ("Registered MCP tools for agent_teams run"; delegates execute these). **Arm.**
  - `src-tauri/src/mcp.rs:3480` and `mcp.rs:~3445` test bodies — pass `(None, false)`.
- `registry_build.rs` already binds `gbrain_dual_write_enabled` at **fn scope** (line 39, from `cfg.memory_os.gbrain_dual_write_pages_enabled`) — reuse it. The `bucket_seal_adapter` local at line 90 is **block-scoped** (browser-tools block) → the MCP block (~222) makes its own.
- `page_dual_write::shadow_write_page(adapter: &Arc<dyn MemoryAdapter>, slug: &str, markdown: &str)` (P2a-1, `pub(crate)`) is reachable from mcp.rs (same crate) — `crate::memory_adapter::page_dual_write::shadow_write_page`.
- `state.bucket_seal_adapter` is `Arc<BucketSealAdapter>`; coerce via `state.bucket_seal_adapter.clone()` assigned to an explicitly-typed `Arc<dyn MemoryAdapter>` (P2a-1 finding: `Arc::clone(&x)` does NOT coerce).

## Worktree setup

Create the worktree under `/Users/ryanliu/Documents/uclaw-worktrees/` on branch `claude/p2a-2-llm-tool-write-intercept` off `origin/main` (using-git-worktrees skill). A fresh worktree fails `cargo build` until the **gitignored build-time resource placeholders** exist — recreate them (the resources resolve relative to `src-tauri/`):
```bash
WT=/Users/ryanliu/Documents/uclaw-worktrees/p2a-2-llm-tool-write-intercept
mkdir -p "$WT/src-tauri/bunembed" "$WT/src-tauri/pyembed" "$WT/src-tauri/gbrain-source"
touch "$WT/src-tauri/bunembed/bun" "$WT/src-tauri/pyembed/python"
echo x > "$WT/src-tauri/gbrain-source/placeholder.txt"
```
(`resources/builtin-automations`, `resources/live-room`, `resources/memory_schema.json` are tracked — already present.) Baseline: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` clean before Task 1.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/mcp.rs` | the interception mechanism: `dual_write_pages` field, `parse_put_page_args`, execute capture+shadow, `create_tool_proxies` signature + arming, `for_plugin` default, test-caller updates |
| `src-tauri/src/agent/tools/registry_build.rs` | arm the agent-loop registry's proxies |
| `src-tauri/src/tauri_commands.rs` | arm the agent-teams run's proxies (line ~15001) |

---

### Task 1: Interception mechanism in mcp.rs (inert — field always `None`)

Adds the field + helper + execute logic, but does NOT change `create_tool_proxies`'s signature yet — the literal and `for_plugin` set `dual_write_pages: None`, so the mechanism is wired and compiles but never fires. (Splitting this way keeps both commits compiling; Task 2 arms it.)

**Files:**
- Modify: `src-tauri/src/mcp.rs` (struct ~1771, `execute` ~1830, `for_plugin` ~1925, `create_tool_proxies` literal ~2682; tests in the `#[cfg(test)] mod` at end)

- [ ] **Step 1: Add the `dual_write_pages` field to the struct**

In `pub struct McpToolProxy { … }` (after `auto_approve: bool,`):

```rust
    /// P2a-2 — `Some(adapter)` ONLY for the (gbrain, put_page) proxy when the
    /// dual-write flag is on; `None` for every other proxy. `None` ⇒ no dual-write.
    /// Snapshotted at construction (proxies rebuild each turn → fresh flag), like
    /// `auto_approve`. See docs/superpowers/specs/2026-06-01-p2a-2-llm-tool-write-intercept-design.md
    dual_write_pages: Option<std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>>,
```

- [ ] **Step 2: Default it to `None` in both construction sites**

In `for_plugin` (mcp.rs:~1932, the `Self { … }` literal), add `dual_write_pages: None,`.
In the `create_tool_proxies` `.map` literal (mcp.rs:~2682, the `McpToolProxy { … }`), add `dual_write_pages: None,` (Task 2 replaces this with the arming `if`).

- [ ] **Step 3: Add the `parse_put_page_args` free fn + tests**

Add as a free fn near `prefixed_tool_name`/`parse_mcp_tool_name` (mcp.rs:~1750):

```rust
/// P2a-2 — extract (slug, content) from a gbrain `put_page` arguments object.
/// Returns `None` if either field is absent or not a string.
fn parse_put_page_args(params: &serde_json::Value) -> Option<(String, String)> {
    let slug = params.get("slug")?.as_str()?.to_string();
    let content = params.get("content")?.as_str()?.to_string();
    Some((slug, content))
}
```

Add to the `#[cfg(test)] mod tests` block at the end of mcp.rs:

```rust
#[test]
fn parse_put_page_args_valid() {
    let v = serde_json::json!({"slug": "a/b", "content": "# hi"});
    assert_eq!(parse_put_page_args(&v), Some(("a/b".to_string(), "# hi".to_string())));
}

#[test]
fn parse_put_page_args_missing_slug_is_none() {
    let v = serde_json::json!({"content": "x"});
    assert_eq!(parse_put_page_args(&v), None);
}

#[test]
fn parse_put_page_args_missing_content_is_none() {
    let v = serde_json::json!({"slug": "a"});
    assert_eq!(parse_put_page_args(&v), None);
}

#[test]
fn parse_put_page_args_non_string_is_none() {
    let v = serde_json::json!({"slug": 5, "content": {"x": 1}});
    assert_eq!(parse_put_page_args(&v), None);
}

#[test]
fn parse_put_page_args_empty_is_none() {
    let v = serde_json::json!({});
    assert_eq!(parse_put_page_args(&v), None);
}
```

- [ ] **Step 4: Capture args before `params` is moved, in `execute`**

At the very top of `execute` (mcp.rs:~1834, before `let start = …` or right after — anywhere before the `JsonRpcRequest::call_tool(req_id, &self.tool_name, params)` line that moves `params`):

```rust
        // P2a-2 — capture dual-write inputs before `params` is moved into the request.
        let dual = self
            .dual_write_pages
            .as_ref()
            .and_then(|a| parse_put_page_args(&params).map(|(s, c)| (a.clone(), s, c)));
```

- [ ] **Step 5: Shadow-write in the success branch**

In the genuine-success arm — the `else` of `if call_result.is_error { … }` that returns `Ok(ToolOutput::success(&text, duration_ms))` (mcp.rs:~1888) — insert, immediately before that `Ok(... success ...)` return:

```rust
                    if let Some((adapter, slug, content)) = dual {
                        crate::memory_adapter::page_dual_write::shadow_write_page(&adapter, &slug, &content).await;
                    }
```

(Place it inside the success `else` block AFTER the `set_error_for_state(state, None)` write and before constructing the `ToolOutput::success`. Because `dual` is `Some` only for the armed gbrain put_page proxy — which after Task 1 never happens since the field is always `None` — this is inert until Task 2. The borrow checker is fine: `dual` is moved here, used once.)

- [ ] **Step 6: Build + test**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/p2a-2-llm-tool-write-intercept/src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
Run: `cargo test --lib mcp::tests::parse_put_page_args 2>&1 | tail -10` → 5 passed.
Run: `cargo test --lib mcp 2>&1 | grep "test result" | tail -1` → existing mcp tests still pass.

- [ ] **Step 7: Commit**

EXPLICIT path only (never `git add -A`/`.` — gitignored build placeholders must not be committed). No `--no-verify`.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/p2a-2-llm-tool-write-intercept
git add src-tauri/src/mcp.rs
git commit -m "feat(mcp): dual_write_pages intercept mechanism in McpToolProxy::execute (inert) (P2a-2)"
```

---

### Task 2: Arm the gbrain put_page proxy + wire the two execution paths

Changes `create_tool_proxies`'s signature, arms the literal, and updates all 4 callers. This is the commit that turns the mechanism on.

**Files:**
- Modify: `src-tauri/src/mcp.rs` (`create_tool_proxies` signature ~2654 + arming literal ~2682; 2 test callers ~3480, ~3445)
- Modify: `src-tauri/src/agent/tools/registry_build.rs` (~222)
- Modify: `src-tauri/src/tauri_commands.rs` (~15001)

- [ ] **Step 1: Change `create_tool_proxies` signature + arm the literal**

Signature (mcp.rs:2654):

```rust
    pub fn create_tool_proxies(
        manager: &SharedMcpManager,
        locked: &McpManager,
        dual_write_adapter: Option<std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>>,
        dual_write_enabled: bool,
    ) -> Vec<McpToolProxy> {
```

In the `.map` literal (mcp.rs:~2682), replace `dual_write_pages: None,` with:

```rust
                    dual_write_pages: if dual_write_enabled
                        && tool.server_id == "gbrain"
                        && tool.name == "put_page"
                    {
                        dual_write_adapter.clone()
                    } else {
                        None
                    },
```

- [ ] **Step 2: Update the two test callers in mcp.rs**

At mcp.rs:~3480 and ~3445 (the `create_tool_proxies(&shared, &*locked)` / `(&shared, &*locked)` test calls), add the two args:

```rust
            McpManager::create_tool_proxies(&shared, &*locked, None, false)
```

(Find each call via `grep -n "create_tool_proxies(" src-tauri/src/mcp.rs`; update every test-body call to pass `None, false`.)

- [ ] **Step 3: Arm the agent-loop registry (registry_build.rs)**

`registry_build.rs` already binds `gbrain_dual_write_enabled` at fn scope (line 39). At the MCP block (~222), the call is:

```rust
        let mgr = state.mcp_manager.read().await;
        let proxies = crate::mcp::McpManager::create_tool_proxies(
            &state.mcp_manager,
            &*mgr,
        );
```

Replace with (make a fn-local adapter handle — the line-90 one is block-scoped):

```rust
        let mgr = state.mcp_manager.read().await;
        let dual_adapter: Option<std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>> =
            Some(Arc::clone(&state.bucket_seal_adapter) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>);
        let proxies = crate::mcp::McpManager::create_tool_proxies(
            &state.mcp_manager,
            &*mgr,
            dual_adapter,
            gbrain_dual_write_enabled,
        );
```

(`Arc` is already imported in registry_build.rs — it's used at line 91. If the `as` cast form is rejected, use the `let`-with-explicit-type-then-`.clone()` form from P2a-1. `gbrain_dual_write_enabled` is the fn-scope binding from line 39.)

- [ ] **Step 4: Arm the agent-teams registry (tauri_commands.rs ~15001)**

The block:

```rust
    let (mcp_proxies_for_factory, gbrain_knowledge_for_factory) = {
        let mgr = state.mcp_manager.read().await;
        let proxies =
            crate::mcp::McpManager::create_tool_proxies(&state.mcp_manager, &*mgr);
        let block = crate::agent::gbrain_prompt::GbrainKnowledgeSection::render(&*mgr)
            .unwrap_or_default();
        (proxies, block)
    };
```

Replace with:

```rust
    let (mcp_proxies_for_factory, gbrain_knowledge_for_factory) = {
        let mgr = state.mcp_manager.read().await;
        let dual_enabled = state
            .memubot_config
            .read()
            .await
            .memory_os
            .gbrain_dual_write_pages_enabled;
        let dual_adapter: Option<std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>> =
            Some(state.bucket_seal_adapter.clone());
        let proxies = crate::mcp::McpManager::create_tool_proxies(
            &state.mcp_manager,
            &*mgr,
            dual_adapter,
            dual_enabled,
        );
        let block = crate::agent::gbrain_prompt::GbrainKnowledgeSection::render(&*mgr)
            .unwrap_or_default();
        (proxies, block)
    };
```

> Confirm `state.memubot_config` read idiom + that `state.bucket_seal_adapter.clone()` coerces to the annotated `Option<Arc<dyn MemoryAdapter>>` (it does via unsized coercion on the typed `let`). Hold no `memubot_config` read-guard across an `.await` — the `.read().await.memory_os.<flag>` reads into the `dual_enabled` bool and the guard drops at the `;`. Ensure the read order doesn't deadlock with the held `mgr` (mcp_manager) read-guard — `memubot_config` is a different lock, so a sequential read is fine, but do the `memubot_config` read either before acquiring `mgr` or as shown (after) — both are distinct locks; if clippy/borrow flags it, read `dual_enabled` + `dual_adapter` before `let mgr = …`.

- [ ] **Step 5: Build + test**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/p2a-2-llm-tool-write-intercept/src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
Run: `cargo test --lib mcp 2>&1 | grep "test result" | tail -1` → pass (the test callers compile with the new args).
Run: `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/p2a-2-llm-tool-write-intercept
git add src-tauri/src/mcp.rs src-tauri/src/agent/tools/registry_build.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(mcp): arm gbrain put_page dual-write + wire agent-loop & teams registries (P2a-2)"
```

---

### Task 3: Whole-slice verification

**Files:** none (verification only)

- [ ] **Step 1: Full build** — `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] **Step 2: Tests** — `cargo test --lib mcp 2>&1 | grep "test result" | tail` (incl. the 5 parse tests) → green; `cargo test --lib page_dual_write 2>&1 | tail -4` → still green.
- [ ] **Step 3: clippy** — `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] **Step 4: Confirm arming is correct** — `grep -n "create_tool_proxies(" src-tauri/src/` shows: registry_build + tauri_commands pass the real adapter + flag; the 2 mcp.rs test calls pass `None, false`. `grep -n "dual_write_pages" src-tauri/src/mcp.rs` shows the field, the `for_plugin` `None`, and the arming `if`.
- [ ] **Step 5: GitNexus** — per CLAUDE.md, `gitnexus_detect_changes()` before the PR (expected symbols: `parse_put_page_args`, `McpToolProxy`, `create_tool_proxies`, `McpToolProxy::for_plugin`, the 2 registry call sites).

## Adjacent-edit checklist (call out in PR body)

- **`create_tool_proxies` signature changed** → all 4 callers updated (registry_build armed, tauri_commands armed, 2 mcp.rs tests `None,false`). Confirm none missed: `grep -rn "create_tool_proxies(" src-tauri/src/`.
- **New `McpToolProxy` field** → both constructors (`create_tool_proxies` literal + `for_plugin`) set it.
- No migration, no new Tauri command, no config change (reuses P2a-1's flag).

## PR shape

One branch `claude/p2a-2-llm-tool-write-intercept`, one PR with a `## Commits (bisectable)` table (Tasks 1–2 = 2 commits). Title: `feat(memory): P2a-2 — LLM mcp__gbrain__put_page write intercept (gated dual-write)`. Body: gbrain primary; only the gbrain put_page proxy armed; covers the dominant LLM write path; **unblocks P2c (must precede it)**; known minor raw-slug-vs-canonical-slug divergence (best-effort, heals on re-sync).

## Self-review notes

- **Spec coverage:** §1 mechanism → Task 1; §2 wiring → Task 2 (incl. the spec-undercounted tauri_commands teams caller — armed); §3 testing → Task 1 parse tests + Task 3; error handling (success-only, best-effort, None-safe) → Task 1 Steps 4–5. ✔
- **Type consistency:** field type `Option<Arc<dyn MemoryAdapter>>` identical in struct/`for_plugin`/`create_tool_proxies` param/registry locals; `parse_put_page_args(&Value) -> Option<(String,String)>` matches the `dual` capture + the `shadow_write_page(&Arc, &str, &str)` call. ✔
- **Bisectability:** Task 1 compiles (field always `None`, mechanism inert); Task 2 compiles (signature + all 4 callers updated together). ✔
- **Follow-the-recon items** (flagged, not placeholders): the `as`-cast vs typed-`let` Arc coercion (Task 2 Steps 3–4, both forms given); lock-ordering for the tauri_commands config read (Task 2 Step 4 note). Each has a concrete primary + fallback.
