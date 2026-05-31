# ADR — gbrain is the primary knowledge layer; L2 Cognitive paused; L3 Engines selectively scavenged

- **Status:** Accepted
- **Date:** 2026-05-20
- **Deciders:** Ryan (user) + the overnight autonomous agent that surfaced the conflicts (L2 in turn 1, L3 finer-grained breakdown in turn 2)
- **Related code:** `src-tauri/src/memory_graph/` (L1 Foundation), `src-tauri/src/gbrain/` (Sprint 2.4 chat extractor), MCP gbrain server config in `mcp.rs`, V35–V43 in `db/migrations.rs`
- **Related docs:** [agent-memory-os-cognitive.md](../superpowers/plans/agent-memory-os-cognitive.md) (paused), [agent-memory-os-cognitive-design.md](../superpowers/specs/2026-05-18-agent-memory-os-cognitive-design.md) (paused), [agent-memory-os-engines.md](../superpowers/plans/agent-memory-os-engines.md) (partially paused — see §"L3 split" below), [agent-memory-os-engines-design.md](../superpowers/specs/2026-05-18-agent-memory-os-engines-design.md) (partially paused), [agent-memory-os.md](../superpowers/plans/agent-memory-os.md) (Foundation — read-only mode), [agent-memory-os-design.md](../superpowers/specs/2026-05-18-agent-memory-os-design.md) (Foundation — read-only mode)
- **Supersedes:** none
- **Superseded by:** none

---

## Context

uClaw has accumulated two parallel knowledge systems with substantial conceptual overlap:

### System 1: L1 Foundation (memory_graph / EntityPage) — already shipped

Built between 2026-04 and 2026-05-18 across Phase 1–7. The Foundation spec ([2026-05-18-agent-memory-os-design.md](../superpowers/specs/2026-05-18-agent-memory-os-design.md)) explicitly cites `garrytan/gbrain` as inspiration ("BrainEngine + put_page auto-link + 双层 page") and the intent was to build a gbrain-like wiki internally in uClaw's SQLite. Shipped infrastructure:

- `memory_nodes` with `kind = EntityPage` + per-page `compiled_truth` + `timeline` (V35)
- `memory_graph/auto_link.rs` (17 KB) — zero-LLM regex auto-link with 7 typed edge kinds
- `memory_graph/wiki_synth.rs` (43 KB) — `RealWikiSynthesizer` (Phase 6b) generates wiki_artifacts (overview/index)
- `memory_graph/recall.rs` (80 KB) — graph propagation + FTS + compiled-truth boost
- `proactive/scenarios/memory_health.rs` (31 KB) — zero-LLM structural health checks
- `proactive/scenarios/memory_lint.rs` (62 KB) — LLM-driven semantic lint
- `proactive/scenarios/tier_escalator.rs` — Phase 6.1 enrichment tier escalator
- `memory_graph/brain_io.rs` (69 KB) + `brain_watcher.rs` — Phase 7 markdown sync (V37)
- Tauri commands: `memory_entity_page_*`, `memory_wiki_get_overview`, `memory_wiki_regenerate`, `memory_wiki_export`, `memory_wiki_sync_from_disk`, `memory_health_*`
- Frontend: `MemoryHealthPanel.tsx`, `MemoryPanel.tsx`, `MemoryGraphView.tsx`, `MemoryNebulaView.tsx`

### System 2: gbrain (real garrytan/gbrain via MCP) — shipped Sprint 2.0–2.4

Between 2026-05-18 and 2026-05-19 we shipped:

- Bundled Bun runtime + gbrain source pipeline (Sprint 2.0, PR #203)
- MCP server seed + PGLite init (Sprint 2.1, PR #204 + init-fix PR #205)
- Launcher + init recovery script (Sprint 2.2, PR #207)
- `/v1/embeddings` local OpenAI-compatible endpoint + embedding-endpoint config UI (PR #214, #216)
- 120 s init timeout + diagnostics row in System tab (Sprint 2.2.5a/b, PR #223)
- Agent system-prompt instruction block telling LLM when to call `mcp__gbrain__put_page` / `query` / `search` (Sprint 2.3, PR #223)
- 2-second embedding URL health-check probe (Sprint 2.2.5c, PR #259)
- Chat-turn auto-extractor that proposes pages from each user turn (Sprint 2.4, PR #259) — default ON, 30k token/day budget

The agent now writes new knowledge via gbrain MCP tools; new knowledge does **not** go through System 1.

### The conflict

| Dimension | gbrain (active path) | L1 Foundation EntityPage (legacy path) |
|---|---|---|
| Page unit | wiki page (slug + frontmatter + markdown) | EntityPage (slug + metadata_json + compiled_truth + timeline) |
| Storage | gbrain's PGLite (`~/.uclaw/gbrain/.gbrain/`) | uClaw SQLite (`memory_nodes`) |
| Write entry | `mcp__gbrain__put_page` | `memory_entity_page_create` IPC + `wiki_compile` |
| Read entry | `mcp__gbrain__query` / `search` | `recall.rs` graph propagation + `memory_wiki_*` IPCs |
| Auto-link | gbrain internal `link-extraction.ts` | `memory_graph/auto_link.rs` (17 KB) |
| LLM compile | gbrain internal pipeline | `wiki_synth.rs` (43 KB) + planned Phase 10 |
| Health/lint | gbrain dream cycle | `memory_health.rs` + `memory_lint.rs` (93 KB combined) |
| Markdown sync | gbrain native | `brain_io.rs` + `brain_watcher.rs` (79 KB combined) |

### L2 Cognitive would deepen the conflict

The L2 Cognitive plan ([agent-memory-os-cognitive.md](../superpowers/plans/agent-memory-os-cognitive.md), Phase 8–14) was authored before Sprint 2.0–2.4 locked in gbrain as the agent's primary knowledge source. Its scope is to add segmented provenance, two-step LLM compile, review-queue, and adaptive RAG **on top of System 1**. Every line of Phase 9–14 would enrich `memory_nodes`, not gbrain. Net result if executed as-is:

- New agent knowledge keeps flowing to gbrain (per Sprint 2.3 prompt)
- L2 Cognitive only enriches the increasingly-stale `memory_nodes` content
- ~1500 LOC of cognitive infrastructure serves an increasingly-orphaned data path
- Two parallel knowledge systems both need maintenance forever

## Decision

**gbrain is uClaw's primary long-term knowledge layer.** L1 Foundation (System 1) enters read-only mode for the EntityPage path; L2 Cognitive is paused.

Specifically:

1. **No new feature work on L1 Foundation's EntityPage path** beyond bug fixes. The shipped code (memory_health, memory_lint, wiki_synth, recall, brain_io, brain_watcher) stays alive and supports any existing data on disk, but no Phase 9–14 work lands on top of it.

2. **L2 Cognitive plan + spec are paused**, not deleted. They retain historical value as a design exploration and as a fall-back if we ever reverse this decision. Both docs get a `STATUS: PAUSED` header pointing to this ADR.

3. **V43 cognitive migration tables stay** (`wiki_log_events`, `page_content_hashes`, `review_queue_items`, `wiki_page_templates`, `analysis_cache`) since they're already in users' DBs from PR #264 and PR #267. They remain empty on production until/unless this decision reverses; removing them would require a destructive migration which is a worse outcome than leaving inert tables.

4. **The V43 `wiki_page_templates` seed (7 rows)** also stays. If/when we revisit cognitive features they'll be there; meanwhile they're 7 KB of inert data.

5. **Agent prompt + extractor stay pointed at gbrain.** No code change to revert Sprint 2.0–2.4 work.

6. **Memory OS docs taxonomy update:**
   - `agent-memory-os.md` (Foundation plan) — header note: Phase 1-7 are in maintenance mode, Phase 4 (Memory Health) and Phase 5 (Memory Lint) continue to run on existing data, no new application-layer work
   - `agent-memory-os-cognitive.md` (Cognitive plan) — `STATUS: PAUSED` header, body retained for reference
   - `agent-memory-os-cognitive-design.md` (Cognitive spec) — `STATUS: PAUSED` header
   - `agent-memory-os-engines.md` / `engines-design.md` (L3 Engines) — `STATUS: PAUSED` header (depended on L2)

7. **CLAUDE.md migration registry** flips V43 row from "in progress (Memory OS Cognitive Layer)" to "paused (gbrain primary — see ADR 2026-05-20)".

8. **L3 Engines is NOT uniformly paused.** A finer-grained audit (turn 2 of the same session) showed L3 has three engines with very different overlap profiles against gbrain. The decision is **split**:

   | L3 component | Status | Reason |
   |---|---|---|
   | Entity Graph Engine (NER + alias resolution + coreference) — Phase 15 | **PAUSED** | gbrain's `chat_extractor.rs` + `maintain` skill already cover regex NER, Aho-Corasick alias dict, LLM disambiguation, and weekly alias dedup. Coreference (intra-document "John … he … the engineer") is anti-pattern (gbrain deliberately omits it as alias table's responsibility). |
   | Timeline Engine (global timeline_events + daily/weekly/monthly aggregation summaries) — Phase 16 | **RETAINED — open for execution** | gbrain has per-page timeline but no global timeline and no aggregation loop. ~400 LOC, fully orthogonal to gbrain, low risk. |
   | Dream Cycle 8-stage pipeline (stages ①-⑥) | **PAUSED** | Stages ①-⑥ overlap with gbrain's 6-stage cycle (`lint / backlinks / sync / extract / embed / orphans`). Stages ⑦ UpdateEmbeddings + ⑧ RefreshGraphEdges are not in gbrain's named stages but are minor; we'll fold them into specific consumers (e.g. an "embedding backfill" cron) rather than re-import the whole pipeline. |
   | Dream Cycle 7 enhancements (§4.12) | **4 RETAINED, 3 PAUSED** | See sub-table below. |

   **L3's 7 advanced enhancements — sub-decision:**

   | Enhancement | Status | Notes |
   |---|---|---|
   | 4.12.1 Importance-Aware Decay (Ebbinghaus + importance-weighted half-life) | **RETAINED** | gbrain has lifecycle stages but no decay algorithm. Knowledge hygiene gap. ~250 LOC + tests. |
   | 4.12.2 Hypothesis Generation (LLM generates Gap-type questions) | PAUSED | Already partly covered by user-driven queries; low marginal value. |
   | 4.12.3 Spaced Repetition (Anki SM-2 + Leitner buckets) | **RETAINED** | Proven learning-science approach to memory consolidation; gbrain has nothing equivalent. ~300 LOC + tests. |
   | 4.12.4 Concept Drift Detection (track entity property version diffs) | **RETAINED** | Catches contradictions, complements gbrain's per-page versioning. ~200 LOC + tests. |
   | 4.12.5 Cross-Source Triangulation (≥2 sources agree → confidence boost) | **RETAINED** | Key once we wire external data sources (API, emails, web). ~250 LOC + tests. |
   | 4.12.6 Predictive Boot Preparation (pre-rank likely-needed entities) | PAUSED | ~10% UX win, optimization-level. |
   | 4.12.7 Synthetic Q&A Materialization (auto-FAQ EntityPages) | PAUSED | High token cost, low signal for small KBs. |

   **L3 Engines spec + plan headers** are updated from "STATUS: PAUSED" to "STATUS: PARTIALLY PAUSED" with the split table inlined. The spec's V36 references — which conflict with the existing V36-skip / V37-V43-used registry — are renumbered to V44 (the next free slot post-V43-cognitive). This is a sibling fix to PR #262's V35→V43 doc renumber for L2.

## Rationale

**Why now:** The cognitive plan was about to enter implementation phase (Phase 8.2.1 WikiSubkind enum was being built when the conflict was surfaced). Pausing now costs nothing — `WikiSubkind` was on a local branch, never pushed. Pausing one phase later (after Phase 9 / Phase 10 lands) would mean ripping out 500+ LOC of provenance + compile work that only enriches the dead path.

**Why gbrain wins:**

- **Already proven:** Sprint 2.0–2.4 shipped without major issues. Sprint 2.3 post-merge QA confirmed agent calls `put_page` / `recall` reliably when prompted. Sprint 2.4's auto-extractor (default-on, budget-gated) is producing real pages.
- **External maintained:** `garrytan/gbrain` is an active upstream project. Bug fixes + new features land for free.
- **Already has the features L2 was planning:** gbrain's dream cycle covers what `memory_health` + `memory_lint` aim at. gbrain's native auto-link covers `auto_link.rs`. gbrain's markdown frontmatter is the native storage shape, not a sync target.
- **Single LLM call site:** Cognitive Phase 10's two-step compile pipeline would be a new LLM call surface uClaw has to budget + monitor. gbrain handles compile internally; uClaw only pays for the cheap `chat_extractor` Haiku calls (already capped at 30k tokens/day in Sprint 2.4).

**Why not Option B (clean split: memory_nodes for internal state, gbrain for user knowledge):**

Considered. Drawbacks: requires drawing a sharp line through `memory_nodes` kinds — `Episode`, `Procedure`, `Curated`, `UserProfile`, `Directive`, `Reference` would arguably belong on either side. The split would be a constant source of "where does this go?" judgment calls. Option A's "everything new → gbrain, everything old → frozen" is sharper and lower-maintenance.

**Why not Option C (status quo, build L2 anyway):**

Considered. Drawbacks: ~1500 LOC of cognitive infrastructure with declining ROI as new knowledge flows away from `memory_nodes`. Reviewer burden + ongoing maintenance cost for two parallel systems. The only upside is "we don't have to admit a strategy change" — not a real engineering reason.

**Why not Option D (bidirectional sync gbrain ↔ memory_nodes):**

Considered. Drawbacks: most complex. Requires a sync protocol that handles conflicts, gbrain schema changes, partial-write recovery, and a clear source-of-truth rule per field. Estimated 2–3 dedicated Sprints. Postponed; could be revisited if cognitive analytics on top of `memory_nodes` becomes genuinely valuable later.

## Consequences

**Positive:**

- Single knowledge layer for new content. No drift between two stores.
- No further investment in code that only enhances a stale data path.
- Sprint 2.0–2.4 gbrain wiring is the strategic surface — every future memory feature improves the one path that matters.
- Plan/spec docs explicitly say PAUSED, preventing a future contributor from spending a Sprint executing the cognitive plan.

**Negative:**

- L1 Foundation's `wiki_synth.rs` / `recall.rs` / `memory_health.rs` / `memory_lint.rs` (~250 KB total) become slow-burn maintenance cost.
- Existing EntityPages in production DBs may stay forever — or need a one-time export to gbrain (see follow-up).
- `MemoryHealthPanel.tsx` and other System-1 UI surfaces still function but will become emptier over time as new content flows elsewhere.

**Neutral but worth noting:**

- The V43 migration tables (PR #264) and seed (PR #267) shipped to users 2026-05-19 / 2026-05-20. They're inert under Option A. Removing them is a destructive migration; the cleaner path is to leave them and update the ADR if direction reverses.
- The 7 `wiki_page_templates` rows seeded in PR #267 are 7 KB of inert data on every install.

## Follow-up work (suggested, not blocking)

1. **One-time EntityPage → gbrain export.** A migration tool that walks `memory_nodes WHERE kind = 'EntityPage'`, formats each as gbrain-compatible YAML frontmatter + markdown, and bulk-inserts via `mcp__gbrain__put_page`. Optional and idempotent. Best done as its own PR after the user has actually accumulated EntityPages they care about; rushing it before validation would risk silently breaking content.

2. **Dead-code audit.** Once the freeze has been in place for a sprint or two, decide whether to physically delete:
   - `proactive/scenarios/memory_lint.rs` + `memory_health.rs` (if user gets value from gbrain's dream cycle instead)
   - `wiki_synth.rs::RealWikiSynthesizer` (if no EntityPages are being written)
   - `brain_io.rs` + `brain_watcher.rs` (gbrain has its own markdown layer)
   Conservative path is to leave them; aggressive path is to delete and reduce surface area.

3. **CONTEXT.md / NORTH-STAR doc.** Project would benefit from a short top-level doc explaining "knowledge in uClaw goes to gbrain. memory_nodes is internal subsystem state." This ADR is the technical reference; CONTEXT.md would be the human-readable narrative. (Note: CLAUDE.md mentions `CONTEXT.md` doesn't exist yet.)

4. **Cognitive ideas worth porting to gbrain via prompt engineering.** Not all L2 Cognitive ideas are obsolete — some apply to gbrain too:
   - Two-step compile (analyze sources → compose page) → could be a future system prompt for the agent's put_page calls
   - Review queue (human-in-the-loop for contradictions) → could surface as a small UI over gbrain's contradiction detection
   - Adaptive RAG (query classifier) → already partially in scope of Sprint 2.4's extractor; could grow on the recall side too

## Verification

This ADR is doc-only; the implementing PR (#TBD) updates:

- `docs/superpowers/plans/agent-memory-os.md` — `STATUS: MAINTENANCE MODE` header (Foundation plan)
- `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` — `STATUS: MAINTENANCE MODE` header (Foundation spec)
- `docs/superpowers/plans/agent-memory-os-cognitive.md` — `STATUS: PAUSED` header (L2 Cognitive plan)
- `docs/superpowers/specs/2026-05-18-agent-memory-os-cognitive-design.md` — `STATUS: PAUSED` header (L2 Cognitive spec)
- `docs/superpowers/plans/agent-memory-os-engines.md` — `STATUS: PARTIALLY PAUSED` header with §8 split table inlined + V36→V44 renumber throughout body (L3 Engines plan)
- `docs/superpowers/specs/2026-05-18-agent-memory-os-engines-design.md` — `STATUS: PARTIALLY PAUSED` header with §8 split table inlined + V36→V44 renumber throughout body (L3 Engines spec)
- `CLAUDE.md` — V43 row in Active migration registry flipped from "in progress (Memory OS Cognitive Layer)" to "paused (see ADR 2026-05-20)"

No code change, no test change.

## Freeze exemptions (2026-05-31)

Two production writers remain on `memory_graph` as **documented exemptions**, pending the deferred gbrain↔openhuman effort (their semantics have no clean `MemoryAdapter` equivalent):
- `proactive/tool_memory.rs` — co-used-tools graph (edges).
- `proactive/skill_parser.rs` — versioned / keyword-indexed / cited-count-ranked learned-skill store.

(task_memory's Episode nodes were migrated to the MemoryAdapter; see sub-project C.)
