# Step 3b — Remove the memU Store (toward zero Python) Design

**Date:** 2026-06-01
**Status:** Design (decomposition + Decision A approved in brainstorming; per-sub-slice specs/plans follow)
**Part of:** Memory two-layer finish-line (ADR `2026-06-01-memory-two-layer-terminal-state.md`), **Step 3** (remove memU). Step 3a (the embedding provider for bucket_seal) shipped — PR #637. This is the rest: remove memU's **store** role and the **remaining embedding callers**, then tear down the Python bridge entirely.

## Problem

Step 3a swapped only `build_embedder` (the bucket_seal seal/recall embedding path) to the in-process `OnnxEmbedder`. Recon found memU is still load-bearing in two ways:

1. **Four direct `MemUClient::embed_text` callers** still hit the Python bridge for embeddings (Step 3a did not touch them):
   - `GeneRetriever` (`agent/gep/retrieval.rs:139,167`) — semantic gene matching; **live** (injected in agent loop, skill_agent, agent_teams).
   - `memu::embedding::embed_skill_body` (`proactive/service.rs:3003`) — skill-body vectors for ranking; **live**.
   - `local_api /v1/embeddings` route (`local_api/routes.rs:370`).
   - `memu_embed_text` Tauri command (`tauri_commands.rs:17666`).
2. **The memU store** — `memu.db` plus an **LLM extraction pipeline**. memU's `memorize` is NOT plain storage: `memu_bridge.py` runs an LLM (chat-model mapping, temperature handling) to turn a conversation into structured memory items (profile/event/knowledge/behavior/skill/tool), which `memory_graph/reflection.rs:12-22` (`map_memu_type_to_kind`) maps into `memory_graph`. Consumers:
   - **Write/extract:** `MemorizationService` (`memorization/service.rs:251,1096`), `ReflectionEngine` (`memory_graph/reflection.rs:472,530`), `ProactiveService` (`proactive/service.rs:2295,2443`).
   - **Read/recall:** `MemuMemoryTool` + `MemuTodosTool` (`agent/tools/memu_tools.rs:222,297,538`), the `memory_graph` recall L3 vector leg (`memory_graph/recall.rs:1128`).

The two-layer terminal state wants **zero external runtimes**. After Step 2 removes Bun (gbrain), memU's Python is the last one.

## Key infrastructure already in place

- **A shared in-process embedder handle already exists:** `AppState.bucket_seal_embedder: Arc<dyn Embedder>` (`app.rs:230`), built once at boot by `build_embedder(&cfg.embedding_endpoint, &data_dir)` (`app.rs:1062`). After Step 3a this is the `OnnxEmbedder`. The four `embed_text` callers can reuse this handle directly — no new wiring to create.
- **`Embedder` trait** (`memory_bucket_seal/score/embed/mod.rs:58`): `name()`, `dim()`, `async embed(text)->Result<Vec<f32>>` (single text). Callers want batch; add an `embed_batch(&[&str])` default method that loops `embed`.
- **bucket_seal recall:** `recall_hybrid(query, namespace, max)` (semantic-over-summaries + FTS backfill) and `recall_semantic` (`memory_bucket_seal/adapter.rs:111,200`) — the recall substitute for the memU vector leg.

## Decision A (approved): replace memU's LLM extraction with a Rust-side native extractor

memU's `memorize` LLM extraction is **preserved as a capability**, reimplemented in Rust: a native extractor reuses memU's extraction prompt(s), calls uClaw's existing LLM provider, and produces the same structured items → fed through the existing `reflection.rs` mapping into `memory_graph`; `bucket_seal` seal handles episodic/semantic summarization. **No capability loss, zero Python.** (Rejected: A2 drop-extraction-entirely — loses auto entity/skill extraction; A3 skills-only — partial loss.) This is the highest-risk sub-slice (3b-3) and gets its own dedicated brainstorm + spec when reached.

## Decomposition — four ordered, independently-shippable sub-slices

Each is its own spec→plan→implement cycle, bisectable, and DONE only when the old memU path is deleted (finish-line discipline). Ordering is by ascending risk and by dependency (teardown last).

### 3b-1 — embedder finish (kill memU's embedding role) — LOW risk, do first

Repoint all four `embed_text` callers to the shared `AppState.bucket_seal_embedder` (`Arc<dyn Embedder>` = `OnnxEmbedder`). After this, **nothing calls memU for embeddings**; the `FASTEMBED_MODEL` env and the embedding half of the bridge are dead.

- Add `Embedder::embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>` default method (loops `embed`); keep per-impl override room.
- Move the pure `cosine_sim` helper out of `memu/embedding.rs` into `score/embed/` (it must survive `src/memu/` deletion in 3b-4).
- `GeneRetriever::new(...)` takes `Option<Arc<dyn Embedder>>` instead of the memU client; `gep/retrieval.rs:139,167` use `.embed()`; threading sites (`tauri_commands.rs:70,2193,11288,15133`) pass `app_state.bucket_seal_embedder`.
- `embed_skill_body` rewritten to take `&Arc<dyn Embedder>`; `proactive/service.rs:3003` passes the shared handle.
- `local_api/routes.rs` `/v1/embeddings` + `memu_embed_text` command use the shared embedder (route stays alive, now in-process — external OpenAI-compatible clients keep working without Python).

**Out of scope:** deleting the bridge/route/command (3b-4). 3b-1 only repoints; the memU embedding path stays callable but unused.

### 3b-2 — recall repoint — MEDIUM risk (Decision C)

Repoint the three memU read/recall consumers to `bucket_seal::recall_hybrid`:
- `MemuMemoryTool` + `MemuTodosTool` (`agent/tools/memu_tools.rs`) → bucket_seal recall (these are user-facing agent memory tools).
- `memory_graph` recall L3 vector leg (`recall.rs:1100-1291`) → replace the `memu.retrieve` leg with a `bucket_seal` semantic leg in the existing FTS+vector RRF/weighted fusion.

**Decision C (resolve in the 3b-2 spec):** the memU vector leg currently adds semantic coverage over `memory_graph` raw nodes; `bucket_seal` semantic recall is scoped to bucket_seal trees (summaries), not memory_graph raw nodes — so it is a *different* surface, not 1:1. The 3b-2 spec will quantify the gap and decide the fusion (likely: keep FTS + graph over memory_graph nodes, add bucket_seal semantic as the new semantic leg, drop memU). Needs its own recon of recall quality.

### 3b-3 — memorize / extraction repoint — HIGH risk (Decision A = A1)

Build the Rust-side LLM extractor (A1). Repoint `MemorizationService`, `ReflectionEngine`, `ProactiveService` off `memu.memorize*`/`create_item` onto: bucket_seal seal (episodic/semantic) + the native extractor → `reflection.rs` mapping → `memory_graph`. Migrate any retained `memu.db` data if needed. **Own brainstorm + spec when reached.**

### 3b-4 — teardown — LOW risk (pure deletion, last)

Once nothing calls memU: delete the bridge boot (`app.rs:try_init_memu`, eager health-check, `main.rs` wiring), `MemUClient`/`MemUBridge`/`src/memu/`, `pyembed/` (+ root copy), the `/v1/embeddings`+`memu_embed_text` if no longer wanted (or keep the route backed by the in-process embedder), `MemUAdapter`, the reflection memU type-mapping if unused, `embedding_endpoint`/memu config fields, and the `tauri.conf.json` bundle entries (`pyembed/python`, `memu_bridge.py`). **Zero Python achieved.**

## Risk

Per-slice. 3b-1 + 3b-4 are mechanical/deletion. 3b-2 carries a recall-quality judgment (Decision C). 3b-3 carries the extraction reimplementation (Decision A1) — the real architectural work, dedicated brainstorm. The decomposition keeps each shippable so we never strand a half-cut path: at every merge, either memU is still fully wired (pre-3b-4) or fully gone (post-3b-4) — no orphaned dependency.

## This spec authorizes 3b-1 to proceed to a plan

3b-1's design above is concrete (shared handle exists, four call sites, `embed_batch` + `cosine_sim` move). 3b-2 and 3b-3 get their own specs (3b-3 a full brainstorm) before implementation. 3b-4 follows mechanically once callers are clear.
