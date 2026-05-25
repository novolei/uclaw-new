# Dirac-A4 — JIT Injection Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Steps use `- [ ]` syntax.

**Goal:** Extend the M2-A `BaselineBlock` trait with an `injection_policy()` method + new `InjectionPolicy` enum + `InjectionContext` struct + `BaselineBlockRegistry::render_with_context()` registry method. Architectural channel only — no new block content, no live wire-up to `compose_system_prompt`.

**Architecture:** Pure addition to `baseline_blocks.rs`. Default trait implementation (`InjectionPolicy::Always`) preserves byte-identical behavior for all 5 existing blocks.

**Tech Stack:** Rust only. No new crates. No DB. No frontend.

**Spec:** `docs/superpowers/specs/2026-05-25-dirac-a4-jit-injection-channel-design.md`

**PR tag:** `[C1-Dirac-A4]`

---

## File Structure

### Modified files

| Path | What changes |
|---|---|
| `src-tauri/src/agent/baseline_blocks.rs` | + `InjectionPolicy` enum, `InjectionContext` struct, trait default method, registry `render_with_context` + `render_all`. +120-160 lines. |
| `src-tauri/src/agent/baseline_blocks.rs::mod tests` | + 7 tests. ~150 lines. |
| `docs/superpowers/MILESTONE_STATUS.md` | One-line entry |

**No new files. No new modules. No callers changed (channel only).**

---

## Pre-flight

- [ ] **Step 0.1: Branch + baseline**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/dirac-a4-jit-injection-channel
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::baseline_blocks 2>&1 | tail -10
```

- [ ] **Step 0.2: Confirm M2-A pilot is the latest reference**

```bash
git log --oneline src-tauri/src/agent/baseline_blocks.rs | head -5
```

Expect: most recent commit is the M2-A pilot landing. If there's a
newer M2-A finalization commit, READ IT first — the trait signature
may have changed and this PR must adapt.

- [ ] **Step 0.3: Locate the registry type**

```bash
grep -n "BaselineBlockRegistry\|fn global\|fn register\|fn iter" src-tauri/src/agent/baseline_blocks.rs
```

Identify:
- Registry type name (likely `BaselineBlockRegistry` per spec, but
  could be `BlockRegistry` etc.)
- How blocks are enumerated (`iter()`, `blocks()`, etc.)
- How the registry is acquired (`global()` singleton, `new()`, etc.)

Use the discovered names throughout this plan.

---

## Task 1: Add `InjectionPolicy` enum + `InjectionContext` struct

**Files:**
- Modify: `src-tauri/src/agent/baseline_blocks.rs`

- [ ] **Step 1.1: Insert types above the trait**

After the file's existing module docstring + `OnceLock` import, but
before the `BaselineBlock` trait, add:

```rust
/// When a `BaselineBlock` should be included in the rendered system
/// prompt. Evaluated per render call against an `InjectionContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionPolicy {
    /// Default — block always appears. Matches pre-A4 behavior.
    Always,
    /// Appears only on the first user request after the task enters
    /// ACT (or AcceptEdits) mode.
    FirstActTurnOnly,
    /// Appears only when the previous turn ended with a tool execution
    /// error.
    OnErrorRecovery,
    /// Appears only when the token budget for this task is > 75% of
    /// the model context window.
    OnContextPressure,
}

impl InjectionPolicy {
    pub fn applies(self, ctx: &InjectionContext) -> bool {
        match self {
            Self::Always => true,
            Self::FirstActTurnOnly => ctx.is_first_act_turn,
            Self::OnErrorRecovery => ctx.last_error_kind.is_some(),
            Self::OnContextPressure => ctx.context_pressure_ratio > 0.75,
        }
    }
}

/// Per-render context that `BaselineBlockRegistry::render_with_context`
/// consults to decide which non-`Always` blocks to include.
#[derive(Debug, Clone, Default)]
pub struct InjectionContext {
    pub is_first_act_turn: bool,
    pub last_error_kind: Option<String>,
    pub context_pressure_ratio: f32,
}

impl InjectionContext {
    /// Equivalent to "Always blocks only." Useful for callers that
    /// don't yet populate the context, and for tests.
    pub fn baseline() -> Self {
        Self::default()
    }
}
```

- [ ] **Step 1.2: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(agent/baseline_blocks): add InjectionPolicy + InjectionContext types

InjectionPolicy {Always, FirstActTurnOnly, OnErrorRecovery, OnContextPressure}
is the per-block policy declaring when a block appears in the rendered
system prompt. InjectionContext is the per-render input the registry
consults.

A4 ships only the types — neither the trait nor the registry consume
them yet. Subsequent commits in this PR add the trait method and
registry render method.

Disjunctive (flat enum) per spec §8.1. Context-pressure threshold is
exclusive > 0.75 matching Dirac auto-condense threshold per spec §8.2.

Spec: docs/superpowers/specs/2026-05-25-dirac-a4-jit-injection-channel-design.md"
```

---

## Task 2: Add trait method + registry render method

- [ ] **Step 2.1: Add default trait method**

Edit the `BaselineBlock` trait, adding after `token_estimate`:

```rust
pub trait BaselineBlock: Send + Sync {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn topics(&self) -> &'static [&'static str];
    fn render(&self) -> String;
    fn token_estimate(&self) -> usize {
        self.render().chars().count() / 4
    }

    /// When this block should be included in the system prompt.
    /// Default: `InjectionPolicy::Always` (preserves pre-A4 behavior).
    /// Override on a block-by-block basis to declare conditional
    /// inclusion (`FirstActTurnOnly`, `OnErrorRecovery`,
    /// `OnContextPressure`). Evaluated by
    /// `BaselineBlockRegistry::render_with_context` per render.
    fn injection_policy(&self) -> InjectionPolicy {
        InjectionPolicy::Always
    }
}
```

The 5 existing impls don't need updating — they inherit the default
`Always` policy.

- [ ] **Step 2.2: Add registry methods**

Locate the `BaselineBlockRegistry` impl block (per Step 0.3 discovery).
Add:

```rust
impl BaselineBlockRegistry {
    /// Render only the blocks whose `injection_policy().applies(ctx)`
    /// returns true. Block order is registry insertion order. Sections
    /// are joined by `\n\n` (matches `baseline.md` blank-line separation).
    pub fn render_with_context(&self, ctx: &InjectionContext) -> String {
        self.iter()  // adapt to actual iteration method
            .filter(|block| block.injection_policy().applies(ctx))
            .map(|block| block.render())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Backward-compat helper: render all blocks unconditionally.
    /// Identical to `render_with_context(&InjectionContext::baseline())`.
    /// Useful for callers (and tests) that don't yet have a context to
    /// pass.
    pub fn render_all(&self) -> String {
        self.render_with_context(&InjectionContext::baseline())
    }
}
```

> **Adaptation note**: if the existing iteration method is different
> from `iter()` (e.g., `blocks() -> &[Arc<dyn BaselineBlock>]`), adjust
> accordingly. The intent is to enumerate registered blocks in
> insertion order.

- [ ] **Step 2.3: Verify no live caller is affected**

```bash
cd src-tauri && grep -rn "render_with_context\|InjectionContext\|InjectionPolicy" src/ --include="*.rs" | grep -v "baseline_blocks.rs" | grep -v "_test"
# Expect: zero hits
```

If any hits appear outside `baseline_blocks.rs`, an unintended caller
change has crept in — revert and reapply with discipline.

- [ ] **Step 2.4: Build + existing tests pass**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::baseline_blocks 2>&1 | tail -10
```

Expect: existing tests pass unchanged.

**Commit checkpoint:**
```
git add -A
git commit -m "feat(agent/baseline_blocks): trait method injection_policy() + registry render_with_context

Trait gains default method returning InjectionPolicy::Always — all 5
existing blocks (ThinkBeforeCoding, SimplicityFirst, SurgicalChanges,
GoalDrivenExecution, NeverFakeProgress) inherit Always and keep their
current rendering unchanged.

BaselineBlockRegistry gains render_with_context(ctx) which filters by
policy + render_all() for back-compat with callers that don't yet
have an InjectionContext.

compose_system_prompt is NOT migrated to render_with_context in this
PR — that's the M2-A finalization PR's job. A4 ships only the channel.

Inspired by Dirac EnvironmentManager.ts:143-146 (didSwitchToActMode
JIT injection pattern) per research doc §1.1 / §7.2 A4."
```

---

## Task 3: Seven tests

**Files:**
- Modify: `src-tauri/src/agent/baseline_blocks.rs` (test mod, likely at bottom)

- [ ] **Step 3.1: `test_injection_policy_always_applies_to_baseline_context`**

```rust
#[test]
fn test_injection_policy_always_applies_to_baseline_context() {
    let ctx = InjectionContext::baseline();
    assert!(InjectionPolicy::Always.applies(&ctx));
}
```

- [ ] **Step 3.2: `test_injection_policy_first_act_turn`**

```rust
#[test]
fn test_injection_policy_first_act_turn() {
    let mut ctx = InjectionContext::baseline();
    assert!(!InjectionPolicy::FirstActTurnOnly.applies(&ctx));
    ctx.is_first_act_turn = true;
    assert!(InjectionPolicy::FirstActTurnOnly.applies(&ctx));
}
```

- [ ] **Step 3.3: `test_injection_policy_on_error_recovery`**

```rust
#[test]
fn test_injection_policy_on_error_recovery() {
    let mut ctx = InjectionContext::baseline();
    assert!(!InjectionPolicy::OnErrorRecovery.applies(&ctx));
    ctx.last_error_kind = Some("anchor_not_found".into());
    assert!(InjectionPolicy::OnErrorRecovery.applies(&ctx));
}
```

- [ ] **Step 3.4: `test_injection_policy_on_context_pressure_threshold`**

```rust
#[test]
fn test_injection_policy_on_context_pressure_threshold() {
    let mut ctx = InjectionContext::baseline();
    ctx.context_pressure_ratio = 0.5;
    assert!(!InjectionPolicy::OnContextPressure.applies(&ctx));
    ctx.context_pressure_ratio = 0.75; // exclusive: 0.75 itself doesn't fire
    assert!(!InjectionPolicy::OnContextPressure.applies(&ctx));
    ctx.context_pressure_ratio = 0.76;
    assert!(InjectionPolicy::OnContextPressure.applies(&ctx));
    ctx.context_pressure_ratio = 0.99;
    assert!(InjectionPolicy::OnContextPressure.applies(&ctx));
}
```

- [ ] **Step 3.5: `test_baseline_block_default_policy_is_always`**

```rust
struct ProbeBlock;
impl BaselineBlock for ProbeBlock {
    fn id(&self) -> &'static str { "test.probe" }
    fn title(&self) -> &'static str { "Probe" }
    fn topics(&self) -> &'static [&'static str] { &["test"] }
    fn render(&self) -> String { "probe".into() }
    // injection_policy intentionally NOT overridden
}

#[test]
fn test_baseline_block_default_policy_is_always() {
    let probe = ProbeBlock;
    assert_eq!(probe.injection_policy(), InjectionPolicy::Always);
}
```

- [ ] **Step 3.6: `test_render_with_context_filters_policies`**

```rust
struct AlwaysBlock;
impl BaselineBlock for AlwaysBlock {
    fn id(&self) -> &'static str { "test.always" }
    fn title(&self) -> &'static str { "Always" }
    fn topics(&self) -> &'static [&'static str] { &["test"] }
    fn render(&self) -> String { "ALWAYS".into() }
    fn injection_policy(&self) -> InjectionPolicy { InjectionPolicy::Always }
}

struct FirstActBlock;
impl BaselineBlock for FirstActBlock {
    fn id(&self) -> &'static str { "test.first_act" }
    fn title(&self) -> &'static str { "First Act" }
    fn topics(&self) -> &'static [&'static str] { &["test"] }
    fn render(&self) -> String { "FIRST_ACT".into() }
    fn injection_policy(&self) -> InjectionPolicy { InjectionPolicy::FirstActTurnOnly }
}

struct OnErrorBlock;
impl BaselineBlock for OnErrorBlock {
    fn id(&self) -> &'static str { "test.on_error" }
    fn title(&self) -> &'static str { "On Error" }
    fn topics(&self) -> &'static [&'static str] { &["test"] }
    fn render(&self) -> String { "ON_ERROR".into() }
    fn injection_policy(&self) -> InjectionPolicy { InjectionPolicy::OnErrorRecovery }
}

#[test]
fn test_render_with_context_filters_policies() {
    let registry = BaselineBlockRegistry::new_for_test(vec![
        Box::new(AlwaysBlock),
        Box::new(FirstActBlock),
        Box::new(OnErrorBlock),
    ]);

    // Baseline context: only Always fires
    let baseline = InjectionContext::baseline();
    let out = registry.render_with_context(&baseline);
    assert_eq!(out, "ALWAYS");

    // First-act + no error: Always + FirstAct fire
    let mut ctx = InjectionContext::baseline();
    ctx.is_first_act_turn = true;
    let out = registry.render_with_context(&ctx);
    assert_eq!(out, "ALWAYS\n\nFIRST_ACT");

    // First-act + error: all three fire
    ctx.last_error_kind = Some("any".into());
    let out = registry.render_with_context(&ctx);
    assert_eq!(out, "ALWAYS\n\nFIRST_ACT\n\nON_ERROR");
}
```

> If `BaselineBlockRegistry::new_for_test` doesn't exist, add a
> `#[cfg(test)] pub fn new_for_test(blocks: Vec<Box<dyn BaselineBlock>>) -> Self`
> constructor. The global singleton shouldn't be touched in tests.

- [ ] **Step 3.7: `test_render_with_context_baseline_matches_pre_a4`**

```rust
#[test]
fn test_render_with_context_baseline_matches_pre_a4() {
    let registry = BaselineBlockRegistry::global(); // or however it's acquired
    let with_ctx = registry.render_with_context(&InjectionContext::baseline());
    let all = registry.render_all();
    assert_eq!(with_ctx, all,
        "render_with_context(baseline) must equal render_all() — all current blocks are Always policy");
}
```

This is the **regression test** that pinpoints any accidental policy
drift on the 5 production blocks. If it fails, one of the existing
blocks accidentally gained a non-Always policy and the system prompt
is now contextually filtered — bug.

- [ ] **Step 3.8: Run all tests**

```bash
cd src-tauri && cargo test --lib agent::baseline_blocks 2>&1 | tail -15
# Expect: existing + 7 new tests passing
```

**Commit checkpoint:**
```
git add -A
git commit -m "test(agent/baseline_blocks): 7 tests for injection policy channel

Covers:
- Always.applies(baseline) == true
- FirstActTurnOnly toggles on is_first_act_turn
- OnErrorRecovery toggles on last_error_kind.is_some()
- OnContextPressure threshold > 0.75 (exclusive: 0.75 itself doesn't fire)
- Default trait impl returns Always (proves backward compat)
- Registry render_with_context filters by policy (3-block fixture)
- Regression: render_with_context(baseline) byte-matches render_all()
  for the production 5-block set (no accidental policy drift)"
```

---

## Task 4: SSoT + PR

- [ ] **Step 4.1: Update MILESTONE_STATUS**

```
| C1-Dirac-A4 | BaselineBlock injection_policy channel | #<PR-number> |
```

Under §M2 detailed status, in the M2-A row or adjacent.

Also: nothing to add about live wire-up — A4 is channel only.

- [ ] **Step 4.2: Drift check + push + PR**

```bash
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -5
git push -u origin claude/dirac-a4-jit-injection-channel

gh pr create \
  --title "[C1-Dirac-A4] feat(agent/baseline_blocks): injection_policy channel for JIT block rendering" \
  --body "..."
```

PR description includes:
- Summary (one paragraph): channel only, no live wire-up, no block
  content changes
- Why (link research doc §7.2 A4 + EnvironmentManager.ts:143-146)
- Commits (bisectable) — 4 commits
- Verification (cargo test output + grep confirmation of no live callers)
- Spec link
- Closes (C1-Dirac-A4)
- **Explicit note**: "M2-A finalization PR will migrate
  `compose_system_prompt` to use `render_with_context`; this PR ships
  only the channel."

- [ ] **Step 4.3: Self-merge gate**

- [ ] CI green
- [ ] PR tag `[C1-Dirac-A4]`
- [ ] Regression test (#7) passes — production block set unaffected
- [ ] Grep confirms zero live callers in `src/` outside `baseline_blocks.rs`

---

## Rollback procedure

```bash
git revert <merge-commit-sha>
git push
```

The enum, struct, trait default method, and registry methods all
disappear. All 5 production blocks unaffected (they never adopted
non-`Always` policy in A4). No data corruption. No content drift.

---

## Closes / unblocks

- C1-Dirac-A4 ✓
- Drives M2 progress ~+2% (small but unblocks future block-policy
  declarations)
- Unblocks M2-A finalization PR (cleaner migration path)
- Long-tail: Phase B/C ship Dirac-style verbose tool spec blocks as
  `FirstActTurnOnly` payload. A4 is the rails those payloads ride on.
- Pairs with A3 — once both land, a future `OnFirstActTurn` block
  titled "File Read-Cache Protocol" can teach the LLM `assume_hash`
  usage on turn 1 only, and disappear from prompt on turn 2+

---

## Task A (autonomous mode only) — Self-verify + adversarial review + auto-merge

> Run only when invoked by the autonomous orchestrator (see
> [`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)).

- [ ] **Step A.1: Stage 2 self-verify (per protocol §3.2) — STRICT SCOPE**

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib agent::baseline_blocks 2>&1 | tail -10
cargo clippy --lib -- -D warnings 2>&1 | tail -5

# STRICT scope: ONLY these files. Any other Rust file = SCOPE CREEP = ESCALATE
git diff --name-only main..HEAD | grep -E "\.rs$" | sort
# Expected ONLY: src-tauri/src/agent/baseline_blocks.rs

git diff --name-only main..HEAD | sort
# Expected total: src-tauri/src/agent/baseline_blocks.rs + docs/superpowers/MILESTONE_STATUS.md

# Zero-live-caller grep — must return EMPTY (no non-test caller exists yet)
cd src-tauri && grep -rn "render_with_context\|InjectionContext\|InjectionPolicy" src/ --include="*.rs" | grep -v "baseline_blocks.rs" | grep -v "_test"
# Expected: empty output

# Production-block-set regression check (most important test)
cargo test --lib agent::baseline_blocks::tests::test_render_with_context_baseline_matches_pre_a4 2>&1 | tail -3
# Must show: test result: ok. 1 passed
```

- [ ] **Step A.2: Spawn adversarial reviewer (protocol §3.3)**

A4-specific HIGH-SCRUTINY focus from spec §12:
- Any edit to `mode_prompts.rs` / `KARPATHY_BASELINE` / `compose_system_prompt`
  → reviewer must REQUEST_CHANGES (high) citing spec §2.2 + §8.5
- Production block-set regression test present + green
- Zero non-test consumers of new types (channel only)
- `InjectionContext` derives `Clone + Debug + Default` (B2 dependency)

- [ ] **Step A.3: Reconcile per protocol §3.4** (any scope creep → ESCALATE, do not auto-fix)

- [ ] **Step A.4: PR open + CI + auto-merge (protocol §3.5)**

```bash
git push -u origin claude/dirac-a4-jit-injection-channel
PR=$(gh pr create --title "[C1-Dirac-A4] feat(agent/baseline_blocks): injection_policy channel for JIT block rendering" --body-file ./pr-body.md --json number -q .number)
gh pr checks $PR --watch --interval 30 --required
gh pr merge $PR --merge --delete-branch
git checkout main && git pull
```

- [ ] **Step A.5: Log + return outcome (protocol §7)**

- [ ] **Step A.6: After A4 merge — write C1 closeout report**

Spec §11 of A4 doesn't explicitly require this, but the orchestrator
DOES require it (per the protocol's sequence-level gate at C1→C2
boundary). After A4 merges, the orchestrator triggers Phase 5 of
its own loop:

1. Pull main
2. Run drift check
3. Generate `docs/superpowers/specs/2026-05-25-phase-a-closeout.md`
   per the prompt in `docs/research/2026-05-25-dirac-phase-a-prompts.md` §"After all four merged — closeout"
4. Open + auto-merge a `[C1-Closeout]` PR with that file
5. Update MILESTONE_STATUS.md to mark C1 closed
6. ONLY THEN proceed to B1

The C1 closeout PR follows the same 5-stage protocol; the reviewer
subagent checks the report is concrete (cites PRs/numbers, not hand-
waving).
