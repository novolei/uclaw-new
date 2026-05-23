# PR8 Tool Family Mesh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Translate high-value jcode tool families into uClaw Capability Mesh cards without changing tool execution behavior.

**Architecture:** PR8 adds a small registry-layer metadata module for jcode-inspired tool families, then annotates existing active builtin tool registry entries with stable family tags. The resolver can answer `CapabilityQuery` requests for active search/read/write/patch/shell families; background and session-search stay catalog-only planned cards until later runtime work lands.

**Tech Stack:** Rust, `serde`, existing `registries::{RegistryHub, ToolEntry, resolve}`, existing `runtime::contracts::CapabilityQuery`, sibling `*_tests.rs`.

---

## Scope Anchors

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh`
- Branch: `codex/agent-os-jcode-pr8-tool-family-mesh`
- Source docs:
  - `/Users/ryanliu/Documents/uclaw/docs/jcode_comparison/README.md`
  - `/Users/ryanliu/Documents/uclaw/docs/jcode_comparison/04_backend_reconstruction_blueprint.md`
  - `/Users/ryanliu/Documents/uclaw/docs/jcode_comparison/06_adr_gap_audit_and_reference_addenda.md`
  - `/Users/ryanliu/Documents/uclaw/docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`
- jcode reference:
  - `/Users/ryanliu/Documents/jcode/src/tool/mod.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/grep.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/glob.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/read.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/write.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/edit.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/patch.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/bash.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/bg.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/session_search.rs`

## ADR Section 18 Answers

| Question | PR8 Answer |
|---|---|
| 1. What user intent does this support? | It helps planner/runtime select the right tool family for local file work, shell work, background-capable work, and prior-session retrieval without exposing implementation internals. |
| 2. What autonomy level can it run at? | Metadata only; it supports L0-L5 planning but executes no autonomous action. Existing tool policies still cap actual execution. |
| 3. What is the source of truth? | Existing `RegistryHub.tools` remains the queryable mesh surface. The new family catalog is static code metadata, not a second runtime registry. |
| 4. Which TaskEvent does it emit? | None in PR8. Future dispatch PRs may map selected cards to `ToolCall`, `ToolResult`, `BoundaryYield`, or `Checkpoint`. |
| 5. What context does it read? | No runtime context. It only maps existing builtin tool ids and jcode reference families. |
| 6. What capability does it require? | Registry read/query capability only. No file, shell, browser, memory, or network access at runtime. |
| 7. Which policy hooks can block it? | None for metadata lookup. Tool execution continues through existing approval/path/safety hooks. |
| 8. What world projection does the UI render? | None in PR8. The output becomes future projection input once tool selection emits events. |
| 9. What harness cases prove it works? | Model-free registry tests: cards are stable, mappings are complete, permissions are conservative, resolver queries find expected tool families. |
| 10. What is the rollback path? | Remove `tool_families.rs`, its exports, and the small hub tag bridge; dispatch behavior remains unchanged. |
| 11. What does this not own? | No tool behavior rewrite, no `Tool::execute` signature change, no background process registry, no session-search implementation, no migrations, no UI. |

## Allowed Files

- Create: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/registries/tool_families.rs`
- Create: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/registries/tool_families_tests.rs`
- Modify: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/registries/mod.rs`
- Modify: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/registries/hub.rs`
- Modify: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

## Explicit Non-Goals

- Do not modify `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/agent/tools/tool.rs`.
- Do not modify `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/agent/dispatcher.rs`.
- Do not modify `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/tauri_commands.rs`.
- Do not modify `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/agent/agentic_loop.rs`.
- Do not modify `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh/src-tauri/src/db/migrations.rs`.
- Do not add a new runtime store or write to `memory_graph`.

## Impact Notes

- `ToolEntry`: LOW, 2 direct callers, 1 affected process (`main`).
- `register_builtin_tools`: LOW, 4 direct callers, 1 affected process (`main`).
- `builtin_tool_catalog`: LOW, 1 direct caller via `register_builtin_tools`.
- `Tool`: HIGH in prior PRs and explorer report; PR8 avoids it.
- `resolve`: not modified; tests exercise existing resolver behavior.

## Task 1: Add Tool Family Card Catalog

**Files:**
- Create: `src-tauri/src/registries/tool_families.rs`
- Create: `src-tauri/src/registries/tool_families_tests.rs`
- Modify: `src-tauri/src/registries/mod.rs`

- [x] **Step 1: Write sibling tests for the static catalog**

Add `src-tauri/src/registries/tool_families_tests.rs`:

```rust
use super::*;

#[test]
fn jcode_inspired_catalog_contains_required_families() {
    let ids: Vec<&str> = jcode_inspired_tool_family_cards()
        .iter()
        .map(|card| card.family_id)
        .collect();

    assert_eq!(
        ids,
        vec![
            "filesystem.search",
            "filesystem.read",
            "filesystem.write",
            "filesystem.patch",
            "shell.command",
            "runtime.background",
            "context.session_search",
        ]
    );
}

#[test]
fn write_patch_shell_and_background_are_permissioned() {
    for family_id in [
        "filesystem.write",
        "filesystem.patch",
        "shell.command",
        "runtime.background",
    ] {
        let card = tool_family_card(family_id).expect("card exists");
        assert!(card.requires_permission, "{family_id} must stay gated");
        assert!(
            card.policy_tags.contains(&"permission.required"),
            "{family_id} should advertise permission.required"
        );
    }
}

#[test]
fn cards_map_to_existing_or_future_tool_ids() {
    let search = tool_family_card("filesystem.search").unwrap();
    assert_eq!(search.tool_ids, &["search"]);
    assert!(search.capability_tags.contains(&"search"));
    assert!(search.capability_tags.contains(&"filesystem"));

    let background = tool_family_card("runtime.background").unwrap();
    assert!(background.tool_ids.is_empty());
    assert!(background.capability_tags.contains(&"background"));
    assert!(background.event_profile.contains(&"checkpoint"));
    assert_eq!(background.execution_status, "planned");

    let session = tool_family_card("context.session_search").unwrap();
    assert!(session.tool_ids.is_empty());
    assert!(session.capability_tags.contains(&"session_search"));
    assert_eq!(session.execution_status, "planned");
}
```

- [x] **Step 2: Run the test and verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib registries::tool_families
```

Expected: compile failure because `tool_families` is not defined.

- [x] **Step 3: Add the minimal catalog implementation**

Create `src-tauri/src/registries/tool_families.rs`:

```rust
//! jcode-inspired tool family metadata for the Capability Mesh.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolFamilyCard {
    pub family_id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub source_reference: &'static str,
    pub tool_ids: &'static [&'static str],
    pub capability_tags: &'static [&'static str],
    pub policy_tags: &'static [&'static str],
    pub event_profile: &'static [&'static str],
    pub harness_subject: &'static str,
    pub cost_tier: &'static str,
    pub reliability_tier: &'static str,
    pub execution_status: &'static str,
    pub requires_permission: bool,
}

pub const JCODE_INSPIRED_TOOL_FAMILY_CARDS: &[ToolFamilyCard] = &[
    ToolFamilyCard {
        family_id: "filesystem.search",
        title: "Filesystem Search",
        summary: "Search file names and file contents with deterministic, capped output.",
        source_reference: "jcode/src/tool/{grep.rs,glob.rs,agentgrep.rs}",
        tool_ids: &["search"],
        capability_tags: &["filesystem", "search", "grep", "glob", "context"],
        policy_tags: &["read_only", "path_policy"],
        event_profile: &["tool_call", "tool_result"],
        harness_subject: "tools.search",
        cost_tier: "local_low",
        reliability_tier: "stable",
        execution_status: "active",
        requires_permission: false,
    },
    ToolFamilyCard {
        family_id: "filesystem.read",
        title: "Filesystem Read",
        summary: "Read bounded workspace files and surface previewable artifacts.",
        source_reference: "jcode/src/tool/read.rs",
        tool_ids: &["file"],
        capability_tags: &["filesystem", "read", "artifact"],
        policy_tags: &["read_only", "path_policy"],
        event_profile: &["tool_call", "tool_result", "artifact_ref"],
        harness_subject: "tools.read",
        cost_tier: "local_low",
        reliability_tier: "stable",
        execution_status: "active",
        requires_permission: false,
    },
    ToolFamilyCard {
        family_id: "filesystem.write",
        title: "Filesystem Write",
        summary: "Create or overwrite workspace files behind preview and path policy.",
        source_reference: "jcode/src/tool/write.rs",
        tool_ids: &["file"],
        capability_tags: &["filesystem", "write", "artifact"],
        policy_tags: &["permission.required", "path_policy", "preview"],
        event_profile: &["permission_requested", "tool_call", "tool_result", "artifact_ref"],
        harness_subject: "tools.write",
        cost_tier: "local_low",
        reliability_tier: "guarded",
        execution_status: "active",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "filesystem.patch",
        title: "Filesystem Patch",
        summary: "Apply surgical edits and patch-like file changes with preview support.",
        source_reference: "jcode/src/tool/{edit.rs,patch.rs,apply_patch.rs,multiedit.rs}",
        tool_ids: &["edit"],
        capability_tags: &["filesystem", "patch", "edit", "diff"],
        policy_tags: &["permission.required", "path_policy", "preview"],
        event_profile: &["permission_requested", "tool_call", "tool_result", "artifact_ref"],
        harness_subject: "tools.patch",
        cost_tier: "local_low",
        reliability_tier: "guarded",
        execution_status: "active",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "shell.command",
        title: "Shell Command",
        summary: "Run bounded shell commands in the workspace sandbox.",
        source_reference: "jcode/src/tool/bash.rs",
        tool_ids: &["shell"],
        capability_tags: &["filesystem", "shell", "process"],
        policy_tags: &["permission.required", "path_policy", "sandbox"],
        event_profile: &["permission_requested", "tool_call", "tool_result"],
        harness_subject: "tools.shell",
        cost_tier: "local_medium",
        reliability_tier: "guarded",
        execution_status: "active",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "runtime.background",
        title: "Background Work",
        summary: "Expose background-capable shell/process semantics for future progress and checkpoint protocol.",
        source_reference: "jcode/src/tool/{bash.rs,bg.rs,batch.rs}",
        tool_ids: &[],
        capability_tags: &["runtime", "background", "progress", "checkpoint"],
        policy_tags: &["permission.required", "human_boundary", "cancelable"],
        event_profile: &["boundary_yield", "checkpoint", "tool_result"],
        harness_subject: "tools.background",
        cost_tier: "local_medium",
        reliability_tier: "planned",
        execution_status: "planned",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "context.session_search",
        title: "Session Search",
        summary: "Future Context Fabric access to prior task/session traces without adding a second memory store.",
        source_reference: "jcode/src/tool/{session_search.rs,conversation_search.rs}",
        tool_ids: &[],
        capability_tags: &["context", "session_search", "conversation_search", "trace"],
        policy_tags: &["read_only", "gbrain_primary", "no_memory_graph_write"],
        event_profile: &["context_access", "memory_recall"],
        harness_subject: "tools.session_search",
        cost_tier: "local_low",
        reliability_tier: "planned",
        execution_status: "planned",
        requires_permission: false,
    },
];

pub fn jcode_inspired_tool_family_cards() -> &'static [ToolFamilyCard] {
    JCODE_INSPIRED_TOOL_FAMILY_CARDS
}

pub fn tool_family_card(family_id: &str) -> Option<&'static ToolFamilyCard> {
    JCODE_INSPIRED_TOOL_FAMILY_CARDS
        .iter()
        .find(|card| card.family_id == family_id)
}

#[cfg(test)]
#[path = "tool_families_tests.rs"]
mod tests;
```

Modify `src-tauri/src/registries/mod.rs`:

```rust
pub mod tool_families;
pub use tool_families::{
    jcode_inspired_tool_family_cards, tool_family_card, ToolFamilyCard,
};
```

- [x] **Step 4: Run the focused test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib registries::tool_families
```

Expected: all `registries::tool_families` tests pass.

## Task 2: Bridge Family Tags Into RegistryHub Tools

**Files:**
- Modify: `src-tauri/src/registries/hub.rs`
- Modify: `src-tauri/src/registries/tool_families.rs`
- Modify: `src-tauri/src/registries/tool_families_tests.rs`

- [x] **Step 1: Add tests for tag projection**

Extend `tool_families_tests.rs`:

```rust
#[test]
fn family_tags_project_to_registry_tags() {
    let tags = registry_tags_for_tool("shell");
    assert_eq!(tags.get("family:shell.command"), Some(&"1"));
    assert!(tags.get("family:runtime.background").is_none());
    assert!(tags.get("tag:background").is_none());
    assert_eq!(tags.get("policy:permission.required"), Some(&"1"));
}

#[test]
fn unknown_tools_get_no_family_tags() {
    let tags = registry_tags_for_tool("not-a-tool");
    assert!(tags.is_empty());
}
```

- [x] **Step 2: Run the test and verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib registries::tool_families
```

Expected: compile failure because `registry_tags_for_tool` is not defined.

- [x] **Step 3: Implement tag projection**

Add to `tool_families.rs`:

```rust
use std::collections::BTreeMap;

pub fn registry_tags_for_tool(tool_id: &str) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    for card in JCODE_INSPIRED_TOOL_FAMILY_CARDS {
        if card.execution_status != "active" {
            continue;
        }
        if !card.tool_ids.iter().any(|id| *id == tool_id) {
            continue;
        }
        tags.insert(format!("family:{}", card.family_id), "1".to_string());
        tags.insert(format!("harness:{}", card.harness_subject), "1".to_string());
        tags.insert(format!("cost:{}", card.cost_tier), "1".to_string());
        tags.insert(format!("reliability:{}", card.reliability_tier), "1".to_string());
        for tag in card.capability_tags {
            tags.insert(format!("tag:{tag}"), "1".to_string());
        }
        for tag in card.policy_tags {
            tags.insert(format!("policy:{tag}"), "1".to_string());
        }
        for event in card.event_profile {
            tags.insert(format!("event:{event}"), "1".to_string());
        }
    }
    tags
}
```

In `hub.rs`, inside `register_builtin_tools`, after adding builtin/tag keys:

```rust
tags.extend(crate::registries::tool_families::registry_tags_for_tool(id));
```

- [x] **Step 4: Add resolver coverage for families**

Add a sibling-safe test if `hub.rs` is split in this task; otherwise add only the minimal assertion to existing hub tests and leave a follow-up note. The assertion:

```rust
let q = CapabilityQuery {
    name: None,
    kind: "filesystem".into(),
    tags: {
        let mut t = std::collections::BTreeMap::new();
        t.insert("family:filesystem.patch".into(), "1".into());
        t
    },
};
let result = crate::registries::resolve(&*hub.tools.read().await, &q);
assert_eq!(result.best(), Some("edit"));
```

Also assert planned cards do not resolve to live tools:

```rust
for family in ["runtime.background", "context.session_search"] {
    let q = CapabilityQuery {
        name: None,
        kind: String::new(),
        tags: {
            let mut t = std::collections::BTreeMap::new();
            t.insert(format!("family:{family}"), "1".into());
            t
        },
    };
    let result = crate::registries::resolve(&*hub.tools.read().await, &q);
    assert!(result.is_empty(), "{family} must stay catalog-only");
}
```

- [x] **Step 5: Run registry tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib registries
```

Expected: registry tests pass; no dispatcher or tool execution tests changed.

## Task 3: Update Status Ledger And Verify

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

- [x] **Step 1: Update Quick View**

Change PR8 from `Not started` to `In progress`, set owner to `Codex`, and describe the active PR branch/worktree.

- [x] **Step 2: Run verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib registries::tool_families
cargo test --manifest-path src-tauri/Cargo.toml --lib registries
git diff --check
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh
```

Expected:

- focused tool-family tests pass;
- registry tests pass;
- diff check has no output;
- GitNexus reports LOW or expected registry-only impact.

- [x] **Step 3: Commit**

Commit message:

```bash
git commit -m "feat(registries): add jcode tool family cards" -m "Verification:
- cargo test --manifest-path src-tauri/Cargo.toml --lib registries::tool_families
- cargo test --manifest-path src-tauri/Cargo.toml --lib registries
- git diff --check
- npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr8-tool-family-mesh"
```
