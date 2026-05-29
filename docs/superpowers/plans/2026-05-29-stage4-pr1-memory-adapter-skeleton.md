# 阶段 4 PR1 — `MemoryAdapter` Trait + Types Skeleton · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the `MemoryAdapter` async trait + the 4 supporting owned types (`MemoryEntry`, `MemoryCategory`, `RecallOpts`, `NamespaceSummary`) + the `AppState` registry fields (`memory_adapters: Arc<HashMap<String, Arc<dyn MemoryAdapter>>>` + `default_memory_backend: Arc<RwLock<String>>`). **Zero concrete impls in this PR** — just the contract + the empty registry shape. The whole point is to validate that the trait shape compiles + the registry threads through `AppState` cleanly before any adapter wraps anything.

**Architecture:** Mirror openhuman's `Memory` trait from `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/traits.rs` verbatim, adapted to uClaw's error type (`anyhow::Result` — already a workspace dep). Put the trait + types in a new module `src-tauri/src/memory_adapter/`. Add registry fields to `AppState` and populate at boot with an empty HashMap. No callers yet; the trait is dead code in this PR (expected `dead_code` warnings — those go away as later PRs add adapters).

**Tech Stack:** Rust 2021, `async-trait` 0.1, `anyhow` 1, `serde` 1, `chrono` 0.4. All already in `src-tauri/Cargo.toml`.

**Related design:** [`docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md`](../specs/2026-05-29-stage4-memory-adapter-design.md) — see "Trait surface" section + `AppState` registry block.

**Reference source:** `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/traits.rs` (port faithfully).

---

## Pre-flight (before Task 1)

1. **Confirm main baseline + design doc on origin:**

   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   git -C /Users/ryanliu/Documents/uclaw log --oneline -3
   ```

   Expected: `## main...origin/main` (in sync). HEAD at `093f61db docs(stage4): memory adapter + bucket-seal port design spec`.

2. **Create worktree + symlinks:**

   ```bash
   git worktree add -b claude/stage4-pr1-memory-adapter-skeleton \
       /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri/bunembed
   ```

3. **Verify workspace deps available:**

   ```bash
   grep -E "^(async-trait|anyhow|chrono|serde)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri/Cargo.toml | head -5
   ```

   Expected: `async-trait = "0.1"`, `anyhow = "1"`, `chrono = { version = "0.4", features = ["serde"] }`, `serde = { version = "1", features = ["derive"] }`. No new dependency adds needed.

4. **Capture baselines:**

   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   ```

   Record the numbers; they are the gates every task must clear. Expected:
   - 0 errors
   - 49 warnings (post-Tier-3 baseline)
   - `agent::dispatcher`: 56/0 (post Tier 2.4 +1 snapshot test)
   - `agent::`: 802/2 (post Tier 2.5 -10 tests + Tier 2.4 +1)

---

## File structure (new)

```
src-tauri/src/memory_adapter/
├── mod.rs                — pub re-exports + module-level doc + tests module
├── traits.rs             — MemoryAdapter trait
└── types.rs              — MemoryEntry, MemoryCategory, RecallOpts, NamespaceSummary
```

**Modified:**
- `src-tauri/src/lib.rs` — add `pub mod memory_adapter;`
- `src-tauri/src/app.rs` — add 2 fields to `AppState`; populate at boot

---

## Task 1: Create `memory_adapter` module scaffold

**Files:**
- Create: `src-tauri/src/memory_adapter/mod.rs`
- Create: `src-tauri/src/memory_adapter/types.rs`
- Create: `src-tauri/src/memory_adapter/traits.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod memory_adapter;`)

### Steps

- [ ] **Step 1.1: Create `types.rs` with the 4 owned types**

```rust
// src-tauri/src/memory_adapter/types.rs

//! Owned types used by the `MemoryAdapter` trait.
//!
//! Ported from openhuman's `src/openhuman/memory/traits.rs` (MemoryEntry,
//! MemoryCategory, RecallOpts, NamespaceSummary). The shape is identical
//! so any future port-from-source reads with no friction.

use serde::{Deserialize, Serialize};

/// Represents a single stored memory entry with associated metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier for the memory entry (usually a UUID).
    pub id: String,
    /// The key or title associated with this memory.
    pub key: String,
    /// The actual content or value of the memory.
    pub content: String,
    /// Optional namespace for logical separation of memories.
    #[serde(default)]
    pub namespace: Option<String>,
    /// The organizational category this memory belongs to.
    pub category: MemoryCategory,
    /// ISO 8601 formatted timestamp of when the memory was created or last updated.
    pub timestamp: String,
    /// Optional session ID if this memory is scoped to a specific interaction.
    pub session_id: Option<String>,
    /// Optional relevance or confidence score, typically from 0.0 to 1.0.
    pub score: Option<f64>,
}

/// Categories used to organize and filter memories by their nature and lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// Long-term foundational facts, user preferences, and permanent decisions.
    Core,
    /// Temporal logs reflecting daily activities or ephemeral state.
    Daily,
    /// Contextual information derived from and relevant to active conversations.
    Conversation,
    /// A user-defined or system-defined custom category.
    Custom(String),
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::Daily => write!(f, "daily"),
            Self::Conversation => write!(f, "conversation"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Optional filters for `MemoryAdapter::recall`.
///
/// All fields default to `None`. `namespace = None` uses the backend's
/// legacy default namespace. Pass `Some("namespace")` to scope the query
/// to a specific namespace.
#[derive(Debug, Default, Clone)]
pub struct RecallOpts<'a> {
    pub namespace: Option<&'a str>,
    pub category: Option<MemoryCategory>,
    pub session_id: Option<&'a str>,
    pub min_score: Option<f64>,
}

/// Summary row returned by `MemoryAdapter::namespace_summaries`, used for
/// agent-side namespace discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceSummary {
    pub namespace: String,
    pub count: usize,
    /// RFC3339 timestamp of most recent `updated_at` in the namespace, if any.
    pub last_updated: Option<String>,
}
```

- [ ] **Step 1.2: Create `traits.rs` with the `MemoryAdapter` trait**

```rust
// src-tauri/src/memory_adapter/traits.rs

//! The `MemoryAdapter` trait — backend-agnostic contract for memory stores.
//!
//! Mirrors openhuman's `Memory` trait from
//! `src/openhuman/memory/traits.rs`. Adapters wrap concrete backends
//! (bucket-seal, legacy KV, legacy Steward graph, gbrain MCP, memU) and
//! present them through this single shape so callers don't need to know
//! which store is underneath.

use async_trait::async_trait;

use super::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

/// The core trait for memory storage and retrieval.
///
/// Any persistence backend (SQLite, in-process KV, vector DB, MCP-wrapped
/// remote, etc.) should implement this trait to be used within the
/// uClaw memory subsystem.
#[async_trait]
pub trait MemoryAdapter: Send + Sync {
    /// Returns the name of the memory backend (e.g. `"bucket_seal"`,
    /// `"legacy_kv"`, `"gbrain"`). Used as the key in
    /// `AppState.memory_adapters`.
    fn name(&self) -> &str;

    /// Stores a new memory entry or updates an existing one.
    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Recalls memories matching a query string using keyword or
    /// semantic search.
    ///
    /// Namespace is passed via `opts.namespace`; `None` uses the
    /// backend's legacy default namespace.
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Retrieves a specific memory entry by exact `(namespace, key)`.
    async fn get(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<Option<MemoryEntry>>;

    /// Lists memory entries, optionally scoped by namespace, category,
    /// session.
    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Removes the entry at `(namespace, key)`. Returns `true` if an
    /// entry existed and was removed, `false` if nothing matched.
    async fn delete(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<bool>;

    /// Clears every entry in a namespace. Returns the number of entries
    /// removed.
    async fn clear_namespace(
        &self,
        namespace: &str,
    ) -> anyhow::Result<u64>;

    /// Returns a summary row for every namespace the backend knows
    /// about, used by namespace-discovery UI affordances.
    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>>;
}
```

- [ ] **Step 1.3: Create `mod.rs` with re-exports**

```rust
// src-tauri/src/memory_adapter/mod.rs

//! `MemoryAdapter` — uClaw's unified memory contract.
//!
//! PR1 of 阶段 4 (see
//! `docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md`).
//! This PR introduces the trait + types only; concrete adapter impls
//! ship in subsequent PRs:
//!
//! - PR2: `LegacyKvAdapter` (wraps `crate::memory::MemoryStore`)
//! - PR3: `LegacyStewardAdapter` (wraps `crate::memory_graph::MemoryGraphStore`)
//! - PR9: `BucketSealAdapter` (new openhuman bucket-seal port)
//! - PR13: `GbrainAdapter` (wraps `mcp__gbrain__*`)
//! - PR14: `MemUAdapter` (wraps `MemUClient`)
//!
//! Until then, `AppState.memory_adapters` is an empty `HashMap`.

mod traits;
mod types;

pub use traits::MemoryAdapter;
pub use types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

#[cfg(test)]
mod tests;
```

- [ ] **Step 1.4: Register the module in `lib.rs`**

Find the existing `pub mod memory;` and `pub mod memory_graph;` declarations in `src-tauri/src/lib.rs` and add the new module nearby (alphabetical placement is fine, or grouped with other `memory_*` mods).

```bash
grep -n "^pub mod memory" /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri/src/lib.rs
```

Add the line:

```rust
pub mod memory_adapter;
```

- [ ] **Step 1.5: Build (no tests yet; dead code expected)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: 0 errors. The trait + types will be flagged as `dead_code` by clippy (the trait has no implementors yet), but this is intentional and acceptable — the dead-code warning disappears as PR2 adds the first concrete adapter.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```

Expected: ≤53 (49 baseline + up to 4 new dead-code warnings for the trait + 4 types). If higher, inspect what new warnings landed.

- [ ] **Step 1.6: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton add -A src-tauri/src/memory_adapter/ src-tauri/src/lib.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton commit -m "feat(memory_adapter): scaffold trait + owned types (PR1.1 of 阶段 4)

Ports openhuman's Memory trait + MemoryEntry/MemoryCategory/RecallOpts/
NamespaceSummary types into a new src-tauri/src/memory_adapter/ module.
No implementors yet; dead-code warnings expected until PR2 adds the
first concrete adapter.

Spec: docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md"
```

Continue to Task 2.

---

## Task 2: Add unit tests for the types

The trait has no impls yet, so it can't be tested. But the owned types HAVE serde + Display logic worth pinning. These tests prevent silent shape drift before downstream PRs depend on them.

**Files:**
- Create: `src-tauri/src/memory_adapter/tests.rs`

### Steps

- [ ] **Step 2.1: Create `tests.rs` with type-shape tests**

```rust
// src-tauri/src/memory_adapter/tests.rs

//! Smoke tests for memory_adapter owned types. The trait has no
//! implementors in PR1; trait behavior is locked by later PRs.

use super::*;

#[test]
fn memory_category_display_round_trip() {
    assert_eq!(MemoryCategory::Core.to_string(), "core");
    assert_eq!(MemoryCategory::Daily.to_string(), "daily");
    assert_eq!(MemoryCategory::Conversation.to_string(), "conversation");
    assert_eq!(
        MemoryCategory::Custom("foo".to_string()).to_string(),
        "foo"
    );
}

#[test]
fn memory_category_serde_round_trip() {
    let core_json = serde_json::to_string(&MemoryCategory::Core).unwrap();
    assert_eq!(core_json, "\"core\"");
    let back: MemoryCategory = serde_json::from_str(&core_json).unwrap();
    assert_eq!(back, MemoryCategory::Core);

    let custom = MemoryCategory::Custom("topic_x".to_string());
    let json = serde_json::to_string(&custom).unwrap();
    let back: MemoryCategory = serde_json::from_str(&json).unwrap();
    assert_eq!(back, custom);
}

#[test]
fn memory_entry_serde_round_trip() {
    let entry = MemoryEntry {
        id: "abc".into(),
        key: "topic".into(),
        content: "Ryan likes coffee.".into(),
        namespace: Some("user_profile".into()),
        category: MemoryCategory::Core,
        timestamp: "2026-05-29T10:00:00Z".into(),
        session_id: Some("sess-42".into()),
        score: Some(0.87),
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: MemoryEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, entry.id);
    assert_eq!(back.namespace, entry.namespace);
    assert_eq!(back.score, entry.score);
}

#[test]
fn recall_opts_defaults_to_all_none() {
    let opts = RecallOpts::default();
    assert!(opts.namespace.is_none());
    assert!(opts.category.is_none());
    assert!(opts.session_id.is_none());
    assert!(opts.min_score.is_none());
}

#[test]
fn namespace_summary_serde_round_trip() {
    let s = NamespaceSummary {
        namespace: "user_profile".into(),
        count: 17,
        last_updated: Some("2026-05-29T09:30:00Z".into()),
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: NamespaceSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(back.count, 17);
    assert_eq!(back.last_updated.as_deref(), Some("2026-05-29T09:30:00Z"));
}
```

- [ ] **Step 2.2: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -5
```

Expected output:
```
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; ... filtered out; finished in 0.0Xs
```

If any test fails, inspect the failure. Most likely cause: the `#[serde(rename_all = "snake_case")]` attribute didn't propagate, in which case `MemoryCategory::Core` would serialize as `"Core"` instead of `"core"`. Fix the attribute placement.

- [ ] **Step 2.3: Verify full agent test suite still green**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
```

Expected: 802/2 baseline preserved.

- [ ] **Step 2.4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton add -A src-tauri/src/memory_adapter/tests.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton commit -m "test(memory_adapter): add 5 type-shape tests (PR1.2 of 阶段 4)

Locks MemoryCategory Display + serde, MemoryEntry serde, RecallOpts
default, NamespaceSummary serde. The trait itself has no impls in PR1
so its behavior is tested by later PRs that ship concrete adapters."
```

Continue to Task 3.

---

## Task 3: Add `memory_adapters` + `default_memory_backend` to `AppState`

**Files:**
- Modify: `src-tauri/src/app.rs`

### Steps

- [ ] **Step 3.1: Locate the `AppState` struct + the `new()` constructor**

```bash
grep -n "^pub struct AppState\|^impl AppState\|pub async fn new" /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri/src/app.rs | head -10
```

The struct declaration is around line 180-300. The `new` impl is later in the file.

- [ ] **Step 3.2: Add the 2 new fields to `AppState`**

Insert these two fields in `AppState` immediately after the existing `memory_graph_store` field (~line 211). The 2 lines go together to keep "memory" related fields clustered.

```rust
    /// PR1 of 阶段 4 — registry mapping backend name → adapter. Empty
    /// HashMap in PR1; PR2-PR14 add concrete adapters one by one. See
    /// `docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md`.
    pub memory_adapters:
        std::sync::Arc<std::collections::HashMap<String, std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>>>,

    /// PR1 of 阶段 4 — name of the backend used when callers don't
    /// specify one explicitly. Starts as `"bucket_seal"` (the eventual
    /// primary) even though no adapter is registered yet — when PR9
    /// registers `BucketSealAdapter`, the default is immediately live.
    pub default_memory_backend: std::sync::Arc<std::sync::RwLock<String>>,
```

- [ ] **Step 3.3: Locate `AppState::new` populator**

```bash
grep -n "memory_graph_store:" /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri/src/app.rs | head -3
```

Find the struct-literal site in `pub async fn new(...)` where `memory_graph_store` is populated (the `Self { ... }` block). The 2 new fields populate alongside it.

- [ ] **Step 3.4: Populate the 2 new fields at construction**

Inside `AppState::new`'s `Self { ... }` block, add immediately after the `memory_graph_store: ...` line:

```rust
    memory_adapters: std::sync::Arc::new(std::collections::HashMap::new()),
    default_memory_backend: std::sync::Arc::new(std::sync::RwLock::new(
        "bucket_seal".to_string(),
    )),
```

- [ ] **Step 3.5: Build**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: 0 errors.

Common errors to watch for:
- `error[E0432]: unresolved import crate::memory_adapter` — `lib.rs` doesn't have `pub mod memory_adapter;` (Task 1 Step 1.4 missed). Add it.
- `error[E0220]: associated type \`Item\` ...` — async-trait setup wrong; verify Task 1 Step 1.2's `#[async_trait]` decoration is on the trait.
- `error[E0277]: \`AppState\` doesn't implement Send/Sync` — `Arc<dyn MemoryAdapter>` needs `Send + Sync` bounds (already in the trait definition's `Send + Sync` bound). If this fires, double-check the trait declaration.

- [ ] **Step 3.6: Warning count check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```

Expected: ≤53. The new fields will likely trigger `dead_code` warnings since nothing reads them — that's OK, PR2 onward will use them.

- [ ] **Step 3.7: Verify test baselines**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
```

Expected:
- `memory_adapter` — 5 passed / 0 failed
- `agent::dispatcher` — 56 passed / 0 failed
- `agent::` — 802 passed / 2 failed (the 2 pre-existing)

- [ ] **Step 3.8: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton add -A src-tauri/src/app.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton commit -m "feat(app): register memory_adapters HashMap + default_memory_backend on AppState (PR1.3 of 阶段 4)

Empty registry today; populated as PR2-PR14 add concrete adapters.
default_memory_backend pre-set to \"bucket_seal\" so when PR9 lands the
BucketSealAdapter, the default is immediately live.

Closes PR1 of 阶段 4."
```

Continue to Task 4.

---

## Task 4: Final audit + push + PR open

**Files:** None (verification + git push + `gh pr create`).

### Steps

- [ ] **Step 4.1: Full battery**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton/src-tauri && cargo test --lib 2>&1 | tail -3
```

Required:
- 0 errors.
- Warnings ≤53 (49 baseline + up to 4 new dead-code).
- `memory_adapter` — 5/0.
- `agent::dispatcher` — 56/0.
- `agent::` — 802/2.
- `cargo test --lib` total — pre-existing count + 5 (from Task 2).

- [ ] **Step 4.2: Verify final chain**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton log --oneline main..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton status -sb
```

Expected: 3 commits ahead of main:
```
<sha3> feat(app): register memory_adapters HashMap + default_memory_backend on AppState (PR1.3 of 阶段 4)
<sha2> test(memory_adapter): add 5 type-shape tests (PR1.2 of 阶段 4)
<sha1> feat(memory_adapter): scaffold trait + owned types (PR1.1 of 阶段 4)
```

Working tree clean.

- [ ] **Step 4.3: Push + open PR**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton push -u origin claude/stage4-pr1-memory-adapter-skeleton
```

Then:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr1-memory-adapter-skeleton && gh pr create --title "feat(memory_adapter): MemoryAdapter trait + types skeleton (PR1 of 阶段 4)" --body "$(cat <<'EOF'
## Summary

First PR of 阶段 4 (memory adapter + openhuman bucket-seal port). This PR introduces the contract + registry only — **zero concrete adapter impls** yet.

- Adds \`src-tauri/src/memory_adapter/\` module with the \`MemoryAdapter\` async trait + 4 owned types (\`MemoryEntry\`, \`MemoryCategory\`, \`RecallOpts\`, \`NamespaceSummary\`), all ported verbatim from openhuman's \`src/openhuman/memory/traits.rs\`.
- Adds \`AppState.memory_adapters: Arc<HashMap<String, Arc<dyn MemoryAdapter>>>\` (empty in this PR) and \`AppState.default_memory_backend: Arc<RwLock<String>>\` (pre-set to \`"bucket_seal"\`).
- 5 type-shape unit tests lock serde + Display behavior so PR2-PR14 can rely on the contract.

Spec: \`docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md\` (committed at 093f61db).

## Commits (bisectable)

| # | Scope | What |
|---|---|---|
| PR1.1 | memory_adapter | Trait + 4 types + mod.rs + lib.rs registration |
| PR1.2 | memory_adapter/tests | 5 type-shape tests (serde / Display / Default) |
| PR1.3 | app | AppState registry fields + boot population |

## Test plan

- [x] \`cargo build\` 0 errors, ≤53 warnings (49 baseline + ≤4 new dead-code warnings)
- [x] \`cargo test --lib memory_adapter\` → 5/0 (new)
- [x] \`cargo test --lib agent::dispatcher\` → 56/0 (baseline)
- [x] \`cargo test --lib agent::\` → 802/2 (baseline)
- [x] \`cargo test --lib\` total grows by 5
- [x] No call sites yet (intentional — PR2 adds first concrete adapter, dead-code warnings clear then)

## Next

PR2 wraps \`crate::memory::MemoryStore\` as \`LegacyKvAdapter\` — first concrete \`MemoryAdapter\` impl. The trait shape gets stress-tested against a real KV backend.
EOF
)"
```

Record the PR URL. The brainstorming session's terminal handoff is now realized — implementation cycle starts at PR1.

---

## Self-Review

**1. Spec coverage:**

Walking the design spec (`2026-05-29-stage4-memory-adapter-design.md`) section by section:

- ✅ Trait surface — Task 1.2 ports the 8-method trait verbatim.
- ✅ Supporting types (`MemoryEntry`, `MemoryCategory`, `RecallOpts`, `NamespaceSummary`) — Task 1.1.
- ✅ `AppState.memory_adapters` HashMap — Task 3.2 + 3.4.
- ✅ `AppState.default_memory_backend` field — Task 3.2 + 3.4 (pre-set to `"bucket_seal"`).
- ✅ Module location `src-tauri/src/memory_adapter/` — Task 1 structure.
- ✅ Mirror openhuman's `Memory` trait — `Send + Sync` bound, `async_trait`, `anyhow::Result` return.
- 🟡 Concrete adapters (BucketSeal, LegacyKv, LegacySteward, Gbrain, MemU) — not in this PR. Tracked in spec's PR sequence.
- 🟡 New IPC family `memory.unified.*` — PR4 per spec sequence.
- 🟡 Recall routing helper — PR15 per spec sequence.

PR1 covers exactly the scope its row in the spec's "Migration PR sequence" table calls out: trait + types skeleton + `AppState` registry shape. No gaps relative to that row.

**2. Placeholder scan:**

- No "TBD", "TODO", "implement later", "fill in details" anywhere.
- No "Add appropriate error handling" — every method's signature comes from openhuman's source and is shown in full.
- No "Write tests for the above" without code — Task 2 shows the 5 test bodies verbatim.
- No "Similar to Task N" — each task is self-contained with full code.
- No undefined references — every type/method referenced in later tasks is defined in Task 1.

**3. Type consistency:**

- `MemoryAdapter` (capital M, capital A) used consistently.
- `MemoryEntry` field names: `id`, `key`, `content`, `namespace`, `category`, `timestamp`, `session_id`, `score` — match between types.rs, tests.rs, and the design spec.
- `MemoryCategory` variants: `Core`, `Daily`, `Conversation`, `Custom(String)` — same in types.rs, tests.rs, design spec.
- `RecallOpts<'a>` lifetime parameter consistent.
- `NamespaceSummary` field order matches design spec.
- `Arc<HashMap<String, Arc<dyn MemoryAdapter>>>` shape matches between Task 3.2's struct declaration and 3.4's populator.
- `anyhow::Result<...>` used consistently as return type.

All checks pass. No gaps.

---

## Cumulative summary

- **Tasks:** 4 (3 implementation + 1 audit/PR-open).
- **Estimated time:** 30-60 minutes (mechanical port + one struct addition).
- **Risk:** Very low. No semantic changes; just a new module + 2 struct fields populated at boot.
- **Total commits:** 3 (one per task; Task 4 is verification + push only).
