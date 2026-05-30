# 阶段 4 PR14 — GbrainAdapter Design Spec

**Status:** Approved design — pending user review gate before plan.
**Date:** 2026-05-30
**Position in 阶段 4 sequence:** PR14 of 15. Follows PR13 (job queue). Precedes PR15 (recall routing in `effective_system_prompt`).

---

## 1. Goal

Wrap gbrain — uClaw's long-term, page-oriented knowledge graph (an MCP server over a CLI transport) — as a `GbrainAdapter` behind the `MemoryAdapter` trait, so it's reachable via the unified `memory.unified.*` IPC + the PR15 recall router alongside `bucket_seal` / `legacy_kv` / `legacy_steward`. Closes the gap-audit §1.5 goal of "every memory store behind one trait" and the 阶段 4 design's "5 concrete impls" (BucketSeal, LegacyKv, LegacySteward, **Gbrain**, MemU).

The adapter is a **marshalling layer** over the existing, tested `gbrain::browse::*` helpers — it does not reimplement the MCP/CLI transport. It maps the KV-shaped trait surface onto gbrain's page-shaped model (slugs, pages, search).

**Out of scope:**
- No change to the agent's own `put_page` path (the agent keeps writing pages directly; the adapter writes under a distinct `{namespace}/` slug prefix).
- No new gbrain MCP tools; no `delete` tool added to gbrain (its allowlist has none).
- No recall-router wiring → PR15. PR14 only makes gbrain *reachable* via the registry.
- MemUAdapter → a later effort (the design's 5th impl); not PR14.

---

## 2. The lossy mapping (KV trait ↔ page graph)

gbrain's bundled tool allowlist: `search`, `query`, `list_pages`, `think`, `get_page`, `put_page` — a wiki-style knowledge graph keyed by **slug**, with no delete. The `MemoryAdapter` trait is KV-shaped (store/recall/get/list/delete/clear_namespace/namespace_summaries keyed by `(namespace, key)`). The adapter bridges them:

| `MemoryAdapter` method | gbrain call | Notes |
|---|---|---|
| `store(ns, key, content, cat, session)` | `put_page(slug, content)` | `slug = "{ns}/{key}"`; category+session folded into a frontmatter line so `get` recovers them. Append/overwrite by slug (gbrain put_page re-fetches the page). |
| `recall(query, limit, opts)` | `search(query, limit)` | `SearchHit → MemoryEntry` (content=snippet, score=Some(similarity)). If `opts.namespace`, keep only slugs with the `"{ns}/"` prefix. `opts.min_score` filters by similarity. |
| `get(ns, key)` | `get_page("{ns}/{key}")` | `PageDetail → MemoryEntry` (content=compiled_truth, timestamp=updated_at). page-not-found → `Ok(None)`. |
| `list(ns, cat, session)` | `list_pages(...)` | filter to the `"{ns}/"` slug prefix; `PageSummary → MemoryEntry`. category/session filters applied if recoverable. |
| `namespace_summaries()` | `list_pages(...)` | group slugs by first segment → one `NamespaceSummary` per prefix (count + latest `updated_at`). |
| `delete(ns, key)` | — | gbrain has no delete tool → **graceful no-op**: `Ok(false)` + `warn`. |
| `clear_namespace(ns)` | — | same → `Ok(0)` + `warn`. |
| `name()` | — | `"gbrain"`. |

**slug ↔ (namespace, key)**: `slug = format!("{namespace}/{key}")`. To recover, split on the **first** `/`: namespace = before, key = after. (MemoryAdapter namespaces are flat strings, so first-`/` split is unambiguous. A slug with no `/` — e.g. an agent-written page — maps to `namespace=None, key=slug`.) This keeps the adapter's writes in a **distinct slug space** from the agent's own `put_page` slugs → no collision/duplication with agent-maintained pages.

---

## 3. Components

### 3.1 `GbrainAdapter` — `memory_adapter/gbrain.rs`

```rust
pub struct GbrainAdapter {
    mcp: SharedMcpManager,   // = Arc<RwLock<McpManager>>; reused by gbrain::browse::*
}

impl GbrainAdapter {
    pub fn new(mcp: SharedMcpManager) -> Self { Self { mcp } }
}

#[async_trait]
impl MemoryAdapter for GbrainAdapter {
    fn name(&self) -> &str { "gbrain" }
    // store / recall / get / list / delete / clear_namespace / namespace_summaries
    // — each delegates to gbrain::browse::* + a pure marshalling fn (below).
}
```

- Holds a clone of the app's `mcp_manager`. All gbrain ops go through `gbrain::browse::{put_page, search, get_page, list_pages}` which take `&SharedMcpManager` and return typed results over the CLI-transport marshalling.
- `GbrainError → anyhow::Error` at the boundary (`.map_err(|e| anyhow::anyhow!("gbrain: {}", e.to_command_string()))`).

### 3.2 Pure marshalling functions (the unit-test surface)

Kept as free `fn`s so they're testable without an MCP server:

```rust
fn slug_for(namespace: &str, key: &str) -> String;            // "{ns}/{key}"
fn split_slug(slug: &str) -> (Option<String>, String);        // (namespace, key) on first '/'
fn search_hit_to_entry(hit: &SearchHit) -> MemoryEntry;       // id=slug, content=snippet, score=Some(similarity)
fn page_detail_to_entry(p: &PageDetail) -> MemoryEntry;       // content=compiled_truth, timestamp=updated_at
fn page_summary_to_entry(s: &PageSummary) -> MemoryEntry;     // content=title, timestamp=updated_at
fn summaries_from_pages(pages: &[PageSummary]) -> Vec<NamespaceSummary>;  // group by first slug segment
fn build_page_body(content: &str, category: &MemoryCategory, session: Option<&str>) -> String;  // frontmatter + content
```

- `MemoryEntry` fields populated: `id` = slug, `key` = split key, `namespace` = `Some(ns)` from split, `content`, `category` (default `Conversation` for read-back unless recovered from frontmatter), `timestamp` (RFC3339 from `updated_at`, or `now` fallback), `score` (Some(similarity) for recall, None otherwise).
- `build_page_body` prepends a minimal frontmatter line (e.g. `<!-- uclaw: category=core session=abc -->` or a YAML stub) so category/session survive a round-trip; `page_detail_to_entry` parses it back (best-effort — default `Conversation` if absent).

### 3.3 Availability / error semantics

- gbrain seeds late (`AppState.gbrain_mcp_id` is `Some("gbrain")` after `seed_bundled_gbrain`, `None` when bun/gbrain binaries are missing). The adapter is **always registered**; it does not check availability at construction.
- When gbrain is absent/disconnected, `gbrain::browse::*` calls fail → the adapter returns `Err` for read/write methods. The unified IPC + PR15 recall router treat a backend that errors as **skippable** (degrade gracefully — gbrain simply contributes nothing).
- `delete`/`clear_namespace` are the only intentional `Ok` results when the op can't be performed (no-op, since gbrain can't delete) — they must not hard-fail a cross-backend clear.

### 3.4 Wiring — `app.rs`

In the memory-adapter registry construction (where `legacy_kv` / `legacy_steward` / `bucket_seal` are registered):

```rust
let gbrain_adapter = std::sync::Arc::new(
    crate::memory_adapter::GbrainAdapter::new(mcp_manager.clone()),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
memory_adapters_map.insert(gbrain_adapter.name().to_string(), gbrain_adapter);
```

- `mcp_manager` is already an AppState field (`SharedMcpManager`), in scope at registry construction.
- No default-backend change (bucket_seal stays default from PR9). gbrain is opt-in via explicit `backend: "gbrain"` on the unified IPC, or included by the PR15 router's multi-backend recall.

### 3.5 `memory_adapter/mod.rs`

`pub mod gbrain;` + `pub use gbrain::GbrainAdapter;`.

---

## 4. Error handling

| Situation | Behavior |
|---|---|
| gbrain not seeded / disconnected | read/write methods → `Err` (caller skips the backend). |
| page-not-found on `get` | `Ok(None)`. |
| `search`/`list` returns empty | `Ok(vec![])`. |
| `delete`/`clear_namespace` | `Ok(false)` / `Ok(0)` + `warn` (gbrain can't delete). |
| `put_page` transport error | `Err` (store failed; caller decides — best-effort callers log+continue). |
| malformed gbrain JSON | surfaced as `GbrainError` from `browse::parse_*` → `Err`. |

Guiding principle (consistent with PR12/PR13): a degraded/absent gbrain never breaks the unified memory surface — it contributes nothing and other backends carry on.

---

## 5. Testing

Hermetic — no live gbrain server. Unit-test the **pure marshalling functions** directly with constructed `SearchHit`/`PageDetail`/`PageSummary` values (the MCP round-trip is already covered by `gbrain::browse`'s own tests + the live CLI transport):

| Test | Asserts |
|---|---|
| `slug_for` / `split_slug` round-trip | `("ns","key") → "ns/key" → ("ns","key")`; no-slash slug → `(None, slug)`; namespace with no key edge cases. |
| `search_hit_to_entry` | id=slug, content=snippet, score=Some(similarity), namespace split. |
| `page_detail_to_entry` | content=compiled_truth, timestamp from updated_at, key/namespace split. |
| `build_page_body` + parse-back | category+session survive the frontmatter round-trip; absent → default Conversation. |
| `summaries_from_pages` | groups by first slug segment, counts, latest updated_at per namespace. |
| `name()` | `"gbrain"`. |

~7-8 tests. No `FakeMcp` needed — the trait methods that call `browse::*` are thin delegations over the tested pure fns; an integration test against a live gbrain is out of scope for CI.

---

## 6. Scope boundaries

- **No agent `put_page` change** — the agent's direct knowledge-graph writes are untouched; the adapter writes under a `{namespace}/` prefix (distinct slug space).
- **No gbrain delete tool** — `delete`/`clear` stay no-ops.
- **No recall-router wiring** — PR15. PR14 registers gbrain; it's reachable via explicit `backend:"gbrain"` but not yet auto-included in recall.
- **No default-backend change** — bucket_seal remains default.
- **No new MCP transport / config** — reuses the existing `mcp_manager` + bundled gbrain seed.
- **No MemUAdapter** — separate later effort.

---

## 7. File plan (preview — detailed in the implementation plan)

| File | New/Mod | Purpose |
|---|---|---|
| `memory_adapter/gbrain.rs` | new | `GbrainAdapter` + pure marshalling fns + tests |
| `memory_adapter/mod.rs` | mod | `pub mod gbrain;` + re-export |
| `app.rs` | mod | register `GbrainAdapter` in the registry |

Est. ~300 source + ~120 tests.

---

## 8. Open adaptation questions (resolved at implementation time)

1. **`put_page` frontmatter format** — verify what `gbrain::browse::put_page` expects for `content` (full markdown incl. frontmatter, per its doc) and whether a `<!-- -->` comment vs a YAML frontmatter block round-trips cleanly through `get_page`'s `compiled_truth`/`frontmatter`/`raw_markdown` fields. Use whichever survives; if frontmatter is stripped from `compiled_truth`, read it from `PageDetail.frontmatter` / `tags` instead.
2. **`search` signature** — confirm `gbrain::browse::search(mcp, query, limit, offset?)` arg list (the `gbrain_search` command passes limit+offset).
3. **`MemoryEntry` exact fields** — read `memory_adapter/types.rs` for `timestamp`/`score`/`session_id` presence; PR9/PR12 established `namespace: Option<String>`, `timestamp: String`, `score: Option<f64>`.
4. **`list_pages` namespace filter** — whether to pass a server-side filter (page_type/tag) or filter client-side by slug prefix. Client-side prefix filter is simplest + matches the slug convention.
5. **Registry construction site** — confirm whether `mcp_manager` is available at the exact point the adapters map is built in `app.rs` (it's built at line ~594, before adapter registration).

---

## 9. Success criteria

- `GbrainAdapter` registered under `"gbrain"`; reachable via `memory.unified.*` with explicit `backend:"gbrain"`.
- store→put_page, recall→search, get→get_page, list→list_pages, namespace_summaries all marshal correctly (asserted via pure-fn tests).
- delete/clear are graceful no-ops; an absent gbrain returns Err (skippable) without breaking other backends.
- The adapter's slug space (`{namespace}/`) doesn't collide with agent-written pages.
- All existing tests stay green; ~7-8 new tests pass. CI hermetic (no live gbrain).
