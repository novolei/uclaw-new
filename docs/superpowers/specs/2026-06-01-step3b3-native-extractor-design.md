# Step 3b-3 — Native Rust Memory Extractor (replace memU's `memorize`) Design

**Date:** 2026-06-01
**Status:** Design (D1=single JSON call, D3=upgrade-to-memory_graph approved in brainstorming; pending spec review → plan)
**Part of:** Memory two-layer finish-line (ADR `2026-06-01-memory-two-layer-terminal-state.md`), Step 3 (remove memU), sub-slice **3b-3** — the WRITE/extraction half. Follows 3b-1 (embedder) + 3b-2 (recall). Precedes 3b-4 (teardown = zero Python).

## Problem

memU's `memorize` is an LLM extraction pipeline (6 LLM calls — one per `memory_type` ∈ profile/event/knowledge/behavior/skill/tool — each with a multi-block XML prompt → typed items + Jaccard dedup). Three consumers call it; recon found their outputs land very differently:

| Consumer | memU call | Output destination | Actually consumed? |
|---|---|---|---|
| **ReflectionEngine** (`memory_graph/reflection.rs`, per-turn) | `memorize(user_input)` | **memory_graph** (create_node/version/route/keyword/boot via `map_memu_type_to_kind`) | **YES** — the core memory_graph population path |
| **MemorizationService** (`memorization/service.rs`, batched) | `memorize` | 3-way: `user_profile_facets` (direct SQLite) / episode→`create_item`→**memu.db** / others→`gbrain_drafts`→gbrain+bucket_seal | partial (facets+drafts used; episode→memu.db dead) |
| **ProactiveService** (`proactive/service.rs`, scenarios) | `memorize_with_config` (returns COUNT only) | **memu.db** (Python-side, opaque) + count telemetry | **NO** — fire-and-forget to an unread store |

**Reframing fact:** after 3b-2, **`memu.db` is no longer read** (recall is fully on bucket_seal). So ProactiveService's `memorize_with_config` and MemorizationService's episode→`create_item` write to a dead store — removing them loses nothing consumed. Only **ReflectionEngine → memory_graph** produces output that's actually used. This shrinks 3b-3 from "replicate the whole pipeline" to "build one Rust extractor + cut over the consumers, faithfully reproducing only the memory_graph-bound path."

## Decisions (approved)

- **D1 = single JSON-mode call.** One `MemoryExtractor::extract(conversation) -> Vec<ExtractedItem>` LLM call producing all typed items as a JSON array, mirroring the production `gbrain/chat_extractor.rs` pattern (prompt → `serde_json` parse, lenient, no tool-use), routed through the existing `MemoryOsLlm::complete_text` (cost-tagged). memU's per-type extraction RULES are ported into one prompt; the OUTPUT (typed items) is what the downstream mapping consumes, so the internal call-count need not match memU. Cheaper (1 call vs 6), simpler, consistent with the repo.
- **D3 = upgrade.** The "dead-store" writes are not faithfully reproduced — they're upgraded: ProactiveService's extraction now PERSISTS to memory_graph (capturing items currently lost to the unread memu.db); MemorizationService's episode→memu.db leg is dropped (episodic is already sealed into bucket_seal via `proactive/task_memory.rs`).

## Substrate (recon-confirmed, all reused)

- **LLM:** `MemoryOsLlm::complete_text(cost_tag, system_prompt, user_prompt, max_tokens)` (`memory_graph/memory_os_llm.rs:62`), backed by `MemoryOsLlmClient` over `ProviderService`. Cost-tag `"memory_extract"`.
- **Extraction template:** `gbrain/chat_extractor.rs` — `extract_system_prompt()` ("output ONLY a JSON array"), `complete_text`, `parse_proposals` (lenient serde + `strip_markdown_fences`). The extractor is a near-clone producing a different item shape.
- **memory_graph write:** `MemoryGraphStore::create_node` / `create_version` / `create_route` / keywords / `add_to_boot`; `map_memu_type_to_kind` (`reflection.rs:12-22`) + `generate_route_path` (`:50-69`). Currently inline in `reflect()` — factor into a reusable `persist_items_to_graph`.
- **bucket_seal coverage check:** `BucketSealAdapter::recall_hybrid` (replaces ReflectionEngine's `memu.retrieve` recall-before-memorize).
- **memU prompts to port:** `~/Documents/memU/src/memu/prompts/memory_type/{profile,event,knowledge,behavior,skill,tool}.py` — readable; condense each type's objective+rules into the single Rust prompt.

## Design

### §1 `MemoryExtractor` (new `src-tauri/src/memory_graph/extractor.rs`)

```rust
pub struct ExtractedItem { pub memory_type: String, pub content: String } // + optional Vec<String> categories
pub struct MemoryExtractor { llm: Arc<dyn MemoryOsLlm> }
impl MemoryExtractor {
    pub async fn extract(&self, conversation: &str) -> Vec<ExtractedItem> { /* complete_text → lenient JSON parse */ }
}
```
- `memory_type` ∈ {profile, event, knowledge, behavior, skill, tool} — same taxonomy `map_memu_type_to_kind` consumes.
- `extract_system_prompt()`: ports memU's 6 per-type rules (e.g. profile <30 words stable traits no meta-phrasing; event time-bound <50w; knowledge objective facts; behavior recurring patterns; skill actionable profile; tool usage+when_to_use) + bilingual rule ("extract in the resource's primary language") + "output ONLY a JSON array `[{\"memory_type\":..,\"content\":..}]`".
- Lenient parse (strip fences, skip malformed) + empty on LLM/parse failure (logged) — same posture as chat_extractor. Built once at boot; shared `Arc<MemoryExtractor>` on `AppState`.

### §2 Reusable graph mapping (`reflection.rs`)

Factor the current inline reflect() persistence into:
```rust
pub fn persist_items_to_graph(store: &MemoryGraphStore, space_id: &str, items: &[ExtractedItem]) -> Result<usize>
```
(dedup → per item: `map_memu_type_to_kind` → create_node/version/route/keyword → Boot eligibility). Pure refactor — `reflect()` calls it unchanged; ProactiveService reuses it.

### §3 Consumer cutovers

- **ReflectionEngine**: replace `memu_client` with `extractor: Arc<MemoryExtractor>` + `bucket_seal_adapter: Arc<BucketSealAdapter>`. Coverage check (was `memu.retrieve`): `recall_hybrid(content, None, k)` + a similarity threshold → skip if covered. Memorize (was `memu.memorize`): `extractor.extract(content)` → `persist_items_to_graph`. Keep all the pre-filters (length/greeting/command) + chip events.
- **ProactiveService** (D3 upgrade): replace `memorize_with_config` (`:2295`, `:2443`) with `extractor.extract(llm_response)` → `persist_items_to_graph(&memory_graph_store, space_id, items)`. The skill_extraction scenario keeps its primary skill-XML→`skill_adapter` path; only its memorize FALLBACK is repointed. Now proactive items land in memory_graph (was: count-only to dead memu.db).
- **MemorizationService**: replace `memu.memorize` (`:251`) with `extractor.extract`. Keep the `user_profile_facets` routing (routes by item kind — works on extracted items) and the `gbrain_drafts`/dual_write leg (gbrain is Step 2, untouched). **Drop** the episode→`create_item`→memu.db leg (`:1096`) — episodic is already in bucket_seal.

### §4 Construction

Build `Arc<MemoryExtractor>` at boot (`app.rs`/`main.rs`) from the `MemoryOsLlmClient` (same way `gbrain/chat_extractor` consumers get their `MemoryOsLlm`); add to `AppState`; thread into ReflectionEngine + MemorizationService + ProactiveService constructors (replacing/alongside the memU client at each).

### Data flow

```
conversation/turn → MemoryExtractor.extract (1 LLM call, cost "memory_extract") → Vec<ExtractedItem>
  ReflectionEngine: coverage-check via bucket_seal recall_hybrid → persist_items_to_graph → memory_graph
  ProactiveService: persist_items_to_graph → memory_graph (NEW — was dead memu.db)
  MemorizationService: facets routing + gbrain_drafts (episode→memu.db dropped)
(no memU, no Python for extraction)
```

## Error handling

LLM/parse failure → `extract` returns empty (logged) → no nodes created that turn → same posture as today's memU-unavailable degradation (reflection already tolerates empty). Coverage-check failure (bucket_seal) → proceed to extract (fail-open, as memU retrieve did).

## Testing

1. **Prompt/parse** (unit): canned LLM JSON → `extract` yields the expected `ExtractedItem`s; lenient parse strips fences + skips malformed; empty on non-JSON. Mirror `chat_extractor`'s `mock_llm` tests.
2. **Mapping** (unit): `persist_items_to_graph` over fixture items creates the right node kinds/routes/boot entries (use an in-memory `MemoryGraphStore`).
3. **ReflectionEngine** (unit): with a mock extractor + a fake bucket_seal adapter, a conversation produces memory_graph nodes; coverage-check skip path works; pre-filters still short-circuit.
4. **ProactiveService**: a scenario with a mock extractor persists items to memory_graph (the D3 upgrade) — assert nodes created.
5. **Extraction-quality spot-check (dev-time, documented):** on 3-5 sample conversations, compare the Rust extractor's items vs memU's for coverage/quality before relying. Recorded as a manual gate; the prompt is tuned if the single-call output is materially thinner than memU's 6-call output.
6. `cargo build` + clippy clean; targeted tests green; **gate:** no `memu`/`MemUClient`/`memorize`/`create_item` in reflection.rs / the proactive scenario paths / memorization service's extraction path.

## Scope / files

| File | Change |
|---|---|
| `memory_graph/extractor.rs` (new) | `MemoryExtractor` + `ExtractedItem` + ported prompt + lenient parse + tests |
| `memory_graph/reflection.rs` | factor `persist_items_to_graph`; ReflectionEngine: memU→extractor + bucket_seal coverage check |
| `proactive/service.rs` | scenario memorize→extractor→`persist_items_to_graph` (D3 upgrade) |
| `memorization/service.rs` | memorize→extractor; drop episode→memu.db; keep facets + drafts |
| `app.rs`/`main.rs` | build `Arc<MemoryExtractor>`; thread into the 3 consumers |

**Out of scope:** `MemUClient` (`retrieve`/`memorize`/`create_item`/`memorize_with_config` methods), the bridge boot, `memu_bridge.py`, `pyembed`, `memu.db`, the `gbrain_drafts`→gbrain leg (Step 2), `MemUAdapter` — all Step 3b-4 / Step 2. After 3b-3, NOTHING in the app calls memU for write/extraction; 3b-4 deletes the now-orphaned client + bridge → zero Python.

## Risk

HIGH (the slice's core). Two real risks:
1. **Extraction quality** — a single condensed-prompt call vs memU's 6 per-type calls. Mitigated by faithfully porting memU's per-type rules into the prompt + the dev-time spot-check (§Testing 5) + the same downstream mapping. The OUTPUT contract (typed items) is identical, so downstream is unaffected by the call-count change.
2. **ReflectionEngine is the core memory_graph writer** — a regression here degrades all of memory_graph. Mitigated by keeping the mapping (`persist_items_to_graph`) behavior-identical (pure refactor in §2 before the cutover in §3) + unit coverage. 

Bisectable: extractor module → mapping refactor (no-op) → ReflectionEngine cutover → ProactiveService upgrade → MemorizationService cutover → verify. Each commit compiles + tests; memU write is removed consumer-by-consumer, and the client/bridge survive until 3b-4 (no half-cut).
