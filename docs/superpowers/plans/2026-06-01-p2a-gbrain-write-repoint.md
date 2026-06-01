# P2a-1 gbrain Write Repoint (gated dual-write) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every Rust-side gbrain page write also shadow-writes the page into the adapter `"pages"` namespace (gated, best-effort), so the adapter stays in sync through the convergence transition.

**Architecture:** A shared `dual_write_page` helper wraps the existing `gbrain::browse::put_page` (gbrain stays the primary write — its `Result` is returned unchanged); when `gbrain_dual_write_pages_enabled` is on and a `bucket_seal` adapter handle is present, the same markdown is mapped to a `pages::Page` and best-effort written to the adapter. The handle is threaded into the 4 Rust write sites (IPC, memorization, memory_policy, ingestion).

**Tech Stack:** Rust, Tauri, `serde_yml` (frontmatter parse), `tracing`, the `MemoryAdapter` trait + `memory_adapter::pages` facade, the gbrain MCP `browse` module.

---

## Recon findings (complete — ground truth for all tasks)

- `gbrain::browse::put_page(mcp, slug, content) -> Result<PageDetail, GbrainError>` (it writes then re-fetches; see `src-tauri/src/gbrain/browse.rs:373`). `PageDetail`/`PageSummary` types are in the same file; `GbrainError` has **no `Display`** — use `%e` only where the type implements it, else `e.to_command_string()` (it does implement what `%e` needs in `tracing` via its error impl — `browse.rs` already uses `error=%e`, so `%e` is fine here).
- `pages::Page { slug, title, page_type, body, tags: Vec<String> }`; `pages::put_page(&Arc<dyn MemoryAdapter>, &Page) -> anyhow::Result<()>`; `pages::get_page(adapter, slug) -> anyhow::Result<Option<Page>>` (`src-tauri/src/memory_adapter/pages.rs:14,33,41`).
- **No site E.** `router.rs:266` only routes to the `gbrain` backend when `default_backend == "gbrain"`; the live default is `bucket_seal`, so `GbrainAdapter::store` (`gbrain.rs:204`) is not an active page-write path. Excluded, confirmed.
- `MemorizationService` (`src-tauri/src/memorization/service.rs`) holds `mcp_manager: Arc<RwLock<Option<SharedMcpManager>>>` set via `set_mcp_manager` (the post-construction-setter pattern to mirror). Wired at `src-tauri/src/main.rs:248` (`mem_svc.set_mcp_manager(...)`), where the `AppState` `state` (line 204) — and thus `state.bucket_seal_adapter` — is in scope (pattern `Arc::clone(&state.bucket_seal_adapter)` already used at main.rs:414).
- `GbrainPolicyTarget` (`src-tauri/src/memory_policy/targets/gbrain.rs:40`) holds `mcp: Option<SharedMcpManager>`; `new(mcp)` at line 45; single caller `src-tauri/src/browser/runtime_memory_policy.rs:81` (`.map(GbrainPolicyTarget::new)`).
- `IngestionService` (`src-tauri/src/ingestion/mod.rs:23`) holds `jobs, provider_service, mcp`; `new(provider_service, mcp)` at line 30; `merge::write_entity(&mcp, &provider, &model, ent)` at line 149. Constructed at `src-tauri/src/app.rs:915`, which is **before** `bucket_seal_adapter` is built (~app.rs:1086) → the construction must move below the adapter (see Task 7), or use a `RwLock` setter (documented fallback).
- `MemoryOsConfig` is at `src-tauri/src/memubot_config.rs:469`; the bool-with-default pattern is `#[serde(default = "default_x")] pub x: bool,` + `fn default_x() -> bool { ... }` (see `default_unified_load_context_enabled` at ~line 428 returning `true`).
- The `bucket_seal_adapter` field on `AppState` is `Arc<crate::memory_bucket_seal::BucketSealAdapter>`; cast to the trait object via `Arc::clone(&state.bucket_seal_adapter) as Arc<dyn crate::memory_adapter::MemoryAdapter>`.

## Worktree setup

Work in an isolated worktree under `/Users/ryanliu/Documents/uclaw-worktrees/` on branch `claude/p2a-gbrain-write-repoint` (the using-git-worktrees skill creates it at execution time). Baseline: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` must be clean before Task 1.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/gbrain/browse.rs` | **add** `split_frontmatter` — inverse of `build_raw_markdown` |
| `src-tauri/src/memory_adapter/page_dual_write.rs` | **new** — `markdown_to_page` + `shadow_write_page` + `dual_write_page` |
| `src-tauri/src/memory_adapter/mod.rs` | **add** `pub mod page_dual_write;` |
| `src-tauri/src/memubot_config.rs` | **add** `gbrain_dual_write_pages_enabled` + default fn |
| `src-tauri/src/tauri_commands.rs:1538` | site A — repoint to `dual_write_page` |
| `src-tauri/src/memorization/service.rs` + `src-tauri/src/main.rs:248` | site B — field + setter + wire + repoint (×2) |
| `src-tauri/src/memory_policy/targets/gbrain.rs` + `src-tauri/src/browser/runtime_memory_policy.rs:81` | site C — constructor param + caller + repoint |
| `src-tauri/src/ingestion/mod.rs` + `src-tauri/src/app.rs:915` | site D — field + new() param + reorder + repoint |

---

### Task 1: `split_frontmatter` in browse.rs

**Files:**
- Modify: `src-tauri/src/gbrain/browse.rs` (add fn near `build_raw_markdown` at line 164; add tests in the existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block in `browse.rs`:

```rust
#[test]
fn split_frontmatter_parses_yaml_block() {
    let md = "---\ntitle: Hello\npage_type: note\ntags:\n  - a\n  - b\n---\n\nbody text here";
    let (fm, body) = split_frontmatter(md);
    assert_eq!(fm.get("title").and_then(|v| v.as_str()), Some("Hello"));
    assert_eq!(fm.get("page_type").and_then(|v| v.as_str()), Some("note"));
    assert_eq!(
        fm.get("tags").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(2)
    );
    assert_eq!(body, "body text here");
}

#[test]
fn split_frontmatter_no_fence_returns_null_and_full() {
    let md = "just a plain body, no frontmatter";
    let (fm, body) = split_frontmatter(md);
    assert!(fm.is_null());
    assert_eq!(body, md);
}

#[test]
fn split_frontmatter_malformed_yaml_returns_null_and_full() {
    let md = "---\n: : not valid yaml : :\n---\nbody";
    let (fm, body) = split_frontmatter(md);
    assert!(fm.is_null());
    assert_eq!(body, md);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib gbrain::browse::tests::split_frontmatter 2>&1 | tail -15`
Expected: FAIL — `cannot find function split_frontmatter`.

- [ ] **Step 3: Implement `split_frontmatter`**

Add immediately after `build_raw_markdown` (after line ~178) in `browse.rs`:

```rust
/// Split a raw markdown page into (frontmatter, body) — the inverse of
/// `build_raw_markdown`. A leading `---\n … \n---\n` fence is parsed as YAML →
/// `serde_json::Value`; if the fence is absent or the YAML is malformed,
/// returns `(serde_json::Value::Null, the_full_input)`. Never panics.
pub fn split_frontmatter(markdown: &str) -> (serde_json::Value, String) {
    let null = serde_json::Value::Null;
    // Must start with a "---\n" opening fence.
    let Some(rest) = markdown.strip_prefix("---\n") else {
        return (null, markdown.to_string());
    };
    // Find the closing "\n---\n" fence.
    let Some(end) = rest.find("\n---\n") else {
        return (null, markdown.to_string());
    };
    let yaml = &rest[..end];
    let body = rest[end + "\n---\n".len()..].trim_start_matches('\n').to_string();
    match serde_yml::from_str::<serde_json::Value>(yaml) {
        Ok(v) if v.is_object() => (v, body),
        _ => (null, markdown.to_string()),
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test --lib gbrain::browse::tests::split_frontmatter 2>&1 | tail -15`
Expected: PASS — 3 tests ok. (If `serde_yml` is not already a dep of this crate, it is — `build_raw_markdown` at line 173 calls `serde_yml::to_string`; no Cargo.toml change.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/gbrain/browse.rs
git commit -m "feat(gbrain): split_frontmatter — inverse of build_raw_markdown (P2a-1)"
```

---

### Task 2: `page_dual_write` helper module

**Files:**
- Create: `src-tauri/src/memory_adapter/page_dual_write.rs`
- Modify: `src-tauri/src/memory_adapter/mod.rs` (add `pub mod page_dual_write;`)

- [ ] **Step 1: Create the module with the pure mapping + the testable adapter half**

Create `src-tauri/src/memory_adapter/page_dual_write.rs`:

```rust
// SPDX-License-Identifier: <match the SPDX header used by sibling files in memory_adapter/>
//! P2a-1 — gated, best-effort dual-write of gbrain pages into the adapter
//! `"pages"` namespace. gbrain stays the PRIMARY write; the adapter copy is a
//! shadow that can never fail the primary. See
//! docs/superpowers/specs/2026-06-01-p2a-gbrain-write-repoint-design.md

use std::sync::Arc;

use crate::gbrain::browse::{self, PageDetail};
use crate::gbrain::GbrainError;
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::{pages, MemoryAdapter};

/// Pure map: a raw gbrain markdown page (frontmatter + body) → the adapter
/// `Page`. Mirrors P2b's `page_detail_to_page`: `body` is the full raw markdown
/// (the authoritative editable source); title/page_type/tags come from the YAML
/// frontmatter, with slug-fallback for the title.
pub(crate) fn markdown_to_page(slug: &str, markdown: &str) -> pages::Page {
    let (fm, _body) = browse::split_frontmatter(markdown);
    let title = fm
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| slug.to_string());
    let page_type = fm
        .get("page_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tags = fm
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    pages::Page {
        slug: slug.to_string(),
        title,
        page_type,
        body: markdown.to_string(),
        tags,
    }
}

/// The adapter half of the dual-write, extracted so it is unit-testable without
/// the MCP call. Best-effort: an adapter error is logged and swallowed.
pub(crate) async fn shadow_write_page(
    adapter: &Arc<dyn MemoryAdapter>,
    slug: &str,
    markdown: &str,
) {
    let page = markdown_to_page(slug, markdown);
    if let Err(e) = pages::put_page(adapter, &page).await {
        tracing::warn!(slug, error = %e, "dual-write shadow to adapter pages failed (gbrain primary ok)");
    }
}

/// Write a page to gbrain (PRIMARY — its `Result` is returned unchanged), and —
/// when `dual_write_enabled` and a handle is present — ALSO shadow-write it to
/// the adapter `"pages"` namespace (best-effort, never fails the primary).
pub async fn dual_write_page(
    mcp: &SharedMcpManager,
    adapter: Option<&Arc<dyn MemoryAdapter>>,
    slug: &str,
    markdown: &str,
    dual_write_enabled: bool,
) -> Result<PageDetail, GbrainError> {
    let res = browse::put_page(mcp, slug, markdown).await;
    if dual_write_enabled {
        if let Some(a) = adapter {
            shadow_write_page(a, slug, markdown).await;
        }
    }
    res
}
```

> Use the Read tool on a sibling file (e.g. `src-tauri/src/memory_adapter/pages.rs`) to copy its exact SPDX header line; the pre-commit hook rejects a missing SPDX.

- [ ] **Step 2: Register the module**

In `src-tauri/src/memory_adapter/mod.rs`, add alongside the other `pub mod` lines:

```rust
pub mod page_dual_write;
```

- [ ] **Step 3: Write the failing tests**

Append to `page_dual_write.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::BucketSealAdapter;

    fn mem_adapter() -> Arc<dyn MemoryAdapter> {
        // In-memory bucket_seal adapter — match the constructor used in
        // memory_adapter/pages.rs or gbrain_page_migration.rs tests.
        Arc::new(BucketSealAdapter::in_memory().expect("in-memory adapter")) as Arc<dyn MemoryAdapter>
    }

    #[test]
    fn markdown_to_page_from_frontmatter() {
        let md = "---\ntitle: My Page\npage_type: note\ntags:\n  - x\n---\n\nhello";
        let p = markdown_to_page("a/b", md);
        assert_eq!(p.slug, "a/b");
        assert_eq!(p.title, "My Page");
        assert_eq!(p.page_type, "note");
        assert_eq!(p.tags, vec!["x".to_string()]);
        assert_eq!(p.body, md); // full raw markdown preserved
    }

    #[test]
    fn markdown_to_page_no_frontmatter_uses_slug_title() {
        let md = "plain body";
        let p = markdown_to_page("my-slug", md);
        assert_eq!(p.title, "my-slug");
        assert_eq!(p.page_type, "");
        assert!(p.tags.is_empty());
        assert_eq!(p.body, md);
    }

    #[tokio::test]
    async fn shadow_write_round_trips_into_pages_namespace() {
        let adapter = mem_adapter();
        let md = "---\ntitle: T\n---\n\nbody";
        shadow_write_page(&adapter, "slug-1", md).await;
        let got = pages::get_page(&adapter, "slug-1").await.unwrap();
        let got = got.expect("page present");
        assert_eq!(got.title, "T");
        assert_eq!(got.body, md);
    }
}
```

> The exact in-memory `BucketSealAdapter` constructor name — confirm against `memory_adapter/pages.rs` or `gbrain_page_migration.rs` tests (they already build an in-memory adapter). Use the same constructor; do not invent one. If those tests use a temp-dir constructor instead, mirror that.

- [ ] **Step 4: Run to verify (write impl first if the test cannot compile)**

The impl from Step 1 is already complete, so the tests should pass directly:
Run: `cd src-tauri && cargo test --lib memory_adapter::page_dual_write 2>&1 | tail -20`
Expected: PASS — 3 tests ok (2 sync + 1 tokio). If the in-memory constructor name was wrong, fix it to match the sibling tests and re-run.

- [ ] **Step 5: Verify build + clippy**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
Run: `cd src-tauri && cargo clippy --lib 2>&1 | grep -E "^error|^warning: unused" | head` → no new warnings for this module.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/memory_adapter/page_dual_write.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory_adapter): page_dual_write helper (gbrain primary + best-effort adapter shadow) (P2a-1)"
```

---

### Task 3: config flag `gbrain_dual_write_pages_enabled`

**Files:**
- Modify: `src-tauri/src/memubot_config.rs` (add field to `MemoryOsConfig` struct at line 469; add `default_*` fn near `default_unified_load_context_enabled` ~line 428)

- [ ] **Step 1: Add the field**

Inside the `MemoryOsConfig` struct (line 469+), add:

```rust
    /// P2a-1 — when on, every Rust-side gbrain page write also shadow-writes the
    /// page into the adapter `"pages"` namespace (best-effort). Default ON keeps
    /// the adapter synced through the convergence transition (the shadow copy is
    /// not read until P2c). Rollback = set false. See page_dual_write.rs.
    #[serde(default = "default_gbrain_dual_write_pages_enabled")]
    pub gbrain_dual_write_pages_enabled: bool,
```

- [ ] **Step 2: Add the default fn**

Near `default_unified_load_context_enabled` (~line 428):

```rust
/// P2a-1 — gbrain→adapter page dual-write defaults ON during the convergence
/// transition. See `MemoryOsConfig::gbrain_dual_write_pages_enabled`.
fn default_gbrain_dual_write_pages_enabled() -> bool {
    true
}
```

- [ ] **Step 3: Add a default-value test**

If `memubot_config.rs` has a `#[cfg(test)]` module asserting config defaults (search for `default_unified_load_context_enabled` usage in tests), add an assertion there; otherwise add a minimal one:

```rust
#[test]
fn gbrain_dual_write_pages_enabled_defaults_on() {
    assert!(default_gbrain_dual_write_pages_enabled());
}
```

- [ ] **Step 4: Run + verify**

Run: `cd src-tauri && cargo test --lib gbrain_dual_write_pages_enabled 2>&1 | tail -8`
Expected: PASS. Then `cargo build 2>&1 | grep -E "^error" | head` → empty (a new `#[serde(default)]` field is backward-compatible — existing config files deserialize fine).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memubot_config.rs
git commit -m "feat(config): gbrain_dual_write_pages_enabled (default on) (P2a-1)"
```

---

### Task 4: Site A — memory put-page IPC

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs:1538` (the `crate::gbrain::browse::put_page(&state.mcp_manager, &slug, &content)` call)

- [ ] **Step 1: Read the call site**

Read `src-tauri/src/tauri_commands.rs` lines ~1525–1545 to see the enclosing command fn, confirm `state` (with `mcp_manager` + `bucket_seal_adapter`) and the config are in scope, and how the config is accessed in this fn (look for `state.memubot_config` / a `MemoryOsConfig` read elsewhere in the file for the idiom).

- [ ] **Step 2: Repoint to `dual_write_page`**

Replace the `browse::put_page` call at line 1538:

```rust
    // before:
    // crate::gbrain::browse::put_page(&state.mcp_manager, &slug, &content).await ...
    // after:
    let dual = {
        let cfg = state.memubot_config.read().await;
        cfg.memory_os.gbrain_dual_write_pages_enabled
    };
    let adapter = Arc::clone(&state.bucket_seal_adapter)
        as Arc<dyn crate::memory_adapter::MemoryAdapter>;
    crate::memory_adapter::page_dual_write::dual_write_page(
        &state.mcp_manager,
        Some(&adapter),
        &slug,
        &content,
        dual,
    )
    .await
```

> The exact config-read expression (`state.memubot_config.read().await`, the path to `memory_os.gbrain_dual_write_pages_enabled`, and whether `MemoryOsConfig` lives at `cfg.memory_os` or directly on the config) MUST be confirmed against an existing `gbrain_dual_write_pages_enabled`-sibling read in this file or `app.rs` (e.g. search how `unified_load_context_enabled` is read at runtime). Use that exact idiom. Ensure `use std::sync::Arc;` is present (it is, file-wide).

- [ ] **Step 3: Build + run any touching test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty. The return value of `dual_write_page` is `Result<PageDetail, GbrainError>` — identical shape to the previous `browse::put_page`, so downstream `?`/match is unchanged.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(memory): site A — put-page IPC dual-writes to adapter (P2a-1)"
```

---

### Task 5: Site B — memorization service

**Files:**
- Modify: `src-tauri/src/memorization/service.rs` (add field + setter near `mcp_manager`/`set_mcp_manager` ~line 72/112; repoint the two `browse::put_page` calls at 474 + 497)
- Modify: `src-tauri/src/main.rs:248` (wire the adapter setter beside `set_mcp_manager`)

- [ ] **Step 1: Add the field + setter (mirror `mcp_manager`)**

In `MemorizationService` struct (~line 72), beside `mcp_manager`:

```rust
    bucket_seal_adapter: Arc<RwLock<Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>>>,
```

In the struct's constructor (`new`, where `mcp_manager: Arc::new(RwLock::new(None))` is set ~line 105), add:

```rust
            bucket_seal_adapter: Arc::new(RwLock::new(None)),
```

Beside `set_mcp_manager` (~line 112):

```rust
    pub async fn set_bucket_seal_adapter(
        &self,
        adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    ) {
        *self.bucket_seal_adapter.write().await = adapter;
    }
```

- [ ] **Step 2: Repoint the two write calls**

The two calls live in a method that already reads `mcp_manager`. Just before they run, snapshot the adapter + flag (clone out of the guards so nothing is held across the await). At the top of the relevant block (before line 474):

```rust
        let adapter_snapshot = self.bucket_seal_adapter.read().await.clone();
        let dual_enabled = self.config.gbrain_dual_write_pages_enabled;
```

> Confirm the memorization config struct actually carries `gbrain_dual_write_pages_enabled`. It likely does NOT — the flag lives on `MemoryOsConfig`, but `MemorizationService` holds a `MemorizationConfig`. RESOLUTION: thread the flag the same way as the adapter — add a `dual_write_pages_enabled: Arc<RwLock<bool>>` set by a `set_dual_write_pages_enabled(bool)` setter wired in main.rs from `memubot_config.memory_os.gbrain_dual_write_pages_enabled`, OR (simpler) read it from a field on `MemorizationConfig` if memorization config is derived from the same source. Pick the setter approach (consistent with `set_mcp_manager`); snapshot becomes `let dual_enabled = *self.dual_write_pages_enabled.read().await;`. Add the field + setter in this step too.

Replace each `crate::gbrain::browse::put_page(mcp_manager, &slug, &merged_markdown)` / `(..., &markdown_content)` call (474, 497) with:

```rust
        let _updated = crate::memory_adapter::page_dual_write::dual_write_page(
            mcp_manager,
            adapter_snapshot.as_ref(),
            &slug,
            &merged_markdown, // (or &markdown_content at the second site)
            dual_enabled,
        )
        .await
```

(`mcp_manager` here is the already-unwrapped `&SharedMcpManager` the existing code passes; keep whatever binding the current code uses.)

- [ ] **Step 3: Wire in main.rs**

At `src-tauri/src/main.rs:248`, after `mem_svc.set_mcp_manager(Some(mcp_manager.clone())).await;`:

```rust
                                mem_svc
                                    .set_bucket_seal_adapter(Some(
                                        Arc::clone(&state.bucket_seal_adapter)
                                            as Arc<dyn uclaw_core::memory_adapter::MemoryAdapter>,
                                    ))
                                    .await;
                                mem_svc
                                    .set_dual_write_pages_enabled(
                                        memubot_config.memory_os.gbrain_dual_write_pages_enabled,
                                    )
                                    .await;
```

> Confirm `state` (AppState, line 204) is in scope at 248 and exposes `bucket_seal_adapter` (recon says yes; main.rs:414 uses `state_ref.bucket_seal_adapter`). If the binding here is named differently, use that name. Confirm the crate path prefix (`uclaw_core::` vs `crate::`) matches how main.rs refers to core types (main.rs:9 uses `uclaw_core::app::AppState`).

- [ ] **Step 4: Build + test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
Run: `cd src-tauri && cargo test --lib memorization 2>&1 | tail -10` → existing memorization tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memorization/service.rs src-tauri/src/main.rs
git commit -m "feat(memorization): site B — dual-write merged/new pages to adapter (P2a-1)"
```

---

### Task 6: Site C — memory_policy gbrain target

**Files:**
- Modify: `src-tauri/src/memory_policy/targets/gbrain.rs` (struct ~40, `new` ~45, the `browse::put_page` at 75)
- Modify: `src-tauri/src/browser/runtime_memory_policy.rs:81` (the `.map(GbrainPolicyTarget::new)` caller)

- [ ] **Step 1: Extend the struct + constructor**

In `targets/gbrain.rs`, add to `GbrainPolicyTarget` (~line 40):

```rust
    adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    dual_write_enabled: bool,
```

Change `new` (~line 45):

```rust
    pub fn new(
        mcp: SharedMcpManager,
        adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
        dual_write_enabled: bool,
    ) -> Self {
        Self { mcp: Some(mcp), adapter, dual_write_enabled }
    }
```

- [ ] **Step 2: Repoint the write at line 75**

Replace `crate::gbrain::browse::put_page(mcp, &request.slug, &request.content)` with:

```rust
            crate::memory_adapter::page_dual_write::dual_write_page(
                mcp,
                self.adapter.as_ref(),
                &request.slug,
                &request.content,
                self.dual_write_enabled,
            )
```

(Keep the surrounding `.await`/error handling exactly as-is — the return type is unchanged `Result<PageDetail, GbrainError>`.)

- [ ] **Step 3: Update the caller**

At `src-tauri/src/browser/runtime_memory_policy.rs:81`, the `.map(GbrainPolicyTarget::new)` now needs the extra args. Read lines ~70–90 to see where `mcp` comes from and whether the bucket_seal adapter + the dual-write flag are reachable in this scope (they may not be — this is browser runtime). Change:

```rust
    // before: .map(GbrainPolicyTarget::new)
    // after (closure threads the handle + flag obtained in this scope):
    .map(|mcp| GbrainPolicyTarget::new(mcp, adapter.clone(), dual_write_enabled))
```

> If the adapter + flag are NOT reachable at `runtime_memory_policy.rs:81`, thread them into the enclosing fn's signature from its caller (recon the call chain in this step). If threading proves to reach far outside this slice's blast radius, fall back to `GbrainPolicyTarget::new(mcp, None, false)` here and record a `log`/`tracing::info!` + a one-line note in the PR body that the memory_policy target's dual-write is deferred (handle not reachable without cross-subsystem plumbing) — do NOT silently drop it. Prefer real threading; use the fallback only if the chain is unreasonably deep.

- [ ] **Step 4: Build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_policy/targets/gbrain.rs src-tauri/src/browser/runtime_memory_policy.rs
git commit -m "feat(memory_policy): site C — gbrain policy target dual-writes to adapter (P2a-1)"
```

---

### Task 7: Site D — ingestion entity merge

**Files:**
- Modify: `src-tauri/src/ingestion/mod.rs` (struct ~23, `new` ~30, the `merge::write_entity` call ~149)
- Modify: `src-tauri/src/ingestion/merge.rs` (`write_entity` signature ~ the fn containing line 59; the `browse::put_page` at line 59)
- Modify: `src-tauri/src/app.rs:915` (pass the adapter + reorder construction below `bucket_seal_adapter`)

- [ ] **Step 1: Add the adapter param to `merge::write_entity`**

Read `src-tauri/src/ingestion/merge.rs` around line 40–70 for the `write_entity` signature. Add two params:

```rust
pub async fn write_entity(
    mcp: &SharedMcpManager,
    adapter: Option<&Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    dual_write_enabled: bool,
    provider: &...,   // keep existing params exactly
    model: &str,
    entity: ...,
) -> ... {
```

Replace the `browse::put_page(mcp, &entity.slug, &content)` at line 59:

```rust
    browse::put_page  // remove
    // →
    crate::memory_adapter::page_dual_write::dual_write_page(
        mcp,
        adapter,
        &entity.slug,
        &content,
        dual_write_enabled,
    )
```

(Preserve the existing `.await` + return handling.)

- [ ] **Step 2: Thread the handle through `IngestionService`**

In `ingestion/mod.rs`, add to the `IngestionService` struct (~line 23):

```rust
    bucket_seal_adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    dual_write_pages_enabled: bool,
```

Change `new` (~line 30):

```rust
    pub fn new(
        provider_service: Arc<ProviderService>,
        mcp: SharedMcpManager,
        bucket_seal_adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
        dual_write_pages_enabled: bool,
    ) -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            provider_service,
            mcp,
            bucket_seal_adapter,
            dual_write_pages_enabled,
        }
    }
```

At the `merge::write_entity(&mcp, &provider, &model, ent)` call (~line 149), pass the new args (the call is inside a method/closure with `&self` or a clone of these fields — clone them before the spawn if needed):

```rust
        match merge::write_entity(
            &mcp,
            self.bucket_seal_adapter.as_ref(),
            self.dual_write_pages_enabled,
            &provider,
            &model,
            ent,
        ).await {
```

> If line 149 runs inside a `tokio::spawn`/closure that has moved `self` away, clone `self.bucket_seal_adapter` + `self.dual_write_pages_enabled` into locals alongside the existing `mcp`/`provider`/`model` captures before the spawn, and pass the locals. Match whatever capture pattern the existing `mcp` uses.

- [ ] **Step 3: Update construction in app.rs (reorder below the adapter)**

At `src-tauri/src/app.rs:915`, the `IngestionService::new(provider_service.clone(), mcp_manager.clone())` runs before `bucket_seal_adapter` exists (~line 1086). Move the `let ingestion = Arc::new(...IngestionService::new(...))` block to **after** `bucket_seal_adapter` is constructed, and pass it:

```rust
        let ingestion = Arc::new(crate::ingestion::IngestionService::new(
            provider_service.clone(),
            mcp_manager.clone(),
            Some(Arc::clone(&bucket_seal_adapter) as Arc<dyn crate::memory_adapter::MemoryAdapter>),
            memubot_config.memory_os.gbrain_dual_write_pages_enabled,
        ));
```

> Verify nothing between the old construction site (915) and the new location (post-1086) reads `ingestion`. If something does (e.g. `ingestion` is referenced before 1086), DON'T reorder — instead keep `new(provider_service, mcp)` taking the adapter as `None`, add a `set_bucket_seal_adapter(&self, ...)` + `set_dual_write_pages_enabled(&self, ...)` pair using `Arc<RwLock<...>>` fields (mirror Task 5's memorization setter), and call them after line 1086. Confirm the `memubot_config` binding name in app.rs (it may be `config` / `memubot_config_arc`); use the in-scope one.

- [ ] **Step 4: Build + ingestion tests**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
Run: `cd src-tauri && cargo test --lib ingestion 2>&1 | tail -10` → existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ingestion/mod.rs src-tauri/src/ingestion/merge.rs src-tauri/src/app.rs
git commit -m "feat(ingestion): site D — entity merge dual-writes to adapter (P2a-1)"
```

---

### Task 8: Whole-slice verification

**Files:** none (verification only)

- [ ] **Step 1: Full build, errors only**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty.

- [ ] **Step 2: Targeted tests**

Run: `cd src-tauri && cargo test --lib page_dual_write 2>&1 | tail -10`
Run: `cd src-tauri && cargo test --lib gbrain::browse::tests::split_frontmatter 2>&1 | tail -10`
Run: `cd src-tauri && cargo test --lib memory_adapter 2>&1 | tail -10`
Expected: all green.

- [ ] **Step 3: clippy clean for touched files**

Run: `cd src-tauri && cargo clippy --lib 2>&1 | grep -E "^error" | head`
Expected: empty.

- [ ] **Step 4: GitNexus change check**

Run impact/change detection per CLAUDE.md before the PR: `gitnexus_detect_changes()` — confirm only the expected symbols (`split_frontmatter`, `dual_write_page`, `markdown_to_page`, `shadow_write_page`, the 4 site fns, `IngestionService::new`, `GbrainPolicyTarget::new`, `MemorizationService` setters) are affected.

---

## Adjacent-edit checklist (call out in the PR body, per CLAUDE.md)

- **No new Tauri command** (site A reuses an existing command) → no `invoke_handler!` change.
- **`IngestionService::new` + `GbrainPolicyTarget::new` signatures changed** → all callers updated (app.rs:915; runtime_memory_policy.rs:81). Confirm no other callers via `grep -rn "IngestionService::new\|GbrainPolicyTarget::new" src-tauri/src/`.
- **No migration** (no schema change).
- **`MemoryOsConfig` new field** is `#[serde(default)]` → backward-compatible with existing config files.

## PR shape

One branch `claude/p2a-gbrain-write-repoint`, one PR with a `## Commits (bisectable)` table (Tasks 1–7 = 7 commits). Title: `feat(memory): P2a-1 — gbrain write repoint (gated dual-write to adapter pages)`. Body notes: gbrain stays primary; gate `gbrain_dual_write_pages_enabled` default on; LLM `mcp__gbrain__put_page` write deferred to P2a-2 (must precede P2c); `GbrainAdapter::store` excluded (router gates it on non-default backend).

## Self-review notes

- **Spec coverage:** §1 core → Tasks 1–3; §2 sites A/B/C/D → Tasks 4–7; testing § from spec → Tasks 1/2/3 unit tests + Task 8; out-of-scope (site E, LLM tool, P2c/d) honored. ✔
- **Type consistency:** `dual_write_page` returns `Result<PageDetail, GbrainError>` everywhere (matches `browse::put_page`); `markdown_to_page`/`shadow_write_page` are `pub(crate)`; `pages::Page` fields used exactly as defined. ✔
- **Known follow-the-recon items** (flagged inline, not placeholders): exact runtime config-read idiom (Tasks 4/5), in-memory adapter constructor name (Task 2), `runtime_memory_policy` handle reachability (Task 6 fallback), ingestion reorder-vs-setter (Task 7 fallback), main.rs crate-path prefix (Task 5). Each has a concrete primary + fallback.
