# ADR — Memory terminal state is TWO layers: bucket_seal (recall) + one rich-structure store (memory_graph); gbrain retires as the duplicate

**Date:** 2026-06-01
**Status:** Accepted. **Supersedes-in-part** `2026-05-31-memory-store-convergence-openhuman-primary.md` on the *terminal shape* only — the convergence **work** (P1 facades, P2 gbrain page repoint, P3 rich-writer repoint) stands; this ADR revises the convergence **end state** from "a single bucket_seal store, gbrain + memory_graph both retire" to the two-layer state below. Builds on the grounded architecture review of 2026-06-01.

## Context

The 2026-05-31 convergence ADR set bucket_seal (openhuman) as the single terminal store and slated **both** gbrain and memory_graph for retirement (P2 = retire gbrain, P3 = migrate memory_graph's rich writers, P4 = remove memory_graph + the freeze hook). P1–P3 shipped: the **simple, adapter-shaped knowledge** — pages, learned skills, co-used-tool edges, tool-usage stats — now lives behind the `MemoryAdapter` seam on bucket_seal (`pages`/`skills`/`edges`/`tool_stats` facades), gated with rollback.

A grounded recon of `main` (2026-06-01) then surfaced a premise the original ADR under-scoped:

- **gbrain and memory_graph both host rich, graph-shaped, versioned, entity data that bucket_seal cannot model** — gbrain: wiki pages + entity backlinks + version history + graph traversal; memory_graph: nodes/edges/versions/keywords **plus** the EntityPage subsystem (versioned entity pages + timeline + tiers), reflection, preference/failure/personality extraction, importance-decay, drift detection, health/lint, and the entire graph-view UI.
- These are **still fully live** with the convergence flags on. memory_graph in particular has **~12 ungated live writers** (reflection on every message, EntityPage CRUD, personality ticks, quick-capture, skill management IPC, `bump_skill_usage`, …) with **no adapter equivalent** and no repoint planned.
- gbrain and memory_graph are **two overlapping implementations of the same thing**: an entity/wiki/graph/version store. The convergence unified only their *simple projections* (pages/skills/edges) onto bucket_seal; their rich layers remain two parallel stores.

Therefore "single bucket_seal store" would require either (a) **dropping real features** (entity graph, version history, backlinks, EntityPages, health/drift) or (b) **rebuilding memory_graph's capabilities inside bucket_seal** — re-inventing the store being torn down. Neither is an optimization. The real redundancy is **two graph stores**, not "two non-bucket_seal stores."

## Decision

**The terminal memory architecture is TWO layers behind one `MemoryAdapter` seam:**

1. **Recall / semantic / episodic + simple-knowledge layer — `bucket_seal`** (the convergence win; unchanged). SQLite + FTS5 + embeddings, chunk→score→tree-seal→`recall_hybrid`. The terminal **primary for recall** and the home of pages / skills / co-used-tool edges / tool-stats (P1–P3 facades). `default_memory_backend = "bucket_seal"`.

2. **Rich-structure layer — exactly ONE store, `memory_graph`** (retained, not retired). Hosts what genuinely needs a graph/versioned/entity model: the entity graph (nodes/edges), version history, EntityPages (+ timeline/tiers), and the reflection/personality/preference/failure/health/drift subsystems. In-repo Rust + SQLite — **no external runtime**.

3. **`gbrain` retires — as the *duplicate* rich store, not as a capability.** Its simple pages already live on bucket_seal; its rich capabilities (backlinks / versions / graph traversal / wiki) **consolidate into memory_graph**, which already models nodes/edges/versions. This removes the external **Bun + PGLite + gbrain-source** dependency (binary slimming) and collapses two graph stores into one.

We retire **the redundancy, not the capability.** "Keep memory_graph, retire gbrain" (rather than the reverse) because memory_graph is in-repo (no external runtime), already hosts strictly more (EntityPages, reflection, personality, health), and has far more live writers — making it the lower-cost survivor.

## Consequences — revised roadmap (replaces P2d/P4)

- **P2d → "gbrain consolidation"** (was "retire gbrain"). The substantive, high-value slice: repoint gbrain's remaining live coupling onto bucket_seal (already done for pages) + memory_graph (for the rich parts) — WikiView's page reads → bucket_seal/memory_graph, its graph/version/backlinks/orphans/stats commands → memory_graph equivalents, the chat extractor's direct `put_page` → bucket_seal, the LLM `mcp__gbrain__*` tools + prompt block → retired/re-backed — **then** tear down the bundled gbrain MCP server boot, the Bun runtime, PGLite, and the `gbrain-source` resource. Decomposable into its own spec/plan slices; itself the next real effort if/when prioritized.

- **P4 → redefined.** memory_graph is **NOT a teardown target** — it is the retained rich-structure layer. The original "remove memory_graph + the freeze hook" is **withdrawn**. The freeze hook's role shifts from "memory_graph is frozen pending deletion" to **"memory_graph is the single sanctioned rich-structure store — do not spawn new divergent graph/entity stores; route new rich data here behind the adapter seam."** A later cleanup may still prune genuinely dead memory_graph code, but the store stays.

- **The convergence flags:** the gbrain flags (`gbrain_dual_write_pages_enabled`, `gbrain_read_repoint_enabled`) retire with gbrain in P2d. The P3 repoint flags (`skill_store_repoint_enabled`, `tool_memory_repoint_enabled`) stay correct under two-layer (skills/co-usage/tool-stats are *simple* knowledge → bucket_seal) and can become unconditional after soak.

- **The `MemoryAdapter` seam stays the single front door.** Both layers sit behind it; the rich layer additionally exposes graph/version/entity operations that are not on the trait (inherent methods / dedicated commands), as today.

## Relationship to prior ADRs

- **`2026-05-31-memory-store-convergence-openhuman-primary.md`** — superseded **only** on the terminal-shape point (single store → two layers; memory_graph retained, not retired). Its P1–P3 decisions and the "bucket_seal is the terminal **recall/primary** store" stand. A supersession note is added to its header.
- **`gbrain-primary-freeze`** — already superseded on "gbrain primary." Consistent here: gbrain is now slated to retire as the duplicate; memory_graph remains frozen-against-divergence but **retained** (its freeze ADR's "delete in P4" expectation is withdrawn).
- The Pi-lightweight philosophy ADR (`2026-05-28`) is unaffected: two well-bounded layers behind one seam is more "lightweight/pluggable" than two overlapping graph stores + an external Bun runtime.

## Status of work

Data-plane convergence (P1–P3) + the bucket_seal config-driven embedding-dim fix are shipped and reversible-by-flag. This ADR records the **terminal-state decision**; the gbrain-consolidation effort (revised P2d) is a future spec→plan→implementation cycle, not implemented here. Recommended to follow a flag-soak period before beginning it.
