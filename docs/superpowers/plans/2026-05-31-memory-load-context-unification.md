# Memory `load_context` Unification Implementation Plan (Sub-project A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Unify the chat turn's ~5-source memory-context assembly behind one router-level `load_context(query, budget, extra)` that recalls via the adapter stack (`bucket_seal` default + `gbrain`) instead of the `memory_graph` `MemoryRecallEngine`. Gated by config (default on), legacy path retained as a gated fallback. Fold in sub-project B's residual cleanup (poison-fallback + roster docs).

**Architecture:** Add `memory_adapter::router::load_context` + pure merge/dedup/budget/format helpers. Chat entry (`tauri_commands.rs`) calls it when `memory_os.unified_load_context_enabled` (default true), else runs the existing `MemoryRecallEngine` assembly verbatim. Proactive/session candidates are pre-fetched by the caller and passed in as `extra` (router stays decoupled from those services). `memory_graph`/`MemoryRecallEngine` is NOT deleted — retired in a future PR.

**Tech Stack:** Rust. No new deps. Spec: `docs/superpowers/specs/2026-05-31-memory-load-context-unification-design.md`.

---

## Source-of-truth references (verified)

- `memory_adapter/types.rs`: `MemoryEntry { id: String, key: String, content: String, namespace: Option<String>, category: MemoryCategory, timestamp: String, session_id: Option<String>, score: Option<f64> }` (11-28); `RecallOpts<'a> { namespace: Option<&'a str>, category: Option<MemoryCategory>, session_id: Option<&'a str>, min_score: Option<f64> }` (62-66).
- `memory_adapter/traits.rs:40`: `async fn recall(&self, query: &str, limit: usize, opts: RecallOpts<'_>) -> anyhow::Result<Vec<MemoryEntry>>`.
- `memory_adapter/router.rs`: `resolve_backend` (115) with poison-fallback `"legacy_kv"` at **line 125**; `route_recall` (182) with the same fallback at **line 195**. `route_recall_in` (137). `state.memory_adapters: Arc<HashMap<String, Arc<dyn MemoryAdapter>>>`; `state.default_memory_backend: Arc<RwLock<String>>` (= `"bucket_seal"`).
- `app.rs`: adapters registered under names `"legacy_kv"`, `"legacy_steward"`, `"bucket_seal"`, `"gbrain"`, `"memu"`.
- `memubot_config.rs`: `MemoryOsConfig` bool-field pattern — `#[serde(default = "default_X")] pub X: bool,` + `fn default_X() -> bool { ... }` + manual `Default` entry (mirror `edit_project_check_enabled` at 422/625).
- `tauri_commands.rs` chat entry memory assembly: ~**2270–2400** (`MemoryRecallEngine::format_recall_for_prompt` → `set_memory_context`; `session_memory_ctx`; `build_browser_task_memory_context`; proactive `prepare_background_context` → `append_memory_context`; `<user_preferences>` append). Plus a gbrain adapter recall at **1821** and another at **11085**. The delegate API: `set_memory_context(String)` / `append_memory_context(&str)` (dispatcher/mod.rs).

---

## CRITICAL facts

1. **Router stays decoupled** — `load_context` takes adapter map + `extra: Vec<MemoryEntry>`; it must NOT reach into `proactive_svc`/session. The caller pre-fetches those and converts to `MemoryEntry`.
2. **Default ON + gated fallback** — `unified_load_context_enabled` default `true`. When `false`, the existing `MemoryRecallEngine` 5-source block runs **verbatim** (preserved, not deleted). The legacy path is the rollback.
3. **`<user_preferences>` + browser-task stay separate** — they are appended independently in BOTH the enabled and disabled paths. Only the recall-class sources move into `load_context`.
4. **`memory_graph`/`MemoryRecallEngine` NOT deleted** here — only bypassed when enabled. Retirement is a future PR.
5. **B residual is task 1** — poison-fallback `legacy_kv`→`bucket_seal` (router.rs:125 + 195) + roster docs + legacy deprecation notes.
6. **Pre-commit hooks** — no `--no-verify`; legacy adapters keep working (don't break explicit-namespace routing).

---

## File Structure

| File | Change | LoC |
|---|---|---|
| `memory_adapter/router.rs` | `load_context` + pure helpers (`merge_dedupe_budget`, `format_entries`) + tests; poison-fallback → `bucket_seal` (2 sites) | ~+140 (incl tests) |
| `memory_adapter/mod.rs` | roster end-state doc block (B) | ~+15 |
| `memory_adapter/legacy_kv.rs`, `legacy_steward.rs` | deprecation doc notes (B) | ~+6 |
| `memubot_config.rs` | `unified_load_context_enabled` (default true) + 2 tests | ~+20 |
| `tauri_commands.rs` | chat entry: gated `load_context` one-liner + retained fallback; pre-fetch proactive/session as `extra` | ~+40 |

---

## Tasks

### Task 1: B residual cleanup (poison-fallback + roster docs)

**Files:** `memory_adapter/router.rs`, `memory_adapter/mod.rs`, `memory_adapter/legacy_kv.rs`, `memory_adapter/legacy_steward.rs`.

- [ ] **Step 1: Poison-fallback → `bucket_seal`.** In `router.rs`, the two `.unwrap_or_else(|| "legacy_kv".to_string())` sites (lines ~125 in `resolve_backend` and ~195 in `route_recall`) read `default_memory_backend`; on RwLock poison they fall back to `"legacy_kv"`. Change BOTH to `"bucket_seal"` (the canonical default). Add a comment: `// poison-fallback to the canonical default, not the legacy adapter`.

- [ ] **Step 2: Test the fallback** (router.rs tests): a test that `resolve_backend_in(&adapters, "bucket_seal", None, "global")` resolves to `bucket_seal` (the existing tests use `"legacy_kv"` as the passed default; add one asserting bucket_seal is resolvable as default). Keep existing `resolve_backend_in` tests green (they pass an explicit default, unaffected).

- [ ] **Step 3: Roster end-state doc.** In `memory_adapter/mod.rs`, add a doc block:
```rust
//! ## Backend roster (end-state, 2026-05-31)
//! - `bucket_seal` — **canonical default** (openhuman bucket-seal port); `default_memory_backend`.
//! - `gbrain` — retained: chat/MCP recall surface.
//! - `memu` — retained: item-based memory (memU bridge).
//! - `legacy_kv` / `legacy_steward` — **deprecated**; reachable only by explicit
//!   `legacy_kv:`/`legacy_steward:` namespace prefix. Data migration + removal
//!   deferred to a future effort. Do not route new writes here.
```

- [ ] **Step 4: Deprecation notes.** Add to the module docs of `legacy_kv.rs` and `legacy_steward.rs` a line: `//! DEPRECATED (2026-05-31): retained for explicit-namespace back-compat only; see memory_adapter/mod.rs roster. New code must not route here.` (Use a doc note, NOT `#[deprecated]` on the type — the adapters are still constructed in app.rs and a `#[deprecated]` attribute would spam build warnings at those live construction sites.)

- [ ] **Step 5: Build + test + commit.**
```bash
cd src-tauri && cargo test --lib memory_adapter::router 2>&1 | tail; cargo build 2>&1 | grep -E "^error" | head
git add -A && git commit -m "feat(memory): poison-fallback → bucket_seal + roster end-state docs (A.1 / B residual)"
```

### Task 2: `unified_load_context_enabled` config

**Files:** `memubot_config.rs`.

- [ ] **Step 1: Write failing tests** (mirror `memory_os_default_project_check_fields`):
```rust
#[test]
fn memory_os_default_unified_load_context_enabled_true() {
    assert!(MemoryOsConfig::default().unified_load_context_enabled);
}
#[test]
fn memory_os_deserializes_without_unified_load_context_field() {
    let json = r#"{"memory_os":{"entity_page_enabled":true}}"#;
    let cfg: MemubotConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.memory_os.unified_load_context_enabled); // serde default
}
```

- [ ] **Step 2: Run → red.** `cd src-tauri && cargo test --lib memubot_config 2>&1 | tail`.

- [ ] **Step 3: Implement.** Add to `MemoryOsConfig`:
```rust
/// When true (default), the chat turn assembles memory context via the unified
/// `memory_adapter::router::load_context` (adapter recall). When false, falls
/// back to the legacy `MemoryRecallEngine` 5-source assembly. Off = instant rollback.
#[serde(default = "default_unified_load_context_enabled")]
pub unified_load_context_enabled: bool,
```
+ `fn default_unified_load_context_enabled() -> bool { true }` (next to the other `default_*` fns) + the entry `unified_load_context_enabled: true,` in `impl Default for MemoryOsConfig`.

- [ ] **Step 4: Run → green + commit.**
```bash
cd src-tauri && cargo test --lib memubot_config 2>&1 | tail
git add -A && git commit -m "feat(config): unified_load_context_enabled (default true) (A.2)"
```

### Task 3: router `load_context` + pure helpers

**Files:** `memory_adapter/router.rs`.

- [ ] **Step 1: Write failing tests** (router.rs tests):
```rust
#[test]
fn merge_dedupe_budget_sorts_dedups_and_truncates() {
    let entries = vec![
        mk_entry("a", "high score fact", Some(0.9)),
        mk_entry("b", "low score fact", Some(0.2)),
        mk_entry("a2", "high score fact", Some(0.5)), // dup content of a
        mk_entry("c", "mid fact", Some(0.6)),
    ];
    // budget large → dedup by content keeps highest-score, sorted desc
    let out = merge_dedupe_budget(entries.clone(), 10_000);
    assert_eq!(out.len(), 3); // a/a2 deduped
    assert!(out[0].score.unwrap() >= out[1].score.unwrap()); // sorted desc
    assert!(out.iter().all(|e| e.content != "" ));
    // tiny budget → truncated to fit
    let tiny = merge_dedupe_budget(entries, 20);
    let chars: usize = tiny.iter().map(|e| e.content.chars().count()).sum();
    assert!(chars <= 20);
}
#[test]
fn format_entries_empty_is_empty_string() {
    assert_eq!(format_entries(&[]), "");
}
#[test]
fn format_entries_renders_content() {
    let out = format_entries(&[mk_entry("a", "remember X", Some(0.9))]);
    assert!(out.contains("remember X"));
}
```
Add a test helper `fn mk_entry(id: &str, content: &str, score: Option<f64>) -> MemoryEntry` constructing a `MemoryEntry` with the given fields (key=id, namespace=None, category=default, timestamp="", session_id=None). (Check `MemoryCategory`'s default/variants in types.rs to pick a valid value.)

- [ ] **Step 2: Run → red.**

- [ ] **Step 3: Implement** in `router.rs`:
```rust
/// Merge recall candidates from all sources: dedup by `content` (keep the
/// highest-scoring duplicate), sort by `score` descending (None treated as 0.0),
/// then truncate so the cumulative `content` char count stays within `budget`.
pub fn merge_dedupe_budget(mut entries: Vec<MemoryEntry>, budget: usize) -> Vec<MemoryEntry> {
    // dedup by content, keep highest score
    entries.sort_by(|a, b| {
        b.score.unwrap_or(0.0).partial_cmp(&a.score.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert(e.content.clone()));
    // budget truncation (char-based)
    let mut used = 0usize;
    entries
        .into_iter()
        .take_while(|e| {
            let n = e.content.chars().count();
            if used + n <= budget { used += n; true } else { false }
        })
        .collect()
}

/// Render budgeted entries into a prompt block (coarse-to-fine: highest-score
/// first). Empty input → empty string.
pub fn format_entries(entries: &[MemoryEntry]) -> String {
    if entries.is_empty() { return String::new(); }
    let mut s = String::from("<memory_context>\n");
    for e in entries {
        s.push_str("- ");
        s.push_str(&e.content);
        s.push('\n');
    }
    s.push_str("</memory_context>");
    s
}

/// Unified recall + assembly. Recalls via the adapter router (default backend +
/// gbrain), merges with caller-supplied `extra` candidates (proactive/session),
/// dedups/budgets/formats. Best-effort: a failing backend contributes nothing.
/// Router stays decoupled from proactive/session services — they arrive as `extra`.
pub async fn load_context(
    adapters: &std::collections::HashMap<String, std::sync::Arc<dyn MemoryAdapter>>,
    default_backend: &str,
    query: &str,
    budget: usize,
    extra: Vec<MemoryEntry>,
) -> String {
    let mut all = extra;
    for name in [default_backend, "gbrain"] {
        if let Some(ad) = adapters.get(name) {
            match ad.recall(query, 6, RecallOpts::default()).await {
                Ok(mut hits) => all.append(&mut hits),
                Err(e) => tracing::debug!(backend = name, error = %e, "load_context: recall failed; skipping source"),
            }
        }
    }
    let budgeted = merge_dedupe_budget(all, budget);
    format_entries(&budgeted)
}
```
(Confirm `RecallOpts` derives `Default`; if not, construct it explicitly `RecallOpts { namespace: None, category: None, session_id: None, min_score: None }`. Dedup the `[default_backend, "gbrain"]` list if `default_backend == "gbrain"` to avoid double-recall — add `.filter(...)` or a small `seen` set.)

- [ ] **Step 4: Run → green + commit.**
```bash
cd src-tauri && cargo test --lib memory_adapter::router 2>&1 | tail
git add -A && git commit -m "feat(memory): router load_context + merge/dedupe/budget/format helpers (A.3)"
```

### Task 4: wire chat entry (gated)

**Files:** `tauri_commands.rs`.

- [ ] **Step 1: RECON** the chat-entry memory block (~2270–2400) + the gbrain recall sites (1821, 11085). Identify: where `MemoryRecallEngine` recall + `session_memory_ctx` + proactive `prepare_background_context` produce their strings/candidates, and where `set_memory_context`/`append_memory_context` are called. Note the `<user_preferences>` + `build_browser_task_memory_context` appends (these STAY).

- [ ] **Step 2: Gate the recall-class assembly.** Wrap the recall-class assembly in:
```rust
let unified = state.memubot_config.read().await.memory_os.unified_load_context_enabled;
if unified {
    // Pre-fetch proactive + session candidates as Vec<MemoryEntry> (convert from
    // their current shapes — reuse whatever the existing block fetches), then:
    let budget = /* existing token/char budget used by MemoryRecallEngine, or a const ~8000 chars */;
    let ctx = crate::memory_adapter::router::load_context(
        &state.memory_adapters,
        &state.default_memory_backend.read().map(|g| g.clone()).unwrap_or_else(|_| "bucket_seal".into()),
        &input.content,
        budget,
        extra_candidates, // proactive + session as Vec<MemoryEntry>
    ).await;
    if !ctx.is_empty() { delegate.set_memory_context(ctx); }
} else {
    // EXISTING MemoryRecallEngine 5-source assembly — moved here verbatim, unchanged.
}
// <user_preferences> + browser-task appends run in BOTH branches (leave them after the if/else).
```
Adapt to the real locals. If converting proactive/session into `MemoryEntry` is awkward, pass them as already-formatted strings folded into `extra` via a single `MemoryEntry { content: <their formatted string>, score: Some(0.5), .. }` wrapper so they still flow through the budget — OR (simpler, acceptable) keep proactive/session as separate `append_memory_context` calls (like user_prefs) and have `load_context` cover only the adapter recall sources. **Pick the simpler that preserves behavior; document which in the commit.** The non-negotiable: when `unified`, the `MemoryRecallEngine`/`memory_graph` recall does NOT run; when `!unified`, the old block runs verbatim.

- [ ] **Step 3: Build + targeted check.** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`. Add/adjust any compile-driven fixes.

- [ ] **Step 4: Commit.**
```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(memory): chat entry uses unified load_context when enabled; legacy gated fallback (A.4)"
```

### Task 5: Verification

- [ ] `cd src-tauri && cargo test --lib memory_adapter 2>&1 | tail` (router load_context + helpers + fallback tests pass).
- [ ] `cargo test --lib memubot_config 2>&1 | tail` (config tests pass).
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo test --lib agent 2>&1 | tail -6` — net green; only the 2 known pre-existing failures (`shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long`).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "memory_adapter|tauri_commands|memubot_config" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **Fallback no-regression:** with `unified_load_context_enabled = false`, the chat path runs the legacy `MemoryRecallEngine` assembly (behavior preserved).
- [ ] **Enabled path:** `unified = true` → memory_context comes from `load_context` (adapter recall), `memory_graph` recall NOT invoked; `<user_preferences>` still present.
- [ ] **Legacy routing intact:** explicit `legacy_kv:`/`legacy_steward:` namespace still resolves (B didn't break it).

---

## Self-Review

- ✅ **Spec coverage:** B residual (Task 1); config gate (Task 2); router `load_context` + helpers (Task 3); chat-entry gated wiring + retained fallback + separate user_prefs/browser (Task 4); verification incl. fallback + enabled + legacy-routing (Task 5). `memory_graph` deletion explicitly deferred; C deferred.
- ✅ **Placeholder scan:** full code for helpers/config/B; Task 4 is a recon-and-transform with an explicit contract + a named simpler-fallback (proactive/session as separate appends if MemoryEntry conversion is awkward) — no vague "handle it".
- ✅ **Type consistency:** `MemoryEntry`/`RecallOpts` per types.rs; `load_context(adapters, default_backend, query, budget, extra) -> String`; `merge_dedupe_budget(Vec<MemoryEntry>, usize) -> Vec<MemoryEntry>`; `format_entries(&[MemoryEntry]) -> String`; config `unified_load_context_enabled: bool`.
- ✅ **Risk-scaled:** medium — recall-backend switch behind a default-on flag with a verbatim legacy fallback (instant rollback); router decoupled from proactive/session; one branch, bisectable. Task 4 is the integration-risk task — isolated to the chat entry, gated, with the old block preserved.
- Decisions: load_context router-level (not trait); proactive/session via `extra` (decoupled) or separate appends (simpler fallback); poison-fallback → bucket_seal; legacy adapters doc-deprecated not `#[deprecated]` (avoid build-warning spam at live construction sites); memory_graph retirement deferred.
