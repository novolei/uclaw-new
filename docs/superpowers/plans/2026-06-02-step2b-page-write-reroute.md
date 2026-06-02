# Step 2b — Page-Write Reroute Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Reroute the three page-write pipelines from gbrain → memory_graph EntityPage + bucket_seal `pages`, then remove the gbrain write leg + the `gbrain_dual_write_pages_enabled` flag + the dead `gbrain_put_page` command. Closes Step 2a's transient gap. gbrain stays alive (read tools + boot = 2c/2d).

**Architecture:** `write_page(store, adapter, space, slug, md)` (EntityPage rich + bucket_seal recall projection) replaces `dual_write_page` (gbrain + shadow). Order: add `write_page` → reroute the 3 callers → remove `dual_write_page`+flag+dead-cmd → verify. The compiler is the guard. `cargo build`/`clippy`/`test` verify.

**Key facts (recon, file:line):**
- `memory_adapter/page_dual_write.rs`: `dual_write_page(mcp, adapter, slug, md, dual_enabled)` (gbrain `browse::put_page` + `shadow_write_page` bucket_seal); helpers `markdown_to_page`, `shadow_write_page` exist here.
- 2a APIs: `MemoryGraphStore::entity_page_put(space, slug, raw_markdown)` (rich, +auto-link/[[slug]] normalize); `memory_adapter::pages::put_page(adapter, &Page)` (bucket_seal).
- **chat_extractor**: `agent/dispatcher/turn_runner.rs:~165-200` — inside a `tokio::spawn` capturing `text_clone, llm, mcp_mgr`; the `for proposal in actionable { mgr.call_tool("gbrain","put_page", {slug, content}) }` block. `ChatDelegate` struct `dispatcher/mod.rs:72` with `Option<Arc<...>>` fields + setters (`set_gene_retriever` :341, `set_gene_repo` :346). Built at `tauri_commands.rs:1932, 11154, 15047` (where `state`/AppState has `memory_graph_store` + `bucket_seal_adapter`).
- **Scheme-A**: `memorization/service.rs` `ingest_draft_file` → `dual_write_page(...)`. MemorizationService has `graph_store` (`:56`, set via `set_graph_store` :151) + a bucket_seal adapter (Step 3b-3).
- **memory_policy**: `classifier.rs:11` DurableKnowledge → `GbrainWrite`; `targets/gbrain.rs:41 GbrainPolicyTarget` executes it (`build_gbrain_write_request` + dual_write).
- **flag** `gbrain_dual_write_pages_enabled`: `memubot_config.rs:423/664/764` (+ tests :1737-1749); reads at `tauri_commands.rs:1390/14950`, `main.rs:247` (`set_dual_write_pages_enabled`), `app.rs:1096`.
- dead cmd: `gbrain_put_page` (`tauri_commands.rs:1380` + macro `main.rs:1285`) — 2a repointed the FE; confirm no caller.

---

## Task 1: `write_page` (alongside `dual_write_page`, no removal yet)

**Files:** `src-tauri/src/memory_adapter/page_dual_write.rs`.

- [ ] **Step 1:** Add:
```rust
/// Write a page to BOTH layers: memory_graph EntityPage (rich, WikiView) +
/// bucket_seal `pages` (recall projection). Replaces the gbrain dual-write.
pub async fn write_page(
    store: &std::sync::Arc<crate::memory_graph::store::MemoryGraphStore>,
    adapter: &std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
    space_id: &str,
    slug: &str,
    markdown: &str,
) -> anyhow::Result<()> {
    store.entity_page_put(space_id, slug, markdown)?;   // authoritative
    shadow_write_page(adapter, slug, markdown).await;    // best-effort bucket_seal (existing helper, logs+swallows)
    Ok(())
}
```
(`shadow_write_page` already does `markdown_to_page` + `pages::put_page`, best-effort.)
- [ ] **Step 2: Test** — fixture `MemoryGraphStore` (in-memory) + a fake/bucket_seal adapter → `write_page` creates an EntityPage (`find_entity_page_by_slug` Some) + a bucket_seal page (`pages::get_page` Some); slug re-write → new EntityPage version.
- [ ] **Step 3: Build + test** — `cargo build 2>&1 | grep -E "^error"` (none); `cargo test --lib memory_adapter::page_dual_write` (green). `dual_write_page` still exists (callers unchanged) — fine.
- [ ] **Step 4: Commit** — `feat(memory): write_page (EntityPage + bucket_seal) — page-write reroute primitive (Step 2b)`

---

## Task 2: Thread store+adapter into ChatDelegate + reroute chat_extractor

**Files:** `agent/dispatcher/mod.rs` (ChatDelegate fields + setters), `agent/dispatcher/turn_runner.rs` (the put_page block), `tauri_commands.rs` (3 construction sites).

- [ ] **Step 1:** Add to `ChatDelegate` (mirror `retriever`): `entity_page_store: Option<Arc<MemoryGraphStore>>` + `page_adapter: Option<Arc<dyn MemoryAdapter>>`, with setters `set_page_writers(&mut self, store, adapter)` (one setter for both). Default `None` in `ChatDelegate::new`.
- [ ] **Step 2:** At the 3 `ChatDelegate::new` sites (`tauri_commands.rs:1932, 11154, 15047`), after construction call `delegate.set_page_writers(Arc::clone(&state.memory_graph_store), Arc::clone(&state.bucket_seal_adapter) as Arc<dyn MemoryAdapter>)` (mirror the adjacent `set_gene_retriever`/`set_gene_repo` calls; confirm `state` field names).
- [ ] **Step 3: Reroute the put_page block** (`turn_runner.rs:~165-200`): the block runs in a `tokio::spawn`. Before the spawn, clone the writers out of `self` (`let page_store = self.entity_page_store.clone(); let page_adapter = self.page_adapter.clone();`) and `move` them into the task (like `mcp_mgr`/`text_clone`). Replace the `for proposal { mgr.call_tool("gbrain","put_page",...) }` loop body with:
```rust
if let (Some(store), Some(adapter)) = (&page_store, &page_adapter) {
    for proposal in actionable {
        if let Err(e) = crate::memory_adapter::page_dual_write::write_page(
            store, adapter, &space_id, &proposal.slug, &proposal.content,
        ).await {
            tracing::warn!(slug = %proposal.slug, error = %e, "[ChatDelegate] extractor write_page failed");
        }
    }
}
```
   (Use the turn's `space_id` — find what's in scope; the dispatcher knows the session's space. If only `"default"` is available, use it.) Drop the `mcp_mgr` use FROM THIS BLOCK (keep mcp_mgr if used elsewhere in the task). Keep the cost-tag/chip-event/confidence gate.
- [ ] **Step 4: Build + clippy + test** — clean; `cargo test --lib agent::dispatcher` (no regressions; the chat_extractor tests, if any, updated for the writer path).
- [ ] **Step 5: Commit** — `refactor(agent): chat_extractor writes EntityPage+bucket_seal via write_page (no gbrain put_page) (Step 2b)`

---

## Task 3: Reroute Scheme-A draft ingestion + memory_policy

**Files:** `memorization/service.rs`, `memory_policy/targets/gbrain.rs` (+ `classifier.rs`/`types.rs` if retiring the action).

- [ ] **Step 1: Scheme-A** (`ingest_draft_file`): replace the `dual_write_page(mcp, adapter, slug, merged_md, dual_enabled)` call with `write_page(&graph_store, &adapter, space, slug, merged_md)`. The service has `graph_store` (via `set_graph_store`, read it like `persist_memorize_results` does at `:999`) + the bucket_seal adapter. Drop the `dual_enabled` flag read + the `mcp` arg.
- [ ] **Step 2: memory_policy** — recon `GbrainPolicyTarget::execute` (`targets/gbrain.rs`): repoint it to call `write_page` (it has, or its ctor can take, the store + adapter) instead of the gbrain dual_write. Keep the `GbrainWrite` action/classifier as-is (the action label stays; only the target's backend swaps — rename to `PageWrite`/`EntityPageWrite` is OPTIONAL/cosmetic, note for later). If `GbrainPolicyTarget` can't reach a store/adapter cleanly, report — but it's constructed in the policy-router wiring where `state` is available.
- [ ] **Step 3: Build + clippy + test** — clean; `cargo test --lib memorization memory_policy` (green; update tests that asserted the gbrain write path).
- [ ] **Step 4: Verify no caller of `dual_write_page` remains** — `grep -rn "dual_write_page" src/` → only its definition (+ tests). If `gbrain_put_page` cmd still calls it, that's removed in T4.
- [ ] **Step 5: Commit** — `refactor(memory): Scheme-A draft + policy GbrainWrite reroute to write_page (Step 2b)`

---

## Task 4: Remove `dual_write_page` + flag + dead `gbrain_put_page` cmd

**Files:** `page_dual_write.rs`, `memubot_config.rs`, `main.rs`, `tauri_commands.rs`, `app.rs`.

- [ ] **Step 1:** Confirm `gbrain_put_page` (`tauri_commands.rs:1380`) has no FE/Rust caller (`grep -rn "gbrain_put_page\|invoke('gbrain_put_page" ui/src src-tauri/src` → only def + macro). If dead → delete the fn + the macro entry (`main.rs:1285`). (If a caller remains, repoint to `write_page` instead.)
- [ ] **Step 2:** Delete `dual_write_page` (now caller-less) from `page_dual_write.rs` + drop the now-unused `browse::put_page` import there (leave `gbrain::browse` itself — agent put_page tool = 2c).
- [ ] **Step 3:** Remove `gbrain_dual_write_pages_enabled`: the field + `default_gbrain_dual_write_pages_enabled` + the `MemoryOsConfig` default + the serde tests (`memubot_config.rs:423/664/764/1734-1749`); `set_dual_write_pages_enabled` + its call (`main.rs:247`); the reads (`tauri_commands.rs:1390/14950`, `app.rs:1096`). The compiler enumerates every site.
- [ ] **Step 4: Build + clippy** — `cargo build 2>&1 | grep -E "^error"` (none); `cargo clippy --lib` (none).
- [ ] **Step 5: Commit** — `refactor(memory): remove dual_write_page + gbrain_dual_write_pages_enabled flag + dead gbrain_put_page cmd (Step 2b)`

---

## Task 5: Whole-slice verification + ship

- [ ] **Step 1:** `cargo build` + `cargo clippy --lib` clean; `cargo test --lib memory_adapter memorization memory_policy agent::dispatcher memory_graph` green.
- [ ] **Step 2: Gates:** `grep -rn "dual_write_page\|gbrain_dual_write_pages_enabled\|gbrain_put_page" src/` → empty (gone). `grep -rn "call_tool(\"gbrain\", \"put_page\"\|mcp__gbrain__put_page" src/` → no PIPELINE write callers (the agent tool, if any, is 2c). bucket_seal/EntityPage writes confirmed in the rerouted sites.
- [ ] **Step 3: Ship** — push → PR (Commits table T1-T4) → rebase-merge → sync → cleanup → reindex.
- [ ] **Step 4: Post-merge soak (manual):** have a substantive conversation → the chat-extractor's proactive pages now appear in WikiView (EntityPages) — confirming 2a's transient gap is closed; no gbrain `put_page` in the log for the pipeline paths.

---

## Self-Review

- **Spec coverage:** write_page (T1), dispatcher reroute (T2), Scheme-A+policy (T3), removal of dual_write+flag+dead-cmd (T4), verify (T5). ✓
- **Ordering keeps each commit compiling:** add write_page (T1) → reroute callers (T2/T3) → remove the now-unused dual_write+flag (T4). ✓
- **Dispatcher threading** (the flagged risk): mirrors the `set_gene_retriever` injection pattern; writers captured into the spawned task like `mcp_mgr`. ✓
- **No placeholders:** real signatures + file:line + the capture-into-spawn detail. The `space_id` source + the policy-target store reachability are the 2 "confirm in impl" points (flagged). ✓
- **Finish-line:** after T4, no pipeline writes gbrain + the flag is gone; gbrain stays alive only for its read tools + the agent put_page tool (2c) + boot (2d). ✓
