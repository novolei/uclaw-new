# Step 3b-3 — Native Rust Memory Extractor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace memU's `memorize` LLM extraction with a native Rust `MemoryExtractor` (single JSON-mode call) and cut over the three write consumers, so nothing in the app calls memU for write/extraction.

**Architecture:** `MemoryExtractor` mirrors the production `gbrain/chat_extractor.rs` (prompt → `MemoryOsLlm::complete_text` → lenient `serde_json` parse). Reflection's items→memory_graph mapping is factored into a reusable `persist_items_to_graph`. ReflectionEngine + ProactiveService persist via it; MemorizationService keeps its facets/drafts routing. D1=single call, D3=ProactiveService upgraded to persist to memory_graph.

**Tech Stack:** Rust, `MemoryOsLlm`, `serde_json`, `MemoryGraphStore`, `BucketSealAdapter`.

**Key facts (recon-confirmed):**
- Template: `gbrain/chat_extractor.rs` (full pattern: `extract_system_prompt`, `complete_text("gbrain_extract", sys, user, max)`, `parse_proposals` + `strip_markdown_fences`, `MockMemoryOsLlm` tests). Clone it.
- `MemoryOsLlm::complete_text(cost_tag, system_prompt, user_prompt, max_tokens) -> Result<MemoryOsLlmOutput>` (`memory_graph/memory_os_llm.rs:62`); `.text` field holds the completion. `MockMemoryOsLlm { canned_text, .. }` for tests.
- ReflectionEngine (`memory_graph/reflection.rs`): struct `{ store: Arc<MemoryGraphStore>, memu_client: Option<Arc<MemUClient>> }` (`:271`), `new(store, memu_client)` (`:277`). `reflect()` flow: memu-availability gate (`:365-389`) → pre-filters length/greeting/command (`:391-461`) → coverage check `memu.retrieve` (`:472`) → `memu.memorize` (`:530`) → per-item persistence (`:594-738`: `map_memu_type_to_kind` → create_node/version/route/keyword/boot). `map_memu_type_to_kind` `:12-22`, `generate_route_path` `:50-69`.
- ProactiveService holds `memory_graph_store: Arc<MemoryGraphStore>` (`:356`), bucket_seal adapter, embedder. memorize_with_config at `service.rs:2295` (skill fallback) + `:2443` (conversation/multimodal). Returns count only.
- MemorizationService (`memorization/service.rs`): `memu.memorize` `:251`; `persist_memorize_results` `:1013` routes by kind → user_profile_facets (SQLite) `:1043`, episode→`create_item`→memu.db `:1093`, others→gbrain_drafts `:1108`.
- `BucketSealAdapter::recall_hybrid(query, namespace, max) -> Vec<MemoryEntry>` for the coverage check.
- `MemoryOsLlmClient` (`memory_os_llm.rs:103`) wraps `ProviderService` — find how existing consumers build it (grep `MemoryOsLlmClient::new`) to build the shared extractor.

---

## Task 1: `MemoryExtractor` module

**Files:** Create `src-tauri/src/memory_graph/extractor.rs`; modify `src-tauri/src/memory_graph/mod.rs` (`pub mod extractor;`).

- [ ] **Step 1: Read** `src-tauri/src/gbrain/chat_extractor.rs` (the template) and `memory_graph/memory_os_llm.rs` (the `MemoryOsLlm` trait + `MockMemoryOsLlm`). Also skim `~/Documents/memU/src/memu/prompts/memory_type/{profile,event,knowledge,behavior,skill,tool}.py` to port each type's objective+rules into the prompt below.

- [ ] **Step 2: Write `extractor.rs`** (clone chat_extractor's shape; output `ExtractedItem`):

```rust
//! Step 3b-3 — native memory extractor (replaces memU's `memorize`).
//!
//! Single JSON-mode LLM call (via `MemoryOsLlm`, cost-tag "memory_extract")
//! that extracts typed memory items from a conversation. Mirrors
//! `gbrain::chat_extractor`'s prompt→parse pattern. Output feeds
//! `reflection::persist_items_to_graph` (memory_graph node creation).

use std::sync::Arc;

use crate::memory_graph::memory_os_llm::MemoryOsLlm;

/// Hard cap on extractor `max_tokens` (typed-item lists are compact).
pub const EXTRACT_MAX_TOKENS: u32 = 1200;
/// Minimum input chars before invoking the LLM (short turns carry no memory).
pub const LLM_MIN_CHARS: usize = 12;

/// One extracted memory item. `memory_type` ∈ profile|event|knowledge|
/// behavior|skill|tool — the taxonomy `map_memu_type_to_kind` consumes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExtractedItem {
    pub memory_type: String,
    pub content: String,
}

pub struct MemoryExtractor {
    llm: Arc<dyn MemoryOsLlm>,
}

impl MemoryExtractor {
    pub fn new(llm: Arc<dyn MemoryOsLlm>) -> Self {
        Self { llm }
    }

    /// Extract typed items from conversation text. Empty Vec on short input,
    /// LLM error, or unparseable response (all logged + swallowed — never
    /// poisons the caller's flow).
    pub async fn extract(&self, conversation: &str) -> Vec<ExtractedItem> {
        if conversation.chars().count() < LLM_MIN_CHARS {
            return vec![];
        }
        let out = match self
            .llm
            .complete_text("memory_extract", extract_system_prompt(), conversation.trim(), EXTRACT_MAX_TOKENS)
            .await
        {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(error = %e, "memory_extractor: LLM call failed");
                return vec![];
            }
        };
        parse_items(&out.text)
    }
}

/// Ported from memU's per-type extraction rules (profile/event/knowledge/
/// behavior/skill/tool), condensed into one JSON-output prompt.
fn extract_system_prompt() -> &'static str {
    "You extract long-term memory items from a conversation for a personal AI \
assistant. Output a JSON array; each item is {\"memory_type\": <type>, \"content\": <text>}.\n\
\n\
Types and rules:\n\
- profile: stable user traits/preferences/attributes. <30 words. Direct facts, \
NO meta-phrasing (\"Enjoys table tennis\" NOT \"User said they like...\"). No timestamps.\n\
- event: time-bound concrete happenings (explicit or implicit time). <50 words. \
NOT general statements.\n\
- knowledge: objective factual knowledge discussed. <50 words. No personal opinions.\n\
- behavior: recurring patterns/routines/solutions (NOT one-off events). <50 words.\n\
- skill: an actionable skill the user/assistant demonstrated — name + what it does + when to use.\n\
- tool: a tool usage pattern/learning — include when to use it.\n\
\n\
Extract in the conversation's primary language (Chinese→Chinese, English→English). \
Only emit items carrying durable value; return [] for small talk / this-turn-only content. \
Output ONLY the JSON array — no markdown fences, no prose."
}

fn parse_items(raw: &str) -> Vec<ExtractedItem> {
    let body = strip_markdown_fences(raw.trim());
    let items: Vec<ExtractedItem> = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, raw_preview = &body.chars().take(120).collect::<String>(),
                "memory_extractor: failed to parse LLM response as JSON array");
            return vec![];
        }
    };
    const VALID: [&str; 6] = ["profile", "event", "knowledge", "behavior", "skill", "tool"];
    items
        .into_iter()
        .filter_map(|mut it| {
            let mt = it.memory_type.trim().to_lowercase();
            if !VALID.contains(&mt.as_str()) || it.content.trim().is_empty() {
                return None;
            }
            it.memory_type = mt;
            it.content = it.content.trim().to_string();
            Some(it)
        })
        .collect()
}

fn strip_markdown_fences(body: &str) -> &str {
    // identical to gbrain::chat_extractor::strip_markdown_fences — copy it verbatim
    let trimmed = body.trim();
    if !trimmed.starts_with("```") { return body; }
    let after_open = match trimmed.find('\n') { Some(idx) => &trimmed[idx + 1..], None => return body };
    if let Some(end) = after_open.rfind("```") { after_open[..end].trim_end() } else { after_open }
}
```

- [ ] **Step 3: Tests** (mirror chat_extractor's `MockMemoryOsLlm` tests): parse a canned `[{"memory_type":"profile","content":"..."}]` → 1 item; `[]` → empty; non-JSON → empty; markdown-fenced JSON → parsed; an item with an invalid `memory_type` or empty content is dropped; short input skips the LLM.

- [ ] **Step 4:** `pub mod extractor;` in `memory_graph/mod.rs`.

- [ ] **Step 5: Build + test** — `cargo build 2>&1 | grep -E "^error"` (none); `cargo test --lib memory_graph::extractor` (pass).

- [ ] **Step 6: Commit** — `feat(memory): native MemoryExtractor (single JSON-mode call, ported memU rules) (Step 3b-3)`

---

## Task 2: Factor `persist_items_to_graph` out of `reflect()`

**Files:** `src-tauri/src/memory_graph/reflection.rs`. Pure refactor — no behavior change.

- [ ] **Step 1: Read** `reflect()`'s per-item persistence block (`:594-738`) — the loop that maps each memU item to a memory_graph node (`map_memu_type_to_kind` → `create_node`/`create_version`/`create_route`/keywords/`add_to_boot`) + the dedup at `:587`.

- [ ] **Step 2: Extract** a free function (or `impl` method on a struct holding `&MemoryGraphStore`):
```rust
pub fn persist_items_to_graph(
    store: &MemoryGraphStore,
    space_id: &str,
    items: &[crate::memory_graph::extractor::ExtractedItem],
) -> anyhow::Result<usize>
```
Move the dedup + per-item create logic here verbatim, reading `item.memory_type` / `item.content` (was `item.get("memory_type")` / serde_json). Return the count of nodes created.

- [ ] **Step 3: Rewire `reflect()`** to convert its current memU `serde_json::Value` items → `Vec<ExtractedItem>` (extract `memory_type` + `content`/`summary` fields) and call `persist_items_to_graph(&self.store, space_id, &items)`. Behavior identical (same nodes created).

- [ ] **Step 4: Test** `persist_items_to_graph` over fixture `ExtractedItem`s with an in-memory `MemoryGraphStore` (mirror existing reflection tests' store setup): assert node kinds (profile→UserProfile, event→Episode, skill/tool→Procedure), routes, and Boot eligibility for Identity/Value/Directive.

- [ ] **Step 5: Build + test** — `cargo build` clean; `cargo test --lib memory_graph::reflection` (no regressions).

- [ ] **Step 6: Commit** — `refactor(memory): factor persist_items_to_graph from reflect() (no-op) (Step 3b-3)`

---

## Task 3: Shared extractor handle + ReflectionEngine cutover

**Files:** `app.rs`/`main.rs` (build `Arc<MemoryExtractor>` + AppState), `memory_graph/reflection.rs`, the `ReflectionEngine::new` call site.

- [ ] **Step 1: Build the shared extractor.** Find how a `MemoryOsLlmClient` is constructed for existing consumers (`grep -rn "MemoryOsLlmClient::new" src-tauri/src/`). At boot (`app.rs`, near where bucket_seal_embedder/adapter are built), build `let memory_extractor = Arc::new(MemoryExtractor::new(Arc::new(MemoryOsLlmClient::new(...)) as Arc<dyn MemoryOsLlm>));`. Add `pub memory_extractor: Arc<crate::memory_graph::extractor::MemoryExtractor>` to `AppState` and assign it.

- [ ] **Step 2: ReflectionEngine struct** — replace `memu_client: Option<Arc<MemUClient>>` with:
```rust
    extractor: Arc<crate::memory_graph::extractor::MemoryExtractor>,
    bucket_seal_adapter: Arc<crate::memory_bucket_seal::BucketSealAdapter>,
```
`new(store, extractor, bucket_seal_adapter)`.

- [ ] **Step 3: `reflect()` cutover**:
  - Delete the memu-availability gate (`:365-389`) — the extractor is always present; reflection always runs (subject to the pre-filters). Keep the length/greeting/command pre-filters unchanged.
  - Coverage check (`:472`, was `memu.retrieve`): `let covered = !self.bucket_seal_adapter.recall_hybrid(&content, None, 3).await.is_empty() && <high-similarity heuristic>;` — replicate the existing skip-if-covered intent (if a close existing memory is recalled, skip). Keep it fail-open (errors → proceed). (Use the same "content already covered" decision the old code made; if it compared a similarity score, approximate with recall_hybrid's top `score`.)
  - Memorize (`:530`, was `memu.memorize`): `let items = self.extractor.extract(&content).await;` then `persist_items_to_graph(&self.store, space_id, &items)?;` Keep the chip event + ReflectionDetail telemetry (report items.len()).

- [ ] **Step 4: Update the `ReflectionEngine::new` call site** (`grep -rn "ReflectionEngine::new" src-tauri/src/`): pass `state.memory_extractor.clone()` + `state.bucket_seal_adapter.clone()`. Tests pass a mock extractor (`MemoryExtractor::new(mock_llm)`) + a fresh `BucketSealAdapter`.

- [ ] **Step 5: Build + clippy + test** — clean; `cargo test --lib memory_graph::reflection` (update tests that constructed ReflectionEngine with a memU client).

- [ ] **Step 6: Commit** — `refactor(memory): ReflectionEngine uses native extractor + bucket_seal coverage check (Step 3b-3)`

---

## Task 4: ProactiveService cutover (D3 upgrade — persist to memory_graph)

**Files:** `src-tauri/src/proactive/service.rs`; its constructor + `main.rs`.

- [ ] **Step 1: Thread the extractor** — add `extractor: Arc<MemoryExtractor>` to ProactiveService (mirror how `embedder`/`bucket_seal_adapter` were threaded in 3b-1/3b-2); pass `state.memory_extractor.clone()` at `main.rs`'s `ProactiveService::new`. Add to ProactiveStateRefs/`clone_state_refs` if used. Tests pass a mock.

- [ ] **Step 2: Replace the two `memorize_with_config` calls** (`:2295` skill fallback, `:2443` conversation/multimodal):
  ```rust
  let items = self.extractor.extract(&llm_response).await;
  let created = crate::memory_graph::reflection::persist_items_to_graph(
      &self.memory_graph_store, &space_id, &items,
  ).unwrap_or(0);
  ```
  (Use the scenario's `space_id`.) Keep the skill_extraction scenario's PRIMARY skill-XML→`skill_adapter` path unchanged — only its memorize fallback is repointed. Keep the `publish_memory_extracted` + chip events (report `created`/`items.len()` instead of the old count).

- [ ] **Step 3: Build + clippy + test** — clean; `cargo test --lib proactive` (no regressions).

- [ ] **Step 4: Commit** — `feat(proactive): scenario extraction persists to memory_graph via native extractor (Step 3b-3)`

---

## Task 5: MemorizationService cutover

**Files:** `src-tauri/src/memorization/service.rs`; its constructor + `main.rs`.

- [ ] **Step 1: Thread the extractor** into MemorizationService (mirror prior threading); pass `state.memory_extractor.clone()` at construction.

- [ ] **Step 2: Replace `memu.memorize`** (`:251`) with `let items = self.extractor.extract(&conversation_text).await;`. Adapt `persist_memorize_results` to take `&[ExtractedItem]` (it currently iterates memU `serde_json::Value` items reading a `kind`/`memory_type` field — read `item.memory_type` instead).

- [ ] **Step 3: Adjust the routing in `persist_memorize_results`**:
  - **Keep** the `user_profile_facets` routing — but note the extractor emits `profile` (not `user_profile`/`identity`/`style`/`goal`). Map: route `memory_type == "profile"` → facets (or keep the existing facet-class inference if it parses content). Confirm the facet classifier still works on `profile` items; if it keyed on memU's finer kinds, simplify to: `profile` → a facet.
  - **Drop** the episode→`create_item`→memu.db leg (`:1093-1107`) entirely (episodic is in bucket_seal; memu.db is unread).
  - **Keep** the `gbrain_drafts` default leg (`:1108`) for the remaining types (gbrain is Step 2).

- [ ] **Step 4: Build + clippy + test** — clean; `cargo test --lib memorization` (no regressions; update tests using a memU client).

- [ ] **Step 5: Commit** — `refactor(memorization): native extractor; drop episode→memu.db; keep facets + drafts (Step 3b-3)`

---

## Task 6: Whole-slice verification + ship

- [ ] **Step 1: Full build + clippy** clean.
- [ ] **Step 2: Gates**
  - `grep -rnE "memu|MemUClient|\.memorize\(|memorize_with_config|create_item" src-tauri/src/memory_graph/reflection.rs src-tauri/src/memorization/service.rs` → no memU write/extraction refs (reflection fully off; memorization off memu.memorize + create_item).
  - `grep -rn "memorize_with_config" src-tauri/src/proactive/service.rs` → none.
  - The `MemUClient` methods (`memorize`/`retrieve`/`create_item`/`memorize_with_config`) may still be DEFINED in `memu/client.rs` (deleted in 3b-4) but have NO app callers now: `grep -rn "\.memorize\(\|\.memorize_with_config\(\|\.create_item\(" src-tauri/src/ | grep -v "memu/client.rs\|memu_bridge.py"` → empty.
- [ ] **Step 3: Targeted tests** — `memory_graph::extractor`, `memory_graph::reflection`, `proactive`, `memorization`, `memory_bucket_seal` all green.
- [ ] **Step 4: Extraction-quality spot-check (manual, documented)** — note in the PR that a dev should compare extractor output vs memU on 3-5 sample conversations before fully relying; tune the prompt if thinner. (memU still exists until 3b-4, so the comparison is possible during soak.)
- [ ] **Step 5: Ship** — push → PR (Commits table: extractor, refactor, reflection, proactive, memorization) → rebase-merge → sync → cleanup → reindex.

---

## Self-Review

- **Spec coverage:** extractor (T1), reusable mapping (T2), ReflectionEngine (T3), ProactiveService D3 upgrade (T4), MemorizationService + drop-dead-write (T5), gates (T6). ✓
- **No placeholders:** T1 has full extractor code; the ported prompt is concrete (refinable against memU source); T2-T5 are precise read+refactor+cutover with line anchors. ✓
- **Type consistency:** `ExtractedItem { memory_type, content }` flows extractor → `persist_items_to_graph` → all 3 consumers. `persist_items_to_graph(&MemoryGraphStore, &str, &[ExtractedItem]) -> Result<usize>` used identically in T3/T4. ✓
- **No regression intermediate:** T2 is a no-op refactor (reflect still works); T3/T4/T5 cut consumers over one at a time, each compiling+green; the `MemUClient` methods + bridge survive until 3b-4 (no half-cut). ✓
- **Finish-line:** after T5, no app code calls memU for write/extraction; 3b-4 deletes the orphaned client + bridge + pyembed → zero Python. ✓
