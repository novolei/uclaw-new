# 阶段 3 P3-5a — `dispatcher.rs` Structural Split · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the monolithic `src-tauri/src/agent/dispatcher.rs` (3,859 LoC, single `pub struct ChatDelegate` with 53 fields + ~50 methods spread across one impl block + `impl StreamSink` + `impl LoopDelegate` + 9 `#[cfg(test)]` modules) into a focused `agent/dispatcher/` directory: `mod.rs` (facade — struct definition, constructor, top-level setters) + 5 focused sibling modules grouping methods by responsibility. **Zero behavior change. Zero signature change. Zero field change.** This is a pure code-relocation refactor that lays the groundwork for P3-5b (field collapse via `Arc<AppState>` + `Arc<AgentApi>` handles).

**Architecture:** Keep `ChatDelegate` as one struct; split its `impl` blocks across files. Rust permits multiple `impl Type { ... }` blocks for the same type across files within the same crate, so methods can be relocated to focused submodules without changing call sites or `&self` access to fields. Trait impls (`impl StreamSink for ChatDelegate`, `impl LoopDelegate for ChatDelegate`) MUST each live in exactly one file — they migrate as a single unit to the most-related submodule.

**Tech Stack:** Rust 2021, no new crates. Existing modules touched: `agent/mod.rs` (mod declaration update only).

**Related design:**
- Pi-convergence gap audit: [`2026-05-27-pi-convergence-gap-audit.md`](../specs/2026-05-27-pi-convergence-gap-audit.md) §1.1 (`dispatcher.rs` 3,853 lines / "71-field god object" — actual 53 fields), §1.2 (5 ContentBlock dup sites — deferred to P3-5b).
- AgentApi handle design: [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §6 (target layout). Note that the spec's "~200 LoC facade + ~2,200 LoC total" figure assumes BOTH structural split (this PR) AND field/dup collapse (P3-5b). 5a alone keeps total LoC ≈ unchanged; the win is module boundary + locality.
- Prior 阶段 3 PRs: #570 (P3-1 skeleton), #571 (P3-2 tools), #572 (P3-3 ProviderService+HookBus), #573 (P3-4 plugin discovery).

---

## Recon-discovered facts vs. spec assumptions

| Spec assumption | Recon (verified against `606f03a4` main, 2026-05-29) | Plan adaptation |
|---|---|---|
| 71 fields on `ChatDelegate` | **53 fields** (audit counted older snapshot or non-struct items) | Use 53 as baseline; 5b target adjusted to ~30 (not ~15) on first pass |
| `dispatcher.rs` is one impl block | Three: `impl ChatDelegate { ~50 methods }` + `impl StreamSink for ChatDelegate { 6 fns }` + `impl LoopDelegate for ChatDelegate { 7 fns }` | Plan partitions methods across modules; each trait impl moves as one unit |
| 5 ContentBlock assembly sites need consolidation | True — verified at lines 1278/1392/1519/1871/2148, but the dedup requires a `content_assembler::assemble_*` extraction that crosses module boundaries | Deferred to 5b after `content_assembler.rs` exists as a stable seam |
| 9 test modules already exist inside `dispatcher.rs` | Confirmed — `browser_runtime_dispatch_patch_tests`, `panic_recovery_tests`, `manifest_suppression_tests`, `manifest_cap_tests`, `plan_guard_relevance_tests`, `truncated_continuation_tests`, `active_plan_history_tests`, `memory_context_delta_render_tests`, `b2_context_wireup_tests` | Each test module migrates alongside its target methods so each commit stays green |
| External callers of `ChatDelegate::new` | 3 sites in `tauri_commands.rs` (lines 1983, 11222, 15090) | Public path `crate::agent::dispatcher::ChatDelegate` MUST stay valid; achieved by keeping struct in `dispatcher/mod.rs` with `pub use` re-exports unchanged |

---

## Target module layout

```
src-tauri/src/agent/dispatcher.rs        — DELETED (rename to dispatcher/mod.rs)
src-tauri/src/agent/dispatcher/
├── mod.rs                 (~700-800 LoC)   facade: struct ChatDelegate {…53 fields}, new(),
│                                           with_agent_queues(), high-level setters,
│                                           generate_capsule_for_turn(), 3 free helpers,
│                                           general tests not tied to a specific submodule
├── observability.rs       (~400 LoC)       all emit_* (text_delta, tool_start, tool_result,
│                                           done, queued_consumed, thinking, thinking_done,
│                                           turn_cost, context_stats, reflection,
│                                           stream_reset, retry_event) + beat() + heartbeat /
│                                           token_budget / compose_stats setters
├── content_assembler.rs   (~700 LoC)       effective_system_prompt + build_dynamic_context +
│                                           persona_prompt_block_best_effort +
│                                           context_manager_for_prompt_blocking +
│                                           estimate_context_pressure_ratio + the prompt-block
│                                           setters (skills_manifest, learned_profile,
│                                           gbrain_knowledge, memory_context) + tests for
│                                           manifest_suppression/manifest_cap/b2_context_wireup/
│                                           memory_context_delta_render
├── safety_gate.rs         (~50 LoC)        resolve_effective_mode
├── model_io.rs            (~450 LoC)       create_turn_snapshot + call_llm + sleep_or_abort +
│                                           impl StreamSink for ChatDelegate (6 fns) + tests
│                                           for panic_recovery / truncated_continuation
└── turn_runner.rs         (~900 LoC)       tool_dispatcher() (lazy) + impl LoopDelegate
                                            (check_signals, before_llm_call,
                                            handle_text_response, execute_tool_calls,
                                            on_usage) + spawn_post_turn_extraction +
                                            tests for browser_runtime_dispatch_patch /
                                            plan_guard_relevance / active_plan_history
```

Total: same ~3,859 LoC (split, not reduced). Net win: each file ≤ ~900 LoC, each one has a clear single responsibility, future edits land in the right place without scrolling 3,000 lines.

---

## Boundaries between modules

**Trait impls (each lives in exactly one file):**
- `impl StreamSink for ChatDelegate` → `model_io.rs` (the 6 streaming callbacks all forward to `emit_*` observability methods OR are the LLM-stream wiring itself; model_io is the better home since `call_llm` ultimately drives them)
- `impl LoopDelegate for ChatDelegate` → `turn_runner.rs` (the 7 loop-step callbacks are the run-loop body)

**Method visibility:**
- Methods that one submodule calls on another (e.g., `model_io::call_llm` invokes `self.emit_text_delta` from `observability`) — both stay `&self` methods on `ChatDelegate`. Because all submodules share the same crate, `&self` access to public/private fields works uniformly. No `pub` upgrades needed unless a method goes from private-in-one-file to cross-file private (`pub(super)`).
- Free helpers at the bottom (`detect_soft_tool_error`, `truncate_utf8`, `load_attached_dirs_for_session`) — stay `pub(crate)` in `mod.rs`. They're called by other modules in `agent/` (not just dispatcher), so visibility doesn't change.

**Tests:** Each `#[cfg(test)] mod foo { ... }` migrates intact alongside the methods it covers. Keeps "test next to code under test" invariant. The `use super::*;` imports may need adjustment to absolute paths once tests move.

---

## Background facts verified against HEAD `606f03a4`

### File sizes & methods

```text
$ wc -l src-tauri/src/agent/dispatcher.rs
3859 src-tauri/src/agent/dispatcher.rs
```

Method roster (approximate, by impl block):

**`impl ChatDelegate`** — 36 methods grouped:
- Constructor + builder: `new`, `with_agent_queues`
- Setters: `set_compose_stats_collector`, `set_context_manager`, `set_heartbeat`, `set_token_budget_collector`, `set_provider`, `set_gene_retriever`, `set_gene_repo`, `set_db`, `set_thinking_enabled`, `set_infra_service`, `set_trajectory_store`, `set_tool_budget`, `set_memory_context`, `clear_memory_context_anchor`, `append_memory_context`, `set_skills_manifest_block`, `set_learned_profile_block`, `set_gbrain_knowledge_block`, `set_learning_pipeline`, `set_gbrain_extractor_pipeline`
- Loop body: `tool_dispatcher` (lazy), `resolve_effective_mode`, `spawn_post_turn_extraction`
- Prompt assembly: `effective_system_prompt`, `build_dynamic_context`, `persona_prompt_block_best_effort`, `context_manager_for_prompt_blocking`, `estimate_context_pressure_ratio`
- GEP: `generate_capsule_for_turn`
- Lifecycle: `stop_handle`, `beat`
- Events: `emit_text_delta`, `emit_tool_start`, `emit_tool_result`, `emit_done`, `emit_queued_consumed`, `emit_thinking`, `emit_thinking_done`, `emit_turn_cost`, `emit_context_stats`, `emit_reflection_status`, `emit_reflection`, `emit_stream_reset`, `emit_retry_event`, `sleep_or_abort`

**`impl StreamSink for ChatDelegate`** (lines 1549-~1670) — 6 methods:
- `on_text_delta`, `on_thinking`, `on_thinking_done`, `on_stream_reset`, `on_retry_event`, `sleep_or_abort`

**`impl LoopDelegate for ChatDelegate`** (lines 1829-~2900) — 7 methods:
- `check_signals`, `before_llm_call`, `create_turn_snapshot`, `call_llm`, `handle_text_response`, `execute_tool_calls`, `on_usage`

**Free `pub(crate) fn`**: `detect_soft_tool_error`, `truncate_utf8`, `load_attached_dirs_for_session`.

### Baselines to hold

- `cargo build`: 0 errors, 50 warnings (post-P3-4 baseline).
- `cargo test --lib agent::`: 796 passed / 2 pre-existing failed.
- `cargo test --lib agent::dispatcher`: subset of above; specific count to capture during Pre-flight.
- `cargo test --lib`: ~3050 passed / 7 pre-existing failed.

After P3-5a: identical pass count, ≤55 warnings. Structural split must NOT change test outcomes — that's the safety net.

### External callers (must remain compileable unchanged)

```text
$ grep -n "ChatDelegate::new" src-tauri/src/tauri_commands.rs
1983:    let mut delegate = crate::agent::dispatcher::ChatDelegate::new(
11222:        let mut delegate = crate::agent::dispatcher::ChatDelegate::new(
15090:                let mut delegate = crate::agent::dispatcher::ChatDelegate::new(
```

These all reference `crate::agent::dispatcher::ChatDelegate`. After conversion to `dispatcher/mod.rs`, the path resolves identically (Rust treats `dispatcher.rs` and `dispatcher/mod.rs` as equivalent declarations of the `dispatcher` module). **No tauri_commands.rs edits required.**

---

## Pre-flight (before Task 1)

1. **Confirm main baseline:**

   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   ```
   Expected: `## main...origin/main` at `606f03a4`.

2. **Create worktree + symlinks:**

   ```bash
   git worktree add -b claude/stage3-p5a-dispatcher-structural-split \
       /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri/bunembed
   ```

3. **Capture baseline build + tests:**

   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   ```
   Record the numbers — they are the gate every subsequent task must clear.

---

## Task 1: Scaffold `dispatcher/` directory + relocate struct, ctor, setters, helpers, generic tests

**Goal:** Convert `dispatcher.rs` → `dispatcher/mod.rs`. Move struct + constructor + builder + setters + 3 free helpers + the test modules that don't have a clearer downstream home. After Task 1 the file lives in its new directory but no methods have been factored out; the `dispatcher` module is API-identical to before.

**Files:**
- Delete: `src-tauri/src/agent/dispatcher.rs`
- Create: `src-tauri/src/agent/dispatcher/mod.rs` (initially holds ALL of the old `dispatcher.rs` content)
- Modify: `src-tauri/src/agent/mod.rs` (no change needed — `pub mod dispatcher;` still resolves correctly to the new `dispatcher/mod.rs`)

### Steps

- [ ] **Step 1.1: git-mv dispatcher.rs → dispatcher/mod.rs**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split
  mkdir -p src-tauri/src/agent/dispatcher
  git mv src-tauri/src/agent/dispatcher.rs src-tauri/src/agent/dispatcher/mod.rs
  ```

  Verify `git status -sb` shows only the rename:
  ```text
  R  src-tauri/src/agent/dispatcher.rs -> src-tauri/src/agent/dispatcher/mod.rs
  ```

- [ ] **Step 1.2: Confirm the rename builds clean and tests still pass**

  ```bash
  cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  ```
  Expected: empty. Rust resolves `pub mod dispatcher;` in `agent/mod.rs` to `dispatcher/mod.rs` identically.

  ```bash
  cargo test --lib agent::dispatcher 2>&1 | tail -3
  cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: identical pass count to Pre-flight.

- [ ] **Step 1.3: Commit the structural rename**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split add -A src-tauri/src/agent/dispatcher.rs src-tauri/src/agent/dispatcher/mod.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split commit -m "refactor(agent): convert dispatcher.rs to dispatcher/ module (P3-5a.1 of 阶段 3)"
  ```

Continue to Task 2.

---

## Task 2: Extract `observability.rs` — all event-emission methods

**Goal:** Move the 13 `emit_*` methods + `beat()` + the three observability setters (`set_heartbeat`, `set_token_budget_collector`, `set_compose_stats_collector`) into a new `dispatcher/observability.rs`. These methods are semantically cohesive (every one fires a frontend event or telemetry record) and have no inter-dependencies with other dispatcher concerns.

**Files:**
- Create: `src-tauri/src/agent/dispatcher/observability.rs`
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (remove relocated methods, add `mod observability;`)

### Steps

- [ ] **Step 2.1: Create observability.rs with the impl block scaffold**

  ```rust
  //! Event emission + telemetry recording for ChatDelegate.
  //!
  //! Every method here fires a `tauri::AppHandle::emit_to` call or pushes a
  //! telemetry snapshot. None of them touch the LLM, the tool registry, or
  //! the loop. Pure I/O fan-out.

  use std::sync::atomic::Ordering;

  use super::ChatDelegate;
  use crate::agent::types::{ReflectionDetail, AgentRetryEvent, TokenUsage, ToolOutput};
  use crate::agent::types::ChatMessage;
  use serde_json::json;

  impl ChatDelegate {
      // methods inserted in Step 2.2
  }
  ```

- [ ] **Step 2.2: Move the methods from mod.rs to observability.rs**

  Cut these method definitions from `dispatcher/mod.rs` and paste them into the `impl ChatDelegate { ... }` block in `observability.rs`. Visibility unchanged:

  - `fn emit_text_delta(&self, chunk: &str)`
  - `fn emit_tool_start(&self, name: &str, id: &str, input: &serde_json::Value)`
  - `fn emit_tool_result(&self, name: &str, id: &str, output: &ToolOutput)`
  - `fn emit_done(&self, text: &str, truncated: bool)`
  - `fn emit_queued_consumed(&self, uuid: &str)`
  - `fn emit_thinking(&self, text: &str)`
  - `fn emit_thinking_done(&self, _duration_ms: u64)`
  - `async fn emit_turn_cost(&self, usage: &TokenUsage)`
  - `fn emit_context_stats(&self, messages: &[ChatMessage], cumulative_input: u32, cumulative_output: u32)`
  - `pub fn emit_reflection_status(&self, assistant_message_id: &str, status: &str)`
  - `pub fn emit_reflection(&self, detail: &ReflectionDetail)`
  - `fn emit_stream_reset(&self)`
  - `fn emit_retry_event(&self, event: AgentRetryEvent)`
  - `fn beat(&self, stage: &str)`
  - `pub fn set_heartbeat(&mut self, heartbeat: Arc<crate::agent::heartbeat::HeartbeatSupervisor>)`
  - `pub fn set_token_budget_collector(&mut self, collector: crate::agent::telemetry::TokenBudgetCollector)`
  - `pub fn set_compose_stats_collector(&mut self, collector: crate::agent::context_manager::ComposeStatsCollector)`

  Each method moves AS-IS, no body edits. Add use statements at the top of observability.rs for any types its bodies reference (`tauri`, `Arc`, etc.). Verify `use super::ChatDelegate;` is present.

- [ ] **Step 2.3: Declare the new submodule in mod.rs**

  At the top of `dispatcher/mod.rs`, after the existing `use` block, add:

  ```rust
  mod observability;
  ```

  No `pub use` needed — the `impl ChatDelegate` block in the submodule auto-exposes the methods on `ChatDelegate`.

- [ ] **Step 2.4: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected:
  - 0 build errors. Warnings ≤55 (Step 1.2 baseline).
  - Same test pass count as Pre-flight.

  Compile-time failures most likely come from missing `use` imports in the new file — chase those down by inspecting the error message and adding the appropriate `use ...;`.

- [ ] **Step 2.5: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split commit -m "refactor(agent): extract dispatcher/observability.rs (P3-5a.2 of 阶段 3)"
  ```

Continue to Task 3.

---

## Task 3: Extract `content_assembler.rs` — system prompt + dynamic context

**Goal:** Move the prompt-assembly methods + the setters for the pre-built prompt blocks + the 4 test modules that cover prompt-assembly behavior into a new `dispatcher/content_assembler.rs`.

**Files:**
- Create: `src-tauri/src/agent/dispatcher/content_assembler.rs`
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (remove relocated methods + tests, add `mod content_assembler;`)

### Steps

- [ ] **Step 3.1: Create content_assembler.rs scaffold**

  ```rust
  //! System prompt + dynamic-context assembly for ChatDelegate.
  //!
  //! `effective_system_prompt` is the system-prompt-string seam (cached by
  //! Anthropic prompt-cache breakpoint). `build_dynamic_context` is the
  //! per-turn block prepended to the last user message. Both must stay
  //! deterministic — small changes here can invalidate the cache and cost
  //! tokens. Tests in this file cover the order invariants.

  use super::ChatDelegate;
  use crate::agent::context_manager::ComposeStats;
  use crate::agent::types::{ChatMessage, ContentBlock};
  use crate::agent::safety::SafetyMode;
  use crate::runtime::context::ContextArtifact;

  impl ChatDelegate {
      // methods inserted in Step 3.2
  }
  ```

  Adjust the `use` block to match what the relocated bodies actually need; `cargo build` will guide you on missing imports.

- [ ] **Step 3.2: Move the methods from mod.rs to content_assembler.rs**

  Methods to relocate (cut from `mod.rs`, paste into the `impl ChatDelegate` block in `content_assembler.rs`):

  - `fn effective_system_prompt(&self, effective_mode: &SafetyMode) -> String`
  - `fn build_dynamic_context(&self) -> String`
  - `fn persona_prompt_block_best_effort(&self) -> Option<String>`
  - `fn context_manager_for_prompt_blocking(&self) -> Option<(String, ComposeStats, Vec<ContextArtifact>)>`
  - `fn estimate_context_pressure_ratio(&self) -> f32`
  - `pub fn set_memory_context(&mut self, context: String)`
  - `pub fn clear_memory_context_anchor(&self)`
  - `pub fn append_memory_context(&mut self, extra: &str)`
  - `pub fn set_skills_manifest_block(&mut self, block: String)`
  - `pub fn set_learned_profile_block(&mut self, block: String)`
  - `pub fn set_gbrain_knowledge_block(&mut self, block: String)`

  Also move the free `pub(crate) fn render_context_fragments` (referenced by `b2_context_wireup_tests`) — it lives near `build_dynamic_context` in the current file.

- [ ] **Step 3.3: Move the prompt-assembly test modules from mod.rs to content_assembler.rs**

  Cut these `#[cfg(test)] mod ... { ... }` blocks and paste them at the bottom of `content_assembler.rs`:

  - `mod manifest_suppression_tests` (dispatcher.rs:3091-3174 in original)
  - `mod manifest_cap_tests` (3175-3230)
  - `mod plan_guard_relevance_tests` (3231-3394)  — **Note:** this one is actually a turn-runner concern; flag the implementer to verify by reading the test bodies whether it covers prompt assembly or tool-call relevance. If turn-runner concern, defer to Task 6.
  - `mod memory_context_delta_render_tests` (3602-3794)
  - `mod b2_context_wireup_tests` (3795-end)

  Each test module's `use super::*;` may need to become `use super::*;` (still works — `super` is the parent `impl` site). If a test references items that didn't move (e.g., `truncate_utf8` from mod.rs), use absolute paths: `crate::agent::dispatcher::truncate_utf8`.

- [ ] **Step 3.4: Declare the new submodule in mod.rs**

  Add `mod content_assembler;` near the top, alongside `mod observability;`.

- [ ] **Step 3.5: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: 0 errors, ≤55 warnings, baselines preserved.

- [ ] **Step 3.6: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split commit -m "refactor(agent): extract dispatcher/content_assembler.rs (P3-5a.3 of 阶段 3)"
  ```

Continue to Task 4.

---

## Task 4: Extract `safety_gate.rs` — effective-mode resolution

**Goal:** Tiny but clean cut. Move the single `resolve_effective_mode` method into its own file so the safety-decision seam has its own home, ready for P3-5b to expand into a thin wrapper around `SafetyManager` queries.

**Files:**
- Create: `src-tauri/src/agent/dispatcher/safety_gate.rs`
- Modify: `src-tauri/src/agent/dispatcher/mod.rs`

### Steps

- [ ] **Step 4.1: Create safety_gate.rs**

  ```rust
  //! Safety mode resolution for ChatDelegate.
  //!
  //! Owns the effective-mode decision: per-session override beats the
  //! global mode held by SafetyManager. In P3-5b this file becomes the
  //! single chokepoint for safety queries (no direct SafetyManager reads
  //! from turn_runner / model_io).

  use super::ChatDelegate;
  use crate::agent::safety::SafetyMode;

  impl ChatDelegate {
      pub(super) async fn resolve_effective_mode(&self) -> SafetyMode {
          // Body cut verbatim from dispatcher/mod.rs
      }
  }
  ```

  **Visibility upgrade:** The current `resolve_effective_mode` is `fn` (private to dispatcher.rs). After the split, both `turn_runner.rs` and `content_assembler.rs` call it cross-file. Change to `pub(super)` so siblings in `dispatcher/` can call it. Do NOT make it `pub(crate)` — keep the seam tight.

- [ ] **Step 4.2: Move the method from mod.rs**

  Cut `async fn resolve_effective_mode(&self) -> SafetyMode { ... }` from `mod.rs`. Paste into the `impl ChatDelegate { }` block in `safety_gate.rs`. Change the signature's leading `fn` to `pub(super) fn` (preserving `async`).

- [ ] **Step 4.3: Declare the submodule**

  Add `mod safety_gate;` to `mod.rs`.

- [ ] **Step 4.4: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  ```
  Expected: 0 errors. Same test pass count.

- [ ] **Step 4.5: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split commit -m "refactor(agent): extract dispatcher/safety_gate.rs (P3-5a.4 of 阶段 3)"
  ```

Continue to Task 5.

---

## Task 5: Extract `model_io.rs` — LLM IO + streaming + `impl StreamSink`

**Goal:** Move `create_turn_snapshot`, `call_llm`, `sleep_or_abort`, and the entire `impl StreamSink for ChatDelegate { ... }` block + its supporting tests into a new `dispatcher/model_io.rs`.

**Files:**
- Create: `src-tauri/src/agent/dispatcher/model_io.rs`
- Modify: `src-tauri/src/agent/dispatcher/mod.rs`

### Steps

- [ ] **Step 5.1: Create model_io.rs scaffold**

  ```rust
  //! LLM streaming + turn snapshot for ChatDelegate.
  //!
  //! `create_turn_snapshot` freezes (model, prompt, tools) at turn start —
  //! the M2-J snapshot seam, enabling hot-swap-safe loops in Sprint-4.
  //! `call_llm` drives the provider stream; `impl StreamSink` receives
  //! delta callbacks. `sleep_or_abort` is the cancellation-aware sleep
  //! that ALL loop pauses go through.

  use std::sync::atomic::Ordering;
  use std::sync::Arc;
  use std::time::Duration;

  use async_trait::async_trait;

  use super::ChatDelegate;
  use crate::agent::loop_traits::{LoopDelegate, LoopOutcome, LoopSignal, StreamSink};
  use crate::agent::types::{AgentRetryEvent, ReasoningContext, TokenUsage, TurnSnapshot};

  impl ChatDelegate {
      // create_turn_snapshot, call_llm, sleep_or_abort go here
  }

  #[async_trait]
  impl StreamSink for ChatDelegate {
      // on_text_delta, on_thinking, on_thinking_done, on_stream_reset,
      // on_retry_event, sleep_or_abort go here
  }
  ```

- [ ] **Step 5.2: Move methods + trait impl from mod.rs**

  Cut from `dispatcher/mod.rs`:
  - `async fn create_turn_snapshot(&self, ...) -> TurnSnapshot` (currently inside `impl LoopDelegate` block — when you move it, decide: is it moving to the LoopDelegate impl in turn_runner.rs OR to the standalone impl in model_io.rs? **Decision:** It's part of `impl LoopDelegate` and the trait impl must live in one file. Move it to turn_runner.rs in Task 6 instead, NOT here. Keep it in mod.rs for now.) ← Re-evaluate before Step 5.2 starts.
  - `async fn call_llm(...)` (part of `impl LoopDelegate`) — same constraint as above. Move with turn_runner in Task 6.
  - `async fn sleep_or_abort(&self, duration: Duration) -> bool` (the inherent method, NOT the trait method — the latter is in `impl StreamSink` and goes with that)
  - The entire `impl StreamSink for ChatDelegate { ... }` block (lines 1549-1670 of original)

  **Revised decision for Task 5:** Move ONLY `impl StreamSink for ChatDelegate` and the inherent `sleep_or_abort` (the one called from inside the dispatcher body, not the trait one). `create_turn_snapshot` + `call_llm` are part of `impl LoopDelegate` and stay with that trait in Task 6.

  After this revision, `model_io.rs` is smaller (~250 LoC: StreamSink impl + inherent sleep_or_abort + supporting `panic_recovery_tests` + `truncated_continuation_tests`).

- [ ] **Step 5.3: Move tests**

  Cut from `mod.rs`:
  - `mod panic_recovery_tests` (covers stream-reset recovery — naturally tied to StreamSink callbacks)
  - `mod truncated_continuation_tests` (covers re-issuing a stream when text is truncated — tied to model IO)

  Paste at the bottom of `model_io.rs`.

- [ ] **Step 5.4: Declare submodule**

  `mod model_io;` in `mod.rs`.

- [ ] **Step 5.5: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: 0 errors. Baselines preserved.

- [ ] **Step 5.6: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split commit -m "refactor(agent): extract dispatcher/model_io.rs + impl StreamSink (P3-5a.5 of 阶段 3)"
  ```

Continue to Task 6.

---

## Task 6: Extract `turn_runner.rs` — `impl LoopDelegate` + tool dispatch + GEP

**Goal:** The biggest extraction. Move the entire `impl LoopDelegate for ChatDelegate { ... }` block (7 methods including `create_turn_snapshot` + `call_llm`) + the `tool_dispatcher()` lazy builder + `spawn_post_turn_extraction` + the turn-running test modules into `dispatcher/turn_runner.rs`.

**Files:**
- Create: `src-tauri/src/agent/dispatcher/turn_runner.rs`
- Modify: `src-tauri/src/agent/dispatcher/mod.rs`

### Steps

- [ ] **Step 6.1: Create turn_runner.rs scaffold**

  ```rust
  //! The agent loop body for ChatDelegate.
  //!
  //! `impl LoopDelegate` is the contract the outer `run_loop` calls into:
  //! check_signals → before_llm_call → create_turn_snapshot → call_llm →
  //! handle_text_response → execute_tool_calls → on_usage. Each is one
  //! step of one iteration of one turn.

  use std::sync::Arc;
  use std::sync::OnceLock;

  use async_trait::async_trait;
  use tauri::Wry;

  use super::ChatDelegate;
  use crate::agent::loop_traits::{LoopDelegate, LoopOutcome, LoopSignal};
  use crate::agent::tool_dispatch::ToolDispatcher;
  use crate::agent::types::{
      ChatMessage, ContentBlock, ReasoningContext, TokenUsage, TurnSnapshot,
  };

  impl ChatDelegate {
      pub(super) fn tool_dispatcher(&self) -> Arc<ToolDispatcher<Wry>> {
          // body cut from mod.rs
      }

      pub(super) fn spawn_post_turn_extraction(&self, reason_ctx: &ReasoningContext) {
          // body cut from mod.rs
      }
  }

  #[async_trait]
  impl LoopDelegate for ChatDelegate {
      // check_signals, before_llm_call, create_turn_snapshot, call_llm,
      // handle_text_response, execute_tool_calls, on_usage go here.
  }
  ```

- [ ] **Step 6.2: Move methods + trait impl from mod.rs**

  Cut from `dispatcher/mod.rs`:
  - `fn tool_dispatcher(&self) -> ...` (the lazy build helper)
  - `fn spawn_post_turn_extraction(&self, reason_ctx: &ReasoningContext)`
  - The entire `#[async_trait] impl LoopDelegate for ChatDelegate { ... }` block (7 methods, lines ~1829-2900 in original)

  Bodies are unchanged. Just relocated.

- [ ] **Step 6.3: Move test modules**

  Cut these from `mod.rs` and paste at the bottom of `turn_runner.rs`:

  - `mod browser_runtime_dispatch_patch_tests` (tests execute_tool_calls' browser-runtime patch handling)
  - `mod active_plan_history_tests` (tests handle_text_response's plan-history scanning)
  - `mod plan_guard_relevance_tests` (if Task 3 left it in mod.rs — see Step 3.3 note)

- [ ] **Step 6.4: Declare submodule**

  `mod turn_runner;` in `mod.rs`. After this, mod.rs should be down to ~700-800 LoC (struct + ctor + setters + helpers + general utility tests + 3 free functions).

- [ ] **Step 6.5: Build + tests (BIG GATE — this is the loop body)**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  ```
  Expected:
  - 0 build errors.
  - All dispatcher tests preserve their pass count.
  - agent:: 796/2 baseline preserved.
  - Warnings ≤55.

  Most likely failure: a method that called `self.foo()` inside the loop body relies on `foo` being defined in the same impl block. Cross-file impl blocks resolve identically, so this should JUST WORK — but if it doesn't, the error message will identify a missing visibility upgrade. Most fixes: change `fn foo(&self)` to `pub(super) fn foo(&self)` so siblings can call it.

- [ ] **Step 6.6: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split commit -m "refactor(agent): extract dispatcher/turn_runner.rs + impl LoopDelegate (P3-5a.6 of 阶段 3)"
  ```

Continue to Task 7.

---

## Task 7: Verify final shape + warning gate + module size audit

**Goal:** Confirm `mod.rs` is now the facade described in the architecture: struct + ctor + general setters + helpers. Confirm each submodule is in the LoC range targeted by the design. Run the full battery one more time. No code changes expected — this is a checkpoint.

**Files:**
- None (verification + final commit only if cleanup edits emerge)

### Steps

- [ ] **Step 7.1: File-size audit**

  ```bash
  wc -l /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri/src/agent/dispatcher/*.rs
  ```
  Expected (rough):
  - `mod.rs`: 700-900 LoC
  - `observability.rs`: 350-450 LoC
  - `content_assembler.rs`: 600-800 LoC
  - `safety_gate.rs`: 30-80 LoC
  - `model_io.rs`: 200-400 LoC
  - `turn_runner.rs`: 800-1100 LoC

  Total: ~3,000-3,700 LoC across 6 files (some shrinkage from removing duplicate `use` lines, none from method dedup — that's 5b).

- [ ] **Step 7.2: Full test battery**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri && cargo test --lib 2>&1 | tail -5
  ```
  Required:
  - 0 errors.
  - ≤55 warnings (50 was the post-P3-4 baseline; some extra `unused_imports` may pop up but should be cleanable).
  - `agent::` 796/2 baseline preserved.
  - `cargo test --lib` total ≥ Pre-flight count.

- [ ] **Step 7.3: Verify external callers still resolve**

  ```bash
  grep -n "agent::dispatcher::ChatDelegate\|agent::dispatcher::detect_soft_tool_error\|agent::dispatcher::truncate_utf8\|agent::dispatcher::load_attached_dirs_for_session" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split/src-tauri/src/tauri_commands.rs
  ```
  These all continue to resolve because:
  1. `ChatDelegate` is still re-exported from `dispatcher::` via the struct definition in `mod.rs`.
  2. The 3 free helpers remained in `mod.rs` (Task 1 didn't relocate them).

- [ ] **Step 7.4 (optional): Clean unused imports**

  If the warning count crept up due to `use` statements that were copied between mod.rs and submodules and are now only needed in one location, run `cargo fix --lib --allow-dirty` to clean them. Review the changes before committing.

- [ ] **Step 7.5: Final commit (if any edits)**

  Only commit if Step 7.4 produced changes:
  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split commit -m "refactor(agent): clean unused imports in dispatcher/ (P3-5a.7 of 阶段 3)"
  ```

- [ ] **Step 7.6: Verify final chain + clean tree**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split log --oneline main..HEAD
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5a-dispatcher-structural-split status -sb
  ```
  Expected: 6-7 commits ahead of `main` (Tasks 1-6 mandatory + optional Task 7 cleanup). Working tree clean.

---

## Self-Review

**1. Spec coverage:**
- ✅ §6.1 module layout (6 files): mod, turn_runner, content_assembler, model_io, safety_gate, observability.
- 🟡 §6.1 LoC targets: spec says ~2,400 total post-split; this PR keeps ~3,000-3,700 because it doesn't collapse the 5 ContentBlock dup sites (deferred to 5b).
- 🟡 §6.2 field reduction 71→~15: deferred to 5b. This PR doesn't change the struct.
- ✅ §6 trait impls (StreamSink, LoopDelegate): each moves to exactly one file (model_io, turn_runner).

**2. Placeholder scan:**
- Task 5 contains a self-correction inline ("**Revised decision for Task 5:** ...") flagging that `create_turn_snapshot` + `call_llm` are part of `impl LoopDelegate` and move with it in Task 6, not in Task 5. This is intentional context for the implementer.
- Task 3 Step 3.3 flags `plan_guard_relevance_tests` for the implementer to verify its true target module (content_assembler vs turn_runner) by reading the test bodies — explicit judgment call, not a placeholder.
- No "TBD" / "TODO" / "implement later".

**3. Type consistency:**
- `ChatDelegate` referenced as `super::ChatDelegate` from every submodule (consistent).
- `pub(super) fn` for cross-module-private upgrades (consistent — Task 4 + likely Task 5/6).
- Trait impls always inside the module owning the trait's natural home (StreamSink → model_io, LoopDelegate → turn_runner).

**4. Bisectability:**
- Each task = one commit. Each commit's `cargo test --lib agent::` MUST pass; that's the gate.
- Tasks ordered by smallest-blast-radius first (observability) → biggest (turn_runner) so any breakage surfaces with the smallest delta.

---

## Cumulative summary

- **Tasks:** 7 (6 mandatory + 1 optional cleanup).
- **Estimated time:** 1.5-2 person-days (Tasks 2-6 are mechanical cut-paste with import wrangling; Task 6 the largest with ~800-1100 LoC moving; Task 1 is just `git mv`).
- **Risk:** Low-medium. Pure relocation, but cross-file `&self` access can surface visibility errors that require `pub(super)` upgrades. No semantic changes — the test suite is the safety net.
- **Total commits:** 6-7 (one per task).
- **Cumulative file impact:**
  - Task 1: 1 (mv)
  - Tasks 2-6: 2-3 each (1 new submodule + 1 mod.rs edit, sometimes a tauri_commands.rs touch if visibility surprises us)
  - Task 7: 0-1

After P3-5a ships: `dispatcher/` is a 6-file module with clear responsibility boundaries. P3-5b can then:
1. Replace ~25 subsystem-ref fields with `Arc<AppState>` + `Arc<AgentApi>` handles (target ~30 fields total).
2. Collapse the 5 ContentBlock assembly sites inside `content_assembler.rs` into a single helper.
3. Potentially extract `gep_capsule.rs` from mod.rs if `generate_capsule_for_turn` grows.
