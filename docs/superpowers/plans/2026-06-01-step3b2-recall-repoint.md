# Step 3b-2 — Recall Repoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Repoint the three memU read/recall consumers to bucket_seal: the two agent memory tools (→ `recall_hybrid`/`list`) and memory_graph passive recall (drop the L3 memU vector leg, add a bucket_seal semantic section).

**Architecture:** Callers use the **concrete** `Arc<BucketSealAdapter>` (= `state.bucket_seal_adapter`) and call `recall_hybrid(query, namespace, max)` — the trait `MemoryAdapter::recall` is **FTS-only** (verified: it queries `mem_tree_chunks_fts MATCH`, no semantic leg), so the trait object is insufficient for semantic recall. This matches the Step 1b skills precedent (`memory_adapter/skills.rs:95` takes `&Arc<BucketSealAdapter>` + `recall_hybrid`).

**Tech Stack:** Rust, `recall_hybrid`/`list` on `BucketSealAdapter`, `MemoryEntry`.

**Key facts (recon-confirmed):**
- `state.bucket_seal_adapter: Arc<crate::memory_bucket_seal::BucketSealAdapter>` (concrete, `app.rs:222`).
- `recall_hybrid(&self, query: &str, namespace: Option<&str>, max_entries: usize) -> Vec<MemoryEntry>` (infallible — returns `Vec`, `adapter.rs:200`). `BucketSealAdapter::list(...)` exists (`adapter.rs:600`, the trait method impl).
- `MemoryEntry` (`memory_adapter/types.rs`): `id, key, content, namespace, category: MemoryCategory, timestamp (RFC3339 String), session_id, score: Option<f64>`.
- Tools wired at `agent/tools/registry_build.rs:79-83` (`register_memu_tools(&mut tools, state.memu_client.clone(), skill_adapter_for_memu)` where `skill_adapter_for_memu = Some(Arc::clone(&state.bucket_seal_adapter) as Arc<dyn MemoryAdapter>)`). `register_memu_tools` at `memu_tools.rs:697`.
- `MemoryRecallEngine` (`recall.rs:335`): `new(store, memu_client, config)`. memU used ONLY in L3 (`recall.rs:1107, 1123, 1128` + field `337/344/349` + import `:10`) → after L3 drop, the field can be removed entirely. 4 construction sites: `tauri_commands.rs:2314`, `proactive/proactive_recall.rs:262` + `:395`, `proactive/hybrid_search.rs:281`.
- L3 `layer_relevant` (`recall.rs:1100`-~1290): FTS leg (`fts_rank_map` by `node_id`) + memU vector leg (`vector_rank_map`), fused via RRF/Weighted with Phase-5 boosts.

---

## Task 1: Part 1 — both agent tools → bucket_seal (drop memU client)

**Files:** `src-tauri/src/agent/tools/memu_tools.rs`, `src-tauri/src/agent/tools/registry_build.rs`

- [ ] **Step 1: Read** `memu_tools.rs` fully for `MemuMemoryTool` (struct + `new`/`with_skill_adapter` + `execute` paths at ~161 skill-ranking, ~220 list-all, ~297 retrieve) and `MemuTodosTool` (struct + `new` + `execute` ~518-589), plus `register_memu_tools` (~697). Note the exact output JSON each path emits (you must preserve the shape).

- [ ] **Step 2: `MemuMemoryTool`** — make the bucket_seal adapter REQUIRED and drop the memU client:
  - Struct: replace `client: Option<Arc<MemUClient>>` + `skill_adapter: Option<Arc<dyn MemoryAdapter>>` with a single required concrete handle:
    ```rust
    adapter: std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>,
    space_id: String,
    ```
    (Keep `space_id`. The skill-ranking fast path currently uses `skill_adapter` via `crate::memory_adapter::skills::*` — those helpers take `&Arc<BucketSealAdapter>`, so pass `&self.adapter`.)
  - `new(adapter: Arc<BucketSealAdapter>)`; drop `with_skill_adapter`.
  - **list-all path** (`is_list_all_memory_query`): replace `client.list_items(None,None,Some(list_limit),Some(0),None)` with
    ```rust
    let entries = self.adapter.list(Some(&self.space_id), Some(list_limit)).await.unwrap_or_default();
    ```
    (Confirm `BucketSealAdapter::list` exact signature at `adapter.rs:600` — it's the trait method `list(&self, namespace: Option<&str>, limit: Option<usize>)` or similar; match it. It returns `Result<Vec<MemoryEntry>>`.) Map each `MemoryEntry` to the existing JSON: `{"content": e.content, "type": category_str(&e.category), "categories": [category_str(&e.category)], "created_at": e.timestamp, "id": e.id}`.
  - **retrieve path**: replace the `client.retrieve_with_context(&query, None, retrieve_limit, enrich_categories)` (+ its 15s timeout wrapper — keep the timeout as a harmless safeguard around the now-local call) with
    ```rust
    let entries = self.adapter.recall_hybrid(&query, Some(&self.space_id), retrieve_limit).await;
    ```
    Map to the existing `"memories"` JSON: `{"content": e.content, "type": category_str(&e.category), "relevance": e.score.unwrap_or(0.0), "categories": [category_str(&e.category)]}` plus the outer `{query, count, limit, enriched: false}`. (`enrich_categories` no longer has meaning — keep the input field for schema compat but ignore it, or note it as deprecated in the description.)
  - Add a small helper `fn category_str(c: &MemoryCategory) -> &'static str` (or reuse an existing enum→str if one exists — grep `impl.*MemoryCategory` first).

- [ ] **Step 3: `MemuTodosTool`** — add the adapter, drop the client:
  - Struct: `adapter: Arc<BucketSealAdapter>`, `space_id: String` (default `"default"`).
  - `new(adapter: Arc<BucketSealAdapter>)`.
  - Replace `client.retrieve_with_context(query, Some(&["event","knowledge"]), 20, false)` with `self.adapter.recall_hybrid(query, Some(&self.space_id), 20).await`.
  - Todo filter: bucket_seal `MemoryEntry` has no `categories` → filter on `e.content.to_lowercase().contains("todo") || e.content.contains("待办")`. Output: `{"content": e.content, "categories": [], "created_at": e.timestamp}` (or derive a single category string). Preserve the outer `{todos, status_filter, count}` shape.

- [ ] **Step 4: `register_memu_tools`** — drop the memU client param:
  ```rust
  pub fn register_memu_tools(
      registry: &mut ToolRegistry,
      adapter: std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>,
  ) {
      registry.register(MemuMemoryTool::new(adapter.clone()));
      registry.register(MemuTodosTool::new(adapter));
  }
  ```

- [ ] **Step 5: `registry_build.rs:79-83`** — call with the concrete adapter:
  ```rust
  crate::agent::tools::memu_tools::register_memu_tools(
      &mut tools,
      std::sync::Arc::clone(&state.bucket_seal_adapter),
  );
  ```
  Remove the now-unused `skill_adapter_for_memu` local + the `state.memu_client.clone()` arg. (If `state.memu_client` is no longer referenced in this file, remove its import; do NOT touch other files' memu usage.)

- [ ] **Step 6: Build + clippy + test**
  - `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` (none)
  - `cargo clippy --lib 2>&1 | grep -E "^error" | head` (none)
  - `cargo test --lib agent::tools::memu_tools 2>&1 | tail -15` (fix/extend any tests that constructed the tools with a memU client — they now take an adapter; use a fresh `BucketSealAdapter` like `memory_adapter/skills.rs::fresh_bucket_seal_adapter` does, or `BucketSealAdapter::new(...)` with a temp store).

- [ ] **Step 7: Commit** (no --no-verify)
  ```bash
  git add src-tauri/src/agent/tools/memu_tools.rs src-tauri/src/agent/tools/registry_build.rs
  git commit -m "refactor(agent): MemuMemoryTool + MemuTodosTool recall via bucket_seal; drop memU client (Step 3b-2)"
  ```

---

## Task 2: Part 2 wiring — thread bucket_seal adapter into MemoryRecallEngine

**Files:** `src-tauri/src/memory_graph/recall.rs`; the 4 construction sites.

This task ADDS the adapter field WITHOUT removing the memU leg yet (keeps behavior; compiles green). Task 3 does the cutover.

- [ ] **Step 1: Add the field** to `MemoryRecallEngine` (`recall.rs:335`) — add alongside `memu_client` (do not remove memu_client yet):
  ```rust
      bucket_seal_adapter: Option<Arc<crate::memory_bucket_seal::BucketSealAdapter>>,
  ```
  Add the param to `new(store, memu_client, bucket_seal_adapter, config)` and assign it.

- [ ] **Step 2: Update the 4 construction sites** to pass the adapter (as `Some(Arc::clone(&state.bucket_seal_adapter))` where state is reachable; `proactive_recall.rs`/`hybrid_search.rs` — confirm the field is reachable there, likely via a struct holding the adapter or `state`; if only `memu_client` is reachable, thread `bucket_seal_adapter` through the same struct that carries `memu_client`):
  - `tauri_commands.rs:2314`
  - `proactive/proactive_recall.rs:262` and `:395`
  - `proactive/hybrid_search.rs:281`
  For any test/utility constructor of `MemoryRecallEngine`, pass `None`.

- [ ] **Step 3: Build + test** — `cargo build 2>&1 | grep -E "^error"` (none); `cargo test --lib memory_graph::recall 2>&1 | tail -15` (no regressions; behavior unchanged).

- [ ] **Step 4: Commit**
  ```bash
  git add src-tauri/src/memory_graph/recall.rs src-tauri/src/tauri_commands.rs src-tauri/src/proactive/proactive_recall.rs src-tauri/src/proactive/hybrid_search.rs
  git commit -m "refactor(recall): thread bucket_seal_adapter into MemoryRecallEngine (Step 3b-2)"
  ```

---

## Task 3: Part 2 cutover — drop L3 memU leg + add bucket_seal semantic section

**Files:** `src-tauri/src/memory_graph/recall.rs`

Semantic capability moves from the memU-fused L3 leg to a dedicated bucket_seal section IN THE SAME COMMIT — no semantic-less intermediate.

- [ ] **Step 1: Read** `layer_relevant` (`recall.rs:1100`-~1290), the `MemoryRecallPlan` struct, `build_recall_plan_with_time` (~397), `format_recall_for_prompt` (~472-831), and `MemoryRecallConfig` (~68-174) end-to-end before editing.

- [ ] **Step 2: Drop the memU vector leg in `layer_relevant`:**
  - `fts_limit` (1107-1112): collapse to the multiplier path only — `let fts_limit = (self.config.seed_limit as f32 * self.config.fts_fallback_limit_multiplier) as usize;`
  - Delete `vector_results` (1122-1137), `vector_rank_map` (1145-1150), and the vector half of `all_ids` (1159-1163).
  - In the fusion loop: drop `vec_r`; RRF uses only the FTS term; Weighted uses only `fts_weight * fts_score`. Drop `vec_r` from the `scored` tuple + the candidate-building `else` "vector-only result" branch (every candidate now comes from `fts_rank_map`). Keep the Phase-5 boost block unchanged.
  - Remove now-dead config reads if they become unused ONLY within this function (`vector_weight`/`rrf_k` may still be used elsewhere — grep before deleting any config field; prefer leaving config fields in place to avoid churn, just stop reading the vector side).

- [ ] **Step 3: Remove `memu_client` from the engine** (now fully unused): delete the field (`recall.rs:337`), the `new` param + assignment (`344/349`), the `use crate::memu::client::MemUClient;` import (`:10`). Update the 4 construction sites (Task 2's) + any test ctor to drop the memU arg. Verify: `grep -n "memu\|MemUClient" src/memory_graph/recall.rs` → no matches.

- [ ] **Step 4: Add the bucket_seal semantic leg + plan field:**
  - `MemoryRecallPlan` struct: add `pub semantic_summaries: Vec<MemoryEntry>` (import `MemoryEntry` from `crate::memory_adapter::types`). Initialize it (default empty) everywhere the plan is constructed.
  - In `build_recall_plan_with_time` (after the L1-L5 legs), add:
    ```rust
    let semantic_summaries = if let Some(ref bs) = self.bucket_seal_adapter {
        bs.recall_hybrid(user_input, None, self.config.seed_limit).await
    } else {
        Vec::new()
    };
    ```
    (namespace = `None` — recall across all bucket_seal trees; the plan author chose None over space-scoping because bucket_seal namespaces are source/topic trees, not memory_graph spaces. Revisit if smoke checks show cross-space bleed.) Assign into the plan.

- [ ] **Step 5: Render the section** in `format_recall_for_prompt` — before the closing `</memory_context>`:
  ```rust
  if !plan.semantic_summaries.is_empty() {
      out.push_str("\n## 语义摘要 / Semantic Summaries (bucket_seal)\n");
      for e in &plan.semantic_summaries {
          // budget-aware snippet, matching the helper used by other sections
          out.push_str(&format!("- {}\n", budgeted_snippet(&e.content, 160)));
      }
  }
  ```
  Use the SAME token-budget helper/pattern the other sections use (read how L3/L5 sections truncate — match it; do not invent a new budgeting path). Give the section a budget allocation consistent with the existing `TokenBudgetAllocation` scheme.

- [ ] **Step 6: Build + clippy + test**
  - `cargo build 2>&1 | grep -E "^error"` (none); `cargo clippy --lib 2>&1 | grep -E "^error"` (none)
  - `cargo test --lib memory_graph::recall 2>&1 | tail -20` (fix tests referencing the removed memU leg / the new field)
  - Add a unit test: a `MemoryRecallEngine` with a fake/empty `bucket_seal_adapter=None` builds a plan with empty `semantic_summaries` and L3 returns FTS-ranked candidates; and `format_recall_for_prompt` omits the section when empty / renders it when populated.

- [ ] **Step 7: Commit**
  ```bash
  git add src-tauri/src/memory_graph/recall.rs
  git commit -m "refactor(recall): drop L3 memU vector leg; add bucket_seal semantic section (Step 3b-2)"
  ```

---

## Task 4: Whole-slice verification + ship

- [ ] **Step 1: Full build + clippy** — `cargo build` + `cargo clippy --lib` clean.
- [ ] **Step 2: Gates**
  - `grep -rn "memu\|MemUClient\|retrieve_with_context\|list_items" src-tauri/src/memory_graph/recall.rs src-tauri/src/agent/tools/memu_tools.rs` → no matches (these consumers are fully off memU).
  - `grep -rn "MemoryRecallEngine::new" src-tauri/src/` → every site passes the adapter, none passes a memU client.
- [ ] **Step 3: Targeted tests** — `cargo test --lib agent::tools::memu_tools && cargo test --lib memory_graph::recall && cargo test --lib memory_bucket_seal` all green.
- [ ] **Step 4: Ship** — push branch → `gh pr create` (Commits table: T1 tools, T2 wiring, T3 cutover) → rebase-merge → sync parent main → cleanup worktree → reindex.

---

## Self-Review

- **Spec coverage:** Part 1 tools (T1), engine wiring (T2), L3 drop + semantic section (T3), gates (T4). ✓
- **No placeholders:** real call signatures + output mappings + removal targets with line anchors; the L3 fusion edit is described precisely (read-first is required for a 160-line function, not a placeholder). ✓
- **Concrete adapter decision:** locked — trait `recall` is FTS-only, so callers use concrete `Arc<BucketSealAdapter>` + `recall_hybrid` (Step 1b precedent). Consistent across T1/T2/T3. ✓
- **No regression intermediate:** T2 adds the field (behavior unchanged); T3 drops the memU leg AND adds the bucket_seal section in one commit (semantic capability never absent between commits). ✓
- **Finish-line:** tools + engine fully off memU; `MemUClient::retrieve`/`list_items` methods + bridge remain for 3b-3/3b-4 (per spec, not half-cut). ✓
