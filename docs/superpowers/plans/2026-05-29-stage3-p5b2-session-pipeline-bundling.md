# 阶段 3 P3-5b2 — `ChatDelegate` Session-Pipeline Field Bundling · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bundle 16 session-scoped configuration fields on `ChatDelegate` into 5 cohesive sub-structs, dropping the field count from **45 → 34** — hitting the spec's target of ~30 (and matching the figure in the user-picked Option A at scope decision time). Each bundle groups fields that are (a) initialized together via the same setter, (b) read together by the same code path, and (c) semantically one "concern."

**Architecture:** Pure data-shape refactor. No public API breaks. The 5 setters (`set_learning_pipeline`, `set_gbrain_extractor_pipeline`, `set_gene_retriever`+`set_gene_repo`, the heartbeat/budget/compose-stats setters, and the prompt-block setters) keep their EXTERNAL signatures but construct the sub-struct internally. Reads change from `self.field` to `self.bundle.field` — mechanical, line-by-line.

The 5 sub-structs live as sibling types in `src-tauri/src/agent/dispatcher/mod.rs` (or in tiny new files under `dispatcher/`). Each has `Default::default()` so `ChatDelegate::new()` initializes them cleanly.

**Tech Stack:** Rust 2021, no new crates.

**Related design:**
- AgentApi handle design: [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §6.2 (field reduction strategy).
- Pi-convergence gap audit: [`2026-05-27-pi-convergence-gap-audit.md`](../specs/2026-05-27-pi-convergence-gap-audit.md) §1.1 (god-object critique).
- 5a structural foundation: [`2026-05-29-stage3-p5a-dispatcher-structural-split.md`](2026-05-29-stage3-p5a-dispatcher-structural-split.md), merged at `0da7bada`.
- 5b1 AppState handle lookup: [`2026-05-29-stage3-p5b1-appstate-handle-injection.md`](2026-05-29-stage3-p5b1-appstate-handle-injection.md), merged at `cce495e2`.

---

## Recon-discovered facts (verified against `cce495e2` main, 2026-05-29)

`ChatDelegate` currently has **45 fields** (post-5b1). Categorization for 5b2:

### The 5 bundles

#### 1. `LearningPipeline` (4 fields → 1, net -3)

Memory OS Sprint 2.0/2.1b chat-turn extractor configuration. All four fields are set together in `set_learning_pipeline(llm, buffer, daily_budget, enabled)` and read together by `turn_runner::before_llm_call` at iteration=0.

| Field today | Type |
|---|---|
| `learning_buffer` | `Option<Arc<crate::learning::candidate::Buffer>>` |
| `learning_llm` | `Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>` |
| `learning_enabled` | `bool` |
| `learning_llm_daily_budget` | `u32` |

#### 2. `GbrainExtractorPipeline` (3 fields → 1, net -2)

Sprint 2.4b gbrain chat-extractor. Set together via `set_gbrain_extractor_pipeline(...)`. (Note: `gbrain_extract_db` and `gbrain_extract_mcp_mgr` were already dropped in 5b1.2/5b1.3 — they're read via `app_state()` now.)

| Field today | Type |
|---|---|
| `gbrain_extractor_enabled` | `bool` |
| `gbrain_extract_llm` | `Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>` |
| `gbrain_extract_daily_budget` | `u32` |

#### 3. `GepPipeline` (3 fields → 1, net -2)

Gene-Expression-Programming retrieval + repository. Set via `set_gene_retriever(...)` + `set_gene_repo(...)`; the `last_gene_matches` accumulator is keyed on every `call_llm` and cleared after Capsule generation.

| Field today | Type |
|---|---|
| `gene_retriever` | `Option<Arc<GeneRetriever>>` |
| `last_gene_matches` | `Mutex<Vec<GeneMatch>>` |
| `gene_repo` | `Option<Arc<Mutex<GeneRepository>>>` |

#### 4. `Telemetry` (3 fields → 1, net -2)

Bundles all per-session telemetry collectors. Each is set independently today via `set_heartbeat()` / `set_token_budget_collector()` / `set_compose_stats_collector()`.

| Field today | Type |
|---|---|
| `heartbeat` | `Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>` |
| `token_budget_collector` | `Option<crate::agent::telemetry::TokenBudgetCollector>` |
| `compose_stats_collector` | `Option<crate::agent::context_manager::ComposeStatsCollector>` |

#### 5. `PromptBlocks` (3 fields → 1, net -2)

Pre-built system-prompt fragments computed once per agent loop and read by `effective_system_prompt` on every iteration. Each set via its own setter (`set_skills_manifest_block`, `set_learned_profile_block`, `set_gbrain_knowledge_block`).

| Field today | Type |
|---|---|
| `skills_manifest_block` | `String` |
| `learned_profile_block` | `String` |
| `gbrain_knowledge_block` | `String` |

Total: 16 fields → 5 sub-structs = **-11 net**. Field count: **45 → 34**.

### Fields that STAY (29 — by design)

Turn-scoped state (atomics, counters, accumulators, mutex snapshots) that has no cohesive sibling to bundle with. Notable ones:
- `llm`, `tools`, `app_handle`, `model`, `system_prompt`, `stop_flag`, `safety_mode`, `conversation_id`, `workspace_root`
- `turn_index`, `thinking_enabled`, `thinking_seq`, `chunk_seq`, `skill_search_used`, `is_first_act_turn`
- `memory_context`, `last_memory_context_snapshot`, `recent_tool_errors`, `last_tool_defs_hash`, `last_error_kind`
- `infra_service`, `trajectory_store`, `tool_budget` (tool-observation handles — could be bundled as `ToolObservers` in a future PR if the spec target shrinks further)
- `context_manager`, `last_injected_fragments`, `provider`, `tool_dispatcher`
- `steering_queue`, `follow_up_queue`

### Baselines to hold

- `cargo build`: 0 errors, **≤50 warnings** (post-5b1 baseline of 49 ideally preserved; ≤50 tolerated for transient cleanup slack).
- `cargo test --lib agent::dispatcher`: **50 passed**.
- `cargo test --lib agent::`: **796 passed / 2 pre-existing failed**.
- `cargo test --lib` total: **3,050 passed / 7 pre-existing failed**.

### External callers (must compile cleanly)

The 5 setter methods (`set_learning_pipeline`, `set_gbrain_extractor_pipeline`, `set_gene_retriever`, `set_gene_repo`, `set_heartbeat`, `set_token_budget_collector`, `set_compose_stats_collector`, `set_skills_manifest_block`, `set_learned_profile_block`, `set_gbrain_knowledge_block`) **keep their external signatures unchanged**. Internally each sub-struct is constructed/updated.

This means **NO changes to `tauri_commands.rs`** for 5b2 — the bundling is purely internal. (Contrast with 5b1, which dropped setter params.)

---

## Target shape

### Sub-struct definitions (added to `dispatcher/mod.rs`)

```rust
/// Sprint 2.0+ chat-turn extractor configuration.
#[derive(Default)]
pub(super) struct LearningPipeline {
    pub(super) buffer: Option<Arc<crate::learning::candidate::Buffer>>,
    pub(super) llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
    pub(super) enabled: bool,
    pub(super) llm_daily_budget: u32,
}

/// Sprint 2.4b gbrain chat-extractor configuration.
#[derive(Default)]
pub(super) struct GbrainExtractorPipeline {
    pub(super) enabled: bool,
    pub(super) llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
    pub(super) daily_budget: u32,
}

/// Gene-Expression-Programming retrieval + repository.
#[derive(Default)]
pub(super) struct GepPipeline {
    pub(super) retriever: Option<Arc<GeneRetriever>>,
    pub(super) last_matches: Mutex<Vec<GeneMatch>>,
    pub(super) repo: Option<Arc<Mutex<GeneRepository>>>,
}

/// Per-session telemetry collectors.
#[derive(Default)]
pub(super) struct Telemetry {
    pub(super) heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
    pub(super) token_budget: Option<crate::agent::telemetry::TokenBudgetCollector>,
    pub(super) compose_stats: Option<crate::agent::context_manager::ComposeStatsCollector>,
}

/// Pre-built system-prompt fragments computed once per agent loop.
#[derive(Default)]
pub(super) struct PromptBlocks {
    pub(super) skills_manifest: String,
    pub(super) learned_profile: String,
    pub(super) gbrain_knowledge: String,
}
```

All 5 derive `Default` so `ChatDelegate::new()` can use `..Default::default()` style or explicit `Default::default()` per field.

### `ChatDelegate` after 5b2

The 16 dropped fields are replaced by 5 sub-struct fields:
```rust
learning: LearningPipeline,
gbrain_extractor: GbrainExtractorPipeline,
gep: GepPipeline,
telemetry: Telemetry,
prompt_blocks: PromptBlocks,
```

### Setter API (external signatures UNCHANGED)

Each setter's body changes to mutate the sub-struct:
- `set_learning_pipeline(&mut self, llm, buffer, daily_budget, enabled)` → mutates `self.learning.*`
- `set_gbrain_extractor_pipeline(&mut self, llm, daily_budget, enabled)` → mutates `self.gbrain_extractor.*`
- `set_gene_retriever(&mut self, retriever)` → `self.gep.retriever = Some(retriever)`
- `set_gene_repo(&mut self, repo)` → `self.gep.repo = Some(repo)`
- `set_heartbeat(&mut self, hb)` → `self.telemetry.heartbeat = Some(hb)`
- `set_token_budget_collector(&mut self, c)` → `self.telemetry.token_budget = Some(c)`
- `set_compose_stats_collector(&mut self, c)` → `self.telemetry.compose_stats = Some(c)`
- `set_skills_manifest_block(&mut self, b)` → `self.prompt_blocks.skills_manifest = b`
- `set_learned_profile_block(&mut self, b)` → `self.prompt_blocks.learned_profile = b`
- `set_gbrain_knowledge_block(&mut self, b)` → `self.prompt_blocks.gbrain_knowledge = b`

`tauri_commands.rs` does NOT need touching.

### Read transformations

Mechanical search-and-replace, one field cohort per task:
- `self.learning_buffer` → `self.learning.buffer`
- `self.learning_llm` → `self.learning.llm`
- `self.gbrain_extract_llm` → `self.gbrain_extractor.llm`
- `self.gene_retriever` → `self.gep.retriever`
- `self.heartbeat` → `self.telemetry.heartbeat`
- `self.skills_manifest_block` → `self.prompt_blocks.skills_manifest`
- etc.

### Field count after 5b2

- 45 (pre-5b2) − 16 (dropped) + 5 (sub-structs) = **34 fields**.
- Matches Option-A target exactly.

---

## Pre-flight (before Task 1)

1. **Confirm main baseline:**

   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   git -C /Users/ryanliu/Documents/uclaw log --oneline -3
   ```
   Expected: `## main...origin/main` at `cce495e2`.

2. **Create worktree + symlinks:**

   ```bash
   git worktree add -b claude/stage3-p5b2-pipeline-bundling \
       /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/bunembed
   ```

3. **Capture baselines:**

   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   ```

---

## Task 1: Bundle `LearningPipeline` (4 fields → 1 sub-struct)

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (define `LearningPipeline` struct + drop 4 fields + replace `set_learning_pipeline` body + update `new()` struct literal)
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs` (every `self.learning_*` read → `self.learning.*`)
- Possibly Modify: `src-tauri/src/agent/dispatcher/content_assembler.rs` (if any `self.learning_*` reads exist there)

### Steps

- [ ] **Step 1.1: Inventory current learning_* references**

  ```bash
  grep -rnE "self\.(learning_buffer|learning_llm|learning_enabled|learning_llm_daily_budget)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/src/agent/dispatcher/
  ```

  Expected: ~5-10 sites across mod.rs (the setter + new()) and turn_runner.rs (before_llm_call extractor spawn).

- [ ] **Step 1.2: Define `LearningPipeline` in mod.rs**

  Add at the top level of `dispatcher/mod.rs` (above the `pub struct ChatDelegate` block, after the `use` statements):

  ```rust
  /// Sprint 2.0+ chat-turn extractor configuration. Set as a single
  /// bundle by `set_learning_pipeline`; read together by
  /// `turn_runner::before_llm_call` at iteration=0.
  #[derive(Default)]
  pub(super) struct LearningPipeline {
      pub(super) buffer: Option<std::sync::Arc<crate::learning::candidate::Buffer>>,
      pub(super) llm: Option<std::sync::Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
      pub(super) enabled: bool,
      pub(super) llm_daily_budget: u32,
  }
  ```

  Verify the exact type paths against current `mod.rs` use statements — adjust if there's a shorter `use` alias.

- [ ] **Step 1.3: Replace 4 fields on ChatDelegate with one `learning: LearningPipeline` field**

  In `pub struct ChatDelegate`:
  - Remove `learning_buffer`, `learning_llm`, `learning_enabled`, `learning_llm_daily_budget` field declarations.
  - Add `learning: LearningPipeline,`.

  In `pub fn new(...)` struct literal:
  - Remove the 4 `learning_*: None / false / 0` initializers.
  - Add `learning: Default::default(),`.

- [ ] **Step 1.4: Rewrite `set_learning_pipeline` body**

  ```bash
  grep -n "pub fn set_learning_pipeline" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/src/agent/dispatcher/mod.rs
  ```

  Body changes from individual field assignments to:
  ```rust
  pub fn set_learning_pipeline(
      &mut self,
      llm: Option<Arc<dyn ...>>,
      buffer: Option<Arc<crate::learning::candidate::Buffer>>,
      daily_budget: u32,
      enabled: bool,
  ) {
      self.learning = LearningPipeline {
          buffer,
          llm,
          enabled,
          llm_daily_budget: daily_budget,
      };
  }
  ```

  Keep the external signature unchanged. Only the body changes.

- [ ] **Step 1.5: Replace reads in turn_runner.rs**

  For each site from Step 1.1:
  - `self.learning_buffer` → `self.learning.buffer`
  - `self.learning_llm` → `self.learning.llm`
  - `self.learning_enabled` → `self.learning.enabled`
  - `self.learning_llm_daily_budget` → `self.learning.llm_daily_budget`

  Most sites are inside `before_llm_call`'s extractor spawn block. Read the surrounding context — if a binding clones one of these into a local for use after a State drop or await, the local binding stays the same name (just sources from the bundle).

- [ ] **Step 1.6: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: 0 errors, ≤50 warnings, dispatcher 50/0, agent:: 796/2.

- [ ] **Step 1.7: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling commit -m "refactor(agent): bundle 4 learning_* fields → LearningPipeline (P3-5b2.1 of 阶段 3)"
  ```

Continue to Task 2.

---

## Task 2: Bundle `GbrainExtractorPipeline` (3 fields → 1)

Same shape as Task 1, for the gbrain extractor.

**Files:**
- Modify: `dispatcher/mod.rs` (define struct + drop 3 fields + rewrite `set_gbrain_extractor_pipeline`)
- Modify: `dispatcher/turn_runner.rs` (replace reads)

### Steps

- [ ] **Step 2.1: Inventory**

  ```bash
  grep -rnE "self\.(gbrain_extractor_enabled|gbrain_extract_llm|gbrain_extract_daily_budget)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/src/agent/dispatcher/
  ```

- [ ] **Step 2.2: Define `GbrainExtractorPipeline`**

  ```rust
  /// Sprint 2.4b gbrain chat-extractor configuration. Set as a single
  /// bundle by `set_gbrain_extractor_pipeline`; read by
  /// `turn_runner::before_llm_call` alongside the learning pipeline.
  #[derive(Default)]
  pub(super) struct GbrainExtractorPipeline {
      pub(super) enabled: bool,
      pub(super) llm: Option<std::sync::Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
      pub(super) daily_budget: u32,
  }
  ```

- [ ] **Step 2.3: Replace fields + update new() + rewrite setter body**

  - Drop the 3 fields from `pub struct ChatDelegate` + their initializers in `new()`.
  - Add `gbrain_extractor: GbrainExtractorPipeline,` to struct + `gbrain_extractor: Default::default(),` to new().
  - Rewrite `set_gbrain_extractor_pipeline` body to assemble `self.gbrain_extractor = GbrainExtractorPipeline { ... }`.

- [ ] **Step 2.4: Replace reads**

  - `self.gbrain_extractor_enabled` → `self.gbrain_extractor.enabled`
  - `self.gbrain_extract_llm` → `self.gbrain_extractor.llm`
  - `self.gbrain_extract_daily_budget` → `self.gbrain_extractor.daily_budget`

- [ ] **Step 2.5: Build + tests + commit**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling commit -m "refactor(agent): bundle 3 gbrain_extract_* fields → GbrainExtractorPipeline (P3-5b2.2 of 阶段 3)"
  ```

Continue to Task 3.

---

## Task 3: Bundle `GepPipeline` (3 fields → 1)

**Files:**
- Modify: `dispatcher/mod.rs` (define struct + drop 3 fields + rewrite `set_gene_retriever` + `set_gene_repo`)
- Modify: `dispatcher/turn_runner.rs` and possibly `dispatcher/mod.rs::generate_capsule_for_turn` (replace reads on `gene_retriever`, `last_gene_matches`, `gene_repo`)

### Steps

- [ ] **Step 3.1: Inventory**

  ```bash
  grep -rnE "self\.(gene_retriever|last_gene_matches|gene_repo)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/src/agent/dispatcher/
  ```

- [ ] **Step 3.2: Define `GepPipeline`**

  ```rust
  /// Gene-Expression-Programming retrieval + capsule generation. The
  /// retriever runs on every `call_llm`; matches accumulate in
  /// `last_matches` and are consumed by `generate_capsule_for_turn`.
  #[derive(Default)]
  pub(super) struct GepPipeline {
      pub(super) retriever: Option<std::sync::Arc<GeneRetriever>>,
      pub(super) last_matches: std::sync::Mutex<Vec<GeneMatch>>,
      pub(super) repo: Option<std::sync::Arc<std::sync::Mutex<GeneRepository>>>,
  }
  ```

  Watch: `Mutex<Vec<...>>` doesn't derive Default by default (Mutex does, but only on stable since Rust 1.70+). Verify `cargo check` accepts the `#[derive(Default)]`. If not, write an explicit `impl Default for GepPipeline { fn default() -> Self { ... } }`.

- [ ] **Step 3.3: Replace fields + update new() + rewrite setters**

  Drop 3 fields from struct + 3 initializers from new(). Add `gep: GepPipeline,` + `gep: Default::default(),`.

  Two setters affected:
  - `set_gene_retriever(&mut self, retriever: Arc<GeneRetriever>)` → body becomes `self.gep.retriever = Some(retriever);`
  - `set_gene_repo(&mut self, repo: Arc<Mutex<GeneRepository>>)` → `self.gep.repo = Some(repo);`

- [ ] **Step 3.4: Replace reads**

  Likely sites:
  - `turn_runner.rs::create_turn_snapshot` (reads `self.gene_retriever` for context fragment injection)
  - `turn_runner.rs::call_llm` (writes to `self.last_gene_matches` after retrieval; reads `self.gene_retriever`)
  - `mod.rs::generate_capsule_for_turn` (reads `self.gene_repo`, drains `self.last_gene_matches`)

  Transform: `self.gene_*` → `self.gep.*`.

- [ ] **Step 3.5: Build + tests + commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling commit -m "refactor(agent): bundle 3 gene_* fields → GepPipeline (P3-5b2.3 of 阶段 3)"
  ```

Continue to Task 4.

---

## Task 4: Bundle `Telemetry` (3 fields → 1)

**Files:**
- Modify: `dispatcher/mod.rs` (define struct + drop 3 fields + rewrite 3 setters)
- Modify: `dispatcher/observability.rs` (the only consumer — `beat()` reads `self.heartbeat`; emit_* may read `self.token_budget_collector` and `self.compose_stats_collector`)

### Steps

- [ ] **Step 4.1: Inventory**

  ```bash
  grep -rnE "self\.(heartbeat|token_budget_collector|compose_stats_collector)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/src/agent/dispatcher/
  ```

- [ ] **Step 4.2: Define `Telemetry`**

  ```rust
  /// Per-session telemetry collectors. Each is wired via its own
  /// setter; all are observability-only and may be `None` in headless
  /// / test contexts.
  #[derive(Default)]
  pub(super) struct Telemetry {
      pub(super) heartbeat: Option<std::sync::Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
      pub(super) token_budget: Option<crate::agent::telemetry::TokenBudgetCollector>,
      pub(super) compose_stats: Option<crate::agent::context_manager::ComposeStatsCollector>,
  }
  ```

- [ ] **Step 4.3: Replace fields + update new() + rewrite 3 setters**

  Drop 3 fields. Add `telemetry: Telemetry,` + `telemetry: Default::default(),`.

  Setters:
  - `set_heartbeat(&mut self, hb)` → `self.telemetry.heartbeat = Some(hb);`
  - `set_token_budget_collector(&mut self, c)` → `self.telemetry.token_budget = Some(c);`
  - `set_compose_stats_collector(&mut self, c)` → `self.telemetry.compose_stats = Some(c);`

- [ ] **Step 4.4: Replace reads**

  - `self.heartbeat` → `self.telemetry.heartbeat`
  - `self.token_budget_collector` → `self.telemetry.token_budget`
  - `self.compose_stats_collector` → `self.telemetry.compose_stats`

  Most in `observability.rs`.

- [ ] **Step 4.5: Build + tests + commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling commit -m "refactor(agent): bundle 3 telemetry fields → Telemetry (P3-5b2.4 of 阶段 3)"
  ```

Continue to Task 5.

---

## Task 5: Bundle `PromptBlocks` (3 fields → 1)

**Files:**
- Modify: `dispatcher/mod.rs` (define struct + drop 3 fields + rewrite 3 setters)
- Modify: `dispatcher/content_assembler.rs` (the only consumer — `effective_system_prompt` reads all 3)

### Steps

- [ ] **Step 5.1: Inventory**

  ```bash
  grep -rnE "self\.(skills_manifest_block|learned_profile_block|gbrain_knowledge_block)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/src/agent/dispatcher/
  ```

- [ ] **Step 5.2: Define `PromptBlocks`**

  ```rust
  /// Pre-built system-prompt fragments. Each is computed once per
  /// agent loop start (skill manifest scan, learning profile fold,
  /// gbrain knowledge instruction); `effective_system_prompt` appends
  /// the non-empty ones on every iteration. Empty string = no append.
  #[derive(Default)]
  pub(super) struct PromptBlocks {
      pub(super) skills_manifest: String,
      pub(super) learned_profile: String,
      pub(super) gbrain_knowledge: String,
  }
  ```

- [ ] **Step 5.3: Replace fields + update new() + rewrite 3 setters**

  Drop 3 fields. Add `prompt_blocks: PromptBlocks,` + `prompt_blocks: Default::default(),`.

  Setters:
  - `set_skills_manifest_block(&mut self, block)` → `self.prompt_blocks.skills_manifest = block;`
  - `set_learned_profile_block(&mut self, block)` → `self.prompt_blocks.learned_profile = block;`
  - `set_gbrain_knowledge_block(&mut self, block)` → `self.prompt_blocks.gbrain_knowledge = block;`

- [ ] **Step 5.4: Replace reads in content_assembler.rs**

  - `self.skills_manifest_block` → `self.prompt_blocks.skills_manifest`
  - `self.learned_profile_block` → `self.prompt_blocks.learned_profile`
  - `self.gbrain_knowledge_block` → `self.prompt_blocks.gbrain_knowledge`

- [ ] **Step 5.5: Build + tests + commit**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling commit -m "refactor(agent): bundle 3 prompt-block fields → PromptBlocks (P3-5b2.5 of 阶段 3)"
  ```

Continue to Task 6.

---

## Task 6: Final audit + cumulative tests + cleanup

### Steps

- [ ] **Step 6.1: Field count audit**

  ```bash
  awk '/^pub struct ChatDelegate/,/^}/' /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri/src/agent/dispatcher/mod.rs | grep -E "^    [a-z_]+:|^    [a-z_]+ *$" | wc -l
  ```
  Expected: **34**.

- [ ] **Step 6.2: Full test battery**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling/src-tauri && cargo test --lib 2>&1 | tail -5
  ```
  Required:
  - 0 errors.
  - ≤50 warnings (target ≤49 — same as post-5b1 baseline).
  - agent:: 796/2, cargo test --lib 3050/7.

- [ ] **Step 6.3: Clean unused imports (if any)**

  If the 5 sub-struct `pub(super)` types' field types are now imported in mod.rs but unused inside the struct literal/reads in mod.rs (because they're accessed via `self.bundle.field`), the imports stay needed for the struct definitions. Should be minimal cleanup needed.

- [ ] **Step 6.4: Commit cleanup if any**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling commit -m "refactor(agent): clean unused imports after P3-5b2 (P3-5b2.6)"
  ```

- [ ] **Step 6.5: Verify final chain**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling log --oneline main..HEAD
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b2-pipeline-bundling status -sb
  ```
  Expected: 5-6 commits ahead of main, clean tree.

---

## Self-Review

**1. Spec coverage:**
- ✅ Field reduction: 45 → 34 (hits Option A target exactly, matches spec ~30 ± 4 tolerance).
- ✅ Pure data-shape refactor — no public API breaks.
- ✅ Each bundle has cohesive setter + reader pattern.
- 🟡 No ContentBlock dedup. Deferred to P3-5b3.

**2. Placeholder scan:**
- No `unimplemented!()` / `TODO` / `TBD`.
- All sub-struct definitions concrete with `#[derive(Default)]`.
- One careful note on Mutex<Vec<T>> Default support (Rust ≥1.70) — implementer to verify.

**3. Type consistency:**
- Sub-struct field naming: `learning.buffer`, `gep.retriever`, `telemetry.heartbeat`, `prompt_blocks.skills_manifest` — short and unambiguous.
- All sub-structs `pub(super)` (scoped to dispatcher module).

**4. Bisectability:**
- One bundle per commit. Each commit's `cargo test --lib agent::` must pass.
- Setter signatures unchanged across commits — no caller breakage at any point.

---

## Cumulative summary

- **Tasks:** 6 (5 mandatory bundles + 1 audit/cleanup).
- **Estimated time:** 1-1.5 person-days.
- **Risk:** Low. Pure data-shape change. Setter API unchanged means no caller breakage. Reads are mechanical search-and-replace.
- **Total commits:** 5-6.

After 5b2 ships: `ChatDelegate` has **34 fields** (down 11 from 45 — total 53→34 across 5b1+5b2). The 5 sub-structs document each cohesive concern in one place. P3-5b3 then collapses the 5 ContentBlock dup sites in `content_assembler.rs` to close the audit's MAJOR critique.
