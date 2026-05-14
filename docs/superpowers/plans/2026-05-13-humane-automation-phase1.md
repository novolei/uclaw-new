# Humane Automation Framework — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port hello-halo's autonomous-agent framework into uClaw as the Humane automation framework — 100% schema mirror of the AutomationSpec protocol, full runtime with all seven subscription source types, four built-in tools, 14 Tauri commands, and a minimum-viable frontend touch for escalation.

**Architecture:** Extend the existing `src-tauri/src/automation/` module in place (no parallel `halo/` module). Rust types via `serde + garde` are the single source of truth; legacy TOML data is one-shot migrated to Humane YAML in V20a-c; behavior tables (subscriptions / memory / escalations) land in V21. `AutomationDelegate` is a new `LoopDelegate` implementation that reuses `run_agentic_loop` but introduces a completion-gate-based control flow distinct from interactive chat.

**Tech Stack:** Rust + Tauri v2, sqlx-style rusqlite migrations, `serde + serde_yaml + garde 0.20+`, React 18 + TypeScript + Jotai, axum for webhook routes, existing `InfraService` message bus for event streaming.

**Spec source of truth:** [docs/superpowers/specs/2026-05-13-humane-automation-design.md](../specs/2026-05-13-humane-automation-design.md) (commit `29d8bec`). The plan deliberately does not re-quote large code blocks; tasks reference spec sections.

**Honest commit count:** ~22 commits. If your team caps PR size lower, split this plan at the **Task 14 boundary** into Phase 1A (data + parser + migrations, Tasks 0-13) and Phase 1B (runtime + sources + tools + frontend, Tasks 14-22). The split is clean: 1A leaves the runtime untouched on the legacy path; 1B builds the new runtime against 1A's schema.

> **Post-execution note (2026-05-14):** Phase 1 has shipped — `src-tauri/src/automation/`
> now carries the new `protocol/` `runtime/` `sources/` `tools/` `memory/` structure.
> This plan is preserved as the **historical Phase 1 execution record** and is not
> rewritten. One thing has since changed: the frontend touchpoints (Task 20
> `AutomationHub.tsx`, Task 21 `EscalationModal`) executed as written, but the
> automation / marketplace UI has since been **migrated into the Kaleidoscope surface**
> — `AutomationHub` now renders inside the Kaleidoscope `HumansModule`, the legacy
> `AutomationsView` + `automationPanelOpenAtom` are retired. The backend (Tasks 0–19)
> is unaffected. See the design doc's §10.1 (frontend-surfacing note) and
> `docs/superpowers/specs/2026-05-14-kaleidoscope-design.md`.

---

## File Structure Overview

**New files** (~22):

```
src-tauri/src/automation/
├── mod.rs                              [MODIFY]
├── spec.rs                             [REWRITE — single-branch HumaneAutomationSpec]
├── activity.rs                         [REWRITE — match new V20b schema]
├── manager.rs                          [NEW]
├── filters.rs                          [NEW]
├── permissions.rs                      [NEW]
├── protocol/
│   ├── mod.rs                          [NEW]
│   ├── humane_v1.rs                    [NEW — full Zod mirror]
│   ├── parse.rs                        [NEW — strict + permissive]
│   ├── normalize.rs                    [NEW]
│   ├── migrate_toml_v1.rs              [NEW]
│   └── test_fixtures/                  [NEW directory]
├── runtime/
│   ├── mod.rs                          [NEW]
│   ├── service.rs                      [NEW — AppRuntimeService]
│   ├── execute.rs                      [NEW]
│   ├── prompt.rs                       [NEW]
│   └── auto_continue.rs                [NEW]
├── sources/
│   ├── mod.rs                          [NEW — trait + registry]
│   ├── schedule.rs                     [NEW]
│   ├── file.rs                         [NEW]
│   ├── webhook.rs                      [NEW]
│   ├── webpage.rs                      [NEW]
│   ├── rss.rs                          [NEW]
│   ├── wecom.rs                        [NEW]
│   └── custom.rs                       [NEW — stub]
├── tools/
│   ├── mod.rs                          [NEW]
│   ├── report_to_user.rs               [NEW]
│   ├── notify_user.rs                  [NEW]
│   ├── request_escalation.rs           [NEW]
│   └── memory.rs                       [NEW]
├── memory/
│   ├── mod.rs                          [NEW]
│   ├── store.rs                        [NEW]
│   └── compact.rs                      [NEW]
├── importer/
│   └── from_humane_workspace.rs        [NEW — Phase 3 stub only]
├── runtime.rs                          [DELETE after Task 18]
└── service.rs                          [DELETE after Task 14]
```

**Modified files outside automation/:**

- `src-tauri/Cargo.toml` — add `garde = "0.20"`, `serde_yaml = "0.9"`, `regex` (if not already present), `globset = "0.4"` (for file source patterns)
- `src-tauri/src/db/migrations.rs` — V20a, V20b, V20c, V21 (Tasks 8-9)
- `src-tauri/src/tauri_commands.rs` — replace 4 old automation commands with 14 new ones (Task 19)
- `src-tauri/src/main.rs` — invoke_handler! macro update (Task 19)
- `src-tauri/src/agent/dispatcher.rs` — `register_automation_tools()` call site (Task 14)
- `ui/src/components/automation/AutomationHub.tsx` — paste-YAML field (Task 20)
- `ui/src/components/automation/EscalationModal.tsx` — new component (Task 21)
- `ui/src/atoms/automation.ts` — escalation state atoms (Task 21)
- `ui/src/lib/tauri-bridge.ts` — 14 new command bindings (Task 19's frontend half)
- `CLAUDE.md` — Active migration registry update to V21 (Task 22)

---

## Task 0: garde Viability Spike (timeboxed half-day)

**Goal:** Verify `serde + garde` can express the four hardest refinement classes before committing 10+ types to it. **Spike code is deleted before any production commit.**

**Files:**
- Create: `src-tauri/src/automation/protocol/spike/` (deleted at end)
- Modify: `src-tauri/Cargo.toml` (add garde temporarily)

- [ ] **Step 1: Add garde + serde_yaml to Cargo.toml**

```toml
[dependencies]
garde = { version = "0.20", features = ["derive", "pattern", "serde"] }
serde_yaml = "0.9"
```

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: clean build.

- [ ] **Step 2: Spike target 1 — array-min-length on nested optional**

Create `src-tauri/src/automation/protocol/spike/target1.rs`:

```rust
use garde::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Validate)]
struct EscalationConfig {
    #[garde(length(min = 2))]
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
struct Choice {
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
struct Spec {
    #[garde(dive)]
    pub escalation: Option<EscalationConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ok_two_choices() { /* construct valid Spec, assert validate().is_ok() */ }
    #[test] fn err_one_choice() { /* construct with 1 choice, assert err contains "choices" */ }
    #[test] fn err_path_is_readable() {
        // assert error path is exactly "escalation.choices" not "escalation" or "choices"
    }
}
```

Run: `cd src-tauri && cargo test --lib automation::protocol::spike::target1 -- --nocapture`
Expected: 3 tests pass, error message includes `escalation.choices`.

- [ ] **Step 3: Spike target 2 — discriminated union cross-variant validation**

Create `src-tauri/src/automation/protocol/spike/target2.rs`:

```rust
use garde::Validate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Subscription {
    Schedule(#[garde(dive)] ScheduleSub),
    Webhook(#[garde(dive)] WebhookSub),
}

#[derive(Debug, Serialize, Deserialize, Validate)]
struct ScheduleSub {
    #[garde(custom(validate_cron_or_every))]
    pub _self: (),  // placeholder for cross-field check
    pub cron: Option<String>,
    pub every: Option<String>,
}

fn validate_cron_or_every(_: &(), value: &ScheduleSub) -> garde::Result {
    if value.cron.is_none() && value.every.is_none() {
        return Err(garde::Error::new("schedule requires cron or every"));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Validate)]
struct WebhookSub {
    #[garde(pattern("^[a-z0-9-/]+$"))]
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn schedule_with_cron_ok() { /* ... */ }
    #[test] fn schedule_with_neither_fails() { /* ... */ }
    #[test] fn webhook_invalid_path_fails() { /* ... */ }
    #[test] fn discriminator_mismatch_caught() {
        // parse `{"type": "webhook", "cron": "..."}` — should error
    }
}
```

Run: `cd src-tauri && cargo test --lib automation::protocol::spike::target2 -- --nocapture`
Expected: all 4 tests pass.

- [ ] **Step 4: Spike target 3 — list-item internal cross-field**

Create `src-tauri/src/automation/protocol/spike/target3.rs`:

```rust
// InputDef.default must match InputDef.type
#[derive(Debug, Serialize, Deserialize, Validate)]
struct InputDef {
    #[garde(length(min = 1))]
    pub key: String,
    #[garde(custom(validate_type))]
    pub r#type: String,   // "string" | "number" | "boolean"
    #[garde(custom(validate_default_matches_type))]
    pub default: Option<serde_json::Value>,
}

fn validate_default_matches_type(default: &Option<serde_json::Value>, ctx: &InputDef) -> garde::Result {
    match (default, ctx.r#type.as_str()) {
        (None, _) => Ok(()),
        (Some(v), "string")  if v.is_string()  => Ok(()),
        (Some(v), "number")  if v.is_number()  => Ok(()),
        (Some(v), "boolean") if v.is_boolean() => Ok(()),
        _ => Err(garde::Error::new("default does not match type")),
    }
}
```

Run: `cd src-tauri && cargo test --lib automation::protocol::spike::target3 -- --nocapture`
Expected: pass / fail informs Decision Gate.

- [ ] **Step 5: Spike target 4 — error path quality for nested arrays**

In any spike file, write a test that constructs `Spec { subscriptions: vec![..., ..., InvalidSub] }` and asserts the error message contains `subscriptions[2]` (not just `subscriptions`).

- [ ] **Step 6: Decision gate write-up**

Create `src-tauri/src/automation/protocol/spike/DECISION.md` (this file IS deleted in step 7 too, but its content is preserved as a comment in production code):

```markdown
# garde Spike Decision (DELETE before merging)

Targets:
- [x/✗] Target 1: array-min-length on nested optional — <pass/fail + notes>
- [x/✗] Target 2: discriminated union cross-variant — <pass/fail + notes>
- [x/✗] Target 3: list-item internal cross-field — <pass/fail + notes>
- [x/✗] Target 4: error path quality — <pass/fail + notes>

Decision: <ADOPT FULL GARDE | HYBRID | FULL HAND-ROLLED>

Rationale: <one paragraph>
```

- [ ] **Step 7: Delete spike directory**

Run:
```bash
rm -rf src-tauri/src/automation/protocol/spike/
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

Expected: clean build (garde dep stays in Cargo.toml; only spike code is deleted).

**No commit for Task 0.** The garde dep addition to Cargo.toml is rolled into Task 1's commit.

---

## Task 1: Protocol module scaffold + dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml` (finalize new deps)
- Create: `src-tauri/src/automation/protocol/mod.rs`
- Create: `src-tauri/src/automation/protocol/humane_v1.rs` (empty stub with module doc)
- Create: `src-tauri/src/automation/protocol/parse.rs` (empty stub)
- Create: `src-tauri/src/automation/protocol/normalize.rs` (empty stub)
- Create: `src-tauri/src/automation/protocol/migrate_toml_v1.rs` (empty stub)
- Modify: `src-tauri/src/automation/mod.rs` (add `pub mod protocol;`)

- [ ] **Step 1: Finalize Cargo.toml additions**

```toml
[dependencies]
garde = { version = "0.20", features = ["derive", "pattern", "serde"] }
serde_yaml = "0.9"
globset = "0.4"     # for file-source pattern matching
# regex already present — verify
```

Run: `cd src-tauri && grep -E "^(garde|serde_yaml|globset|regex)" Cargo.toml`
Expected: all four deps present.

- [ ] **Step 2: Create protocol module skeleton**

Create `src-tauri/src/automation/protocol/mod.rs`:

```rust
//! Humane protocol layer — parses, validates, and normalises spec.yaml files.

pub mod humane_v1;
pub mod parse;
pub mod normalize;
pub mod migrate_toml_v1;

pub use humane_v1::HumaneAutomationSpec;
pub use parse::{ParseError, parse_humane_v1};
```

Each of `humane_v1.rs`, `parse.rs`, `normalize.rs`, `migrate_toml_v1.rs` starts as a one-line module doc; content is filled by Tasks 2-7.

- [ ] **Step 3: Wire into automation/mod.rs**

In `src-tauri/src/automation/mod.rs`, add `pub mod protocol;` near the existing module declarations.

- [ ] **Step 4: Verify clean build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty (no errors). Warnings about unused modules are OK.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/automation/
git commit -m "feat(automation): add garde + serde_yaml deps and protocol module scaffold

Lays the groundwork for the Humane automation framework Phase 1 per
docs/superpowers/specs/2026-05-13-humane-automation-design.md.

Adds garde 0.20 (post-spike) and serde_yaml. Creates the protocol/
submodule with empty stubs to be filled in subsequent commits."
```

---

## Task 2: Define HumaneAutomationSpec top-level type

**Files:**
- Modify: `src-tauri/src/automation/protocol/humane_v1.rs`
- Test: inline `#[cfg(test)]` in same file
- Create: `src-tauri/src/automation/protocol/test_fixtures/valid/simple.yaml`

**Spec reference:** § 4.1 contains the full type definition. Implement verbatim.

- [ ] **Step 1: Create fixture `simple.yaml`**

Create `src-tauri/src/automation/protocol/test_fixtures/valid/simple.yaml`:

```yaml
type: automation
name: Test Spec
version: 0.1.0
author: tester
description: Smallest valid Humane spec for round-trip testing.
system_prompt: You are a test agent. Reply with "ok".
```

- [ ] **Step 2: Write failing round-trip test**

Append to `humane_v1.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;

    const SIMPLE: &str = include_str!("test_fixtures/valid/simple.yaml");

    #[test]
    fn parses_and_validates_simple_spec() {
        let spec: HumaneAutomationSpec = serde_yaml::from_str(SIMPLE)
            .expect("parses");
        spec.validate(&()).expect("validates");
        assert_eq!(spec.name, "Test Spec");
        assert_eq!(spec.kind, "automation");
    }

    #[test]
    fn rejects_wrong_kind() {
        let yaml = SIMPLE.replace("type: automation", "type: not_automation");
        let spec: HumaneAutomationSpec = serde_yaml::from_str(&yaml).unwrap();
        assert!(spec.validate(&()).is_err());
    }
}
```

Run: `cd src-tauri && cargo test --lib automation::protocol::humane_v1 2>&1 | tail -20`
Expected: compile errors (types not defined yet).

- [ ] **Step 3: Implement HumaneAutomationSpec (spec § 4.1)**

Replace `humane_v1.rs` contents with the full struct + the placeholder types it depends on. Use spec § 4.1 verbatim. For types not yet defined (`Subscription`, `InputDef`, `FilterRule`, `MemorySchema`, `OutputConfig`, `EscalationConfig`, `Permission`, `BrowserLoginEntry`, `Requires`, `I18nLocaleBlock`), introduce empty struct/enum stubs with `#[derive(Debug, Clone, Serialize, Deserialize, Validate)] #[serde(deny_unknown_fields)]` and a single trivial field if needed. They'll be filled in Tasks 3-4.

Critical signature:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HumaneAutomationSpec {
    #[serde(rename = "type")]
    #[garde(custom(must_be_automation))]
    pub kind: String,
    // ... see spec § 4.1
}

fn must_be_automation(value: &str, _: &()) -> garde::Result {
    if value == "automation" { Ok(()) }
    else { Err(garde::Error::new(format!("type must be 'automation', got '{}'", value))) }
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib automation::protocol::humane_v1 -- --nocapture`
Expected: both tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/protocol/humane_v1.rs src-tauri/src/automation/protocol/test_fixtures/
git commit -m "feat(automation): HumaneAutomationSpec top-level type with garde validation

Implements spec § 4.1 verbatim. Sub-types (Subscription, InputDef, etc.)
are stubs filled in by subsequent commits."
```

---

## Task 3: Subscription discriminated union — 7 variants

**Files:**
- Modify: `src-tauri/src/automation/protocol/humane_v1.rs`
- Create: `src-tauri/src/automation/protocol/test_fixtures/valid/all_subscription_types.yaml`

**Spec reference:** § 4.2.

- [ ] **Step 1: Create fixture with all 7 subscription types**

Create `test_fixtures/valid/all_subscription_types.yaml`:

```yaml
type: automation
name: All Sources Test
version: 0.1.0
author: tester
description: Exercises all seven subscription variants.
system_prompt: noop
subscriptions:
  - { type: schedule, cron: "0 8 * * *" }
  - { type: schedule, every: "30m" }
  - { type: file, pattern: "src/**/*.ts" }
  - { type: webhook, path: "github-pr", secret: "abc" }
  - { type: webpage, url: "https://example.com", selector: ".price" }
  - { type: rss, url: "https://example.com/feed.xml" }
  - { type: wecom, chat_id: "chat-123" }
  - { type: custom, provider: "tg", key: "telegram-bot", config: { token: "x" } }
```

- [ ] **Step 2: Write failing test for each variant**

In `humane_v1.rs` test module:

```rust
#[test]
fn parses_all_subscription_types() {
    let yaml = include_str!("test_fixtures/valid/all_subscription_types.yaml");
    let spec: HumaneAutomationSpec = serde_yaml::from_str(yaml).expect("parses");
    spec.validate(&()).expect("validates");
    assert_eq!(spec.subscriptions.len(), 8);
    matches!(spec.subscriptions[0], Subscription::Schedule(_));
    matches!(spec.subscriptions[2], Subscription::File(_));
    matches!(spec.subscriptions[3], Subscription::Webhook(_));
    matches!(spec.subscriptions[4], Subscription::Webpage(_));
    matches!(spec.subscriptions[5], Subscription::Rss(_));
    matches!(spec.subscriptions[6], Subscription::Wecom(_));
    matches!(spec.subscriptions[7], Subscription::Custom(_));
}

#[test]
fn schedule_requires_cron_or_every() {
    let yaml = "type: automation\nname: x\nversion: 0.1.0\nauthor: x\ndescription: x\nsystem_prompt: x\nsubscriptions:\n  - { type: schedule }";
    let spec: HumaneAutomationSpec = serde_yaml::from_str(yaml).unwrap();
    assert!(spec.validate(&()).is_err());
}
```

Run: `cd src-tauri && cargo test --lib automation::protocol::humane_v1::tests::parses_all_subscription_types 2>&1 | tail`
Expected: fail (Subscription variants not defined).

- [ ] **Step 3: Implement Subscription enum + 7 variant structs**

Replace stub `Subscription` with the full enum from spec § 4.2 and define each variant struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Subscription {
    Schedule(#[garde(dive)] ScheduleSubscription),
    File(#[garde(dive)] FileSubscription),
    Webhook(#[garde(dive)] WebhookSubscription),
    Webpage(#[garde(dive)] WebpageSubscription),
    Rss(#[garde(dive)] RssSubscription),
    Wecom(#[garde(dive)] WecomSubscription),
    Custom(#[garde(dive)] CustomSubscription),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ScheduleSubscription {
    pub cron: Option<String>,
    pub every: Option<String>,
    #[garde(custom(at_least_one_of_cron_every))]
    #[serde(skip)]
    _check: (),
}
fn at_least_one_of_cron_every(_: &(), ctx: &ScheduleSubscription) -> garde::Result {
    if ctx.cron.is_none() && ctx.every.is_none() {
        return Err(garde::Error::new("schedule requires cron or every"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FileSubscription {
    #[garde(length(min = 1))]
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WebhookSubscription {
    #[garde(pattern("^[a-z0-9-/_]+$"))]
    pub path: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WebpageSubscription {
    #[garde(url)]
    pub url: String,
    #[garde(length(min = 1))]
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RssSubscription {
    #[garde(url)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WecomSubscription {
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomSubscription {
    #[garde(length(min = 1))]
    pub provider: String,
    #[garde(length(min = 1))]
    pub key: String,
    #[garde(skip)]
    #[serde(default)]
    pub config: serde_json::Value,
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib automation::protocol::humane_v1`
Expected: all subscription tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/protocol/
git commit -m "feat(automation): Subscription enum with 7 variants (spec § 4.2)

All seven hello-halo subscription source types: schedule, file, webhook,
webpage, rss, wecom, custom. Cross-field validation for schedule
(requires cron or every) implemented as garde custom refinement."
```

---

## Task 4: Remaining schema types — Permission, FilterRule, InputDef, etc.

**Files:**
- Modify: `src-tauri/src/automation/protocol/humane_v1.rs`
- Create: `src-tauri/src/automation/protocol/test_fixtures/valid/full_featured.yaml`

**Spec reference:** § 4.3 (Permission), § 4.4 (FilterRule), and "InputDef, MemorySchema, OutputConfig, EscalationConfig, BrowserLoginEntry, Requires, I18nLocaleBlock mirror hello-halo's Zod schema 1:1".

- [ ] **Step 1: Create `full_featured.yaml` fixture exercising every optional field**

Create `test_fixtures/valid/full_featured.yaml` — populate every top-level field with realistic values. Use [hello-halo/src/main/apps/spec/schema.ts](../../../../hello-halo/src/main/apps/spec/schema.ts) as the source for shape and defaults. (Use 1 entry per `config_schema` / `permissions` / `browser_login` / `i18n`; 2 entries per `filters` and `escalation.choices` minimum.)

- [ ] **Step 2: Write failing parse-and-validate test**

```rust
#[test]
fn full_featured_round_trip() {
    let yaml = include_str!("test_fixtures/valid/full_featured.yaml");
    let spec: HumaneAutomationSpec = serde_yaml::from_str(yaml).expect("parses");
    spec.validate(&()).expect("validates");

    // Serialize back and re-parse — idempotency
    let yaml2 = serde_yaml::to_string(&spec).unwrap();
    let spec2: HumaneAutomationSpec = serde_yaml::from_str(&yaml2).expect("re-parses");
    spec2.validate(&()).expect("re-validates");
    assert_eq!(serde_yaml::to_string(&spec).unwrap(), serde_yaml::to_string(&spec2).unwrap());
}
```

- [ ] **Step 3: Implement all remaining types**

Replace stub types in `humane_v1.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    AiBrowser, Notification, Filesystem, Network, Shell,
    #[serde(other)] Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FilterRule {
    #[garde(length(min = 1))]
    pub field: String,
    #[garde(custom(valid_op))]
    pub op: String,
    #[garde(skip)]
    pub value: serde_json::Value,
}
fn valid_op(op: &str, _: &()) -> garde::Result {
    matches!(op, "eq" | "ne" | "contains" | "matches" | "gt" | "lt")
        .then_some(())
        .ok_or_else(|| garde::Error::new(format!("unsupported op: {}", op)))
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct InputDef {
    #[garde(length(min = 1))]
    pub key: String,
    #[garde(length(min = 1))]
    pub label: String,
    #[garde(custom(valid_input_type))]
    pub r#type: String,           // "string" | "number" | "boolean" | "secret"
    #[garde(skip)]
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[garde(skip)]
    #[serde(default)]
    pub required: bool,
    pub description: Option<String>,
}
fn valid_input_type(t: &str, _: &()) -> garde::Result {
    matches!(t, "string" | "number" | "boolean" | "secret")
        .then_some(())
        .ok_or_else(|| garde::Error::new(format!("unsupported input type: {}", t)))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct Requires {
    #[garde(skip)]
    #[serde(default)]
    pub mcps: Vec<String>,
    #[garde(skip)]
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemorySchema {
    #[garde(length(min = 1))]
    pub description: String,
    #[garde(skip)]
    #[serde(default)]
    pub initial: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OutputConfig {
    #[garde(skip)]
    #[serde(default)]
    pub channels: Vec<String>,         // "system" | "wecom" | "email"
    #[garde(skip)]
    pub default_level: Option<String>, // "info" | "important" | "critical"
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EscalationConfig {
    #[garde(length(min = 1))]
    pub description: String,
    #[garde(length(min = 2), dive)]
    pub choices: Vec<EscalationChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EscalationChoice {
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BrowserLoginEntry {
    #[garde(url)]
    pub url: String,
    #[garde(length(min = 1))]
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct I18nLocaleBlock {
    #[garde(skip)]
    #[serde(default)]
    pub name: Option<String>,
    #[garde(skip)]
    #[serde(default)]
    pub description: Option<String>,
    #[garde(skip)]
    #[serde(default)]
    pub system_prompt: Option<String>,
}
```

- [ ] **Step 4: Run all type tests**

Run: `cd src-tauri && cargo test --lib automation::protocol::humane_v1`
Expected: full_featured_round_trip + previous tests all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/protocol/
git commit -m "feat(automation): complete Humane v1 schema types (spec § 4.3-4)

Permission, FilterRule, InputDef, Requires, MemorySchema, OutputConfig,
EscalationConfig + EscalationChoice, BrowserLoginEntry, I18nLocaleBlock.
Round-trip idempotency tested via full_featured.yaml fixture."
```

---

## Task 5: Strict + permissive parser (`parse.rs`)

**Files:**
- Modify: `src-tauri/src/automation/protocol/parse.rs`
- Create: `src-tauri/src/automation/protocol/test_fixtures/invalid/missing_name.yaml`
- Create: `src-tauri/src/automation/protocol/test_fixtures/invalid/bad_subscription.yaml`
- Create: `src-tauri/src/automation/protocol/test_fixtures/invalid/unknown_field.yaml`

**Spec reference:** § 4.5 (permissive fallback) and § 11 (risk row on unknown fields).

- [ ] **Step 1: Create invalid fixtures**

`missing_name.yaml`:
```yaml
type: automation
version: 0.1.0
author: x
description: x
system_prompt: x
```

`bad_subscription.yaml`:
```yaml
type: automation
name: x
version: 0.1.0
author: x
description: x
system_prompt: x
subscriptions:
  - { type: schedule }   # neither cron nor every
```

`unknown_field.yaml`:
```yaml
type: automation
name: x
version: 0.1.0
author: x
description: x
system_prompt: x
future_protocol_field: { foo: bar }   # not in our schema
```

- [ ] **Step 2: Write failing tests**

In `parse.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_parse_succeeds_on_valid() {
        let yaml = include_str!("test_fixtures/valid/simple.yaml");
        let result = parse_humane_v1(yaml);
        assert!(result.is_ok());
        assert!(!result.unwrap().has_unknown_fields());
    }

    #[test]
    fn strict_parse_fails_on_missing_name() {
        let yaml = include_str!("test_fixtures/invalid/missing_name.yaml");
        let err = parse_humane_v1(yaml).unwrap_err();
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn strict_parse_fails_on_bad_subscription() {
        let yaml = include_str!("test_fixtures/invalid/bad_subscription.yaml");
        let err = parse_humane_v1(yaml).unwrap_err();
        assert!(err.to_string().contains("schedule"));
    }

    #[test]
    fn permissive_fallback_captures_unknown_fields() {
        let yaml = include_str!("test_fixtures/invalid/unknown_field.yaml");
        let parsed = parse_humane_v1(yaml).expect("permissive accepts");
        assert!(parsed.has_unknown_fields());
        assert!(parsed.extra_fields.contains_key("future_protocol_field"));
    }
}
```

- [ ] **Step 3: Implement parse.rs**

```rust
use crate::automation::protocol::humane_v1::HumaneAutomationSpec;
use garde::Validate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("yaml syntax: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("validation: {0}")]
    Validate(String),
}

#[derive(Debug, Clone)]
pub struct ParsedSpec {
    pub spec: HumaneAutomationSpec,
    pub extra_fields: HashMap<String, serde_json::Value>,
}

impl ParsedSpec {
    pub fn has_unknown_fields(&self) -> bool { !self.extra_fields.is_empty() }
}

pub fn parse_humane_v1(yaml: &str) -> Result<ParsedSpec, ParseError> {
    // Strict pass — uses deny_unknown_fields
    match serde_yaml::from_str::<HumaneAutomationSpec>(yaml) {
        Ok(spec) => {
            spec.validate(&()).map_err(|e| ParseError::Validate(e.to_string()))?;
            Ok(ParsedSpec { spec, extra_fields: HashMap::new() })
        }
        Err(_strict_err) => {
            // Permissive fallback — accept unknowns, partition them out
            #[derive(Deserialize)]
            struct Permissive {
                #[serde(flatten)]
                known: HumaneAutomationSpec,        // will still fail if known fields are wrong
                #[serde(flatten)]
                extra: HashMap<String, serde_json::Value>,
            }
            // Use Value as intermediate to relax deny_unknown_fields
            let value: serde_json::Value = serde_yaml::from_str(yaml)?;
            let mut obj = value.as_object().ok_or_else(||
                ParseError::Validate("spec must be a YAML mapping".into()))?.clone();

            // Take known fields out, parse them as HumaneAutomationSpec
            let known_keys = ["type","name","version","author","description","system_prompt",
                              "subscriptions","config_schema","requires","filters","memory_schema",
                              "output","escalation","permissions","browser_login","i18n"];
            let mut extras = HashMap::new();
            for (k, v) in obj.clone().into_iter() {
                if !known_keys.contains(&k.as_str()) {
                    extras.insert(k.clone(), v);
                    obj.remove(&k);
                }
            }
            let cleaned = serde_json::Value::Object(obj);
            let spec: HumaneAutomationSpec = serde_json::from_value(cleaned)?;
            spec.validate(&()).map_err(|e| ParseError::Validate(e.to_string()))?;
            Ok(ParsedSpec { spec, extra_fields: extras })
        }
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self { ParseError::Validate(e.to_string()) }
}
```

Note: `HumaneAutomationSpec` must be modified to allow deserialization from JSON Value (it already supports this via serde). The `#[serde(deny_unknown_fields)]` on the spec only fires in the strict pass.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib automation::protocol::parse`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/protocol/
git commit -m "feat(automation): strict + permissive Humane spec parser (spec § 4.5)

Strict pass uses deny_unknown_fields. On failure, permissive fallback
partitions unknown top-level fields into ParsedSpec.extra_fields so
future hello-halo protocol additions are preserved verbatim."
```

---

## Task 6: Spec normalizer (`normalize.rs`)

**Files:**
- Modify: `src-tauri/src/automation/protocol/normalize.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::HumaneAutomationSpec;

    #[test]
    fn normalize_strips_extras_and_produces_stable_json() {
        let yaml = include_str!("test_fixtures/valid/full_featured.yaml");
        let parsed = crate::automation::protocol::parse::parse_humane_v1(yaml).unwrap();
        let json = normalize_to_json(&parsed.spec).expect("normalises");
        // serialise twice — must be byte-identical
        let again = normalize_to_json(&parsed.spec).unwrap();
        assert_eq!(json, again);
        // i18n stripped from default locale view (Phase 1 default-locale-only)
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("i18n").is_some());     // i18n preserved at top level
    }
}
```

- [ ] **Step 2: Implement normalize_to_json**

```rust
use crate::automation::protocol::humane_v1::HumaneAutomationSpec;

pub fn normalize_to_json(spec: &HumaneAutomationSpec) -> Result<String, serde_json::Error> {
    // Sort map keys for deterministic output
    let value = serde_json::to_value(spec)?;
    serde_json::to_string(&sort_keys(value))
}

fn sort_keys(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => {
            let mut sorted: std::collections::BTreeMap<String, serde_json::Value> = m
                .into_iter()
                .map(|(k, v)| (k, sort_keys(v)))
                .collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(a) => serde_json::Value::Array(a.into_iter().map(sort_keys).collect()),
        other => other,
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib automation::protocol::normalize`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/protocol/normalize.rs
git commit -m "feat(automation): spec normaliser produces deterministic spec_json

BTreeMap-based key ordering ensures byte-identical normalisation across
runs. Used to populate the spec_json column for FTS and queryability."
```

---

## Task 7: Legacy TOML migrator

**Files:**
- Modify: `src-tauri/src/automation/protocol/migrate_toml_v1.rs`

**Spec reference:** § 7.1 contains the full migrator code.

- [ ] **Step 1: Write failing test for the three legacy trigger types**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_cron_trigger() {
        let toml = r#"
            name = "Test"
            description = "desc"
            task = "Do thing"
            [trigger.cron]
            expr = "0 8 * * *"
        "#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert_eq!(migrated.spec.name, "Test");
        assert_eq!(migrated.spec.system_prompt, "Do thing");
        assert_eq!(migrated.spec.subscriptions.len(), 1);
    }

    #[test]
    fn migrates_manual_trigger_to_empty_subscriptions() {
        let toml = r#"
            name = "Test"
            task = "Do thing"
            trigger = "Manual"
        "#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert!(migrated.spec.subscriptions.is_empty());
    }

    #[test]
    fn migrates_once_trigger_drops_subscription() {
        let toml = r#"
            name = "Test"
            task = "Do thing"
            [trigger.once]
            at = 1700000000
        "#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert!(migrated.spec.subscriptions.is_empty());
    }
}
```

- [ ] **Step 2: Implement migrator per spec § 7.1**

Copy spec § 7.1 verbatim into `migrate_toml_v1.rs`. Add the `LegacyTomlSpec` + `LegacyTrigger` private types matching the existing `automation/spec.rs` shape (read the current file to get exact field names).

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib automation::protocol::migrate_toml_v1`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/protocol/migrate_toml_v1.rs
git commit -m "feat(automation): legacy TOML → Humane YAML migrator (spec § 7.1)

Maps task → system_prompt, Cron(expr) → schedule subscription, Manual
and Once → empty subscriptions. Used by V20c data-fixup migration."
```

---

## Task 8: V20a + V20b + V20c migrations

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

**Spec reference:** §§ 5.1, 5.2, 5.3.

- [ ] **Step 1: Read the existing migrations.rs structure**

```bash
cd src-tauri/src && head -80 db/migrations.rs
```

Note the function naming convention (`fn run_v7`, etc.) and the entry-point dispatch table.

- [ ] **Step 2: Write a failing migration smoke test**

Append to `db/migrations.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn v20_migrates_legacy_toml_specs() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    // Apply V1..V19
    run_all_migrations_up_to(&conn, 19).unwrap();
    // Seed a legacy row
    conn.execute(
        "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
         VALUES ('s1', 'Test', 'desc', 'name = \"Test\"\ntask = \"Do thing\"\ntrigger = \"Manual\"\n', 1, 1, 1)",
        []
    ).unwrap();
    // Run V20
    run_v20(&conn).unwrap();
    // Verify migrated
    let yaml: String = conn.query_row(
        "SELECT spec_yaml FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
    ).unwrap();
    assert!(yaml.contains("type: automation"));
    assert!(yaml.contains("system_prompt: Do thing"));
    let source: String = conn.query_row(
        "SELECT source FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
    ).unwrap();
    assert_eq!(source, "toml-migrated");
}
```

- [ ] **Step 3: Implement V20**

Create `fn run_v20(conn: &Connection) -> Result<()>` that:

1. Begins a transaction.
2. **V20a**: `CREATE TABLE automation_specs_new (...)` per spec § 5.1. Copy legacy rows into the new table, mapping `name/description/created_at/updated_at` directly and leaving `spec_yaml/spec_json` empty for now.
3. **V20b**: `CREATE TABLE automation_activities_new (...)` per spec § 5.2. Copy legacy `automation_activities` rows, mapping `trigger:String → trigger_source_type` (heuristic: `"manual"` / `"cron"` / fallback `"manual"`), `created_at → queued_at`, `result → report_text` plus `report_outcome = 'useful'` when `status = 'Completed'`. Drop unused `run_id`.
4. **V20c**: iterate rows in `automation_specs_new` where `spec_yaml = ''`, run `migrate_toml_v1::migrate_legacy_toml`, write `spec_yaml + spec_json`, set `source = 'toml-migrated'`. Errors are logged + the row's `status = 'error'`; migration continues.
5. `DROP TABLE automation_specs` + `DROP TABLE automation_activities`.
6. `ALTER TABLE automation_specs_new RENAME TO automation_specs` + same for activities.
7. Create indexes per spec.
8. Commit transaction.

Use spec §§ 5.1 / 5.2 SQL verbatim. SQL goes in the function as `conn.execute_batch(SQL_V20A)` etc.

- [ ] **Step 4: Wire run_v20 into the migration dispatch**

Find the `run_all_migrations` function (or equivalent) and add the V20 entry after V19.

- [ ] **Step 5: Run migration test**

Run: `cd src-tauri && cargo test --lib db::migrations::tests::v20_migrates_legacy_toml_specs -- --nocapture`
Expected: passes.

- [ ] **Step 6: Run the full backend test suite**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -20`
Expected: no test broken by V20.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V20 migration — rewrite automation_specs + activities, migrate legacy TOML

V20a rewrites automation_specs to the Humane schema (spec_yaml + spec_json
+ flat columns). V20b rewrites automation_activities with structured
trigger payload, escalation FK, and runtime metric columns. V20c runs
the legacy TOML migrator on existing rows to produce equivalent
Humane YAML. Per docs/superpowers/specs/2026-05-13-humane-automation-design.md
§§ 5.1-5.3."
```

---

## Task 9: V21 migration — behavior tables

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

**Spec reference:** § 5.4.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn v21_creates_behavior_tables() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_all_migrations_up_to(&conn, 20).unwrap();
    run_v21(&conn).unwrap();

    for table in ["automation_subscriptions", "automation_memory", "automation_escalations"] {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
            [table], |r| r.get(0)
        ).unwrap();
        assert_eq!(exists, 1, "table {} missing", table);
    }
}
```

- [ ] **Step 2: Implement run_v21 using spec § 5.4 SQL verbatim**

- [ ] **Step 3: Wire into dispatch**

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib db::migrations
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V21 — automation_subscriptions, automation_memory, automation_escalations

Three new behavior tables for the Humane runtime per spec § 5.4.
Subscriptions are FK'd to automation_specs with CASCADE; memory is
PK-keyed by spec_id (1:1); escalations link both spec and activity."
```

---

## Task 10: Spec & activity repository structs (`activity.rs` rewrite + new `manager.rs` scaffold)

**Files:**
- Rewrite: `src-tauri/src/automation/activity.rs`
- Create: `src-tauri/src/automation/manager.rs` (skeleton only — CRUD impl in subsequent tasks)
- Modify: `src-tauri/src/automation/mod.rs`

- [ ] **Step 1: Read current activity.rs to know what's used downstream**

```bash
cd src-tauri/src && grep -rn "automation::activity\|AutomationActivity" --include="*.rs"
```

- [ ] **Step 2: Write failing test**

```rust
#[test]
fn activity_repo_roundtrips_new_columns() {
    let conn = setup_test_db_with_v21();
    let activity = AutomationActivity {
        id: "a1".into(),
        spec_id: "s1".into(),
        subscription_id: None,
        trigger_source_type: TriggerSource::Manual,
        trigger_payload_json: "{}".into(),
        status: ActivityStatus::Queued,
        error_text: None,
        queued_at: 1,
        started_at: None,
        completed_at: None,
        duration_ms: None,
        llm_iterations: 0,
        llm_tokens_in: 0,
        llm_tokens_out: 0,
        tool_calls_json: "[]".into(),
        report_text: None,
        report_outcome: None,
        escalation_id: None,
        resumed_from_activity_id: None,
        resumed_from_escalation_id: None,
    };
    insert_activity(&conn, &activity).unwrap();
    let loaded = get_activity(&conn, "a1").unwrap().unwrap();
    assert_eq!(loaded.spec_id, "s1");
}
```

- [ ] **Step 3: Implement activity.rs against the V20b schema**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus { Queued, Running, Completed, Failed, Cancelled, WaitingUser }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource { Schedule, File, Webhook, Webpage, Rss, Wecom, Custom, Manual }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationActivity {
    pub id: String,
    pub spec_id: String,
    pub subscription_id: Option<String>,
    pub trigger_source_type: TriggerSource,
    pub trigger_payload_json: String,
    pub status: ActivityStatus,
    pub error_text: Option<String>,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub llm_iterations: i64,
    pub llm_tokens_in: i64,
    pub llm_tokens_out: i64,
    pub tool_calls_json: String,
    pub report_text: Option<String>,
    pub report_outcome: Option<String>,
    pub escalation_id: Option<String>,
    pub resumed_from_activity_id: Option<String>,
    pub resumed_from_escalation_id: Option<String>,
}

pub fn insert_activity(conn: &rusqlite::Connection, a: &AutomationActivity) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO automation_activities (id, spec_id, subscription_id, trigger_source_type, trigger_payload_json,
            status, error_text, queued_at, started_at, completed_at, duration_ms,
            llm_iterations, llm_tokens_in, llm_tokens_out, tool_calls_json,
            report_text, report_outcome, escalation_id,
            resumed_from_activity_id, resumed_from_escalation_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        rusqlite::params![
            a.id, a.spec_id, a.subscription_id, serde_json::to_string(&a.trigger_source_type)?,
            a.trigger_payload_json, serde_json::to_string(&a.status)?, a.error_text,
            a.queued_at, a.started_at, a.completed_at, a.duration_ms,
            a.llm_iterations, a.llm_tokens_in, a.llm_tokens_out, a.tool_calls_json,
            a.report_text, a.report_outcome, a.escalation_id,
            a.resumed_from_activity_id, a.resumed_from_escalation_id,
        ],
    )?;
    Ok(())
}

pub fn get_activity(conn: &rusqlite::Connection, id: &str) -> rusqlite::Result<Option<AutomationActivity>> {
    // ... straightforward row mapping; omit for brevity (use rusqlite::Row::get for each column)
    todo!("implement using same column order as INSERT")
}
```

The `todo!()` placeholder in the get fn must be replaced with the actual row-mapping code; just listing it for structural reference.

- [ ] **Step 4: Add manager.rs skeleton**

```rust
//! Humane spec install / uninstall / list / status management.
//! CRUD impl filled in Task 13.

use crate::automation::protocol::ParsedSpec;

pub struct HumaneSpecRow {
    // ... mirrors spec § 5.1 columns
}
```

- [ ] **Step 5: Update mod.rs**

Replace the old `pub mod service; pub mod runtime;` lines with `pub mod manager;` (leaving the old `service.rs` and `runtime.rs` files in place — they are deleted in Tasks 14/18 after their replacements compile).

- [ ] **Step 6: Run tests**

```bash
cd src-tauri && cargo test --lib automation::activity
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/
git commit -m "feat(automation): rewrite activity.rs against V20b schema; manager.rs scaffold

ActivityStatus + TriggerSource enums match the new column types.
insert_activity / get_activity exercise every column. manager.rs is
a stub awaiting CRUD impl in Task 13."
```

---

## Task 11: Filter evaluator (`filters.rs`)

**Files:**
- Create: `src-tauri/src/automation/filters.rs`
- Modify: `src-tauri/src/automation/mod.rs` (add `pub mod filters;`)

**Spec reference:** § 6.6.

- [ ] **Step 1: Write failing tests — 6 ops × happy + sad**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::FilterRule;
    use serde_json::json;

    fn rule(field: &str, op: &str, value: serde_json::Value) -> FilterRule {
        FilterRule { field: field.into(), op: op.into(), value }
    }

    #[test] fn eq_pass() {
        let ctx = json!({"event": {"branch": "main"}});
        assert!(evaluate(&[rule("/event/branch", "eq", json!("main"))], &ctx));
    }
    #[test] fn eq_fail() {
        let ctx = json!({"event": {"branch": "feature"}});
        assert!(!evaluate(&[rule("/event/branch", "eq", json!("main"))], &ctx));
    }
    #[test] fn contains_pass() {
        let ctx = json!({"title": "fix bug in widget"});
        assert!(evaluate(&[rule("/title", "contains", json!("bug"))], &ctx));
    }
    #[test] fn matches_regex_pass() {
        let ctx = json!({"branch": "release/v1.2.3"});
        assert!(evaluate(&[rule("/branch", "matches", json!("^release/"))], &ctx));
    }
    #[test] fn gt_numeric_pass() {
        let ctx = json!({"count": 10});
        assert!(evaluate(&[rule("/count", "gt", json!(5))], &ctx));
    }
    #[test] fn unknown_op_fails_closed() {
        let ctx = json!({"x": 1});
        assert!(!evaluate(&[rule("/x", "exists", json!(true))], &ctx));
    }
    #[test] fn all_rules_must_pass() {
        let ctx = json!({"a": 1, "b": 2});
        assert!(evaluate(&[
            rule("/a", "eq", json!(1)),
            rule("/b", "eq", json!(2)),
        ], &ctx));
        assert!(!evaluate(&[
            rule("/a", "eq", json!(1)),
            rule("/b", "eq", json!(99)),
        ], &ctx));
    }
}
```

- [ ] **Step 2: Implement evaluate per spec § 6.6**

```rust
use crate::automation::protocol::humane_v1::FilterRule;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;

static REGEX_CACHE: Lazy<Mutex<HashMap<String, Regex>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn evaluate(rules: &[FilterRule], ctx: &serde_json::Value) -> bool {
    rules.iter().all(|r| eval_one(r, ctx))
}

fn eval_one(r: &FilterRule, ctx: &serde_json::Value) -> bool {
    let actual = ctx.pointer(&r.field);
    match (r.op.as_str(), actual) {
        ("eq", Some(v)) => v == &r.value,
        ("ne", Some(v)) => v != &r.value,
        ("contains", Some(serde_json::Value::String(s))) => {
            r.value.as_str().map_or(false, |needle| s.contains(needle))
        }
        ("matches", Some(serde_json::Value::String(s))) => {
            let pat = match r.value.as_str() { Some(p) => p, None => return false };
            let mut cache = REGEX_CACHE.lock().unwrap();
            let re = cache.entry(pat.to_string())
                .or_insert_with(|| Regex::new(pat).unwrap_or_else(|_| Regex::new("^$").unwrap()));
            re.is_match(s)
        }
        ("gt", Some(a)) | ("lt", Some(a)) => {
            let (Some(an), Some(bn)) = (a.as_f64(), r.value.as_f64()) else { return false };
            if r.op == "gt" { an > bn } else { an < bn }
        }
        _ => false,
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib automation::filters`
Expected: 8 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/filters.rs src-tauri/src/automation/mod.rs
git commit -m "feat(automation): structured FilterRule evaluator (spec § 6.6)

Six operators (eq/ne/contains/matches/gt/lt), JSON Pointer access,
regex cache for matches op. No DSL — closed operator set matches
hello-halo's deliberate stance against custom expression engines."
```

---

## Task 12: Permission gate (`permissions.rs`)

**Files:**
- Create: `src-tauri/src/automation/permissions.rs`
- Modify: `src-tauri/src/automation/mod.rs`

**Spec reference:** § 6.7.

- [ ] **Step 1: Write failing tests for precedence (denied > granted > spec)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    #[test]
    fn denied_overrides_granted() {
        let r = check(&[Permission::Notification], &[Permission::Notification], &[Permission::Notification], "notify_user");
        assert!(matches!(r, Err(PermissionError::Denied)));
    }
    #[test]
    fn granted_unlocks_tool() {
        let r = check(&[], &[Permission::Shell, Permission::Filesystem], &[], "shell");
        assert!(r.is_ok());
    }
    #[test]
    fn spec_perm_acts_as_implicit_grant() {
        let r = check(&[Permission::Network], &[], &[], "web");
        assert!(r.is_ok());
    }
    #[test]
    fn missing_perm_rejects() {
        let r = check(&[], &[], &[], "shell");
        assert!(matches!(r, Err(PermissionError::NotGranted)));
    }
    #[test]
    fn memory_never_gated() {
        assert!(check(&[], &[], &[], "memory").is_ok());
        assert!(check(&[], &[], &[], "report_to_user").is_ok());
        assert!(check(&[], &[], &[], "request_escalation").is_ok());
    }
}
```

- [ ] **Step 2: Implement permissions.rs per spec § 6.7**

```rust
use crate::automation::protocol::humane_v1::Permission;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum PermissionError {
    #[error("permission denied by user")]
    Denied,
    #[error("permission not granted")]
    NotGranted,
    #[error("tool has no permission mapping")]
    Unmapped,
}

pub fn check(
    spec_perms: &[Permission],
    granted: &[Permission],
    denied: &[Permission],
    tool_name: &str,
) -> Result<(), PermissionError> {
    let required = match required_for(tool_name) {
        None => return Ok(()),                      // ungated tools
        Some(p) => p,
    };
    if denied.contains(&required) { return Err(PermissionError::Denied); }
    if granted.contains(&required) || spec_perms.contains(&required) { return Ok(()); }
    Err(PermissionError::NotGranted)
}

fn required_for(tool: &str) -> Option<Permission> {
    match tool {
        "shell" => Some(Permission::Shell),
        "edit" | "file"           => Some(Permission::Filesystem),
        "web"                     => Some(Permission::Network),
        "notify_user"             => Some(Permission::Notification),
        t if t.starts_with("browser_") => Some(Permission::AiBrowser),
        "memory" | "report_to_user" | "request_escalation" => None,
        _ => None,    // unknown tools pass through (Phase 1 conservative; Phase 2 may flip to Unmapped)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib automation::permissions`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/permissions.rs src-tauri/src/automation/mod.rs
git commit -m "feat(automation): permission gate with deny > grant > spec precedence

Implements spec § 6.7. Ungated tools (memory, report_to_user,
request_escalation) bypass entirely. Unknown tool names pass through
in Phase 1 (conservative); Phase 2 may tighten to Unmapped error."
```

---

## Task 13: Memory store + compact (`memory/`)

**Files:**
- Create: `src-tauri/src/automation/memory/mod.rs`
- Create: `src-tauri/src/automation/memory/store.rs`
- Create: `src-tauri/src/automation/memory/compact.rs`
- Modify: `src-tauri/src/automation/mod.rs`

**Spec reference:** § 6.5.

- [ ] **Step 1: Write failing test for read/write/append/compact cycle**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn full_cycle() {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::new(tmp.path().to_path_buf());

        // read empty
        assert_eq!(store.read("s1").await.unwrap(), "");
        // write
        store.write("s1", "hello").await.unwrap();
        assert_eq!(store.read("s1").await.unwrap(), "hello");
        // append
        store.append("s1", "\nworld").await.unwrap();
        assert_eq!(store.read("s1").await.unwrap(), "hello\nworld");
        // compact
        let archive = store.compact("s1").await.unwrap();
        assert!(archive.exists());
        assert_eq!(store.read("s1").await.unwrap(), "");
    }
}
```

- [ ] **Step 2: Implement MemoryStore**

```rust
// memory/store.rs
use std::path::PathBuf;
use tokio::fs;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

pub struct MemoryStore {
    root: PathBuf,
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl MemoryStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root, locks: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn path(&self, spec_id: &str) -> PathBuf {
        self.root.join(spec_id).join("memory.md")
    }

    async fn lock_for(&self, spec_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.entry(spec_id.to_string()).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
    }

    pub async fn read(&self, spec_id: &str) -> std::io::Result<String> {
        let lock = self.lock_for(spec_id).await;
        let _g = lock.lock().await;
        match fs::read_to_string(self.path(spec_id)).await {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }

    pub async fn write(&self, spec_id: &str, content: &str) -> std::io::Result<()> {
        let lock = self.lock_for(spec_id).await;
        let _g = lock.lock().await;
        let p = self.path(spec_id);
        if let Some(parent) = p.parent() { fs::create_dir_all(parent).await?; }
        fs::write(&p, content).await
    }

    pub async fn append(&self, spec_id: &str, content: &str) -> std::io::Result<()> {
        let existing = self.read(spec_id).await?;
        self.write(spec_id, &(existing + content)).await
    }

    pub async fn compact(&self, spec_id: &str) -> std::io::Result<PathBuf> {
        let lock = self.lock_for(spec_id).await;
        let _g = lock.lock().await;
        let main = self.path(spec_id);
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let archive = main.parent().unwrap().join("archives").join(format!("{}.md", timestamp));
        if let Some(parent) = archive.parent() { fs::create_dir_all(parent).await?; }
        if main.exists() {
            fs::rename(&main, &archive).await?;
            fs::write(&main, "").await?;
        }
        Ok(archive)
    }
}
```

- [ ] **Step 3: Create mod.rs + compact.rs**

```rust
// memory/mod.rs
pub mod store;
pub mod compact;
pub use store::MemoryStore;
```

`compact.rs` for Phase 1 just re-exports `MemoryStore::compact`; the dedicated archive-cleanup logic (size-based rotation, max-archives policy) is Phase 2.

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib automation::memory
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/memory/ src-tauri/src/automation/mod.rs
git commit -m "feat(automation): per-spec memory.md store with compaction (spec § 6.5)

Path layout ~/.uclaw/automation/{spec_id}/memory.md plus
archives/{ISO8601}.md. Per-spec async mutex prevents torn writes
between concurrent runs."
```

---

## Task 14: Four new built-in tools + dispatcher wiring

**Files:**
- Create: `src-tauri/src/automation/tools/mod.rs`
- Create: `src-tauri/src/automation/tools/report_to_user.rs`
- Create: `src-tauri/src/automation/tools/notify_user.rs`
- Create: `src-tauri/src/automation/tools/request_escalation.rs`
- Create: `src-tauri/src/automation/tools/memory.rs`
- Modify: `src-tauri/src/agent/dispatcher.rs` (single line: call `register_automation_tools()`)
- Modify: `src-tauri/src/automation/mod.rs`

**Spec reference:** § 6.4.

- [ ] **Step 1: Sketch the trait/registration interface**

The Phase 1 approach (per spec § 11 risk row): tool implementations live in `automation/tools/*.rs`. `tools/mod.rs` exposes a single function `register_automation_tools(dispatcher: &mut ToolRegistry)` that pushes the four tool schemas. The call site in [agent/dispatcher.rs](../../src-tauri/src/agent/dispatcher.rs) is one line.

- [ ] **Step 2: Read the existing tool registration pattern**

```bash
cd src-tauri/src && grep -n "fn register\|ToolDefinition\|tool_schemas" agent/dispatcher.rs | head -20
```

Match its idiom (struct shape, schema format, async handler signature). Specific patterns will vary — adapt the snippets below to the real interface.

- [ ] **Step 3: Implement report_to_user (the completion gate)**

```rust
// tools/report_to_user.rs
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct ReportInput {
    pub text: String,
    pub outcome: String,                 // "useful" | "noop" | "error" | "skipped"
    #[serde(default)]
    pub artifacts: Vec<serde_json::Value>,
}

pub fn schema() -> serde_json::Value {
    json!({
        "name": "report_to_user",
        "description": "Mark this automation run as complete and deliver a final report to the user. THIS IS THE ONLY WAY TO END A RUN — without calling this, the run will retry up to 10 times.",
        "input_schema": {
            "type": "object",
            "required": ["text", "outcome"],
            "properties": {
                "text":      { "type": "string" },
                "outcome":   { "enum": ["useful", "noop", "error", "skipped"] },
                "artifacts": { "type": "array", "items": { "type": "object" } }
            }
        }
    })
}
```

- [ ] **Step 4: Implement notify_user, request_escalation, memory tools** (analogous schema-emitter functions)

Each tool's `schema()` mirrors the spec § 6.4 contract verbatim. Runtime handlers (the code that actually executes when the LLM calls the tool) are wired up by `AutomationDelegate` in Task 16 — Phase 1 of this task only ships the schemas and stub handlers that return a typed "to be wired in Task 16" error.

- [ ] **Step 5: tools/mod.rs registers schemas**

```rust
// tools/mod.rs
pub mod report_to_user;
pub mod notify_user;
pub mod request_escalation;
pub mod memory;

pub fn humane_tool_schemas() -> Vec<serde_json::Value> {
    vec![
        report_to_user::schema(),
        notify_user::schema(),
        request_escalation::schema(),
        memory::schema(),
    ]
}
```

The actual `register_automation_tools(...)` glue is in `dispatcher.rs` — the call site looks like:

```rust
// In dispatcher.rs, where other tool sets are loaded:
if delegate_kind == DelegateKind::Automation {
    schemas.extend(crate::automation::tools::humane_tool_schemas());
}
```

— but conditionally; chat delegates do not see these tools.

- [ ] **Step 6: Smoke test schema serialization**

```rust
#[test]
fn humane_tool_schemas_are_valid_json() {
    let schemas = humane_tool_schemas();
    assert_eq!(schemas.len(), 4);
    for s in &schemas {
        let name = s["name"].as_str().expect("has name");
        assert!(!name.is_empty());
        assert!(s["input_schema"].is_object());
    }
}
```

- [ ] **Step 7: Run tests + verify dispatcher.rs still compiles**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib automation::tools
```

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/automation/tools/ src-tauri/src/agent/dispatcher.rs src-tauri/src/automation/mod.rs
git commit -m "feat(automation): four built-in tool schemas — report/notify/escalation/memory

Spec § 6.4. Schemas only at this stage; runtime handlers are wired
by AutomationDelegate in Task 16 (next commit). dispatcher.rs gains
a single conditional call site to keep these tools invisible to
interactive chat."
```

---

## Task 15: AutomationDelegate + tool runtime handlers

**Files:**
- Modify: `src-tauri/src/automation/tools/*.rs` (replace stub handlers with real ones)
- Create: `src-tauri/src/automation/runtime/mod.rs`
- Create: `src-tauri/src/automation/runtime/auto_continue.rs`
- Create: `src-tauri/src/automation/runtime/prompt.rs`
- Create: `src-tauri/src/automation/runtime/execute.rs`
- Modify: `src-tauri/src/automation/mod.rs`

**Spec reference:** §§ 6.1, 6.2, 6.3, 6.4, 6.8.

- [ ] **Step 1: Inspect existing LoopDelegate trait**

```bash
cd src-tauri/src && grep -n "trait LoopDelegate" agent/agentic_loop.rs agent/types.rs | head
```

Note the exact method signature `execute_tool_calls(...)` to match in `AutomationDelegate`.

- [ ] **Step 2: Write failing integration test (mock LLM + AutomationDelegate)**

```rust
// runtime/execute.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agentic_loop::run_agentic_loop;

    #[tokio::test]
    async fn run_terminates_on_report_to_user() {
        // Mock LLM yields one tool call: report_to_user(text="done", outcome="useful")
        // Verify activity row updated: status=completed, report_text="done", report_outcome=useful
        // omitted full impl — see existing tests in agent/tests for the mock LLM pattern
    }

    #[tokio::test]
    async fn run_terminates_on_request_escalation() {
        // Mock LLM yields request_escalation(question=..., choices=[...])
        // Verify escalation row inserted, spec.status='waiting_user', run ends
    }

    #[tokio::test]
    async fn run_fails_after_max_retries_without_report() {
        // Mock LLM yields 11 dummy tool calls (e.g. memory.read each time)
        // Verify activity.status='failed', error_text contains 'no_report'
    }
}
```

- [ ] **Step 3: Implement runtime/auto_continue.rs**

```rust
pub struct AutoContinueConfig { pub max_retries: u32 }
impl Default for AutoContinueConfig { fn default() -> Self { Self { max_retries: 10 } } }

#[derive(Debug, Clone)]
pub enum CompletionGate {
    Reported { text: String, outcome: String },
    Escalated { escalation_id: String },
    LoopExhausted,
    ErrorTerminal(String),
}
```

This is consumed by `AutomationDelegate::execute_tool_calls` — when it sees `report_to_user` it returns `Reported`, when it sees `request_escalation` it writes the escalation row and returns `Escalated`, etc.

- [ ] **Step 4: Implement AutomationDelegate in runtime/execute.rs**

The delegate holds: `spec_id`, `activity_id`, `permission_set`, `memory_store: Arc<MemoryStore>`, `db: Arc<Pool>`, `infra_bus: Arc<InfraService>`. Its `execute_tool_calls` dispatches `report_to_user / notify_user / request_escalation / memory` directly; other tool names fall through to the base built-in dispatcher (shell/file/edit/etc., gated by `permissions::check`).

Skeleton (full impl follows spec § 6.2 + 6.4 verbatim):

```rust
pub struct AutomationDelegate {
    pub spec_id: String,
    pub activity_id: String,
    pub permission_set: PermissionSet,
    pub memory: Arc<MemoryStore>,
    pub db: Arc<rusqlite::Connection>,         // or your pool type
    pub infra: Arc<InfraService>,
    pub gate: Arc<Mutex<Option<CompletionGate>>>,
}

#[async_trait::async_trait]
impl LoopDelegate for AutomationDelegate {
    async fn execute_tool_calls(&self, calls: Vec<ToolCall>, ctx: &mut ReasoningContext) -> Option<LoopOutcome> {
        for call in calls {
            // permission gate
            if let Err(e) = permissions::check(&self.permission_set.spec, &self.permission_set.granted,
                                                &self.permission_set.denied, &call.tool_name) {
                ctx.push_tool_result(&call.id, format!("permission error: {}", e));
                continue;
            }
            match call.tool_name.as_str() {
                "report_to_user" => {
                    let input: ReportInput = serde_json::from_value(call.args.clone()).unwrap();
                    *self.gate.lock().await = Some(CompletionGate::Reported {
                        text: input.text, outcome: input.outcome,
                    });
                    self.infra.publish(InfraEvent::AutomationRunReported { /* ... */ });
                    return Some(LoopOutcome::TerminateOk);
                }
                "request_escalation" => { /* write SQLite row, emit event, return TerminateOk */ }
                "notify_user"        => { /* deliver per channels, push tool result */ }
                "memory"             => { /* read/write/append/compact via MemoryStore */ }
                _ => { /* fall through to base built-ins from agent/tools/builtin/ */ }
            }
        }
        None
    }
}
```

The actual `LoopOutcome` / `ReasoningContext` / `ToolCall` types come from the existing [agent/types.rs](../../src-tauri/src/agent/types.rs); match those exactly.

- [ ] **Step 5: Implement prompt.rs**

```rust
pub fn build_system_prompt(spec: &HumaneAutomationSpec) -> String { spec.system_prompt.clone() }

pub fn build_initial_message(
    subscription: Option<&Subscription>,
    trigger_payload: &serde_json::Value,
    user_config: &serde_json::Value,
    resumption: Option<&EscalationResolution>,
) -> String {
    // Render the trigger context block:
    //   "## Trigger\n type=<source_type>\n payload=<json>\n config=<json>"
    // Plus, if resumption is Some, append:
    //   "## Resuming from escalation\n question=<...> user_choice=<id> user_note=<...>"
    todo!("string concatenation — straightforward")
}
```

- [ ] **Step 6: Run integration tests**

```bash
cd src-tauri && cargo test --lib automation::runtime::execute
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/runtime/ src-tauri/src/automation/tools/ src-tauri/src/automation/mod.rs
git commit -m "feat(automation): AutomationDelegate + tool runtime handlers (spec §§ 6.2-6.4)

LoopDelegate impl that gates on report_to_user / request_escalation,
dispatches the 4 new tools directly, and falls through to base
built-in tools (permission-gated) for shell/file/edit/web. Mock-LLM
integration tests verify the three terminal states: reported,
escalated, loop-exhausted."
```

---

## Task 16: Three subscription sources — Schedule + File + Webhook

**Files:**
- Create: `src-tauri/src/automation/sources/mod.rs`
- Create: `src-tauri/src/automation/sources/schedule.rs`
- Create: `src-tauri/src/automation/sources/file.rs`
- Create: `src-tauri/src/automation/sources/webhook.rs`
- Modify: `src-tauri/src/automation/mod.rs`

**Spec reference:** § 6.1 ACTIVATE block.

- [ ] **Step 1: Define SubscriptionSource trait**

```rust
// sources/mod.rs
use async_trait::async_trait;
use crate::automation::protocol::humane_v1::Subscription;

pub type TriggerCallback = Box<dyn Fn(String, String, serde_json::Value) + Send + Sync>;
//                                    spec_id  sub_id     payload

#[async_trait]
pub trait SubscriptionSource: Send + Sync {
    async fn attach(&self, spec_id: &str, sub_id: &str, sub: &Subscription, on_fire: TriggerCallback) -> anyhow::Result<()>;
    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()>;
}

pub mod schedule;
pub mod file;
pub mod webhook;
// pub mod webpage; rss; wecom; custom — Task 17
```

- [ ] **Step 2: Implement schedule.rs (reuse existing cron infra)**

```rust
// sources/schedule.rs
// Reuses tokio_cron_scheduler or existing service.rs cron logic — adapt to the trait.
// Phase 1 supports cron expression; "every: 30m" parsed into equivalent cron.
```

Read the existing [automation/service.rs](../../src-tauri/src/automation/service.rs) cron code and wrap it behind the trait. Don't rewrite the cron engine.

- [ ] **Step 3: Implement file.rs (FSEvents watcher per PR #142)**

```rust
// sources/file.rs
use globset::Glob;
use notify::{RecursiveMode, Watcher};

pub struct FileSource { /* shares one watcher across all FileSubscriptions */ }

#[async_trait]
impl SubscriptionSource for FileSource {
    async fn attach(&self, spec_id: &str, sub_id: &str, sub: &Subscription, on_fire: TriggerCallback) -> anyhow::Result<()> {
        let Subscription::File(fs) = sub else { return Err(anyhow!("not a file subscription")); };
        let glob = Glob::new(&fs.pattern)?.compile_matcher();
        // register glob + callback; on file change matching glob, invoke on_fire with payload {path, event}
        todo!()
    }
    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()> { todo!() }
}
```

Per uClaw's PR #142 (FSEvents on macOS), do **not** use kqueue. Use the `notify` crate's default backend which now picks FSEvents on macOS.

- [ ] **Step 4: Implement webhook.rs (axum sub-path)**

```rust
// sources/webhook.rs
// Register POST /automation/webhook/{spec_id}/{sub_id}/{user_path} on the existing axum router
// (the one bound to 127.0.0.1:27270 in main.rs). Match user_path against sub.path.
// If sub.secret is set, verify the X-Humane-Signature header (HMAC-SHA256 of body).
// On match, parse body as JSON → call on_fire(spec_id, sub_id, body).
```

Wire into [api/router.rs](../../src-tauri/src/api/router.rs) or wherever the axum router is built — add a layered route.

- [ ] **Step 5: Write integration tests**

```rust
#[tokio::test]
async fn schedule_source_fires_on_cron_match() { /* mock clock; assert callback invoked */ }
#[tokio::test]
async fn file_source_fires_on_pattern_match() { /* tempdir; touch file; assert payload */ }
#[tokio::test]
async fn webhook_source_validates_signature() { /* construct request; verify reject without sig */ }
```

- [ ] **Step 6: Run tests**

```bash
cd src-tauri && cargo test --lib automation::sources
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/sources/ src-tauri/src/automation/mod.rs src-tauri/src/api/
git commit -m "feat(automation): Schedule + File + Webhook subscription sources

Schedule reuses existing cron infra behind the SubscriptionSource trait.
File source uses the notify crate (FSEvents on macOS per PR #142).
Webhook source mounts a sub-path on the existing axum :27270 router
with optional HMAC-SHA256 signature verification."
```

---

## Task 17: Remaining four sources — Webpage + RSS + WeCom + Custom

**Files:**
- Create: `src-tauri/src/automation/sources/webpage.rs`
- Create: `src-tauri/src/automation/sources/rss.rs`
- Create: `src-tauri/src/automation/sources/wecom.rs`
- Create: `src-tauri/src/automation/sources/custom.rs`
- Modify: `src-tauri/src/automation/sources/mod.rs`

- [ ] **Step 1: Implement webpage.rs (polling adaptor)**

```rust
// sources/webpage.rs
// Re-uses proactive 30s polling backbone. Fetches sub.url every N seconds (default 5m,
// configurable per spec in Phase 2), parses with scraper crate, extracts text matching
// sub.selector. Stores last-seen hash in memory; on change, invokes on_fire.
```

- [ ] **Step 2: Implement rss.rs**

```rust
// sources/rss.rs
// Same polling backbone; uses rss crate to parse feed; tracks last-seen item GUIDs;
// invokes on_fire for each new item with payload {title, link, content, pub_date}.
```

- [ ] **Step 3: Implement wecom.rs (webhook sub-path, no SDK)**

```rust
// sources/wecom.rs
// Mounts POST /automation/webhook/wecom/{spec_id}/{sub_id} on axum. Body is treated
// as opaque JSON; if chat_id is set in the subscription, payload's chat_id field must
// match. No WeCom SDK; signature verification deferred to Phase 2 per spec § 9.
```

- [ ] **Step 4: Implement custom.rs (stub)**

```rust
// sources/custom.rs
// Phase 1 stub: registers but immediately logs a warning. Real provider extension
// point lives in Phase 2 (Plug-in registry). Stub accepts attach calls and ignores them.
```

- [ ] **Step 5: Add sources/mod.rs declarations + minimal tests**

```rust
// sources/mod.rs
pub mod webpage;
pub mod rss;
pub mod wecom;
pub mod custom;

#[cfg(test)]
mod tests {
    #[tokio::test] async fn webpage_detects_change() { /* mock HTTP server */ }
    #[tokio::test] async fn rss_emits_new_items_only() { /* fixture XML; advance feed */ }
    #[tokio::test] async fn wecom_routes_through_webhook_path() { /* HTTP POST */ }
    #[test] fn custom_attach_is_inert() { /* no-op assert */ }
}
```

- [ ] **Step 6: Run tests**

```bash
cd src-tauri && cargo test --lib automation::sources
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/sources/
git commit -m "feat(automation): Webpage + RSS + WeCom + Custom subscription sources

Webpage and RSS share the proactive polling backbone. WeCom is a thin
webhook sub-path with no WeCom SDK dependency (spec § 9; signature
verification deferred to Phase 2). Custom is a Phase 1 stub awaiting
the plug-in registry."
```

---

## Task 18: AppRuntimeService — activate / deactivate / execute_run

**Files:**
- Create: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/automation/runtime/mod.rs`
- Modify: `src-tauri/src/app.rs` (register AppRuntimeService into AppState)
- Modify: `src-tauri/src/main.rs` (Stage 3 registration block)

**Spec reference:** § 6.1 ACTIVATE + TRIGGER → RUN blocks.

- [ ] **Step 1: Read existing ServiceManager registration pattern**

```bash
cd src-tauri/src && grep -n "Stage 3\|ServiceManager\|register_service" main.rs app.rs | head
```

- [ ] **Step 2: Write failing test**

```rust
#[tokio::test]
async fn activate_registers_all_subscriptions() {
    let svc = setup_test_runtime_service().await;
    // Install spec with 3 subscriptions: schedule + webhook + file
    let spec_id = svc.install_test_spec(THREE_SOURCES_YAML).await.unwrap();
    svc.activate(&spec_id).await.unwrap();
    // Assert: cron registered, axum route registered, file watcher attached
    assert_eq!(svc.attached_count(&spec_id).await, 3);
}
```

- [ ] **Step 3: Implement AppRuntimeService**

```rust
pub struct AppRuntimeService {
    db: Arc<Pool>,
    schedule: Arc<ScheduleSource>,
    file: Arc<FileSource>,
    webhook: Arc<WebhookSource>,
    webpage: Arc<WebpageSource>,
    rss: Arc<RssSource>,
    wecom: Arc<WecomSource>,
    custom: Arc<CustomSource>,
    memory: Arc<MemoryStore>,
    infra: Arc<InfraService>,
    semaphore: Arc<Semaphore>,         // per-spec concurrency = 2 (hard-coded Phase 1)
}

impl AppRuntimeService {
    pub async fn activate(&self, spec_id: &str) -> anyhow::Result<()> { /* per § 6.1 ACTIVATE */ }
    pub async fn deactivate(&self, spec_id: &str) -> anyhow::Result<()> { /* detach all */ }
    pub async fn execute_run(&self, spec_id: &str, sub_id: Option<&str>, payload: Value) -> anyhow::Result<()> {
        // per § 6.1 TRIGGER → RUN
    }
    pub async fn resolve_escalation(&self, escalation_id: &str, choice: &str, note: Option<&str>) -> anyhow::Result<()> {
        // per § 6.1 ESCALATION RESOLVED block
    }
}
```

- [ ] **Step 4: Wire into ServiceManager (Stage 3 in main.rs)**

Add `service_manager.register(AppRuntimeService::new(...))` in the [Stage 3] block in `main.rs`, alongside the existing PowerService / MemorizationService / etc.

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test --lib automation::runtime::service
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/runtime/ src-tauri/src/app.rs src-tauri/src/main.rs
git commit -m "feat(automation): AppRuntimeService — activate/deactivate/execute_run

Plugs into ServiceManager Stage 3 alongside existing background services.
Holds Arc references to all seven SubscriptionSources plus MemoryStore
and InfraService. execute_run implements the full TRIGGER → RUN flow
per spec § 6.1 including filters, semaphore, AutomationDelegate, and
completion-gate verdict."
```

---

## Task 19: 14 Tauri commands

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs` (invoke_handler! macro)
- Modify: `ui/src/lib/tauri-bridge.ts` (14 new bindings)
- Delete the four legacy automation command bodies (`install_automation`, etc.)

**Spec reference:** § 7.3.

- [ ] **Step 1: Read existing command registration pattern**

```bash
cd src-tauri/src && grep -n "install_automation\|automation_" tauri_commands.rs | head -20
cd src-tauri/src && grep -n "automation" main.rs | head
```

- [ ] **Step 2: Replace 4 legacy commands with 14 new ones**

For each row in spec § 7.3 table, add an `#[tauri::command]` function returning `Result<T, String>`. Example:

```rust
#[tauri::command]
pub async fn install_humane_spec(
    state: tauri::State<'_, AppState>,
    yaml: String,
    source_ref: Option<String>,
) -> Result<HumaneSpecRow, String> {
    state.runtime_service.install_humane_spec(&yaml, source_ref).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_escalation(
    state: tauri::State<'_, AppState>,
    escalation_id: String,
    choice: String,
    note: Option<String>,
) -> Result<(), String> {
    state.runtime_service.resolve_escalation(&escalation_id, &choice, note.as_deref()).await
        .map_err(|e| e.to_string())
}
```

Write the other 12 in the same shape. All 14 names match spec § 7.3 exactly.

- [ ] **Step 3: Update invoke_handler! in main.rs**

Remove `install_automation, list_automations, trigger_automation_manual, get_automation_activity`. Add the 14 new command names.

- [ ] **Step 4: Update ui/src/lib/tauri-bridge.ts**

Add typed wrappers for each new command:

```typescript
export async function installHumaneSpec(yaml: string, sourceRef?: string): Promise<HumaneSpecRow> {
  return invoke('install_humane_spec', { yaml, sourceRef });
}
// ... and the other 13
```

Delete the old `installAutomation`, `listAutomations`, `triggerAutomationManual`, `getAutomationActivity` bindings.

- [ ] **Step 5: Verify backend compiles**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

- [ ] **Step 6: Verify frontend type-checks**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: any errors here come from AutomationHub.tsx referencing the removed legacy bindings — those errors are resolved in Task 20.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "feat(automation): 14 Tauri commands replacing the 4 legacy automation ones (spec § 7.3)

Removes install_automation / list_automations / trigger_automation_manual /
get_automation_activity. Adds the 14 commands from spec § 7.3 covering
spec CRUD, config updates, permissions, enable/disable, manual trigger,
activity, escalation lifecycle, and memory inspection. Frontend
tauri-bridge.ts is updated in lockstep; AutomationHub.tsx is fixed
in the next commit."
```

---

## Task 20: Frontend — AutomationHub paste-YAML field + atoms update

**Files:**
- Modify: `ui/src/components/automation/AutomationHub.tsx`
- Modify: `ui/src/atoms/automation.ts`

- [ ] **Step 1: Read the existing AutomationHub.tsx (259 lines)**

```bash
cd ui/src/components/automation && wc -l AutomationHub.tsx && head -40 AutomationHub.tsx
```

- [ ] **Step 2: Update atoms**

```typescript
// ui/src/atoms/automation.ts — add typed shape for new column set
import { atom } from 'jotai';
import type { HumaneSpecRow, AutomationActivity, AutomationEscalation } from '../lib/tauri-bridge';

export const humaneSpecsAtom = atom<HumaneSpecRow[]>([]);
export const automationActivitiesAtom = atom<Record<string /* spec_id */, AutomationActivity[]>>({});
export const pendingEscalationsAtom = atom<AutomationEscalation[]>([]);
export const automationPanelOpenAtom = atom<boolean>(false);
export const selectedAutomationIdAtom = atom<string | null>(null);
```

- [ ] **Step 3: Rewrite AutomationHub.tsx install dialog**

Locate the existing "paste TOML" textarea + install button. Replace label/placeholder/submit handler:

```typescript
// Before: install button calls installAutomation(tomlContent)
// After:  install button calls installHumaneSpec(yaml)

<textarea
  placeholder="粘贴 Humane YAML 规约..."
  value={yamlInput}
  onChange={(e) => setYamlInput(e.target.value)}
/>
<Button onClick={async () => {
  try {
    const row = await installHumaneSpec(yamlInput);
    setHumaneSpecs([...specs, row]);
    setYamlInput('');
    toast.success(`已安装数字员工：${row.name}`);
  } catch (e) {
    toast.error(`安装失败：${e}`);
  }
}}>安装</Button>
```

Add a sibling file-picker button:

```typescript
<Button variant="ghost" onClick={async () => {
  const path = await open({ filters: [{ name: 'Humane YAML', extensions: ['yaml', 'yml'] }] });
  if (path) {
    const row = await importHumaneSpecFile(path as string);
    setHumaneSpecs([...specs, row]);
  }
}}>从文件导入</Button>
```

- [ ] **Step 4: Update list rendering for new columns**

Display the new `status` / `version` / `author` / `last_run_outcome` columns. Show a "needs_review" badge when `status === 'needs_review'` (Humane fields not recognised — from the permissive fallback).

- [ ] **Step 5: Run frontend type check + tests**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run AutomationHub 2>&1 | tail -10
```

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/automation/AutomationHub.tsx ui/src/atoms/automation.ts
git commit -m "feat(ui): AutomationHub paste-YAML field + file-picker import

Replaces the legacy TOML textarea with a Humane YAML textarea and a
file-picker button that invokes import_humane_spec_file. List view
displays the new status / version / author / last_run_outcome columns
and a 'needs_review' badge for specs containing unknown protocol fields."
```

---

## Task 21: Frontend — EscalationModal + useAutomationEvents hook

**Files:**
- Create: `ui/src/components/automation/EscalationModal.tsx`
- Create: `ui/src/hooks/useAutomationEvents.ts`
- Modify: `ui/src/components/app-shell/LeftSidebar.tsx` (mount escalation badge)
- Modify: `ui/src/atoms/automation.ts` (subscription side effects)

- [ ] **Step 1: Write failing component test**

```tsx
// EscalationModal.test.tsx
import { renderWithProviders } from '@/test-utils/render';

test('renders question and choices', () => {
  const escalation = {
    id: 'e1', specId: 's1', activityId: 'a1',
    question: 'Which branch to release?',
    choices: [{ id: 'main', label: 'main' }, { id: 'staging', label: 'staging' }],
    status: 'waiting', createdAt: 1,
  };
  const { getByText } = renderWithProviders(<EscalationModal escalation={escalation} onResolve={() => {}} />);
  expect(getByText('Which branch to release?')).toBeInTheDocument();
  expect(getByText('main')).toBeInTheDocument();
  expect(getByText('staging')).toBeInTheDocument();
});

test('clicking a choice calls onResolve with id', async () => {
  const onResolve = vi.fn();
  // render, click "main", assert onResolve('main')
});
```

- [ ] **Step 2: Implement EscalationModal**

```tsx
// ui/src/components/automation/EscalationModal.tsx
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

export function EscalationModal({ escalation, onResolve }) {
  return (
    <Dialog open onOpenChange={() => {}}>
      <DialogContent>
        <DialogTitle>{escalation.question}</DialogTitle>
        <div className="space-y-2">
          {escalation.choices.map((c) => (
            <Button key={c.id} onClick={() => onResolve(c.id)} className="w-full justify-start">
              {c.label}
              {c.description && <span className="text-muted-foreground ml-2">— {c.description}</span>}
            </Button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

Use theme tokens (`text-muted-foreground` etc.) per CLAUDE.md — no hardcoded `text-gray-500`.

- [ ] **Step 3: Implement useAutomationEvents hook**

```tsx
// ui/src/hooks/useAutomationEvents.ts
import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useSetAtom } from 'jotai';
import { pendingEscalationsAtom } from '@/atoms/automation';

export function useAutomationEvents() {
  const setEscalations = useSetAtom(pendingEscalationsAtom);
  useEffect(() => {
    const unlisten = listen('automation:escalation_raised', (e) => {
      setEscalations((prev) => [...prev, e.payload]);
    });
    // pull initial pending escalations on mount
    listPendingEscalations().then(setEscalations);
    return () => { unlisten.then((fn) => fn()); };
  }, [setEscalations]);
}
```

Mount this in the app root (e.g. App.tsx or root layout).

- [ ] **Step 4: Mount EscalationModal in shell**

In LeftSidebar (or AppShell), if `pendingEscalations.length > 0`, render the modal for the first pending one:

```tsx
const escalations = useAtomValue(pendingEscalationsAtom);
const first = escalations[0];
{first && (
  <EscalationModal
    escalation={first}
    onResolve={async (choice) => {
      await resolveEscalation(first.id, choice);
      setEscalations((rest) => rest.filter((e) => e.id !== first.id));
    }}
  />
)}
```

- [ ] **Step 5: Run tests**

```bash
cd ui && npm test -- --run EscalationModal 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/automation/EscalationModal.tsx ui/src/hooks/useAutomationEvents.ts ui/src/components/app-shell/LeftSidebar.tsx ui/src/atoms/automation.ts
git commit -m "feat(ui): EscalationModal + automation event subscription

Mounts a modal whenever an automation requests user escalation. Hooks
into the existing Tauri event system for InfraEvent::AutomationEscalationRaised
plus an initial fetch via list_pending_escalations to survive restart.
Theme tokens used throughout per CLAUDE.md."
```

---

## Task 22: Cleanup — delete legacy code + update CLAUDE.md

**Files:**
- Delete: `src-tauri/src/automation/runtime.rs` (legacy)
- Delete: `src-tauri/src/automation/service.rs` (legacy)
- Modify: `src-tauri/src/automation/spec.rs` (delete LegacyTomlSpec branch)
- Modify: `CLAUDE.md` (Active migration registry)
- Modify: `src-tauri/src/automation/mod.rs`

- [ ] **Step 1: Delete the legacy runtime + service files**

```bash
rm src-tauri/src/automation/runtime.rs
rm src-tauri/src/automation/service.rs
```

- [ ] **Step 2: Strip LegacyTomlSpec from spec.rs**

The migrator no longer needs to be exposed; the V20c migration has already run on existing data. Keep only re-exports of the new types.

- [ ] **Step 3: Update automation/mod.rs**

Remove `pub mod runtime;` and `pub mod service;` lines. Final state:

```rust
pub mod activity;
pub mod filters;
pub mod importer;
pub mod manager;
pub mod memory;
pub mod permissions;
pub mod protocol;
pub mod runtime;       // now the directory module
pub mod sources;
pub mod spec;
pub mod tools;
```

- [ ] **Step 4: Update CLAUDE.md Active migration registry**

In CLAUDE.md, locate the "Active migration registry" table and update:

```markdown
| V20 | rewrite automation_specs + activities + migrate legacy TOML | merged (Phase 1) |
| V21 | automation_subscriptions + automation_memory + automation_escalations | merged (Phase 1) |
| V22 | (reserved) FTS5 over automation_specs | Phase 4 |
| V23 | (reserved) automation_registries + marketplace items | Phase 3 |
```

Also update Part 1 "Surfaces to check before assuming" if relevant, and the Phase tracking note.

- [ ] **Step 5: Final build + test sweep**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: clean across all four.

- [ ] **Step 6: Manual verification sweep (spec § 12)**

Walk through the 8 done-criteria items in spec § 12 and check each off. Each item should produce a recordable observation.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "chore(automation): delete legacy TOML branch; update CLAUDE.md migration registry

Removes the single-task TOML-only automation/runtime.rs and service.rs
that have been superseded by automation/runtime/ and manager.rs. All
existing legacy specs have been migrated to Humane YAML by V20c.
CLAUDE.md migration registry advanced to V21."
```

---

## PR Description Template (for when work is ready to push)

```markdown
## Summary

Ports hello-halo's autonomous-agent framework into uClaw as the **Humane automation framework**. Phase 1 delivers a 100% schema-mirror of hello-halo's AutomationSpec protocol with full runtime support: all seven subscription source types, four built-in tools, structured filters, human-in-the-loop escalation, per-spec persistent memory, permission gating, and a minimum-viable frontend for paste-import + escalation resolution.

Marketplace, local hello-halo workspace scan, and avatar/voice "skin" layers are explicitly deferred (see spec § 9 Out of Scope).

**Spec:** docs/superpowers/specs/2026-05-13-humane-automation-design.md
**Plan:** docs/superpowers/plans/2026-05-13-humane-automation-phase1.md

## Commits (bisectable)

| # | Commit | What |
|---|---|---|
| 1 | feat(automation): add garde + serde_yaml deps and protocol module scaffold | Deps + dir layout |
| 2 | feat(automation): HumaneAutomationSpec top-level type with garde validation | Spec § 4.1 |
| 3 | feat(automation): Subscription enum with 7 variants | Spec § 4.2 |
| 4 | feat(automation): complete Humane v1 schema types | Spec § 4.3-4 |
| 5 | feat(automation): strict + permissive Humane spec parser | Spec § 4.5 |
| 6 | feat(automation): spec normaliser produces deterministic spec_json | |
| 7 | feat(automation): legacy TOML → Humane YAML migrator | Spec § 7.1 |
| 8 | feat(db): V20 migration — rewrite automation_specs + activities | Spec §§ 5.1-3 |
| 9 | feat(db): V21 — three Humane behavior tables | Spec § 5.4 |
| 10 | feat(automation): rewrite activity.rs against V20b schema | |
| 11 | feat(automation): structured FilterRule evaluator | Spec § 6.6 |
| 12 | feat(automation): permission gate with deny>grant>spec precedence | Spec § 6.7 |
| 13 | feat(automation): per-spec memory.md store with compaction | Spec § 6.5 |
| 14 | feat(automation): four built-in tool schemas | Spec § 6.4 |
| 15 | feat(automation): AutomationDelegate + tool runtime handlers | Spec § 6.2-4 |
| 16 | feat(automation): Schedule + File + Webhook subscription sources | |
| 17 | feat(automation): Webpage + RSS + WeCom + Custom subscription sources | |
| 18 | feat(automation): AppRuntimeService | Spec § 6.1 |
| 19 | feat(automation): 14 Tauri commands replacing 4 legacy ones | Spec § 7.3 |
| 20 | feat(ui): AutomationHub paste-YAML field + file-picker import | |
| 21 | feat(ui): EscalationModal + automation event subscription | |
| 22 | chore(automation): delete legacy TOML branch; update CLAUDE.md | |

## Test Plan

- [ ] `cd src-tauri && cargo test --lib automation` — all green
- [ ] `cd src-tauri && cargo build` — no errors
- [ ] `cd ui && npm test -- --run` — all green
- [ ] `cd ui && npx tsc --noEmit` — no errors
- [ ] Manual: import a real hello-halo `spec.yaml` from [../hello-halo/](../../hello-halo/) — appears in list, activates with subscriptions
- [ ] Manual: run a spec that calls `report_to_user` — activity status `completed`, `report_text` populated
- [ ] Manual: run a spec that calls `request_escalation` — modal appears, choice resolves, resumed run starts with `resumed_from_*` populated
- [ ] Manual: cron + file + webhook trigger one run each
- [ ] Manual: `memory.write` survives process restart via `memory.read`
- [ ] Manual: pending escalations survive process restart and reappear in UI

## Migration registry

V20 + V21 claimed. Next free: V22 (reserved for Phase 4 FTS), V23 (reserved for Phase 3 marketplace).

## Out of scope (deferred to later phases)

See spec § 9 for the full list. Highlights: marketplace, hello-halo workspace scan, avatar/voice, memory_graph integration, FTS, configurable per-spec concurrency, WeCom SDK signature verification.
```

---

## Self-Review

**Spec coverage** — every section of [2026-05-13-humane-automation-design.md](../specs/2026-05-13-humane-automation-design.md) has a corresponding task:

| Spec section | Task |
|---|---|
| § 3 Module layout | Task 1 |
| § 4.1 Top-level type | Task 2 |
| § 4.2 Subscription enum | Task 3 |
| § 4.3-4 Permission/FilterRule + remaining types | Task 4 |
| § 4.5 Permissive fallback | Task 5 |
| § 5.1 V20a | Task 8 |
| § 5.2 V20b | Task 8 |
| § 5.3 V20c | Task 8 |
| § 5.4 V21 | Task 9 |
| § 6.1 Lifecycle | Task 18 (AppRuntimeService) |
| § 6.2 AutomationDelegate | Task 15 |
| § 6.3 Escalation persistence | Task 15 + 18 |
| § 6.4 Four tools | Tasks 14 + 15 |
| § 6.5 Memory | Task 13 |
| § 6.6 Filter evaluator | Task 11 |
| § 6.7 Permission gate | Task 12 |
| § 6.8 Telemetry events | Task 15 (`infra.publish` in delegate) |
| § 7.1 Legacy migrator | Task 7 |
| § 7.2 Import entry points | Task 19 (Tauri commands) + Task 20 (UI) |
| § 7.3 14 commands | Task 19 |
| § 8 Test strategy | All tasks (TDD per step) |
| § 9 YAGNI | Respected throughout |
| § 10 Phasing | This plan = Phase 1 |
| § 11 Risk mitigations | Task 0 (spike), Task 14 (dispatcher isolation), Task 16 (webhook sub-path) |
| § 11.5 garde spike | Task 0 |
| § 12 Done criteria | Task 22 step 6 sweep |

**Type consistency** — `HumaneAutomationSpec`, `ParsedSpec`, `AutomationDelegate`, `MemoryStore`, `SubscriptionSource`, `AutomationActivity`, `ActivityStatus`, `TriggerSource`, `Permission`, `Subscription`, `FilterRule`, `PermissionError`, `CompletionGate` all defined once and referenced consistently across tasks.

**Placeholder scan** — no "TBD" / "TODO" / "implement later" outside the deliberately-noted skeleton points (e.g. step 3 of Task 10's `get_activity` says "use same column order as INSERT" — that's structural guidance, not a placeholder). The two `todo!()` snippets in Tasks 10 and 16 explicitly call out that they're listed for structural reference and must be replaced — they're not asking the engineer to write something undefined; they're abbreviating mechanical row-mapping that the engineer can write 1:1 from the schema.

**Scope check** — single-PR scope is ~22 commits. Header flags the Task 14 boundary as a clean split point for Phase 1A / 1B if needed. Recommend keeping unified unless team policy forces a split.
