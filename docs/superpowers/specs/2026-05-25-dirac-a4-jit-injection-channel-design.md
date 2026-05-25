# Dirac-A4 — JIT Injection Channel for BaselineBlock (C1)

> **Context**: Phase A item #4 from
> [`docs/research/2026-05-25-dirac-reverse-engineering.md`](../../research/2026-05-25-dirac-reverse-engineering.md) §7.2.
> v1.1-corrected: this is *not* about extracting BEHAVIOR.md (that file
> is dev-time only). It is about extending the M2-A `BaselineBlock`
> registry to support per-block injection policies — preparing the
> channel for future Dirac-style verbose tool specs to land without
> polluting long-task prompts.
> Companion plan: [`plans/2026-05-25-dirac-a4-jit-injection-channel.md`](../plans/2026-05-25-dirac-a4-jit-injection-channel.md).
> **C1 slot**: M2 closeout. Smallest of the 4 Phase A items. Independent
> of A1/A2/A3.

## 1. Background

### 1.1 uClaw's actual system-prompt pipeline (vs. my v1.0 misreading)

uClaw composes the system prompt in two layers:

1. **Baseline** — `src-tauri/src/agent/mode_prompts.rs::compose_system_prompt`
   currently `include_str!`'s `baseline.md` (66 lines, ~800 tokens
   per `baseline_blocks.rs` header comment lines 5-7).
2. **Mode addition** — appends a mode-specific `.md` for Ask /
   AcceptEdits / Plan / Supervised / Yolo.

`BEHAVIOR.md` and `CLAUDE.md` at the repo root are **Claude Code
developer documentation** for working on uClaw — they never enter the
uClaw app's system prompt at runtime.

### 1.2 M2-A pilot already in flight

`src-tauri/src/agent/baseline_blocks.rs` ships the `BaselineBlock`
trait (lines 35-64) with 3 example blocks (`ThinkBeforeCoding`,
`SimplicityFirst`, `SurgicalChanges`) + 2 more in lines 118+
(`GoalDrivenExecution`, `NeverFakeProgress`). Per the header comment
(lines 18-26):

> This pilot lands the **trait + 3 example blocks** (mirroring the
> first three guardrails) so the architecture is in place. **No callers
> are changed**; `compose_system_prompt` still uses `KARPATHY_BASELINE`
> verbatim. M2-A follow-up PRs will:
>
> 1. Author the remaining 9 blocks (guardrails 4-7 + 3 helper sections)
> 2. Cut `baseline.md` over to the registry
> 3. Wire `compose_system_prompt` to render the active set instead of
>    `KARPATHY_BASELINE`

A4 **adds one piece of architecture** to that trait — `injection_policy`
— so when M2-A finishes wiring the registry to `compose_system_prompt`,
blocks can self-declare which turns they should appear on.

### 1.3 Why now (vs. waiting for M2-A finalization)

Two reasons:

1. **Adding the trait method now is a no-op for current callers**. The
   default `injection_policy() -> InjectionPolicy::Always` produces
   identical output to today's "static include." The PR is small,
   bisectable, and doesn't touch `compose_system_prompt`.
2. **It locks the architecture before the registry wire-up.** If M2-A
   finalizes without injection_policy, every block's API contract gets
   re-touched when we add it later. Adding the trait method *now*
   keeps blast radius small.

### 1.4 What this enables (deferred to Phase B/C, not this PR)

- Phase B / C may want to ship Dirac-style verbose tool specs (hash-
  anchor edit protocol, multi-file batching protocol — ~3-6 KB each).
  Those land as new `BaselineBlock`s tagged `FirstActTurnOnly` or
  `OnErrorRecovery`.
- Result: the verbose spec appears in turn 1 of an ACT-mode task,
  conditions cache hits on long-prefix turns 2+, and disappears from
  prompt for long tasks. **Matches Dirac
  EnvironmentManager.ts:143-146 JIT pattern**.

**A4 itself ships only the trait extension + registry-render integration
+ tests. No new block content, no live wire-up.** This is critical for
scope control — A4 is a *channel*, not a *payload*.

## 2. Scope

Single PR. Two files modified, no new modules.

### 2.1 In scope

1. Add `InjectionPolicy` enum to `baseline_blocks.rs`:
   ```rust
   pub enum InjectionPolicy {
       Always,                    // every system-prompt build
       FirstActTurnOnly,          // only on first turn after entering ACT mode
       OnErrorRecovery,           // only after a tool-execution error
       OnContextPressure,         // only when token budget > 75%
   }
   ```
2. Add `fn injection_policy(&self) -> InjectionPolicy { InjectionPolicy::Always }`
   default method on `BaselineBlock` trait.
3. Add `InjectionContext` struct passed to the registry render function:
   ```rust
   pub struct InjectionContext {
       pub is_first_act_turn: bool,
       pub last_error_kind: Option<String>,
       pub context_pressure_ratio: f32,  // 0.0..1.0
   }
   ```
4. Add `BaselineBlockRegistry::render_with_context(ctx: &InjectionContext) -> String`
   that consults each block's policy.
5. Add `BaselineBlock::token_estimate()` still defaults same as today.
6. ~6 new unit tests covering each policy variant.

### 2.2 Out of scope

- **Wiring `compose_system_prompt` to use `render_with_context`** — that's
  part of the M2-A finalization PR, not A4. A4 ships the channel; the
  finalization PR plugs it in.
- **Maintaining `is_first_act_turn` flag on TaskState** — that's
  `agentic_loop.rs` work. A4 declares the *interface* (the field in
  `InjectionContext`); the M2-A finalization PR populates it.
- **Adding new blocks with non-`Always` policies** — Phase B/C.
- **Removing/migrating `baseline.md` `include_str!`** — that's the
  M2-A finalization PR.

## 3. Design

### 3.1 Trait extension

In `baseline_blocks.rs`, extend the trait:

```rust
pub trait BaselineBlock: Send + Sync {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn topics(&self) -> &'static [&'static str];
    fn render(&self) -> String;
    fn token_estimate(&self) -> usize {
        self.render().chars().count() / 4
    }

    /// NEW: when this block should be included in the system prompt.
    /// Default is Always — matches pre-A4 behavior. Blocks declaring
    /// non-Always policies will be conditionally rendered by
    /// `BaselineBlockRegistry::render_with_context`.
    fn injection_policy(&self) -> InjectionPolicy {
        InjectionPolicy::Always
    }
}
```

### 3.2 InjectionPolicy enum

```rust
/// When a `BaselineBlock` should be included in the rendered system
/// prompt. Evaluated per render call against an `InjectionContext`.
///
/// Policy hierarchy is **disjunctive** — a block declaring
/// `FirstActTurnOnly` is included iff the context says first ACT turn,
/// regardless of error/pressure state. There is currently no
/// composite/AND policy; add later if a real use case demands it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionPolicy {
    /// Default — block always appears. Matches pre-A4 behavior.
    Always,
    /// Appears only on the first user request after the task enters
    /// ACT mode. Used for verbose tool/protocol specs that the LLM
    /// learns from once and recalls for subsequent turns via prompt
    /// cache (or tool error feedback).
    FirstActTurnOnly,
    /// Appears only when the previous turn ended with a tool execution
    /// error. Used for recovery hints / structured retry guidance.
    OnErrorRecovery,
    /// Appears only when the token budget for this task is > 75% of
    /// the model context window. Used for context-pressure-aware
    /// guidance (e.g., "prefer surgical edits", "batch tool calls").
    OnContextPressure,
}

impl InjectionPolicy {
    /// Returns true if a block with this policy should be included
    /// in a render under the given context.
    pub fn applies(self, ctx: &InjectionContext) -> bool {
        match self {
            Self::Always => true,
            Self::FirstActTurnOnly => ctx.is_first_act_turn,
            Self::OnErrorRecovery => ctx.last_error_kind.is_some(),
            Self::OnContextPressure => ctx.context_pressure_ratio > 0.75,
        }
    }
}
```

### 3.3 InjectionContext

```rust
/// Per-render context that `BaselineBlockRegistry::render_with_context`
/// consults to decide which non-`Always` blocks to include.
///
/// Populated by `compose_system_prompt`'s caller (likely
/// `dispatcher::effective_system_prompt` or `agentic_loop`).
/// During A4's PR, callers don't populate this — it stays as the
/// channel interface for the M2-A finalization PR to plug into.
#[derive(Debug, Clone, Default)]
pub struct InjectionContext {
    /// True iff this is the first user request after the task entered
    /// ACT (or AcceptEdits) mode. Reset to false on subsequent turns
    /// within the same mode. Reset to true if user toggles back to
    /// Plan and re-enters ACT.
    pub is_first_act_turn: bool,
    /// Some(kind) iff the last tool execution returned a structured
    /// error. None on success / first turn / non-tool turns. Used by
    /// `OnErrorRecovery` blocks to surface recovery guidance.
    pub last_error_kind: Option<String>,
    /// Ratio of estimated tokens used / model context window. 0.0 ..= 1.0.
    /// Used by `OnContextPressure` blocks to gate inclusion.
    pub context_pressure_ratio: f32,
}

impl InjectionContext {
    /// Default — equivalent to "Always blocks only." Useful for
    /// callers that don't yet populate the context (current
    /// compose_system_prompt) and for testing.
    pub fn baseline() -> Self {
        Self::default()
    }
}
```

### 3.4 Registry

`baseline_blocks.rs` has a `BaselineBlockRegistry` (per header comment;
verify the actual name during impl). Add:

```rust
impl BaselineBlockRegistry {
    /// Render only the blocks whose `injection_policy().applies(ctx)`
    /// returns true, joined by `\n\n` (matches the blank-line separation
    /// of baseline.md). Block order is registry insertion order.
    pub fn render_with_context(&self, ctx: &InjectionContext) -> String {
        self.iter()
            .filter(|block| block.injection_policy().applies(ctx))
            .map(|block| block.render())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Backward-compat: render all blocks unconditionally. Identical
    /// to render_with_context(&InjectionContext::baseline()) — kept
    /// for any caller that doesn't yet have a context to pass.
    pub fn render_all(&self) -> String {
        self.render_with_context(&InjectionContext::baseline())
    }
}
```

> **If the existing registry shape differs from `iter() -> Iterator<&dyn BaselineBlock>`,
> adapt the method to match. The intent is "iterate registered blocks in
> insertion order and render those whose policy applies."**

### 3.5 What `compose_system_prompt` does today vs. tomorrow

**Today** (`mode_prompts.rs`):
```rust
pub fn compose_system_prompt(/* ... */) -> String {
    let mut out = String::new();
    out.push_str(KARPATHY_BASELINE);  // include_str!("prompts/baseline.md")
    out.push_str(mode_addition(mode));
    // ...
    out
}
```

**Tomorrow** (M2-A finalization PR, NOT this A4 PR):
```rust
pub fn compose_system_prompt(/* ... */, ctx: &InjectionContext) -> String {
    let mut out = String::new();
    out.push_str(&BaselineBlockRegistry::global().render_with_context(ctx));
    out.push_str(mode_addition(mode));
    // ...
    out
}
```

A4 lands `render_with_context`. M2-A finalization rewires `compose_system_prompt`
to use it.

## 4. Interfaces

### 4.1 Public additions to `agent::baseline_blocks`

```rust
pub enum InjectionPolicy { Always, FirstActTurnOnly, OnErrorRecovery, OnContextPressure }
impl InjectionPolicy { pub fn applies(self, ctx: &InjectionContext) -> bool; }

pub struct InjectionContext {
    pub is_first_act_turn: bool,
    pub last_error_kind: Option<String>,
    pub context_pressure_ratio: f32,
}
impl InjectionContext { pub fn baseline() -> Self; }

pub trait BaselineBlock {
    // ... existing methods unchanged ...
    fn injection_policy(&self) -> InjectionPolicy { InjectionPolicy::Always }
}

impl BaselineBlockRegistry {
    pub fn render_with_context(&self, ctx: &InjectionContext) -> String;
    pub fn render_all(&self) -> String;
}
```

### 4.2 Existing 5 blocks override `injection_policy`?

**No.** All 5 existing blocks keep the default `Always` policy.
A4 does NOT change the rendered content of any current block.

### 4.3 Backward compatibility

`render_all()` is provided so callers that don't yet have an
`InjectionContext` don't break. M2-A finalization PR will migrate the
sole caller (`compose_system_prompt`) to `render_with_context`. After
that, `render_all` may be deprecated — out of scope for A4.

## 5. Tests

Inline in `baseline_blocks.rs` test module. Six tests:

| # | Test | Scenario |
|---|---|---|
| 1 | `test_injection_policy_always_applies_to_baseline_context` | `Always.applies(&InjectionContext::baseline())` == `true` |
| 2 | `test_injection_policy_first_act_turn` | `FirstActTurnOnly.applies(ctx)` true iff `ctx.is_first_act_turn` |
| 3 | `test_injection_policy_on_error_recovery` | `OnErrorRecovery.applies(ctx)` true iff `ctx.last_error_kind.is_some()` |
| 4 | `test_injection_policy_on_context_pressure_threshold` | Threshold at 0.75 exclusive: 0.74 → false, 0.76 → true, 0.75 → false |
| 5 | `test_baseline_block_default_policy_is_always` | A test block without `injection_policy` override → `Always` |
| 6 | `test_render_with_context_filters_policies` | Registry with 3 blocks (Always, FirstActTurnOnly, OnErrorRecovery) under `is_first_act_turn=true, error=None` → renders block 1 + block 2 only |

## 6. Verification

### 6.1 Local

```bash
cd src-tauri && cargo test --lib agent::baseline_blocks 2>&1 | tail -10
# Expect: existing + 6 new tests passing

cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo clippy --lib -- -D warnings | tail -5
```

### 6.2 No system-prompt content drift

Critical: rendering all current blocks with `InjectionContext::baseline()`
must produce **byte-identical output** to today's static include for
the registered blocks.

Add a regression test:

```rust
#[test]
fn test_render_with_context_baseline_matches_pre_a4() {
    let registry = BaselineBlockRegistry::global(); // or however it's constructed
    let rendered = registry.render_with_context(&InjectionContext::baseline());
    // Expected: same content as render_all() — since all existing
    // blocks are Always policy
    assert_eq!(rendered, registry.render_all());
}
```

This is also test #7. (Spec says "6" but #7 is critical so we ship it.)

### 6.3 No live caller affected

```bash
cd src-tauri && grep -rn "render_with_context\|InjectionContext\|InjectionPolicy" src/ --include="*.rs" | grep -v "baseline_blocks.rs"
# Expect: zero hits. A4 introduces the API but no production caller uses it yet.
```

If any hits appear, an unintended caller change has crept in.

## 7. Migration / rollback

- **DB migration**: none.
- **Backward compat**: 100%. Default policy `Always` keeps every
  existing block in output. `render_all()` preserves the pre-A4
  rendering shape.
- **Rollback**: revert PR. The trait method, enum, struct, and registry
  methods disappear. No data corruption.
- **Feature flag**: not needed. Strict architectural addition.

## 8. Decisions (locked 2026-05-25)

### 8.1 Disjunctive policies, no AND-composite

- **Decision**: `InjectionPolicy` is a flat enum, not `BitFlags` or
  `Vec<Policy>`. A block has exactly one policy.
- **Why**: simpler API, easier to reason about, matches the use cases
  on the table. Adding composite later is straightforward — wrap the
  enum in `Vec<InjectionPolicy>` or add an `All(Vec<_>)` variant.
- **Trade-off accepted**: a block that wants "FirstActTurnOnly OR
  OnErrorRecovery" needs to be registered twice (once per policy) or
  pick the more conservative one.

### 8.2 Context pressure threshold = 0.75 (exclusive)

- **Decision**: `OnContextPressure.applies` returns true iff
  `ctx.context_pressure_ratio > 0.75`.
- **Why**: matches Dirac's `ApiConversationManager.determineContextCompaction`
  threshold (research doc §2.1 — Dirac triggers auto-condense at 0.75).
  Inclusive vs exclusive: chose `>` (exclusive) so a value of exactly
  0.75 doesn't fire — feels like noise at the boundary; bumps to
  0.76+ are more decisive.
- **Tunability**: NOT a runtime config in A4. If usage shows 0.75 is
  wrong, change the constant and re-test. A runtime knob can ship
  later if patterns emerge.

### 8.3 `last_error_kind: Option<String>` rather than typed enum

- **Decision**: stringly-typed for now.
- **Why**: error kinds vary by tool and aren't yet centralized in
  uClaw. Locking a typed enum prematurely risks frequent refactors.
  When `OnErrorRecovery` blocks ship in Phase B/C, the actual usage
  will inform whether typing is worthwhile.

### 8.4 `is_first_act_turn` field owned by caller, not block

- **Decision**: the agentic_loop / dispatcher owns the `is_first_act_turn`
  flag. Blocks consume it via `InjectionContext`.
- **Why**: blocks are stateless — they shouldn't track turn state.
  Centralizing turn-state in the loop (where `taskState.didSwitchToActMode`
  lives in Dirac per `EnvironmentManager.ts:143`) is cleaner.
- **Implication for M2-A finalization PR**: must add
  `is_first_act_turn` field to `TaskState` or equivalent, and toggle
  it per turn. Documented as upstream dep.

### 8.5 No block content changes in A4

- **Decision**: A4 ships ONLY the policy channel. The 5 existing
  blocks (`ThinkBeforeCoding`, `SimplicityFirst`, `SurgicalChanges`,
  `GoalDrivenExecution`, `NeverFakeProgress`) keep their content
  and `Always` policy unchanged.
- **Why**: scope discipline. Mixing channel + payload makes A4 harder
  to bisect and review. Future PRs ship new blocks (or override
  existing blocks' policies) with explicit motivation.

## 9. Concrete commit plan

```
Commit 1: feat(agent/baseline_blocks): add InjectionPolicy enum + InjectionContext struct
Commit 2: feat(agent/baseline_blocks): add injection_policy() trait method (default Always)
          + render_with_context registry method + render_all backward-compat
Commit 3: test(agent/baseline_blocks): 7 tests covering all policy variants + baseline equivalence
Commit 4: docs(MILESTONE_STATUS): record C1-Dirac-A4 completion
```

Four commits, ~150-200 lines of diff. Bisectable.

## 10. Estimated effort

- Enum + struct + trait method: 1 hour
- Registry method: 0.5 hour
- Tests: 1-1.5 hours
- **Total: 0.5 day** (matches research doc estimate)

## 11. Closes / unblocks

- C1-Dirac-A4 ✓
- Drives M2 progress ~+2% (small but architectural — preps the
  channel for Phase B/C verbose-spec blocks)
- Unblocks M2-A finalization PR — that PR can immediately use
  `render_with_context` when migrating `compose_system_prompt`
- Long-tail: Phase B/C ship hash-anchor protocol spec, multi-file
  batch protocol spec as `FirstActTurnOnly` blocks via this channel
- Pairs with A3 conceptually — A3's `assume_hash` short-circuit
  benefits most when the system prompt teaches the LLM about it; a
  future `OnFirstActTurn` block titled "Read-Cache Protocol" can be
  the payload that ships via this channel

## 12. Autonomous execution mode

When this PR is executed via the autonomous orchestrator (see
[`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)):

- **Hardest scope discipline** (Stage 1 + Stage 3 critical focus):
  A4 is CHANNEL ONLY. If `git diff --stat main..HEAD` shows ANY edit
  to `src-tauri/src/agent/mode_prompts.rs`, `KARPATHY_BASELINE`,
  or `compose_system_prompt`, that's scope creep — reviewer must
  REJECT with `REQUEST_CHANGES (high)`. Spec §2.2 + §8.5 are
  explicit; reviewer cites them.
- **Regression test (#7)** is the most important test (Stage 3): it
  proves the 5 production blocks remain unaffected. Reviewer reads
  this test specifically and confirms it operates on the production
  `BaselineBlockRegistry::global()` (or equivalent), not a test
  fixture.
- **Zero-live-caller grep** (Stage 2 #5 + Stage 3): plan Step 2.3
  greps for `render_with_context|InjectionContext|InjectionPolicy`
  outside `baseline_blocks.rs`. Should be zero non-test hits.
  Reviewer verifies the orchestrator's grep result.
- **A4 unlocks B2**: this PR's `InjectionContext` is consumed by B2.
  The reviewer also checks `InjectionContext` struct is `Clone +
  Debug + Default` so B2 can store and pass it.
- **Risk class**: LOW — architectural addition with strict scope. The
  smallest PR in Phase A.
