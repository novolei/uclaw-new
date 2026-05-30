# Follow-up PR18 — Multi-backend Recall Fan-out (bucket_seal + gbrain) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Extend PR15's bucket_seal-only prompt recall to ALSO proactively recall from gbrain (the long-term knowledge graph), rendered as a separate labelled sub-section — so the agent sees both its recent cascade memory (bucket_seal) and its long-term knowledge (gbrain) every turn. memU is excluded (already injected via the legacy `MemoryRecallEngine` path → no double-injection).

**Architecture:** Generalize PR15's `render_bucket_seal_recall(entries, budget)` to `render_recall_block(marker, entries, budget)` and rename `append_bucket_seal_recall` → `append_unified_recall`. The unified helper runs two best-effort, budgeted, sectioned legs: bucket_seal (`recall_hybrid`, existing) + gbrain (`state.memory_adapters.get("gbrain").recall(...)`, new). Each renders its own `## Relevant Memory (...)` block appended to the memory context. No cross-scale ranking (sectioned, honest). Sectioned because bucket_seal cosine vs gbrain search-similarity are different scales.

**Tech Stack:** Rust, `async-trait`, `anyhow`, `tracing`, the `MemoryAdapter` registry. No new deps.

---

## Source-of-truth references (verified)

- `tauri_commands.rs:1794` — `async fn append_bucket_seal_recall(state: &AppState, delegate: &mut ChatDelegate, query: &str)` (PR15): gated non-empty, calls `state.bucket_seal_adapter.recall_hybrid(query, None, 6)`, renders via `render_bucket_seal_recall(&entries, 1500)`, `delegate.append_memory_context(...)`. Called at the chat send site + agent send site(s).
- `agent/memory_recall_block.rs` (PR15) — `pub const BUCKET_SEAL_RECALL_MARKER: &str = "## Relevant Memory (bucket-seal)"`; `pub fn render_bucket_seal_recall(entries: &[MemoryEntry], token_budget: usize) -> Option<String>` (greedy budget, first-entry floor, marker + `- [{score:.2} · {ns}] {content}` lines).
- `app.rs` — `state.memory_adapters: Arc<HashMap<String, Arc<dyn MemoryAdapter>>>` (registry; `"gbrain"` registered in PR14). `state.bucket_seal_adapter: Arc<BucketSealAdapter>` (concrete, PR13).
- `memory_adapter/traits.rs` — `async fn recall(&self, query: &str, limit: usize, opts: RecallOpts<'_>) -> anyhow::Result<Vec<MemoryEntry>>`.
- `memory_adapter/types.rs` — `RecallOpts<'a> { namespace: Option<&'a str>, category: Option<MemoryCategory>, session_id: Option<&'a str>, min_score: Option<f64> }`; `MemoryEntry { ..., namespace: Option<String>, score: Option<f64> }`.
- `gbrain.rs` (PR14) — `GbrainAdapter::recall` = gbrain `search` → `SearchHit → MemoryEntry` (score=Some(similarity)). Returns Err when gbrain absent (caller skips).
- **memU exclusion rationale**: the legacy "Memory Recall Integration" block (`tauri_commands.rs:~2171`) already injects memU via `MemoryRecallEngine(memory_graph_store, memu_client)`. Fanning out to the MemUAdapter (PR17) would duplicate it. PR18 fans out bucket_seal + gbrain only.

---

## CRITICAL facts

1. **Sectioned, not ranked** — two independent `## Relevant Memory (bucket-seal)` + `## Relevant Memory (gbrain)` blocks. No merge/dedup across backends (different score scales; a chunk and a gbrain page rarely share an id anyway).
2. **Each leg best-effort + independent** — gbrain absent (`registry.get("gbrain")` None) or `recall` Err → skip the gbrain block; bucket_seal unaffected. Never blocks the turn.
3. **memU excluded** — already in the legacy path. Do NOT add a memU leg.
4. **Additive** — appended after the legacy recall, same as PR15. No legacy change.
5. **Generalize, don't duplicate** — `render_bucket_seal_recall` becomes `render_recall_block(marker, ...)`; PR15's bucket_seal call passes `BUCKET_SEAL_RECALL_MARKER`. Keep the old fn name as a thin wrapper OR update its one caller — prefer renaming + updating the caller (it's internal).

---

## File Structure

| File | Mod | Change |
|---|---|---|
| `agent/memory_recall_block.rs` | mod | `render_bucket_seal_recall` → `render_recall_block(marker: &str, entries, budget)`; add `pub const GBRAIN_RECALL_MARKER`; keep `BUCKET_SEAL_RECALL_MARKER`; +1 test (gbrain marker) |
| `tauri_commands.rs` | mod | rename `append_bucket_seal_recall` → `append_unified_recall`; add the gbrain leg; update the call sites |

Est. ~90 source + ~30 tests.

---

## Adaptation responsibilities

1. **`render_recall_block` signature** — `pub fn render_recall_block(marker: &str, entries: &[MemoryEntry], token_budget: usize) -> Option<String>`. The body is PR15's `render_bucket_seal_recall` with the hardcoded marker replaced by the param. Keep the existing entry-line format + budget logic + first-entry floor. Update the 3 existing PR15 tests to call `render_recall_block(BUCKET_SEAL_RECALL_MARKER, ...)`.
2. **Old fn callers** — `render_bucket_seal_recall` is called once (in `append_bucket_seal_recall`). After renaming to `render_recall_block`, update that call. `grep -rn render_bucket_seal_recall src-tauri/src` to catch any other caller.
3. **gbrain leg** — `if let Some(adapter) = state.memory_adapters.get("gbrain") { match adapter.recall(query, 6, RecallOpts { namespace: None, category: None, session_id: None, min_score: None }).await { Ok(entries) if !entries.is_empty() => { if let Some(block) = render_recall_block(GBRAIN_RECALL_MARKER, &entries, 1500) { delegate.append_memory_context(&format!("\n\n{block}")); } }, Ok(_) => {}, Err(e) => tracing::debug!(error = %format!("{e:#}"), "gbrain recall skipped"), } }`. Verify `RecallOpts` construction (lifetime — all None, fine).
4. **Helper rename** — `append_bucket_seal_recall` → `append_unified_recall`. Update BOTH call sites (chat + agent). `grep -rn append_bucket_seal_recall src-tauri/src` to find them (PR15 wired chat send + agent send).
5. **Order** — bucket_seal leg first (primary), then gbrain leg, both appended after the legacy recall (the helper is already called after legacy set_memory_context per PR15).
6. **GBRAIN_RECALL_MARKER** — `pub const GBRAIN_RECALL_MARKER: &str = "## Relevant Memory (gbrain)";`.
7. **Pre-commit hooks** — no `--no-verify`.

---

## Tasks

### Task 1: generalize the render block

- [ ] **Step 1: Update tests + add gbrain-marker test** in `agent/memory_recall_block.rs`:
  - Change the 3 existing tests to call `render_recall_block(BUCKET_SEAL_RECALL_MARKER, ...)`.
  - Add:
```rust
    #[test]
    fn renders_with_gbrain_marker() {
        let block = render_recall_block(GBRAIN_RECALL_MARKER, &[entry("g1", "page recap", 0.8)], 1500).unwrap();
        assert!(block.contains(GBRAIN_RECALL_MARKER));
        assert!(block.contains("page recap"));
    }
```

- [ ] **Step 2: Run → fail.** `cd src-tauri && cargo test --lib agent::memory_recall_block 2>&1 | tail`

- [ ] **Step 3: Implement** — rename `render_bucket_seal_recall` to `render_recall_block(marker: &str, entries, budget)`, replace the hardcoded `BUCKET_SEAL_RECALL_MARKER` with `marker`, add `pub const GBRAIN_RECALL_MARKER`. Keep `BUCKET_SEAL_RECALL_MARKER`.

- [ ] **Step 4: Run → pass.** Commit:
```bash
git add src-tauri/src/agent/memory_recall_block.rs
git commit -m "refactor(agent): generalize render_recall_block(marker) + GBRAIN_RECALL_MARKER (PR18.1)"
```

### Task 2: unified fan-out helper + wiring

- [ ] **Step 1: Read PR15's `append_bucket_seal_recall`** (tauri_commands.rs:1794) + its call sites (`grep -rn append_bucket_seal_recall src-tauri/src`).

- [ ] **Step 2: Rename + extend** to `append_unified_recall`:

```rust
/// Fan out proactive recall to bucket_seal + gbrain, appending one labelled
/// sub-section per backend to the memory context. Best-effort + sectioned
/// (no cross-backend ranking). memU is excluded (injected via the legacy
/// MemoryRecallEngine path). Never blocks the turn.
async fn append_unified_recall(
    state: &AppState,
    delegate: &mut crate::agent::dispatcher::ChatDelegate,
    query: &str,
) {
    if query.trim().is_empty() {
        return;
    }
    use crate::agent::memory_recall_block::{render_recall_block, BUCKET_SEAL_RECALL_MARKER, GBRAIN_RECALL_MARKER};

    // bucket_seal leg (semantic + FTS hybrid).
    let bs = state.bucket_seal_adapter.recall_hybrid(query, None, 6).await;
    if let Some(block) = render_recall_block(BUCKET_SEAL_RECALL_MARKER, &bs, 1500) {
        delegate.append_memory_context(&format!("\n\n{block}"));
        tracing::info!(entries = bs.len(), "bucket_seal recall injected");
    }

    // gbrain leg (long-term knowledge graph search).
    if let Some(adapter) = state.memory_adapters.get("gbrain") {
        let opts = crate::memory_adapter::types::RecallOpts {
            namespace: None, category: None, session_id: None, min_score: None,
        };
        match adapter.recall(query, 6, opts).await {
            Ok(entries) if !entries.is_empty() => {
                if let Some(block) = render_recall_block(GBRAIN_RECALL_MARKER, &entries, 1500) {
                    delegate.append_memory_context(&format!("\n\n{block}"));
                    tracing::info!(entries = entries.len(), "gbrain recall injected");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::debug!(error = %format!("{e:#}"), "gbrain recall skipped (best-effort)"),
        }
    }
}
```

- [ ] **Step 3: Update the call sites** — replace `append_bucket_seal_recall(...)` with `append_unified_recall(...)` at the chat send site + the agent send site(s). (For the agent path, PR15 pre-computed the bucket_seal block as a String outside the spawn; the gbrain leg must follow the SAME pattern — pre-compute outside the spawn if `state` isn't moveable. **Verify the agent-site structure**: if PR15 pre-computed a `bucket_seal_recall_block_for_spawn: Option<String>` outside the spawn and appended inside, extend that to also pre-compute a `gbrain_recall_block_for_spawn` and append both inside. If the agent site calls the helper directly with `&state` available + async, just rename the call.)

**Adaptation:** the agent-path spawn constraint (state not moveable) is the one tricky bit — mirror exactly how PR15 handled it. If PR15 used the helper directly at the agent site (state available pre-spawn), the rename suffices. If it pre-computed blocks, pre-compute both legs. Read the PR15 agent-site code (`grep -n append_bucket_seal_recall` + surrounding ~30 lines) before changing.

- [ ] **Step 4: Full build.** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`

- [ ] **Step 5: Commit:**
```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(tauri): unified recall fan-out (bucket_seal + gbrain, sectioned) (PR18.2)"
```

### Task 3: Verification

- [ ] `cd src-tauri && cargo test --lib agent::memory_recall_block 2>&1 | tail` (4 pass: 3 updated + 1 gbrain marker).
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "memory_recall_block|tauri_commands"` (clean).
- [ ] `grep -rn "append_bucket_seal_recall\|render_bucket_seal_recall" src-tauri/src` (empty — fully renamed, no stale callers).
- [ ] `grep -rn "append_unified_recall" src-tauri/src/tauri_commands.rs` (defined + called at all PR15 sites).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **Additive check**: `git diff main -- src-tauri/src/tauri_commands.rs | grep "^-" | grep -iE "set_memory_context|MemoryRecallEngine|memu"` (no legacy deletions).
- [ ] **memU-not-fanned-out check**: confirm `append_unified_recall` has NO `memory_adapters.get("memu")` / no memu leg.

---

## Self-Review

- ✅ Spec coverage: bucket_seal leg (existing, retained) + gbrain leg (new), sectioned, best-effort, memU excluded, additive.
- ✅ No placeholders. The agent-site spawn handling is a concrete "mirror PR15's pattern" instruction with a read-first directive.
- ✅ Type consistency: `render_recall_block(marker: &str, &[MemoryEntry], usize) -> Option<String>`, `append_unified_recall(&AppState, &mut ChatDelegate, &str)`, `RecallOpts` 4-field all-None, `adapter.recall(query, 6, opts)` matches the trait. `GBRAIN_RECALL_MARKER`/`BUCKET_SEAL_RECALL_MARKER` consts.
- Decisions: sectioned (no cross-scale ranking); memU excluded (legacy-path dup avoidance); gbrain via the registry `Arc<dyn MemoryAdapter>` (bucket_seal via the concrete handle for `recall_hybrid`); each leg independently best-effort.
