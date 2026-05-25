# Dirac-B2 — ContextManager Wire-Up + ContextToolSet Registration (C2)

> **Context**: Phase B item #2 from
> [`docs/research/2026-05-25-dirac-reverse-engineering.md`](../../research/2026-05-25-dirac-reverse-engineering.md) §7.2.
> Companion plan: [`plans/2026-05-25-dirac-b2-context-manager-wireup.md`](../plans/2026-05-25-dirac-b2-context-manager-wireup.md).
>
> **C2 SLOT — DO NOT START UNTIL C1 CLOSES**. Per
> [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](2026-05-22-pr-integration-strategy.md) §7.
>
> **Parallelism with B1**: This PR and B1 touch disjoint files
> (`dispatcher.rs` + `context_manager/manager.rs` + `runtime/context_tools.rs`
> vs B1's `anchor_state.rs` + `tools/builtin/{file,edit}.rs`). They
> CAN ship in parallel during C2. Recommended order: B1 first (its
> token-saving impact is larger and more self-contained); B2 second
> (depends on A4's `InjectionContext` channel being merged).

## 1. Background

### 1.1 What's built but not wired

uClaw has two M2 pilots that are "built but not wired" per
MILESTONE_STATUS.md (status as of 2026-05-22):

- **M2-B `ContextManager`** (`src-tauri/src/agent/context_manager/manager.rs`):
  Full `for_prompt(query) → ComposedContext` API exists (lines 75-189).
  Has `ComposeQuery`, `ComposeStats`, fragment scoring, token-budget
  capping, sequential fetch. **Nothing calls `for_prompt`** —
  `dispatcher.rs::ChatDelegate::effective_system_prompt` (line 640)
  builds the prompt directly from `mode_prompts::compose_system_prompt`
  + `skills_manifest_block`.

- **M2-F `ContextToolSet`** (`src-tauri/src/runtime/context_tools.rs`):
  7-tool API surface (search, read, fold, cite, compare, pin,
  release). `search` + `read` implemented; the other 5 are
  `Err(unimplemented)` stubs. **Not registered into
  ToolRegistry** — neither `dispatcher.rs` nor `main.rs` lists them.

### 1.2 What the North Star asks for

ADR `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` §2:

> "Context as a tool: context is retrieved on demand instead of
>  preloaded until it explodes."

The two pilots together implement this — `ContextManager` selects
fragments per turn under budget; `ContextToolSet` lets the agent
explicitly query/pin/release fragments mid-turn. Without wire-up,
the agent has neither capability.

### 1.3 Why B2 is small (vs B1's 3-day scope)

The hard work — the `ContextManager` algorithm, the fragment trait,
the scoring — is done. B2 is integration plumbing:

1. Construct a `ContextManager` per-session in `ChatDelegate`
2. Route `effective_system_prompt` through `ContextManager::for_prompt`
3. Adapt to the A4 `InjectionContext` (now that A4 has shipped, the
   baseline render call needs to pass an `InjectionContext`)
4. Register `ContextToolSet::search` + `read` into the tool registry
5. Define a Tauri command to expose `ComposeStats` to the frontend
   (M2-J already has the snapshot shape; just add fragment fields)

## 2. Scope

Single PR. Three files modified, one Tauri command added.

### 2.1 In scope

1. **Hold a `ContextManager` per `ChatDelegate`**:
   - Add `context_manager: Arc<RwLock<ContextManager>>` field to
     `ChatDelegate` (`dispatcher.rs:~92`)
   - Construct in `ChatDelegate::new` (or wherever delegate is built)
   - Populate fragments at session start — for now, just register the
     "available" fragments from any pre-existing setup. Fragment
     lifecycle (when fragments enter/leave the set) stays a M2-D
     concern.

2. **Route `effective_system_prompt` through `ContextManager`**:
   - Replace the direct `mode_prompts::compose_system_prompt` call
     with a path that:
     a. Builds an `InjectionContext` from current task state
        (`is_first_act_turn`, `last_error_kind`, `context_pressure_ratio`)
     b. Calls `context_manager.for_prompt(&compose_query)` →
        `ComposedContext`
     c. Renders the system prompt = `composed.system_prompt` (uses
        A4's `render_with_context` internally)
   - **Preserve byte-stable system prompt across turns** — the existing
     cache_control: ephemeral discipline (dispatcher.rs:641-644
     comment) must not regress. Fragment injection happens via
     `build_dynamic_context` (line 675+), NOT via system prompt.

3. **Inject `ContextManager`-selected fragments into the dynamic context block**:
   - `build_dynamic_context` (line 675+) already injects per-turn
     content (time, workspace, memory). Extend it to also inject
     `composed.injected_fragments`.
   - Fragment rendering: each `ContextArtifact` becomes a
     `<context_fragment id="...">...</context_fragment>` block.

4. **Register `ContextToolSet::search` + `read` into the tool registry**:
   - Wrap each `ContextToolSet` operation as a `Tool` impl
   - Register in the same code path that registers `EditTool` /
     `ReadFileTool` etc.
   - Tool names: `context.search`, `context.read` (dotted namespace
     mirrors the M2-F module layout). Schema includes `topics: string[]`
     for search, `ref: string` for read.

5. **Expose `ComposeStats` via Tauri command**:
   - Add `get_compose_stats` Tauri command that reads the most recent
     `ComposeStats` from the current session's `ChatDelegate`
   - Wire into M2-J's `TokenBudgetSnapshot` if that already exists;
     otherwise emit as standalone for now
   - Frontend M2-J PR will consume

6. **~6 new tests** + 1 integration test (50-turn fixture compares
   token cost before/after wire-up).

### 2.2 Out of scope

- **Implementing the 5 unimplemented `ContextToolSet` stubs** (fold,
  cite, compare, pin, release) — those are individual M2-G/D PRs.
  B2 wires the working 2 (search, read).
- **Fragment lifecycle management** (auto-add/remove based on
  workspace activity) — M2-D follow-up.
- **Frontend UI for `ComposeStats`** — M2-J's job. B2 ships the
  backend command only.
- **Persisting `ContextManager` state to disk** — fragment set is
  session-local. Persistence is a future M4 World Projection +
  checkpoint concern.
- **Removing the deprecated `skills_manifest_block` suppression**
  (dispatcher.rs:657+) — that's a follow-up rework once fragments
  subsume the manifest's job. B2 keeps it; it's orthogonal.

## 3. Design

### 3.1 `ChatDelegate` gains a `ContextManager`

```rust
// dispatcher.rs

use crate::agent::context_manager::{ComposeQuery, ContextManager, ComposeStats};
use crate::agent::baseline_blocks::InjectionContext;

pub struct ChatDelegate {
    // ... existing fields ...
    /// Per-session context orchestrator. Constructed at session start
    /// with the available fragment set. `effective_system_prompt`
    /// calls `for_prompt` on every turn.
    context_manager: Arc<RwLock<ContextManager>>,
    /// Most recent ComposeStats — read by get_compose_stats Tauri
    /// command for the M2-J frontend.
    last_compose_stats: Arc<Mutex<ComposeStats>>,
    /// Track whether this is the first ACT-mode turn (per A4
    /// InjectionContext semantics).
    is_first_act_turn: AtomicBool,
    /// Last tool error kind, if any — used by InjectionContext.last_error_kind.
    last_error_kind: Mutex<Option<String>>,
}
```

### 3.2 `effective_system_prompt` rewrite

```rust
fn effective_system_prompt(&self, effective_mode: &SafetyMode) -> String {
    // Build InjectionContext from current task state
    let context_pressure = self.estimate_context_pressure_ratio();
    let last_error = self.last_error_kind.lock().unwrap().clone();
    let inj_ctx = InjectionContext {
        is_first_act_turn: self.is_first_act_turn.load(Ordering::Relaxed),
        last_error_kind: last_error,
        context_pressure_ratio: context_pressure,
    };

    // Compose query — topics come from current task hints (kept
    // empty for now; M2-D wires real topic extraction)
    let query = ComposeQuery::defaults_with_topics(vec![]);

    // Synchronous wrapper around async for_prompt (effective_system_prompt
    // is called from the LLM hot path which is sync). Use tokio::block_on
    // ONLY if no runtime is active; otherwise spawn and await. In dispatcher
    // context we're inside a tokio runtime, so use a oneshot + spawn pattern.
    let composed = self.context_manager_for_prompt_blocking(&query, &inj_ctx);

    // Store stats for the Tauri snapshot command
    {
        let mut stats = self.last_compose_stats.lock().unwrap();
        *stats = composed.stats.clone();
    }

    // System prompt is byte-stable per session (preserves cache_control hits).
    // The InjectionContext-conditional content (A4 FirstActTurnOnly blocks)
    // IS part of the system prompt for now — accepted trade-off: turn 1 has
    // a different prompt than turns 2+, but turns 2+ have identical prompts.
    let mode_addition = crate::agent::mode_prompts::mode_addition(effective_mode);
    let suppress_manifest = self.skill_search_used.load(Ordering::Relaxed);

    if self.skills_manifest_block.is_empty() || suppress_manifest {
        format!("{}\n{}", composed.system_prompt, mode_addition)
    } else {
        format!("{}\n{}{}", composed.system_prompt, mode_addition, self.skills_manifest_block)
    }
}
```

> **Cache discipline note**: `composed.system_prompt` content depends
> on `InjectionContext.is_first_act_turn`. Turn 1 has the FirstActTurnOnly
> block; turn 2+ does not. This is **intentional** per A4 spec — accept
> a one-time cache miss between turn 1 and turn 2 in exchange for
> dropping ~3-6 KB from every turn-2+ prompt. Turns 2+ are byte-stable
> against each other → cache breakpoint still works.

### 3.3 Sync wrapper for async `for_prompt`

`ContextManager::for_prompt` is `async`. `effective_system_prompt`
is sync. Two options:

- **Option A**: make `effective_system_prompt` async. Cascades through
  the LLM hot path — invasive.
- **Option B** (recommended): use `tokio::runtime::Handle::current().block_on`
  with a fallback to spawning a oneshot task. Simpler.

```rust
impl ChatDelegate {
    fn context_manager_for_prompt_blocking(
        &self,
        query: &ComposeQuery,
        inj_ctx: &InjectionContext,
    ) -> ComposedContext {
        let cm = self.context_manager.clone();
        let q = query.clone();
        let ic = inj_ctx.clone();

        // Inside a tokio runtime — use the runtime's block_in_place
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let cm_read = cm.read().await;
                cm_read.for_prompt(&q).await
                // Note: ComposeQuery doesn't currently take an InjectionContext.
                // ContextManager.for_prompt internally calls baseline_system_prompt
                // → baseline_blocks::render_all. We need to extend ContextManager
                // to accept InjectionContext and call render_with_context.
                // (Spec §3.4 below.)
            })
        })
    }
}
```

> **Limitation**: `block_in_place` only works on multi-thread runtimes.
> If uClaw uses a single-thread runtime in any context, this needs
> refinement. Verify during pre-flight Step 0.4.

### 3.4 Small `ContextManager` API extension

`ContextManager::for_prompt(query)` currently calls
`self.baseline_system_prompt()` which calls `baseline_blocks::render_all`.
After A4, we want to call `render_with_context(&inj_ctx)`. So:

```rust
// context_manager/manager.rs

pub async fn for_prompt_with_injection(
    &self,
    query: &ComposeQuery,
    injection_ctx: &InjectionContext,
) -> ComposedContext {
    // Same body as existing for_prompt, but replace the
    // baseline_system_prompt() call with:
    let system_prompt = baseline_blocks::render_with_context(injection_ctx);
    // ... rest unchanged ...
}

/// Backward-compat shim — calls for_prompt_with_injection with
/// InjectionContext::baseline() (no FirstActTurnOnly blocks, etc.)
pub async fn for_prompt(&self, query: &ComposeQuery) -> ComposedContext {
    self.for_prompt_with_injection(query, &InjectionContext::baseline()).await
}
```

Tests against existing `for_prompt` keep working; new caller in
`dispatcher.rs` uses `for_prompt_with_injection`.

### 3.5 Fragment injection into dynamic context

```rust
fn build_dynamic_context(&self, messages: &[ChatMessage]) -> String {
    // ... existing time + workspace + memory blocks ...

    // B2: inject ContextManager fragments
    let stats_snapshot = self.last_compose_stats.lock().unwrap().clone();
    if stats_snapshot.fragments_selected > 0 {
        // The composed.injected_fragments isn't stored on self today.
        // Either: (a) store it alongside last_compose_stats, or (b)
        // re-run for_prompt_with_injection here. (a) is cheaper.
        let fragments = self.last_injected_fragments.lock().unwrap().clone();
        for art in &fragments {
            block.push_str(&format!(
                "\n<context_fragment id=\"{}\">\n{}\n</context_fragment>",
                art.ref_id, art.content
            ));
        }
    }

    // ... rest of existing build_dynamic_context ...
}
```

This requires `ChatDelegate` to also store the `injected_fragments`
from the last `for_prompt_with_injection` call. Add a field:

```rust
last_injected_fragments: Arc<Mutex<Vec<ContextArtifact>>>,
```

Populated in `effective_system_prompt` after `for_prompt_with_injection`
returns.

### 3.6 ContextToolSet wrapping as Tools

```rust
// new file: src-tauri/src/agent/tools/builtin/context_search.rs
pub struct ContextSearchTool {
    toolset: Arc<RwLock<ContextToolSet>>,
}

#[async_trait]
impl Tool for ContextSearchTool {
    fn name(&self) -> &str { "context.search" }
    fn description(&self) -> &str {
        "Search the available context fragments for ones matching one or more topics. Returns matching ContextRef identifiers. Use context.read to fetch a specific ref's content."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "topics": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Topic tags to search (e.g., 'rust', 'auth', 'database'). Case-insensitive substring match."
                }
            },
            "required": ["topics"]
        })
    }
    fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let topics: Vec<String> = serde_json::from_value(params["topics"].clone())
            .map_err(|e| ToolError::InvalidParams(format!("topics: {e}")))?;
        let toolset = self.toolset.read().await;
        let refs = toolset.search(&topics);
        let out = serde_json::to_string_pretty(&refs).unwrap();
        Ok(ToolOutput::success(&out, 0))
    }
}

// Similar shape for ContextReadTool ("context.read")
```

Register both in the same code path that registers other builtin
tools (likely `main.rs` or `dispatcher.rs::register_builtin_tools`).

### 3.7 `get_compose_stats` Tauri command

```rust
// tauri_commands.rs

#[tauri::command]
pub fn get_compose_stats(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ComposeStats, String> {
    let session_mgr = state.session_manager.read().unwrap();
    let session = session_mgr.get(&session_id)
        .ok_or_else(|| format!("session {session_id} not found"))?;
    let delegate = session.chat_delegate.read().unwrap();
    Ok(delegate.last_compose_stats.lock().unwrap().clone())
}
```

Register in `main.rs::invoke_handler!`.

## 4. Interfaces

### 4.1 ChatDelegate field additions

```rust
context_manager: Arc<RwLock<ContextManager>>,
last_compose_stats: Arc<Mutex<ComposeStats>>,
last_injected_fragments: Arc<Mutex<Vec<ContextArtifact>>>,
is_first_act_turn: AtomicBool,
last_error_kind: Mutex<Option<String>>,
```

### 4.2 ContextManager API additions

```rust
pub async fn for_prompt_with_injection(
    &self,
    query: &ComposeQuery,
    injection_ctx: &InjectionContext,
) -> ComposedContext;

// Backward compat shim:
pub async fn for_prompt(&self, query: &ComposeQuery) -> ComposedContext;
```

### 4.3 New builtin tools

```
context.search { topics: string[] } -> ContextRef[]
context.read { ref: string } -> ContextArtifact
```

### 4.4 New Tauri command

```
get_compose_stats(session_id: string) -> ComposeStats
```

## 5. Tests

| # | Test | Scenario |
|---|---|---|
| 1 | `chat_delegate_builds_with_empty_context_manager` | New ChatDelegate has an empty ContextManager; `effective_system_prompt` returns baseline render |
| 2 | `effective_system_prompt_uses_injection_context_first_turn` | `is_first_act_turn=true` → FirstActTurnOnly blocks (added via test fixture) appear in prompt |
| 3 | `effective_system_prompt_excludes_first_turn_blocks_after_first_turn` | After 1st turn, set flag false → re-render → FirstActTurnOnly block GONE |
| 4 | `compose_stats_populated_after_effective_system_prompt` | After 1 call, `last_compose_stats` reflects fragments_available count |
| 5 | `context_search_tool_returns_matching_refs` | Register 2 fragments with topic "rust" → call context.search{topics:["rust"]} → returns 2 refs |
| 6 | `context_read_tool_returns_fragment_artifact` | Register 1 fragment → context.read{ref: "..."} → returns artifact content |
| 7 | `get_compose_stats_command_round_trip` | Tauri command returns the stats struct populated by step 4 |

Plus **1 integration test** (`tests/context_wireup_bench.rs`):
- Fixture session with 5 fragments, 20 turns
- Assert: `compose_stats.fragments_selected` shows actual selection
  over the run (not all-zeros) → proves wire-up active
- Assert: total tokens / turn ≤ pre-B2 baseline (don't regress)

## 6. Verification

### 6.1 Local

```bash
cd src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::context_manager 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::context 2>&1 | tail -10
cd src-tauri && cargo test --test context_wireup_bench 2>&1 | tail -10
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo clippy --lib -- -D warnings | tail -5
```

### 6.2 Cache-discipline check

The cache_control:ephemeral hit rate on system prompt is sacred (see
dispatcher.rs:641-644 comment). Verify post-B2:

```bash
# Run a 5-turn session against a real Anthropic endpoint.
# Inspect rollout JSONL or provider response cache_read_input_tokens.
# Expected: turn 1 cache_read = 0 (initial); turns 2-5 cache_read > 0
#           AND byte-stable from turn 2 onward.
```

If turn 2-5 system prompt bytes vary (other than the InjectionContext
trickle from A4), the rebuild is leaking dynamic content somewhere
that shouldn't be in system prompt. Investigate.

### 6.3 50-turn bench (recommended)

```bash
cargo run --bin bench-50turn -- --fixture refactor-3-file --compare-baseline pre-B2.json
```

Expected outcome: tokens / task drop 10-20% from baseline. The drop
isn't dramatic (most of B-phase wins come from B1's anchor stability).
B2's win is positioning for M2-D fragment lifecycle work — without
this wire-up, future fragment work has nowhere to land.

## 7. Migration / rollback

- **DB migration**: none.
- **Backward compat**:
  - `ContextManager::for_prompt(query)` still works — calls into
    `for_prompt_with_injection` with `InjectionContext::baseline()`.
  - Existing `mode_prompts::compose_system_prompt` still exists (B2
    uses `BaselineBlockRegistry::render_with_context` internally, but
    `mode_addition` still comes from `mode_prompts`).
  - Tool registry additions — `context.search` / `context.read` are
    new tools; existing tools unchanged.
- **Rollback**: revert PR. ChatDelegate fields disappear,
  effective_system_prompt returns to direct mode_prompts path. Tool
  registry loses context.* tools (no consumers of them yet outside
  this PR). No data corruption.
- **Feature flag**: optional — could gate the
  `for_prompt_with_injection` route behind a config flag. **Not
  recommended for B2** — the route IS the feature. If something
  breaks, revert.

## 8. Decisions (locked 2026-05-25)

### 8.1 `for_prompt_with_injection` as new method, not breaking change

- **Why**: extending the existing `for_prompt` signature breaks every
  existing caller (tests, future callers). New method + shim is
  cleaner. Cost: one extra public method on ContextManager.

### 8.2 Sync wrapper via `block_in_place`, not async-cascade

- **Why**: cascading async through `effective_system_prompt` →
  `LLM hot path` is invasive (changes ~6 call sites + their callers).
  `block_in_place` is one localized hack. If we hit single-thread
  runtime issues, revisit — but uClaw uses `#[tokio::main]` with
  default multi-thread runtime, so this works.

### 8.3 Fragments injected via `<context_fragment>` XML in dynamic context

- **Why**: keeps system prompt byte-stable. Fragments are per-turn
  content → belong in the per-turn dynamic context block. XML tags
  give the LLM a clear "this is grounded context" signal.
- **Alternative considered**: tool-results-style injection. Rejected
  — fragments aren't tool results, they're system-supplied context.
  Mismatch in role would confuse the LLM.

### 8.4 No InjectionContext field in ComposeQuery

- **Why**: ComposeQuery is about *what* fragments to pick. InjectionContext
  is about *which baseline blocks* to render. Different concerns;
  keep them separate. The caller (dispatcher) passes both, ContextManager
  uses them for their respective jobs.

### 8.5 `context.search` + `read` only — defer the other 5

- **Why**: those 5 are `Err(unimplemented)` stubs in M2-F today.
  Registering stubs as tools confuses the LLM (it'll call them and
  get errors). Wire up only what works; ship the rest in dedicated
  PRs as they get implemented.

### 8.6 Acceptable cache miss on turn 1 → turn 2 boundary

- **Why**: A4's FirstActTurnOnly blocks deliberately make turn 1
  different from turn 2+. The one-time cache miss costs ~one
  prompt-cache-write. Turns 2-N are byte-stable against each other,
  so cache hits resume immediately. Net win when long tasks have
  20+ turns reading from the byte-stable turn-2 baseline.

## 9. Concrete commit plan

```
Commit 1: feat(context_manager): add for_prompt_with_injection accepting InjectionContext
          + back-compat shim for_prompt
Commit 2: feat(dispatcher): hold ContextManager + InjectionContext state on ChatDelegate;
          route effective_system_prompt through for_prompt_with_injection
Commit 3: feat(dispatcher): inject ContextManager fragments into build_dynamic_context
Commit 4: feat(tools/builtin/context): ContextSearchTool + ContextReadTool wrapping ContextToolSet
          + register in builtin tool list
Commit 5: feat(tauri_commands): get_compose_stats command + invoke_handler register
Commit 6: test(context_manager + dispatcher + context tools + tauri): 7 unit + 1 integration test
Commit 7: docs(MILESTONE_STATUS): record C2-Dirac-B2 completion
```

Seven commits, ~500-700 lines of diff. Bisectable.

## 10. Estimated effort

- ContextManager API extension: 0.25 day
- ChatDelegate fields + effective_system_prompt rewrite: 0.5 day
- Fragment injection in dynamic context: 0.25 day
- ContextToolSet → Tool wrapping + registration: 0.25 day
- get_compose_stats command: 0.25 day
- Tests + integration bench: 0.5 day
- **Total: 2 days** (matches research doc estimate)

## 11. Closes / unblocks

- C2-Dirac-B2 ✓
- Closes M2-B (ContextManager wire-up — was "pilot only, wire-up missing")
- Closes M2-F partial (context tools 2-of-7 wired; remaining 5 are
  stubs for future PRs)
- Drives M2 progress to ~75% (from ~58%) — major closeout milestone
- Unblocks M2-D (fragment lifecycle) — the wire-up is the prerequisite
  for any meaningful "add fragment when X happens" logic
- Unblocks M2-J full UI — `ComposeStats` now exposed via Tauri command
- Pairs with B1 — together, B1+B2 close out M2 and prepare M3
  Capability Mesh

## 12. Autonomous execution mode

When this PR is executed via the autonomous orchestrator (see
[`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)):

- **C1→C2 boundary check + A4 dependency** (Stage 1 pre-flight):
  orchestrator MUST verify (a) C1 closed in MILESTONE_STATUS.md,
  (b) C1-Dirac-A4 merged (B2 uses `InjectionContext`). If either
  unmet → escalate.
- **Cache discipline is sacred** (Stage 3 critical focus): spec §3.2
  promises turns 2-N have byte-stable system prompt. The reviewer
  must trace `effective_system_prompt`'s output and confirm:
  - On turn 2+ with `is_first_act_turn=false`, the rendered system
    prompt has no per-turn-varying content (no timestamps, no memory
    blobs, no fragment content)
  - Fragments inject via `build_dynamic_context` (per-turn block),
    NOT system_prompt
- **Sync→async bridge** (Stage 2 + Stage 3): plan Step 0.3 verifies
  multi-thread tokio runtime. Reviewer confirms `block_in_place`
  pattern OR channel-fallback documented in commit message.
- **No stub-tools registered** (Stage 3): spec §8.5 — only
  `context.search` + `context.read` registered. The other 5
  unimplemented ops MUST NOT appear in ToolRegistry. Reviewer greps
  for `context.fold|context.cite|context.compare|context.pin|context.release`
  outside test code and confirms zero hits.
- **Integration bench is the wire-up proof** (Stage 2 #10 + Stage 3):
  the 20-turn fixture test (plan Step 7.8) must assert
  `fragments_selected > 0` on at least half the turns. If the
  integration test passes but `fragments_selected` is always 0, the
  wire-up isn't actually live — reviewer flags this specifically.
- **MILESTONE_STATUS update** (Stage 2 #8): B2 closes M2-B ("pilot,
  wire-up missing" → "wired"). The MS edit must reflect this status
  change, not just add a row.
- **Risk class**: MEDIUM-HIGH — touches the LLM hot path
  (`effective_system_prompt`); cache discipline regression would be
  invisible until token-cost reports surface days later.
