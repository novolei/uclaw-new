# 阶段 3 P3-6 — `assemble_system_prompt` Single Seam + Golden Snapshots · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the prompt-assembly seam from **4 implicit layers** (`mode_prompts::compose_*` → `effective_system_prompt` appends skill manifest → `build_dynamic_context` separately builds time/memory/profile/gbrain/fragments → `turn_runner.rs:767` splices dynamic into last user message) into **1 explicit single function**: `pub fn assemble_system_prompt(SystemPromptContext) -> AssembledPrompt`. Add 5 golden-snapshot tests that lock the current byte-stable prompt format so future changes can't drift silently. **Closes 阶段 3** — this is the last PR before stage transition.

**Architecture:** Pure data-shape refactor. ALL inputs (base prompt, workspace, mode, injection signals, persona, manifest + suppress flag, memory context + prior snapshot, learned profile, gbrain knowledge, selected fragments, current time) become fields of a `SystemPromptContext` struct. ALL outputs (system prompt string, per-turn dynamic block string, new memory-context snapshot) become fields of an `AssembledPrompt` struct. The new function `assemble_system_prompt` is **pure** — no `&self`, no side effects, no hidden state reads. ChatDelegate's two existing methods (`effective_system_prompt`, `build_dynamic_context`) become thin wrappers that build the context, call the function, and propagate side effects (set snapshot, flip first_act_turn, store fragments, record telemetry).

**Tech Stack:** Rust 2021, no new crates. The 5 snapshot tests use plain string comparison — no `insta` or other snapshot framework (matches the codebase's existing pattern).

**Related design:**
- Pi-convergence gap audit: [`2026-05-27-pi-convergence-gap-audit.md`](../specs/2026-05-27-pi-convergence-gap-audit.md) §1.2 MAJOR ("模型看到什么 由 4 个互不知情的层拼成,无一权威").
- AgentApi handle design: [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §7 "`assemble_system_prompt` single seam with golden snapshot tests".
- Prior 阶段 3 PRs: #570 (P3-1), #571 (P3-2), #572 (P3-3), #573 (P3-4), #574 (P3-5a), #575 (P3-5b1), #576 (P3-5b2), #577 (P3-5b3). Merged to main at `901744f0`.

---

## Recon-discovered facts (verified against `901744f0` main, 2026-05-29)

`effective_system_prompt` lives at `dispatcher/content_assembler.rs:21-95` (~75 LoC).
`build_dynamic_context` lives at `dispatcher/content_assembler.rs:157-312` (~155 LoC).

### Input inventory (everything the two methods read from `&self` today)

| Input | Source field | Used by |
|---|---|---|
| Base system prompt | `self.system_prompt: String` | `effective_system_prompt` (via `mode_prompts::compose_*`) |
| Workspace root | `self.workspace_root: Option<PathBuf>` | BOTH (compose uses for `[WORKSPACE]` block; dynamic uses for time block) |
| Effective safety mode | passed in as `&SafetyMode` | `effective_system_prompt` |
| InjectionContext signals | `self.is_first_act_turn`, `self.last_error_kind`, `self.estimate_context_pressure_ratio()` | `effective_system_prompt` (passed to `compose_*` + baseline_blocks) |
| Persona block | computed via `self.persona_prompt_block_best_effort()` (reads `AppState.db` + `PersonaStore`) | `effective_system_prompt` |
| Skill manifest block | `self.prompt_blocks.skills_manifest: String` | `effective_system_prompt` |
| Skill manifest suppress flag | `self.skill_search_used: AtomicBool` | `effective_system_prompt` |
| Memory context | `self.memory_context: Option<String>` | `build_dynamic_context` |
| Prior memory snapshot | `self.last_memory_context_snapshot: Mutex<Option<LineFragmentSnapshot>>` | `build_dynamic_context` |
| Learned profile block | `self.prompt_blocks.learned_profile: String` | `build_dynamic_context` |
| gbrain knowledge block | `self.prompt_blocks.gbrain_knowledge: String` | `build_dynamic_context` |
| Selected context fragments | `self.last_injected_fragments: Mutex<Vec<ContextArtifact>>` | `build_dynamic_context` |
| Current time | `chrono::Local::now()` inside `build_dynamic_context` | `build_dynamic_context` |

### Side effects (everything the two methods write to `&self` today)

| Side effect | Source | Triggered by |
|---|---|---|
| Stash selected fragments | `self.last_injected_fragments` ← `composed.injected_fragments` | `effective_system_prompt` (after `for_prompt_with_injection`) |
| Record compose stats | `self.telemetry.compose_stats.record(&conversation_id, ...)` | `effective_system_prompt` |
| Flip first-act flag | `self.is_first_act_turn.store(false, ...)` | `effective_system_prompt` |
| Update memory snapshot | `self.last_memory_context_snapshot` ← new snapshot | `build_dynamic_context` |
| Log diff statistics | `tracing::info!/debug!` lines | `build_dynamic_context` |

### Baselines to hold

- `cargo build`: 0 errors, **≤49 warnings** (post-5b3 baseline).
- `cargo test --lib agent::dispatcher`: **50 passed / 0 failed**.
- `cargo test --lib agent::`: **796 passed / 2 pre-existing failed**.
- `cargo test --lib` total: **3,050 passed / 7 pre-existing failed** (+3 from P3-5b3 → 3,053 passing on `901744f0`).

After P3-6: +5 new golden snapshot tests in `dispatcher/content_assembler.rs` → dispatcher count rises to 55. agent:: total rises by 5 to 801.

### External callers / scope

This refactor touches:
- `src-tauri/src/agent/dispatcher/content_assembler.rs` — main work
- `src-tauri/src/agent/dispatcher/turn_runner.rs` — IF Task 4 collapses the 2 separate call sites into one (optional)

NO changes to: `app.rs`, `tauri_commands.rs`, the LLM providers, `agentic_loop.rs`, the `uclaw_message_types` crate.

---

## Target shape

### `SystemPromptContext` struct (pure input bundle)

```rust
/// All inputs needed to assemble both the system prompt AND the per-turn
/// dynamic block. Built once per `call_llm`; consumed by exactly one
/// `assemble_system_prompt` call.
///
/// Holds owned data so the assembly function is pure (no `&self`, no
/// hidden state reads, no async). Construction reads from `ChatDelegate`
/// state once, then the resulting `SystemPromptContext` can be tested in
/// isolation with arbitrary inputs.
pub(super) struct SystemPromptContext {
    /// `ChatDelegate.system_prompt` — the user's session-scope base prompt.
    pub base_system_prompt: String,
    /// Workspace root for `[WORKSPACE]` system-prompt block + time-block path.
    pub workspace_root: Option<std::path::PathBuf>,
    /// Per-session safety mode (override or global).
    pub effective_mode: crate::safety::SafetyMode,
    /// A4 injection signals (3 fields).
    pub injection_context: crate::agent::baseline_blocks::InjectionContext,
    /// Persona block from PersonaStore (pre-computed since DB access is
    /// not pure; caller passes `None` if PersonaStore unavailable).
    pub persona_block: Option<String>,
    /// Skill manifest block; empty string when no skills exist.
    pub skills_manifest_block: String,
    /// True after the agent has called `skill_search` in this loop —
    /// suppresses the manifest on subsequent calls (PR 2026-05-13).
    pub skills_manifest_suppress: bool,
    /// Memory recall results (Option to distinguish "not set" from
    /// "explicitly empty"). Read by the dynamic block, not the system prompt.
    pub memory_context: Option<String>,
    /// Prior turn's memory_context snapshot for delta annotation
    /// (M2-D Phase 2 Bundle 16-B). `None` on the first turn.
    pub prior_memory_snapshot: Option<crate::agent::context_diff::LineFragmentSnapshot>,
    /// Pre-built `## User Profile (Learned)` block; empty when disabled.
    pub learned_profile_block: String,
    /// Pre-built gbrain instructions; empty when gbrain disconnected.
    pub gbrain_knowledge_block: String,
    /// ContextManager-selected fragments to inject in the dynamic block.
    pub injected_fragments: Vec<crate::runtime::context::ContextArtifact>,
    /// Current wall-clock time. Passed in (rather than read via `Local::now()`
    /// inside the function) so snapshot tests can pin a deterministic time.
    pub now: chrono::DateTime<chrono::Local>,
}
```

### `AssembledPrompt` struct (pure output bundle)

```rust
/// Outputs of `assemble_system_prompt`. Caller propagates side effects
/// (snapshot store, first-act-flag flip, telemetry) based on what's
/// returned here.
pub(super) struct AssembledPrompt {
    /// The byte-stable system prompt (Anthropic prompt-cache hits here).
    pub system: String,
    /// The per-turn dynamic block — prepended to the last user message.
    pub dynamic_for_last_user: String,
    /// Snapshot to store back to `ChatDelegate.last_memory_context_snapshot`
    /// for the next turn's diff. `None` when no memory_context was injected.
    pub new_memory_context_snapshot:
        Option<crate::agent::context_diff::LineFragmentSnapshot>,
}
```

### `assemble_system_prompt` — the single seam

```rust
/// Single-seam prompt assembly. Pure: takes a fully-populated
/// `SystemPromptContext`, returns an `AssembledPrompt`. No `&self`,
/// no I/O, no time reads, no DB access. Side effects (snapshot store,
/// first-act-flag flip, telemetry record) are the caller's job.
///
/// Replaces the 4-layer assembly that lived across `mode_prompts::compose_*`,
/// `effective_system_prompt`, `build_dynamic_context`, and the
/// `turn_runner.rs::call_llm` splicing point (P3-6 of the 阶段-3
/// Pi-convergence remediation — gap-audit §1.2 MAJOR).
pub(super) fn assemble_system_prompt(ctx: SystemPromptContext) -> AssembledPrompt {
    // 1. System half — byte-stable for prompt cache.
    //    mode_prompts::compose + persona + skills manifest (with suppress).
    let mut system = crate::agent::mode_prompts::compose_system_prompt_with_injection_and_persona(
        &ctx.base_system_prompt,
        ctx.workspace_root.as_deref(),
        &ctx.effective_mode,
        &ctx.injection_context,
        ctx.persona_block.as_deref(),
    );
    if !ctx.skills_manifest_block.is_empty() && !ctx.skills_manifest_suppress {
        system.push_str(&ctx.skills_manifest_block);
    }

    // 2. Dynamic half — per-turn block prepended to last user message.
    let dynamic_for_last_user = build_dynamic_block(&ctx);

    // 3. New snapshot — caller stores back for next turn.
    let new_memory_context_snapshot = ctx
        .memory_context
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| {
            crate::agent::context_diff::LineFragmentSnapshot::from_text("memory_context", s)
        });

    AssembledPrompt {
        system,
        dynamic_for_last_user,
        new_memory_context_snapshot,
    }
}

/// Internal helper for the dynamic-block construction. Extracted so
/// the snapshot tests can call it directly with a known time.
fn build_dynamic_block(ctx: &SystemPromptContext) -> String {
    // ... time block + memory_context (with delta annotation) + learned
    // profile + gbrain knowledge + fragments. Body is the current
    // `build_dynamic_context` body verbatim except for:
    //   - reads `ctx.now` instead of `Local::now()`
    //   - reads `ctx.workspace_root` instead of `self.workspace_root`
    //   - reads `ctx.memory_context` instead of `self.memory_context`
    //   - reads `ctx.prior_memory_snapshot` instead of locking `self.last_memory_context_snapshot`
    //   - reads `ctx.learned_profile_block` instead of `self.prompt_blocks.learned_profile`
    //   - reads `ctx.gbrain_knowledge_block` instead of `self.prompt_blocks.gbrain_knowledge`
    //   - reads `ctx.injected_fragments` instead of locking `self.last_injected_fragments`
    // NOTE: the tracing log lines for diff stats STAY — they're observability,
    // not state.
}
```

### `ChatDelegate.effective_system_prompt` becomes a thin wrapper

```rust
pub(super) fn effective_system_prompt(&self, effective_mode: &crate::safety::SafetyMode) -> String {
    // Build the context once (reads + locks).
    let ctx = self.build_prompt_context(effective_mode.clone());

    // Stash fragments + record stats (existing side effect, moved here
    // since the assemble fn no longer reads ContextManager).
    let inj_ctx = ctx.injection_context.clone();
    let query = crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]);
    let composed = self.context_manager_for_prompt_blocking(&query, &inj_ctx);
    if let Ok(mut slot) = self.last_injected_fragments.lock() {
        *slot = composed.injected_fragments.clone();
    }
    if let Some(collector) = &self.telemetry.compose_stats {
        collector.record(&self.conversation_id, composed.stats.clone());
    }

    // Hot-path side effect: first-act flag transitions to false after this read.
    self.is_first_act_turn.store(false, std::sync::atomic::Ordering::Relaxed);

    // Call the single seam.
    let assembled = assemble_system_prompt(ctx);

    // Propagate snapshot side effect.
    if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
        *slot = assembled.new_memory_context_snapshot;
    }

    assembled.system
}
```

### `ChatDelegate.build_dynamic_context` becomes a thin wrapper

```rust
pub(super) fn build_dynamic_context(&self) -> String {
    // Build context with a placeholder effective_mode (mode isn't used by
    // the dynamic half — only by the system half — but we need *some*
    // value to populate the struct).
    let dummy_mode = crate::safety::SafetyMode::default();  // any value works
    let ctx = self.build_prompt_context(dummy_mode);
    let assembled = assemble_system_prompt(ctx);

    // Note: snapshot side effect is no-op here because `effective_system_prompt`
    // ran first in the same turn and already stored the snapshot.
    assembled.dynamic_for_last_user
}
```

(Alternative — see Task 4 — collapse both calls in `turn_runner.rs::call_llm` into ONE call that uses both halves of the same `AssembledPrompt`.)

### `ChatDelegate.build_prompt_context` (new private builder)

```rust
fn build_prompt_context(&self, effective_mode: crate::safety::SafetyMode) -> SystemPromptContext {
    SystemPromptContext {
        base_system_prompt: self.system_prompt.clone(),
        workspace_root: self.workspace_root.clone(),
        effective_mode,
        injection_context: crate::agent::baseline_blocks::InjectionContext {
            is_first_act_turn: self.is_first_act_turn.load(std::sync::atomic::Ordering::Relaxed),
            last_error_kind: self.last_error_kind.lock().ok().and_then(|g| g.clone()),
            context_pressure_ratio: self.estimate_context_pressure_ratio(),
        },
        persona_block: self.persona_prompt_block_best_effort(),
        skills_manifest_block: self.prompt_blocks.skills_manifest.clone(),
        skills_manifest_suppress: self.skill_search_used.load(std::sync::atomic::Ordering::Relaxed),
        memory_context: self.memory_context.clone(),
        prior_memory_snapshot: self.last_memory_context_snapshot.lock().ok().and_then(|g| g.clone()),
        learned_profile_block: self.prompt_blocks.learned_profile.clone(),
        gbrain_knowledge_block: self.prompt_blocks.gbrain_knowledge.clone(),
        injected_fragments: self.last_injected_fragments.lock().ok().map(|g| g.clone()).unwrap_or_default(),
        now: chrono::Local::now(),
    }
}
```

---

## Pre-flight (before Task 1)

1. **Confirm main baseline:**

   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   git -C /Users/ryanliu/Documents/uclaw log --oneline -3
   ```
   Expected: `## main...origin/main` at `901744f0`.

2. **Create worktree + symlinks:**

   ```bash
   git worktree add -b claude/stage3-p6-single-seam \
       /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri/bunembed
   ```

3. **Capture baselines:**

   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   ```

---

## Task 1: Define `SystemPromptContext` + `AssembledPrompt` structs

**Goal:** Add the two pure data-bundle structs to `dispatcher/content_assembler.rs`. No callers yet — just the type definitions.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/content_assembler.rs` (add struct definitions near the top)

### Steps

- [ ] **Step 1.1: Find the insertion point**

  Read `content_assembler.rs` lines 1-12 to find the top of the file (use statements + `impl ChatDelegate`). The two new structs go BEFORE the `impl ChatDelegate { ... }` block at line 12.

- [ ] **Step 1.2: Insert struct definitions**

  Insert at top of file (after `use` statements, before `impl ChatDelegate`):

  ```rust
  /// All inputs needed to assemble both the system prompt AND the per-turn
  /// dynamic block. Built once per `call_llm`; consumed by exactly one
  /// `assemble_system_prompt` call. P3-6 single-seam input bundle.
  ///
  /// Holds owned data so the assembly function is pure (no `&self`, no
  /// hidden state reads, no async). Construction reads from `ChatDelegate`
  /// state once, then this can be tested in isolation with arbitrary inputs.
  pub(super) struct SystemPromptContext {
      pub base_system_prompt: String,
      pub workspace_root: Option<std::path::PathBuf>,
      pub effective_mode: crate::safety::SafetyMode,
      pub injection_context: crate::agent::baseline_blocks::InjectionContext,
      pub persona_block: Option<String>,
      pub skills_manifest_block: String,
      pub skills_manifest_suppress: bool,
      pub memory_context: Option<String>,
      pub prior_memory_snapshot: Option<crate::agent::context_diff::LineFragmentSnapshot>,
      pub learned_profile_block: String,
      pub gbrain_knowledge_block: String,
      pub injected_fragments: Vec<crate::runtime::context::ContextArtifact>,
      pub now: chrono::DateTime<chrono::Local>,
  }

  /// Outputs of `assemble_system_prompt`. Caller propagates side effects
  /// (snapshot store, first-act-flag flip, telemetry record) based on
  /// what's returned here. P3-6 single-seam output bundle.
  pub(super) struct AssembledPrompt {
      pub system: String,
      pub dynamic_for_last_user: String,
      pub new_memory_context_snapshot:
          Option<crate::agent::context_diff::LineFragmentSnapshot>,
  }
  ```

  Verify the type paths against actual `use` statements in the file. If `crate::safety::SafetyMode` is already imported under a different alias, use that. Same for `crate::agent::baseline_blocks::InjectionContext`, `crate::agent::context_diff::LineFragmentSnapshot`, `crate::runtime::context::ContextArtifact`.

- [ ] **Step 1.3: Build**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  ```

  Expected: 0 errors. The new structs will trigger `dead_code` warnings since nothing uses them yet — that's OK, accept up to 2 extra warnings (51 total).

- [ ] **Step 1.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam add -A src-tauri/src/agent/dispatcher/content_assembler.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam commit -m "feat(agent): add SystemPromptContext + AssembledPrompt structs (P3-6.1 of 阶段 3)"
  ```

Continue to Task 2.

---

## Task 2: Implement `assemble_system_prompt` + `build_dynamic_block` (pure)

**Goal:** Add the pure single-seam function. NO `&self`. Body is the merged logic from current `effective_system_prompt` + `build_dynamic_context`, refactored to read from `SystemPromptContext` instead of `self`.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/content_assembler.rs` (add the new pure function + a `build_dynamic_block` helper)

### Steps

- [ ] **Step 2.1: Add `assemble_system_prompt` after the structs**

  Insert after the struct definitions from Task 1 (still before `impl ChatDelegate`):

  ```rust
  /// Single-seam prompt assembly. Pure: takes a fully-populated
  /// `SystemPromptContext`, returns an `AssembledPrompt`. No `&self`,
  /// no I/O, no time reads, no DB access.
  ///
  /// Side effects (snapshot store, first-act-flag flip, telemetry
  /// record) are the caller's job — propagate based on the returned
  /// `AssembledPrompt`.
  ///
  /// Replaces the 4-layer assembly previously spread across
  /// `mode_prompts::compose_*`, `effective_system_prompt`,
  /// `build_dynamic_context`, and the `turn_runner.rs::call_llm`
  /// splice point (P3-6 of 阶段-3 Pi-convergence remediation,
  /// addresses gap-audit §1.2 MAJOR).
  pub(super) fn assemble_system_prompt(ctx: SystemPromptContext) -> AssembledPrompt {
      // 1. System half — byte-stable for prompt cache.
      let mut system = crate::agent::mode_prompts::compose_system_prompt_with_injection_and_persona(
          &ctx.base_system_prompt,
          ctx.workspace_root.as_deref(),
          &ctx.effective_mode,
          &ctx.injection_context,
          ctx.persona_block.as_deref(),
      );
      if !ctx.skills_manifest_block.is_empty() && !ctx.skills_manifest_suppress {
          system.push_str(&ctx.skills_manifest_block);
      }

      // 2. Dynamic half — per-turn block prepended to last user message.
      let dynamic_for_last_user = build_dynamic_block(&ctx);

      // 3. New snapshot for next turn's diff.
      let new_memory_context_snapshot = ctx
          .memory_context
          .as_deref()
          .filter(|s| !s.is_empty())
          .map(|s| crate::agent::context_diff::LineFragmentSnapshot::from_text("memory_context", s));

      AssembledPrompt {
          system,
          dynamic_for_last_user,
          new_memory_context_snapshot,
      }
  }
  ```

- [ ] **Step 2.2: Add `build_dynamic_block` private helper**

  Below `assemble_system_prompt`, add:

  ```rust
  /// Internal helper — builds the per-turn dynamic block from a
  /// `SystemPromptContext`. Extracted so tests can hit it directly.
  fn build_dynamic_block(ctx: &SystemPromptContext) -> String {
      // body: copy + adapt the current `build_dynamic_context` body.
      // 1. Time block from ctx.now (NOT chrono::Local::now()).
      // 2. workspace_root from ctx.workspace_root.
      // 3. memory_context block (with delta annotation logic) from
      //    ctx.memory_context + ctx.prior_memory_snapshot.
      // 4. learned_profile_block from ctx.learned_profile_block.
      // 5. gbrain_knowledge_block from ctx.gbrain_knowledge_block.
      // 6. Fragments via render_context_fragments(ctx.injected_fragments).
      // (Tracing log lines for diff stats STAY.)
  }
  ```

  Adapting the existing `build_dynamic_context` body (lines 157-312 of current `content_assembler.rs`):
  - Replace `chrono::Local::now()` with `ctx.now`.
  - Replace `&self.workspace_root` with `&ctx.workspace_root`.
  - Replace `self.memory_context.as_deref().filter(...)` with `ctx.memory_context.as_deref().filter(...)`.
  - Replace `self.last_memory_context_snapshot.lock().ok().and_then(|g| g.clone())` with `ctx.prior_memory_snapshot.clone()`.
  - **REMOVE** the snapshot-store side effect at lines 267-269 (it's now `assemble_system_prompt`'s job to compute the new snapshot; the CALLER stores it).
  - Replace `self.prompt_blocks.learned_profile` with `&ctx.learned_profile_block` (and similarly for gbrain).
  - Replace `self.last_injected_fragments.lock()` with `&ctx.injected_fragments`.

- [ ] **Step 2.3: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  Expected: 0 errors, dispatcher 50/0 (still — wrappers not changed yet), agent:: 796/2.

- [ ] **Step 2.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam add -A src-tauri/src/agent/dispatcher/content_assembler.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam commit -m "feat(agent): add assemble_system_prompt + build_dynamic_block pure functions (P3-6.2 of 阶段 3)"
  ```

Continue to Task 3.

---

## Task 3: `effective_system_prompt` + `build_dynamic_context` become thin wrappers

**Goal:** Replace the bodies of the two ChatDelegate methods with calls to `assemble_system_prompt`. The wrappers handle the side effects (snapshot store, first-act flip, fragment stash, telemetry). The 50 existing dispatcher tests prove behavioral equivalence.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/content_assembler.rs` (rewrite the 2 method bodies + add `build_prompt_context` private builder)

### Steps

- [ ] **Step 3.1: Add `build_prompt_context` private method on ChatDelegate**

  Inside the `impl ChatDelegate { ... }` block, add a private builder before `effective_system_prompt`:

  ```rust
  /// Build a fully-populated SystemPromptContext from current
  /// ChatDelegate state. Side-effect-free (just reads + clones).
  fn build_prompt_context(
      &self,
      effective_mode: crate::safety::SafetyMode,
  ) -> SystemPromptContext {
      SystemPromptContext {
          base_system_prompt: self.system_prompt.clone(),
          workspace_root: self.workspace_root.clone(),
          effective_mode,
          injection_context: crate::agent::baseline_blocks::InjectionContext {
              is_first_act_turn: self.is_first_act_turn.load(std::sync::atomic::Ordering::Relaxed),
              last_error_kind: self.last_error_kind.lock().ok().and_then(|g| g.clone()),
              context_pressure_ratio: self.estimate_context_pressure_ratio(),
          },
          persona_block: self.persona_prompt_block_best_effort(),
          skills_manifest_block: self.prompt_blocks.skills_manifest.clone(),
          skills_manifest_suppress: self.skill_search_used.load(std::sync::atomic::Ordering::Relaxed),
          memory_context: self.memory_context.clone(),
          prior_memory_snapshot: self.last_memory_context_snapshot.lock().ok().and_then(|g| g.clone()),
          learned_profile_block: self.prompt_blocks.learned_profile.clone(),
          gbrain_knowledge_block: self.prompt_blocks.gbrain_knowledge.clone(),
          injected_fragments: self.last_injected_fragments.lock().ok().map(|g| g.clone()).unwrap_or_default(),
          now: chrono::Local::now(),
      }
  }
  ```

- [ ] **Step 3.2: Rewrite `effective_system_prompt`**

  Replace the existing body (lines 21-95) with:

  ```rust
  pub(super) fn effective_system_prompt(
      &self,
      effective_mode: &crate::safety::SafetyMode,
  ) -> String {
      // Side effect 1: stash fragments + record stats (was inline before).
      let inj_ctx = crate::agent::baseline_blocks::InjectionContext {
          is_first_act_turn: self.is_first_act_turn.load(std::sync::atomic::Ordering::Relaxed),
          last_error_kind: self.last_error_kind.lock().ok().and_then(|g| g.clone()),
          context_pressure_ratio: self.estimate_context_pressure_ratio(),
      };
      let query = crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]);
      let composed = self.context_manager_for_prompt_blocking(&query, &inj_ctx);
      if let Ok(mut slot) = self.last_injected_fragments.lock() {
          *slot = composed.injected_fragments.clone();
      }
      if let Some(collector) = &self.telemetry.compose_stats {
          collector.record(&self.conversation_id, composed.stats.clone());
      }

      // Side effect 2: first-act flag transition (one-way).
      self.is_first_act_turn.store(false, std::sync::atomic::Ordering::Relaxed);

      // Call the single seam. NOTE: build_prompt_context re-reads
      // is_first_act_turn AFTER the flip above — by design, the
      // injection context here sees the "after first read" state.
      let ctx = self.build_prompt_context(effective_mode.clone());
      let assembled = assemble_system_prompt(ctx);

      // Side effect 3: store new snapshot for next turn's diff.
      if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
          *slot = assembled.new_memory_context_snapshot;
      }

      assembled.system
  }
  ```

  **Subtle:** the order of operations matters here. Currently, `effective_system_prompt` reads `is_first_act_turn` INTO `inj_ctx` BEFORE flipping it. Then `compose_*` is called WITH the pre-flip value. The new code mirrors this by reading the `inj_ctx` value BEFORE the flip, then flipping, then building the SystemPromptContext (which re-reads the AFTER-flip value into its own `injection_context`). For golden snapshots to match, this must match exactly.

  The fix is to build the `SystemPromptContext` BEFORE the flip:

  ```rust
  // Build context BEFORE side effects to capture pre-flip first-act value.
  let ctx = self.build_prompt_context(effective_mode.clone());

  // Side effect 1: fragments + stats (uses inj_ctx from ctx).
  let composed = self.context_manager_for_prompt_blocking(
      &crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]),
      &ctx.injection_context,
  );
  if let Ok(mut slot) = self.last_injected_fragments.lock() {
      *slot = composed.injected_fragments.clone();
  }
  if let Some(collector) = &self.telemetry.compose_stats {
      collector.record(&self.conversation_id, composed.stats.clone());
  }
  // Side effect 2: first-act flag transition (AFTER ctx capture).
  self.is_first_act_turn.store(false, std::sync::atomic::Ordering::Relaxed);

  // Call the single seam.
  let assembled = assemble_system_prompt(ctx);

  // Side effect 3: store new snapshot.
  if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
      *slot = assembled.new_memory_context_snapshot;
  }

  assembled.system
  ```

- [ ] **Step 3.3: Rewrite `build_dynamic_context`**

  Replace the existing body (lines 157-312) with:

  ```rust
  pub(super) fn build_dynamic_context(&self) -> String {
      // The mode is unused by the dynamic half; pass any value.
      let ctx = self.build_prompt_context(crate::safety::SafetyMode::default());
      let assembled = assemble_system_prompt(ctx);
      // Note: snapshot side effect is NO-OP here because
      // effective_system_prompt ran first this turn and already stored
      // the snapshot from its assembled.new_memory_context_snapshot.
      assembled.dynamic_for_last_user
  }
  ```

  Caveat: this wrapper runs the WHOLE `assemble_system_prompt` (system + dynamic) and throws away the system half. That's wasted work. Task 4 fixes this by collapsing the 2 call sites into ONE in `turn_runner.rs::call_llm`. For Task 3 we accept the temporary inefficiency to keep changes localized to one file.

- [ ] **Step 3.4: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  Expected: 0 errors, warnings ≤49 (or back at 49 since the `dead_code` from Task 1 now resolves), dispatcher 50/0, agent:: 796/2.

  **If dispatcher tests fail**, the new wrappers don't match the old behavior exactly. Most likely culprits:
  - Side effect ordering: read `inj_ctx` BEFORE flipping `is_first_act_turn`.
  - The fragment stash uses `composed.injected_fragments` from `context_manager_for_prompt_blocking`, NOT `ctx.injected_fragments` (the latter is the prior turn's stash; the former is fresh from this turn's compose).
  - `SafetyMode::default()` may not exist as a constant — use `SafetyMode::Allow` or whatever the actual default is.

- [ ] **Step 3.5: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam add -A src-tauri/src/agent/dispatcher/content_assembler.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam commit -m "refactor(agent): effective_system_prompt + build_dynamic_context → thin wrappers around assemble_system_prompt (P3-6.3 of 阶段 3)"
  ```

Continue to Task 4.

---

## Task 4: Collapse `turn_runner.rs` to single-call (eliminate wasted work)

**Goal:** Replace the 2 separate call sites in `turn_runner.rs::call_llm` (line 473 `effective_system_prompt` + line 767 `build_dynamic_context`) with ONE call to a new ChatDelegate method that returns both halves.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/content_assembler.rs` (add a `pub(super) fn assemble_prompt(&self, mode) -> AssembledPrompt` method that exposes both halves)
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs` (refactor 2 calls into 1 — store the AssembledPrompt across the function body)

### Steps

- [ ] **Step 4.1: Add `assemble_prompt` method on ChatDelegate**

  In `content_assembler.rs::impl ChatDelegate`, add:

  ```rust
  /// Run the full single-seam assembly + propagate side effects.
  /// Returns BOTH halves so callers don't need to invoke twice.
  ///
  /// This is the canonical entry point post-P3-6. The legacy
  /// `effective_system_prompt` + `build_dynamic_context` methods remain
  /// as thin convenience accessors that each call this and pick a half.
  pub(super) fn assemble_prompt(
      &self,
      effective_mode: &crate::safety::SafetyMode,
  ) -> AssembledPrompt {
      // Build context BEFORE side effects (capture pre-flip first-act).
      let ctx = self.build_prompt_context(effective_mode.clone());

      // Side effect 1: fragments + stats (used by dynamic half).
      let composed = self.context_manager_for_prompt_blocking(
          &crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]),
          &ctx.injection_context,
      );
      if let Ok(mut slot) = self.last_injected_fragments.lock() {
          *slot = composed.injected_fragments.clone();
      }
      if let Some(collector) = &self.telemetry.compose_stats {
          collector.record(&self.conversation_id, composed.stats.clone());
      }
      // Side effect 2: first-act flag transition.
      self.is_first_act_turn.store(false, std::sync::atomic::Ordering::Relaxed);

      // Call the single seam.
      let assembled = assemble_system_prompt(ctx);

      // Side effect 3: store new snapshot.
      if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
          *slot = assembled.new_memory_context_snapshot.clone();
      }

      assembled
  }
  ```

  Refactor `effective_system_prompt` + `build_dynamic_context` to call this:

  ```rust
  pub(super) fn effective_system_prompt(&self, effective_mode: &crate::safety::SafetyMode) -> String {
      self.assemble_prompt(effective_mode).system
  }

  pub(super) fn build_dynamic_context(&self) -> String {
      // Build with default mode — the dynamic half doesn't read it.
      // Caveat: this still triggers the side effects (stash fragments,
      // store snapshot) a second time. In `turn_runner::call_llm`, callers
      // should use `assemble_prompt` directly to avoid the double-side-effect.
      self.assemble_prompt(&crate::safety::SafetyMode::default()).dynamic_for_last_user
  }
  ```

- [ ] **Step 4.2: Refactor `turn_runner.rs::call_llm`**

  Read lines around 473 + 767 of `turn_runner.rs`. The current shape:
  ```rust
  // Line 473:
  let effective_prompt = self.effective_system_prompt(&effective_mode);
  // ... ~250 lines later ...
  // Line 767:
  let dyn_ctx = self.build_dynamic_context();
  ```

  Replace with:
  ```rust
  // Line 473 (or wherever the first call was):
  let assembled = self.assemble_prompt(&effective_mode);
  let effective_prompt = assembled.system.clone();
  // ... later, instead of build_dynamic_context() ...
  let dyn_ctx = assembled.dynamic_for_last_user.clone();
  ```

  Verify by reading the surrounding context that `effective_prompt` and `dyn_ctx` are used as `&str` and that cloning is acceptable.

- [ ] **Step 4.3: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  Expected: 0 errors, warnings ≤49, dispatcher 50/0, agent:: 796/2.

- [ ] **Step 4.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam add -A src-tauri/src/agent/dispatcher/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam commit -m "refactor(agent): turn_runner.call_llm calls assemble_prompt once for both halves (P3-6.4 of 阶段 3)"
  ```

Continue to Task 5.

---

## Task 5: Add 5 golden snapshot tests

**Goal:** Lock the current prompt format with 5 representative snapshot tests. Each test constructs a known `SystemPromptContext`, calls `assemble_system_prompt`, and asserts on the resulting `AssembledPrompt.system` + `.dynamic_for_last_user` against literal expected strings.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/content_assembler.rs` (add a `#[cfg(test)] mod assemble_snapshot_tests` block at the bottom of the file)

### The 5 scenarios

1. **Vanilla restricted mode, no workspace, no memory, no skills** — baseline minimum.
2. **Restricted + workspace + first-act-turn=true** — covers `[WORKSPACE]` block + the A4 first-act gate.
3. **Allow mode + skills manifest + skill_search_used=false** — verifies manifest IS appended.
4. **Allow mode + skills manifest + skill_search_used=true** — verifies manifest is SUPPRESSED.
5. **Restricted + memory_context with prior snapshot (small drift)** — verifies the delta annotation block is rendered.

### Steps

- [ ] **Step 5.1: Add the test module skeleton**

  At the BOTTOM of `content_assembler.rs`, after the existing test modules, add:

  ```rust
  #[cfg(test)]
  mod assemble_snapshot_tests {
      use super::*;
      use chrono::TimeZone;

      /// Deterministic time pinned to 2026-05-29 14:30 Monday local
      /// for all snapshot tests. Matches the format produced by
      /// build_dynamic_block's `now.year/month/day/weekday/hour/minute`.
      fn fixed_now() -> chrono::DateTime<chrono::Local> {
          chrono::Local.with_ymd_and_hms(2026, 5, 29, 14, 30, 0).single().unwrap()
      }

      fn base_ctx() -> SystemPromptContext {
          SystemPromptContext {
              base_system_prompt: "You are uclaw.".to_string(),
              workspace_root: None,
              effective_mode: crate::safety::SafetyMode::Restricted,
              injection_context: crate::agent::baseline_blocks::InjectionContext {
                  is_first_act_turn: false,
                  last_error_kind: None,
                  context_pressure_ratio: 0.0,
              },
              persona_block: None,
              skills_manifest_block: String::new(),
              skills_manifest_suppress: false,
              memory_context: None,
              prior_memory_snapshot: None,
              learned_profile_block: String::new(),
              gbrain_knowledge_block: String::new(),
              injected_fragments: Vec::new(),
              now: fixed_now(),
          }
      }
      // ... 5 #[test] functions inserted below
  }
  ```

- [ ] **Step 5.2: Write the 5 tests**

  Each test takes the form:
  ```rust
  #[test]
  fn snapshot_NAME() {
      let ctx = SystemPromptContext {
          // override the fields that differ from base
          ..base_ctx()
      };
      let assembled = assemble_system_prompt(ctx);

      // System assertion — pin the exact full string:
      let expected_system = r#"<exact expected system prompt>"#;
      assert_eq!(assembled.system, expected_system, "system prompt drifted");

      // Dynamic assertion — pin the exact full string:
      let expected_dynamic = r#"<exact expected dynamic block>"#;
      assert_eq!(assembled.dynamic_for_last_user, expected_dynamic, "dynamic block drifted");
  }
  ```

  **Implementer strategy:** Don't hand-write the expected strings. For each scenario:
  1. Write the test with a placeholder `assert_eq!(actual, "<TODO>")`.
  2. Run the test (it fails); read the actual output from the failure message.
  3. Copy the actual output into the expected string.
  4. Re-run; test passes. This is the "snapshot first run" pattern — the first run captures the baseline, subsequent runs detect drift.

  Verify the captured output is REASONABLE before committing — if it contains stale paths, transient timestamps, or other non-deterministic content, fix the test setup before locking it in.

  The 5 test names:
  - `snapshot_vanilla_restricted_no_workspace`
  - `snapshot_restricted_workspace_first_act_turn`
  - `snapshot_allow_with_skills_manifest_not_suppressed`
  - `snapshot_allow_with_skills_manifest_suppressed`
  - `snapshot_restricted_memory_context_with_small_drift`

- [ ] **Step 5.3: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  Expected: 0 errors. dispatcher rises from 50 to 55 passed. agent:: from 796 to 801.

- [ ] **Step 5.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam add -A src-tauri/src/agent/dispatcher/content_assembler.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam commit -m "test(agent): 5 golden snapshot tests for assemble_system_prompt (P3-6.5 of 阶段 3)"
  ```

Continue to Task 6.

---

## Task 6: Final audit + cleanup

### Steps

- [ ] **Step 6.1: Verify final structure**

  ```bash
  grep -nE "^pub\(super\) (struct|fn) (SystemPromptContext|AssembledPrompt|assemble_system_prompt|assemble_prompt|build_prompt_context)|^fn build_dynamic_block" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri/src/agent/dispatcher/content_assembler.rs
  ```
  Expected: 6 lines (2 structs + 4 functions).

  ```bash
  grep -c "^    #\[test\]" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri/src/agent/dispatcher/content_assembler.rs
  ```
  Expected: 5 new + pre-existing count.

- [ ] **Step 6.2: Full test battery**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo test --lib 2>&1 | tail -5
  ```

  Required:
  - 0 errors.
  - Warnings ≤49.
  - dispatcher 55/0 (was 50, +5 new snapshot tests).
  - agent:: 801/2 (was 796, +5).
  - cargo test --lib total: 3055/7 (was 3050).

- [ ] **Step 6.3: Clean unused imports (if any)**

  After the refactor, some `use` statements at the top of `content_assembler.rs` may have become unused (e.g., direct `chrono::Datelike` imports if all chrono ops now go through `ctx.now`).

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam/src-tauri && cargo build 2>&1 | grep -E "^warning:" -A 3 | grep -B 1 "content_assembler" | head -20
  ```

- [ ] **Step 6.4: Commit cleanup (if any)**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam add -A src-tauri/src/agent/dispatcher/content_assembler.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam commit -m "refactor(agent): clean unused imports after P3-6 (P3-6.6)"
  ```

- [ ] **Step 6.5: Verify final chain**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam log --oneline main..HEAD
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p6-single-seam status -sb
  ```

  Expected: 5-6 commits ahead of main, clean tree.

---

## Self-Review

**1. Spec coverage:**
- ✅ Gap-audit §1.2 MAJOR "4 layers don't know about each other" — replaced by ONE `SystemPromptContext` + ONE `assemble_system_prompt` function + thin wrappers.
- ✅ Golden snapshot tests lock 5 representative inputs.
- ✅ Pure function isolated for testability (no `&self`, no I/O, no time reads).

**2. Placeholder scan:**
- Task 2's body sketch for `build_dynamic_block` is left as "adapt the current body" — implementer needs to do the line-by-line transcription. This is OK because the existing function is ~155 LoC and pasting it would bloat the plan; the adaptation rules in Step 2.2 are concrete.
- Task 5 explicitly uses the "snapshot first run" pattern — implementer captures the actual output the first time, then locks. Not a placeholder; an intentional methodology.

**3. Type consistency:**
- `SystemPromptContext` named consistently across all 6 tasks.
- `AssembledPrompt` named consistently.
- `assemble_system_prompt` (pure) vs `assemble_prompt` (method on ChatDelegate) — distinct names so callers know which is which.

**4. Bisectability:**
- Task 1: types only, dead code (1 commit)
- Task 2: pure function added, still dead code (1 commit)
- Task 3: wrappers swap, behavior preserved (1 commit)
- Task 4: turn_runner collapse, removes wasted double-compute (1 commit)
- Task 5: 5 snapshot tests added (1 commit)
- Task 6: optional cleanup (0-1 commits)

Each commit's `cargo test --lib agent::` must pass at the same baseline; the 5 new tests come in Task 5 only.

---

## Cumulative summary

- **Tasks:** 6 (5 mandatory + 1 optional cleanup).
- **Estimated time:** 1.5-2 person-days. Task 2 (porting `build_dynamic_context` body to read from ctx) is the most careful work; Task 5 (writing 5 snapshot tests) is mostly mechanical.
- **Risk:** Medium. Prompts are byte-sensitive — Anthropic prompt cache misses if anything reorders. The 50 existing dispatcher tests + the 5 new snapshots together verify the assembly is byte-stable.
- **Total commits:** 5-6.

**After P3-6 ships, 阶段 3 closes.** The agent framework is on the Pi-aligned single-handle + single-seam architecture. Next is **阶段 4** (openhuman-style bucket-seal memory tree to replace the 8 storage layers) or **阶段 5** (hermes-style coding edit reliability).
