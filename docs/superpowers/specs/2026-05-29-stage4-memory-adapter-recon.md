# 阶段 4 (Memory Adapter) — Recon & Hand-off Note

> **Status:** Pre-brainstorming recon, captured 2026-05-29 by Claude after 阶段 3 closed (PRs #570-#579 merged). The next session should invoke `superpowers:brainstorming` to turn this into a formal design spec.

## Why this stage exists

The Pi-convergence gap audit ([`2026-05-27-pi-convergence-gap-audit.md`](2026-05-27-pi-convergence-gap-audit.md)) §1.5 flags that uClaw has **~8 memory storage layers** assembled in parallel, with the "freeze" semantic being decorative only (no real seal/checkpoint contract). The philosophy ADR ([`docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`](../../adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md)) says:

> Memory: modernize via openhuman ideas behind one `MemoryAdapter`. Detailed gbrain↔openhuman architecture deferred to a dedicated effort.

阶段 4 is that dedicated effort.

## Recon findings (2026-05-29, against main at `b808a431`)

The actual memory surface is bigger than "8 stores" — closer to **12-15 distinct holdings + helpers**. Inventory:

### Top-level `src-tauri/src/` paths

| Path | Purpose | Status |
|---|---|---|
| `memory.rs` | `MemoryStore` + `MemoryEntry` (basic KV over SQLite) | Active |
| `memory_contract/` | Old contract types | **Marked for deletion in 阶段 2 per audit** (not yet executed) |
| `memory_policy/` | `MemoryPolicyExecutor` | **Marked for deletion in 阶段 2 per audit** (not yet executed) |
| `memory_graph/` | Steward memory graph store (gbrain-style: pages, links, summaries) | Active |
| `automation/memory/` | Automation-specific memory | Verify scope during brainstorming |

### `AppState` Arc-held subsystems (from `app.rs:185-280`)

1. `memory_store: Arc<MemoryStore>` — basic KV
2. `memory_graph_store: Arc<MemoryGraphStore>` — Steward graph
3. `memu_client: Option<Arc<MemUClient>>` — optional Python memU service
4. `wiki_synthesizer: Arc<dyn WikiSynthesizer>` — overview synth (Phase 3)
5. `lint_analyzer: Arc<dyn LintAnalyzer>` — memory_lint scenario (Phase 5)
6. `entity_synthesizer: Arc<dyn EntitySynthesizer>` — EntityPage synth (Phase 6.2)
7. `brain_watcher: Mutex<Option<BrainWatcherHandle>>` — fs watcher over brain dir (Phase 7.4)
8. `learning_llm: Option<Arc<dyn MemoryOsLlm>>` — LLM for chat-turn extractor + facet cache

### Additional per-session / cached memory state

- `facet_cache` (Sprint 1) — user profile facet cache
- Per-session cached composed `memory_context` (Bundle 20)
- `ChatDelegate.memory_context` + `ChatDelegate.last_memory_context_snapshot` (P3-5b1 already moved these to AppState lookup where possible)
- gbrain integration (external memory system via MCP)

### External callers

- `tauri_commands.rs` exposes many `memory_*` IPC endpoints (set/get/search/list/clear/bulk_import/count/clear)
- `tauri_commands.rs` exposes `memory_graph_*` IPC endpoints
- The agent loop reads memory via `ChatDelegate.memory_context` (set by recall engine before each loop)

## Open design questions to surface during brainstorming

1. **What does `MemoryAdapter` trait look like?** What's the minimum surface every memory backend must implement? (Likely: `recall(query) -> ContextString`, `record(entry)`, `seal(scope)` for the freeze story).

2. **Which of the 12-15 surfaces collapse behind the adapter?** The basic `MemoryStore` + `memory_graph_store` are obvious. What about `memu_client`? The synthesizers (`wiki`, `lint`, `entity`) — are they memory backends or memory *augmenters*?

3. **What's the migration order?** Audit-cited 阶段 2 work (delete `memory_contract/`, `memory_policy/`) should likely happen first — they're already-dead modules. Then build adapter. Then migrate one store at a time.

4. **How do we borrow from openhuman?** Per CLAUDE.md user memory `reference-agent-frameworks.md`, openhuman is a local Rust repo with "类脑 memory" (brain-like memory). Specifically the **bucket-seal tree** design. The next session should `Read` openhuman's relevant modules to ground the design.

5. **Backward compat for IPC endpoints**: the existing `memory_*` and `memory_graph_*` Tauri commands have UI dependencies. The adapter migration must keep these stable OR co-design a UI migration.

6. **gbrain↔openhuman bridge**: gbrain is the current external memory system (MCP); openhuman is the proposed new substrate. Are they coexisting? Migrating? The philosophy ADR says "modernize via openhuman ideas" — that could mean (a) replace gbrain with openhuman, (b) make gbrain one backend of the adapter and openhuman another, (c) keep gbrain for MCP integration and use openhuman ideas only for the in-process store.

7. **"Freeze" semantic — what does it actually need to do?** The audit says it's decorative. What's the real contract? (Likely: bucket-seal — a snapshot of memory state at a specific point, used by the agent loop for compaction + by the UI for "show me what the agent had in mind at turn N").

## Recommended next-session workflow

1. Read this recon doc.
2. Read the audit §1.5 + §1.6 (`docs/superpowers/specs/2026-05-27-pi-convergence-gap-audit.md`).
3. Read the philosophy ADR (`docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`).
4. Read openhuman's relevant modules (local repo path; check `CLAUDE.md` references for the location).
5. Invoke `superpowers:brainstorming` to turn the open questions above into a design spec.
6. Once the spec is approved, invoke `superpowers:writing-plans` for the first PR in the 阶段 4 series — likely the "delete deprecated `memory_contract/` + `memory_policy/`" cleanup (audit-cited 阶段 2 work, should land before adapter work).
7. Then `subagent-driven-development` per PR.

## 阶段 3 commit chain (for context)

Merged sequence (newest first):
- `b808a431` Orphan wrapper cleanup (warnings 50→49) — PR #579
- `97cc0450` P3-6 single seam + 5 golden snapshots — PR #578 (**closed 阶段 3**)
- `901744f0` P3-5b3 ContentBlock helper — PR #577
- `9ce2f95b` P3-5b2 session-config bundling — PR #576
- `cce495e2` P3-5b1 app_state() accessor — PR #575
- `0da7bada` P3-5a dispatcher structural split — PR #574
- `606f03a4` P3-4 plugin discovery — PR #573
- `0a4b20b0` P3-3 ProviderService + HookBus — PR #572
- `96d2c2e0` P3-2 ToolDispatch migration — PR #571
- (P3-1 AgentApi skeleton merged earlier as #570)

`ChatDelegate` field count across the series: **53 → 34** (-19 net).
