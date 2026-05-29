# 阶段 4 — Memory Adapter + openhuman Bucket-Seal Tree Port · Design

> **Status:** Approved design (brainstorming session 2026-05-29). Supersedes the pre-brainstorming recon at [`2026-05-29-stage4-memory-adapter-recon.md`](2026-05-29-stage4-memory-adapter-recon.md). Next step: invoke `superpowers:writing-plans` to produce per-PR implementation plans.

## Goal

Close gap-audit §1.5 ("8 parallel memory stores, freeze decorative") by introducing **`MemoryAdapter`** — the single contract every memory backend implements — and porting **openhuman's full bucket-seal tree** (`src/openhuman/memory/tree/`) as the new **primary** backend. All existing in-process backends + gbrain MCP + optional memU wrap as legacy adapters behind the trait. The new IPC layer (`memory.unified.*`) routes via the trait; existing `memory_*` and `memory_graph_*` Tauri commands stay stable so the UI can migrate at its own pace.

This is medium-scope: ~12-15 PRs over 4-6 weeks. It does **not** retire any legacy backend within this stage (that's a follow-up consolidation stage once production callers have migrated).

## Reference sources

- Pi-convergence gap audit §1.5: [`2026-05-27-pi-convergence-gap-audit.md`](2026-05-27-pi-convergence-gap-audit.md)
- Re-audit confirming §1.5 still open: [`2026-05-29-stage3-closeout-gap-reaudit.md`](2026-05-29-stage3-closeout-gap-reaudit.md)
- Pre-brainstorming recon: [`2026-05-29-stage4-memory-adapter-recon.md`](2026-05-29-stage4-memory-adapter-recon.md)
- Philosophy ADR (target state): [`docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`](../../adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md)
- Source-of-truth port reference: `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/` (local checkout). Pin to whatever commit is current at PR1 start; record the SHA in PR descriptions for reproducibility.

## Decisions captured during brainstorming

| Topic | Decision |
|---|---|
| Stage 4 scope | Option B (medium) — trait + bucket-seal as **new primary backend**, old backends coexist |
| Port depth | B1 — **full** bucket-seal tree (canonicalize + chunker + content_store + score + 3 trees + jobs + retrieval) |
| gbrain | **Wrap as `GbrainAdapter`** behind the trait |
| memory.rs + memory_graph | **Wrap as `LegacyKvAdapter` + `LegacyStewardAdapter`** |
| memU | **Wrap as `MemUAdapter`** |
| Trait shape | Approach α — **faithful mirror** of openhuman's `Memory` trait (8 methods: name + 7 async CRUD/recall) |
| Recall routing | **Single backend per call** (no cross-backend fan-out merge in 阶段 4) |
| IPC backward compat | **Keep** existing `memory_*` + `memory_graph_*` Tauri commands. **Add** new `memory.unified.*` layer routing through trait. UI migrates incrementally |
| "Freeze" semantic | Bucket-seal's native L0→L1→L2 cascade-seal **replaces** the decorative freeze for new writes; legacy backends keep `enforce_freeze` warn-only |
| `importance_decay` regression | Deferred to follow-up stage; no decision in 阶段 4 |

## Trait surface

Defined in new module `src-tauri/src/memory_adapter/mod.rs`. Mirror openhuman's `Memory` trait verbatim where possible — same method names, same signatures (adapted for uClaw's error type), same supporting types.

```rust
// src-tauri/src/memory_adapter/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    #[serde(default)]
    pub namespace: Option<String>,
    pub category: MemoryCategory,
    pub timestamp: String,
    pub session_id: Option<String>,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Core,
    Daily,
    Conversation,
    Custom(String),
}

#[derive(Debug, Default, Clone)]
pub struct RecallOpts<'a> {
    pub namespace: Option<&'a str>,
    pub category: Option<MemoryCategory>,
    pub session_id: Option<&'a str>,
    pub min_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceSummary {
    pub namespace: String,
    pub count: usize,
    pub last_updated: Option<String>,
}

#[async_trait]
pub trait MemoryAdapter: Send + Sync {
    /// Backend identifier — e.g. `"bucket_seal"`, `"legacy_kv"`, `"gbrain"`.
    fn name(&self) -> &str;

    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>>;

    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool>;

    async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64>;

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>>;
}
```

**`AppState` registry:**

```rust
// src-tauri/src/app.rs (additions)
use std::collections::HashMap;
use std::sync::Arc;

pub struct AppState {
    // ...existing fields...
    pub memory_adapters: Arc<HashMap<String, Arc<dyn MemoryAdapter>>>,
    pub default_memory_backend: Arc<std::sync::RwLock<String>>,  // e.g. "bucket_seal"
}
```

The map is built once at `AppState::new` time. Adding new adapters in the future is a one-line registration plus the impl.

## Backend roster

| Adapter | Source | Workspace dir | Role |
|---|---|---|---|
| `BucketSealAdapter` | new (port of openhuman `tree/`) | `<DATA_DIR>/memory_bucket/` | Primary; default for new writes |
| `LegacyKvAdapter` | wraps `crate::memory::MemoryStore` | `<DATA_DIR>/memory.db` (existing) | KV read/write; tarpit until UI migrates |
| `LegacyStewardAdapter` | wraps `crate::memory_graph::MemoryGraphStore` | `<DATA_DIR>/memory_graph/` (existing) | Pages/links/importance; writes still warn-only on `enforce_freeze` |
| `GbrainAdapter` | wraps `mcp__gbrain__*` MCP tool calls | external (MCP server) | Long-term knowledge graph |
| `MemUAdapter` | wraps `crate::memU::MemUClient` | external (Python service) | Optional; `None` when memU unavailable |

Synthesizers (`wiki_synthesizer`, `lint_analyzer`, `entity_synthesizer`) and the fs watcher (`brain_watcher`) stay separate — they're memory *augmenters*, not backends, and don't fit the `MemoryAdapter` shape.

## Recall routing

- IPC: `memory.unified.recall(backend: Option<String>, query, limit, opts)` looks up the named backend in `AppState.memory_adapters` (or falls back to the default) and calls its `recall`.
- Agent loop: `effective_system_prompt → memory_context` flows through a new `crate::memory_adapter::route_recall(state, namespace, query, opts)` helper. It selects ONE backend by:
  1. Explicit namespace prefix (`bucket_seal:`, `gbrain:`, `legacy:`) overrides selection.
  2. Otherwise, fall back to `state.default_memory_backend`.
- **No cross-backend fan-out** in 阶段 4. Multi-backend `union by score` is a future stage option if needed.

This keeps the trait surface simple and lets each backend's recall semantics stay native. Callers that want multi-backend results call `recall` multiple times explicitly.

## Bucket-seal port — `BucketSealAdapter` impl

Port openhuman's `src/openhuman/memory/tree/` (~10K LoC) as `src-tauri/src/memory_bucket_seal/`. Layer-by-layer fidelity:

```text
memory_bucket_seal/
├── mod.rs                      pub fn re-exports + BucketSealAdapter facade
├── traits.rs                   internal helpers
├── canonicalize.rs             normalise inputs to Markdown + provenance
├── chunker.rs                  deterministic IDs, ≤3k-token bounded segments
├── content_store/
│   ├── atomic.rs               stage_chunk / stage_summary (atomic file ops)
│   ├── paths.rs                slug + workspace path layout
│   └── store.rs                chunks SQLite DB schema + queries
├── score/
│   ├── embed.rs                local Ollama / remote provider
│   ├── extract.rs              entity extraction LLM
│   └── resolver.rs             canonicalize entities
├── tree_source/                per-source rolling buffer
│   ├── store.rs / types.rs / registry.rs
│   ├── bucket_seal.rs          THE core: append_leaf + cascade-seal
│   └── summariser.rs           pluggable Summariser trait
├── tree_topic/                 per-entity summary trees (hotness-driven)
├── tree_global/                single daily global digest
└── jobs/                       background worker pool
    ├── queue.rs                Seal + embedding job queues
    ├── worker.rs               Tokio task draining jobs
    └── scheduler.rs            Daily-digest scheduling
```

**Adaptations from openhuman → uClaw:**

- Error type: `anyhow::Result<T>` → matches openhuman; can co-exist with uClaw's `crate::error::Error` via `?` and `From` impls.
- Config: replace `crate::openhuman::config::Config` reads with uClaw's `MemubotConfig` (already in `src-tauri/src/memubot_config.rs`). Add `memory_bucket_seal: BucketSealConfig` substruct.
- Workspace path: `~/.openhuman` → `<DATA_DIR>/memory_bucket/`. Configurable via `MemubotConfig.memory_bucket_seal.workspace_dir`.
- Embedder: openhuman uses its own `local_ai/` module. uClaw uses `crate::agent::memory_graph::memory_os_llm::MemoryOsLlm` for similar work. The port can either:
  1. Carry openhuman's `score/embed.rs` and route via uClaw's existing Ollama/Anthropic providers (preferred).
  2. Pluck only the algorithm and reimplement on top of uClaw's existing providers.
  
  **Decision:** carry openhuman's `embed.rs` verbatim and inject a uClaw-provided `EmbeddingProvider` trait at the boundary. PR7 lands the embed module + the trait + uClaw's default provider impl.
- Summariser: openhuman uses an LLM-backed `Summariser` trait with a fallback `InertSummariser` (deterministic, no LLM). uClaw ports the trait + InertSummariser + adds a uClaw-LLM-backed impl using the existing `MemoryOsLlm` interface.
- Background jobs: openhuman's job worker pool uses tokio + a SQLite job queue. Ported as-is. Workers spawn at boot from `AppState::new`.

**Schema:** new SQLite DB at `<DATA_DIR>/memory_bucket/chunks.db`. Schema mirrors openhuman's `mem_tree_chunks` + summary tables. Use uClaw's migration framework (`src/db/migrations.rs`) to manage version. **Schema is its own file**, NOT part of the main `<DATA_DIR>/uclaw.db` — bucket-seal is self-contained.

**Provenance:** every chunk carries `(source_kind, source_id, ingested_at, provenance_metadata)` per openhuman. UI surfaces this for "where did this come from?" affordances.

## IPC layer

Existing IPC commands stay unchanged:
- `memory_set`, `memory_get`, `memory_search`, `memory_list`, `memory_clear`, `memory_bulk_import`, `memory_count` — keep routing to `LegacyKvAdapter` via existing handlers.
- `memory_graph_*` (search, list, get_page, etc.) — keep routing to `LegacyStewardAdapter`.

**New IPC family** `memory.unified.*`:

```rust
#[tauri::command]
async fn memory_unified_recall(
    state: tauri::State<'_, AppState>,
    backend: Option<String>,          // None → state.default_memory_backend
    query: String,
    limit: usize,
    namespace: Option<String>,
    category: Option<String>,
    session_id: Option<String>,
    min_score: Option<f64>,
) -> Result<Vec<MemoryEntryDto>, String>;

#[tauri::command]
async fn memory_unified_record(
    state: tauri::State<'_, AppState>,
    backend: Option<String>,
    namespace: String,
    key: String,
    content: String,
    category: String,
    session_id: Option<String>,
) -> Result<(), String>;

// plus: memory_unified_get / list / delete / clear_namespace / namespace_summaries
//       memory_unified_list_backends / set_default_backend

// bucket-seal-specific introspection:
#[tauri::command]
async fn memory_bucket_seal_stats(...) -> Result<BucketSealStatsDto, String>;
#[tauri::command]
async fn memory_bucket_seal_list_chunks(...) -> Result<Vec<ChunkDto>, String>;
#[tauri::command]
async fn memory_bucket_seal_fetch_summary(...) -> Result<SummaryDto, String>;
```

Register in `src-tauri/src/main.rs` `invoke_handler!` alongside existing commands. UI can adopt the new family on its own schedule.

## "Freeze" semantic — resolved by bucket-seal's native sealing

The audit's "decorative freeze" complaint (`memory_graph` writes weren't really blocked) is addressed structurally:

- `LegacyStewardAdapter` continues to call `enforce_freeze("<callsite>")` on every mutating method, preserving the existing warn-only observability.
- **New writes default to `BucketSealAdapter`** where seal semantics are real (`tree_source::bucket_seal::append_leaf` cascades L0→L1→L2 according to token-budget + sibling-fanout gates).
- Documentation: the term "freeze" is restated. `memory_graph` is **"legacy backend in deprecation maintenance mode"** — it still accepts writes (with `tracing::warn!`) but is no longer the canonical destination.
- A future consolidation stage migrates the 12 production writer call sites to `BucketSealAdapter` and then upgrades `enforce_freeze` to panic-by-default.

## Migration PR sequence (15 PRs)

| # | PR | Tier | LoC | Goal |
|---|---|---|---:|---|
| 1 | `memory_adapter` trait + types skeleton | infra | ~250 | `MemoryAdapter` trait + `MemoryEntry`/`MemoryCategory`/`RecallOpts`/`NamespaceSummary` + `AppState.memory_adapters: HashMap` + `default_memory_backend` field. Zero adapters yet. |
| 2 | `LegacyKvAdapter` (wraps `memory.rs::MemoryStore`) | wrap | ~200 | First concrete adapter. Trait shape proved against existing KV store. |
| 3 | `LegacyStewardAdapter` (wraps `memory_graph::MemoryGraphStore`) | wrap | ~250 | Second wrap. Preserves freeze warn-only. Demonstrates trait works for graph-shaped data. |
| 4 | `memory.unified.*` IPC layer | IPC | ~350 | New IPC routes through trait + backend selection. UI can begin testing. |
| 5 | `memory_bucket_seal::content_store` port | bucket-seal | ~600 | Atomic file ops + chunks SQLite DB. Standalone module. |
| 6 | `memory_bucket_seal::canonicalize` + `chunker` port | bucket-seal | ~400 | Deterministic chunking pipeline. |
| 7 | `memory_bucket_seal::score` port + `EmbeddingProvider` trait + uClaw impl | bucket-seal | ~700 | Embeddings + entity extraction + canonicalize. Local Ollama default. |
| 8 | `memory_bucket_seal::tree_source` port (append_leaf + cascade-seal) | bucket-seal | ~800 | **THE core.** Single source tree end-to-end. |
| 9 | `BucketSealAdapter` impl over source-tree-only | adapter | ~300 | `recall` = FTS over chunks scoped by namespace. First adapter that's NOT a wrap. Registered in `AppState.memory_adapters`. |
| 10 | `memory_bucket_seal::tree_topic` port | bucket-seal | ~500 | Per-entity trees. Recall starts using topic results. |
| 11 | `memory_bucket_seal::tree_global` port | bucket-seal | ~400 | Daily global digest. |
| 12 | `memory_bucket_seal::jobs` background worker pool | bucket-seal | ~500 | Async embeddings + summaries + seals. Spawned at AppState boot. |
| 13 | `GbrainAdapter` (wraps `mcp__gbrain__*`) | wrap | ~250 | gbrain as a trait impl. |
| 14 | `MemUAdapter` (wraps `MemUClient`) | wrap | ~200 | memU as a trait impl. Optional `None` when memU unavailable. |
| 15 | Recall routing in `effective_system_prompt` | wiring | ~150 | `ChatDelegate.memory_context` populated via `route_recall(state, namespace, query, opts)`. **End-to-end first agent loop run on bucket-seal as default.** |

Each PR ≤ ~800 LoC; each shippable on its own with green CI + tests. PRs 5-12 are the bucket-seal port (8 PRs); the rest are infra/wrap/wiring.

**Dependencies:**
- PRs 1-4 must land before any later PR (define the trait + IPC seam).
- PRs 5-9 are sequential (each layer depends on the previous).
- PRs 10-12 can land in any order after PR 9.
- PRs 13-14 are independent.
- PR 15 depends on PR 9 (needs at least one real `BucketSealAdapter`).

## Out of scope (deferred)

- **`importance_decay` regression** (§1.5 #4 ⚠️) — decision deferred to a follow-up stage. The bucket-seal adapter has its own scoring/importance signals; importance_decay's interaction with the new system needs design once bucket-seal is operational.
- **Cross-backend fan-out recall** — single-backend-per-call is sufficient for 阶段 4. Future stage can add a `UnifiedRecall` aggregator if needed.
- **Retiring legacy backends** — `memory.rs::MemoryStore` and `memory_graph::MemoryGraphStore` stay alive (wrapped). True data migration + retirement is a follow-up consolidation stage once the 12 production writer call sites move to bucket-seal.
- **Synthesizers** (`wiki_synthesizer`, `lint_analyzer`, `entity_synthesizer`) and `brain_watcher` — not memory backends; stay separate.
- **UI migration** — adopt new `memory.unified.*` IPC at UI team's pace; no UI work blocks 阶段 4 backend completion.

## Success criteria

- ✅ `MemoryAdapter` trait exists with 5 concrete impls (BucketSeal, LegacyKv, LegacySteward, Gbrain, MemU).
- ✅ `BucketSealAdapter` is a full openhuman bucket-seal port: chunking + score + 3 trees + jobs operational.
- ✅ `memory.unified.*` IPC routes through the trait by backend name.
- ✅ `effective_system_prompt → memory_context` flows via `route_recall` against the default backend (BucketSeal by default).
- ✅ Existing UI keeps working (no break to `memory_*` or `memory_graph_*` IPCs).
- ✅ `cargo test --lib` baselines: net new tests positive; pre-existing failures unchanged.

## Risks

| Risk | Mitigation |
|---|---|
| openhuman code uses `anyhow::Result` everywhere; uClaw uses `crate::error::Error` in some paths | Adapt at boundaries; `anyhow::Result<T>` is the trait return type, callers `.map_err(Into::into)` where needed |
| Openhuman's `Summariser` calls LLMs; risks LLM budget blow-up | Default to `InertSummariser` (deterministic, no LLM). LLM-backed summariser is opt-in via `MemubotConfig.memory_bucket_seal.summariser_kind` |
| Workspace path `<DATA_DIR>/memory_bucket/` collides with existing data | New dir; doesn't touch existing `memory.db` or `memory_graph/` |
| Bucket-seal jobs queue contention with main agent loop | Job workers run on a dedicated tokio runtime handle pool; SQLite uses `busy_timeout = 15s` per openhuman precedent |
| 15-PR series stretches over 4-6 weeks; intermediate state has incomplete bucket-seal | Each PR shippable + tested; `BucketSealAdapter::recall` returns sensible results from PR9 onward (single-tree FTS), enhanced as later PRs land |

## Next step

Invoke `superpowers:writing-plans` to produce a per-PR implementation plan starting with PR1 (`memory_adapter` trait + types skeleton). The brainstorming session ends here; transition is the brainstorming skill's terminal state.
