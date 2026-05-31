# ADR — Memory-store convergence: openhuman/bucket-seal is the terminal primary store; gbrain + memory_graph retire in phases

- **Date:** 2026-05-31
- **Status:** Accepted
- **Deciders:** Ryan (user) + Claude (this session)
- **Supersedes:** the "gbrain is the primary knowledge layer" decision of `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md` (see *Coordination* below — that ADR's freeze of `memory_graph` remains in force until Phase 4).
- **Resolves:** the deferred decision in `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md` §6.7 ("gbrain ↔ openhuman 的详细取舍 … 后期开专项单独定 … 本 ADR 不预决").
- **Related code:** `src-tauri/src/memory_bucket_seal/` (openhuman port), `src-tauri/src/memory_adapter/` (the seam + GbrainAdapter), `src-tauri/src/gbrain/` + the gbrain MCP server (`mcp.rs`), `src-tauri/src/memory_graph/` (frozen L1 + tool_memory/skill_parser rich writers).

---

## Context

uClaw currently runs **three coexisting memory systems**, with partly-decided roles:

| System | What it is | Status (pre-ADR) |
|---|---|---|
| **memory_graph** (L1 Foundation / EntityPage) | uClaw's own SQLite knowledge graph + ~200 KB of wiki-synth / recall-propagation / auto-link / markdown-sync infra | **FROZEN** (gbrain-primary-freeze ADR) — but still hosts the production rich writers `proactive/tool_memory.rs` (co-used-tools edge graph) + `proactive/skill_parser.rs` (versioned / keyword-indexed / cited-count-ranked learned-skill store) |
| **gbrain** (real `garrytan/gbrain` via MCP) | page-oriented knowledge graph; the **active write path** for new chat knowledge (`mcp__gbrain__put_page`/`query`/`search`) | ADR-decided **primary** for knowledge; a **heavy external dependency** — bundled Bun runtime + PGLite at `~/.uclaw/gbrain/`, a separate subprocess |
| **bucket_seal** (openhuman port) | pure-Rust SQLite + FTS5 consolidation tree (score-admission, bucket-seal cascade, hotness/recency decay, coarse-to-fine recall) | **canonical default** `MemoryAdapter` backend (阶段 4); chat recall + task episodes (sub-project A / C) route here |

This three-way split is the residue of two prior decisions taken before bucket_seal existed: the gbrain-primary-freeze ADR made gbrain primary *relative to the then-frozen memory_graph*; the Pi-lightweight philosophy ADR (§6.7) then set the *direction* as "openhuman modernization behind one `MemoryAdapter` seam" but explicitly **deferred** the gbrain↔openhuman end-state.

The tension the philosophy ADR left open: gbrain is an **external, non-Rust, separate-process** dependency (Bun + PGLite) — the opposite of the Pi-lightweight "one handle, same stack, fewer moving parts" goal — while bucket_seal/openhuman is **same-stack (Rust + SQLite/FTS5)** and the §6.7-preferred modernization. Meanwhile the frozen memory_graph still hosts live rich writers (sub-project C exempted them precisely because this end-state was undecided).

## Decision

**openhuman/bucket_seal is the terminal primary memory store.** All memory — chat/episodic, page-knowledge, and the specialized rich stores — converges behind the single `MemoryAdapter` seam onto bucket_seal. **gbrain and memory_graph retire in phases.** This supersedes the gbrain-primary-freeze ADR's "gbrain primary" (which was an interim relative to the frozen memory_graph): the terminal primary is openhuman, not gbrain.

Rationale: it is the only end-state consistent with the Pi-lightweight philosophy — **one store, same stack (Rust + SQLite/FTS5), no external Bun/PGLite subprocess** — and it realizes §6.7's stated openhuman-modernization direction. The cost (rebuilding gbrain's auto-link/wiki capabilities + a PGLite→SQLite data migration) is accepted as the price of removing the heavy external dependency and collapsing three stores to one.

## Phased roadmap

This ADR records the decision and the phasing only. **Each phase is its own future effort** (spec → plan → implementation); none is implemented by this ADR.

- **P1 — `MemoryAdapter` capability growth.** Grow bucket_seal / the adapter seam to host what gbrain and the rich writers need: page-knowledge (put / get / search in gbrain's shape), graph edges (for tool_memory's co-used-tools), and versioning + keyword-index + ranking (for skill_parser's `list_top` / dedup / decay). Itself decomposable (e.g. edges, versioning, page-store as separate slices).
- **P2 — migrate gbrain knowledge → adapter.** PGLite → SQLite data migration; repoint the `put_page`/`query`/`search` write+read paths (incl. the agent system-prompt instruction block) to the adapter; **retire the gbrain MCP server + the Bun runtime + PGLite dependency.**
- **P3 — migrate memory_graph rich writers → the extended adapter.** Sub-project C's deferred migration (tool_memory edge graph, skill_parser versioned store) onto P1's new capabilities; retire / internalize memory_graph's still-used wiki/recall infra.
- **P4 — cleanup.** Remove the gbrain source / Bun runtime / PGLite, memory_graph's dead infrastructure, and the `check-memory-graph-freeze.sh` hook (no longer needed once nothing writes the old graph).

Ordering is intentional: capabilities first (P1), then the larger external-dependency removal (P2), then the in-repo rich-writer migration (P3), then teardown (P4). Each phase leaves the system working.

## Consequences

**Positive:** one memory store; same stack (no external Bun/PGLite subprocess at runtime — smaller install, fewer failure modes, faster boot); the §6.7 "single `MemoryAdapter` seam, openhuman modernization" realized end-to-end; the freeze hook + the "8-store" residue ultimately deleted.

**Negative / risks:** large multi-phase program; bucket_seal/adapter must grow non-trivial capabilities (graph edges, versioning, page-store, ranking) it does not have today; a PGLite → SQLite data migration (P2) with real user knowledge at stake (needs careful migration + a validation/rollback window, like sub-project A's gated fallback); gbrain's mature auto-link / dream-cycle / wiki-synth behaviors must be re-derived or consciously dropped. Until P4, the gbrain-primary-freeze ADR's `memory_graph` freeze **remains in force** (the freeze hook stays active; the C exemptions for tool_memory/skill_parser stand).

## Coordination with the freeze ADR

`gbrain-primary-freeze` is **superseded only on the "gbrain is the terminal primary" point**. Its operational decisions stay in effect during the transition: `memory_graph` remains frozen (read-only legacy, freeze hook active) until P3 migrates its rich writers and P4 removes it; "L2 Cognitive paused" stands. When P4 completes, the freeze ADR is fully retired (a note will be added there).

## Alternatives considered

- **B. Coexist with clear division** (gbrain = pages, bucket_seal = episodic, memory_graph rich writers → specialized stores). Lower migration cost, preserves existing investment — but keeps three stores + the external Bun/PGLite dependency, contradicting the Pi-lightweight "fewer moving parts, same stack" goal. Rejected as the *terminal* state (though it approximates the interim state during P1–P2).
- **C. Borrow-ideas-only** (status quo + a clean adapter seam, no migration). Smallest, zero migration risk — but the memory side never converges; the external gbrain dependency and the frozen-graph rich-writer split persist indefinitely. Rejected: it declares the §6.7 effort permanently deferred rather than deciding it.

> This is a decision record, not an implementation plan. Each phase (P1–P4) must go through `superpowers:brainstorming` → `writing-plans` before any code lands, and respect the ADR §18 spec rules.
