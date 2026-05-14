# Humane Automation Framework — Phase 1 Design

**Status**: Draft, awaiting user review
**Author**: brainstorming session, 2026-05-13
**Phase**: 1 of 4 (see §10 Phasing)
**Module**: `src-tauri/src/automation/`
**Migrations claimed**: V20a, V20b, V20c, V21
**Next phases reserved**: V22 (FTS), V23 (marketplace)

---

## 1. Background & Goal

uClaw today ships a minimal "automation" module ([src-tauri/src/automation/](../../../src-tauri/src/automation/), 5 files, ~25% of target shape) that supports TOML-defined specs with a single `task` string, three trigger types (Cron / Once / Manual), and a single agent loop per run. There is no notion of subscriptions, filters, escalation, persistent memory, or marketplace distribution.

The `hello-halo` project ([../hello-halo/](../../../../hello-halo/)) has spent significant effort designing an end-to-end autonomous AI application framework: declarative YAML specs, seven trigger source types, a `report_to_user` completion-gate pattern, human-in-the-loop escalation, per-app persistent memory, a permissions model, and a public Digital Human Protocol (DHP) marketplace.

The framework hello-halo built is, despite the marketing name "数字人 / AI Digital Human", **not** a TTS/Avatar/Live2D system. It is a spec-driven autonomous-agent runtime. uClaw's existing `automation/` module is a stripped-down ancestor of the same concept.

**Phase 1 goal**: Port hello-halo's automation framework into uClaw as the **Humane** automation framework — a 100% schema mirror of hello-halo's `AutomationSpec` protocol, a complete runtime that supports all seven subscription sources, filters, escalation, memory, and permissions, plus one-way `hello-halo → uClaw` spec import. Marketplace, local hello-halo workspace scan, and the future avatar/voice "skin" layer are explicitly deferred.

**Naming convention**: We call our internal protocol, types, runtime, and DB values **"Humane"**. We retain "hello-halo" only when referring to the external source project. Examples: `HumaneAutomationSpec`, `spec_format = 'humane-yaml-v1'`, `source = 'humane-workspace'`. UI text in the frontend uses "数字员工" (Chinese) / "AI Worker" (English).

---

## 2. Locked Constraints

The following were locked during brainstorming (2026-05-13) and constrain every design choice below:

1. **Framework-first**: no avatar / TTS / ASR / Live2D in Phase 1. That is a separate roadmap track.
2. **One-way import**: hello-halo `spec.yaml` files must be parseable and runnable in uClaw. Reverse export (uClaw spec → hello-halo) is **not** a Phase 1 goal.
3. **100% schema fidelity**: every field of hello-halo's Zod schema is mirrored, parsed, validated, stored, and round-trippable. Unknown future fields are preserved verbatim in a permissive fallback path.
4. **Marketplace + local workspace scan**: both happen in Phase 3, not Phase 1. Phase 1 ships file-level and paste-level import.
5. **Module name**: `automation` (do not rename; this matches hello-halo's own code module name, preserves migration V7, and avoids churn).
6. **Phase 1 boundary**: full runtime, not parse-and-store-only. All seven subscription sources, filters, escalation, memory, permissions, and the four new built-in tools ship together.
7. **TOML legacy data**: one-shot migrated to Humane YAML in V20, after which the legacy TOML parse branch is **deleted**. No dual-format runtime.
8. **Validation library**: `serde` + `garde` derive. Rust types are the single source of truth. Discriminated unions via `#[serde(tag = "type")]`. Cross-field refinements via `#[garde(custom = ...)]`.
9. **Memory storage**: per-spec `memory.md` file at `~/.uclaw/automation/{spec_id}/memory.md`, mirroring hello-halo. `memory_graph` integration is a separate post–Phase 1 PR.
10. **Permissions**: JSON arrays on the spec table, mirroring hello-halo's `InstalledApp.permissions` shape. Audit history written to V14 `permission_audit_log`. No new permissions table.
11. **Activities table**: rebuilt from scratch in V20b. Old `automation_activities` schema is too thin to carry the new runtime's metrics and resumption chain.

---

## 3. Architecture & Module Layout

```
src-tauri/src/automation/
├── mod.rs                          # re-exports + submodule declarations
├── spec.rs                         # ★ rewritten: HumaneAutomationSpec + subtypes, serde+garde
├── protocol/
│   ├── mod.rs
│   ├── humane_v1.rs                # 100% mirror of hello-halo's Zod schema
│   ├── parse.rs                    # YAML bytes → HumaneAutomationSpec, with field paths in errors
│   ├── normalize.rs                # default-fill, strip i18n, render canonical spec_json
│   └── migrate_toml_v1.rs          # one-shot legacy TOML → Humane YAML migrator
├── manager.rs                      # install / uninstall / list / update / status machine
├── runtime/
│   ├── mod.rs
│   ├── service.rs                  # AppRuntimeService (activate / deactivate / execute)
│   ├── execute.rs                  # single-run executor: inject tools → agent loop → completion gate
│   ├── prompt.rs                   # system_prompt + initial_message construction
│   └── auto_continue.rs            # report_to_user completion gate + 10-retry loop
├── sources/                        # subscription source types
│   ├── mod.rs                      # SubscriptionSource trait + registry
│   ├── schedule.rs                 # cron / every (reuses existing scheduler)
│   ├── file.rs                     # FSEvents watcher (PR #142)
│   ├── webhook.rs                  # plugs into existing axum :27270
│   ├── webpage.rs                  # reuses proactive 30s polling
│   ├── rss.rs                      # same backbone
│   ├── wecom.rs                    # WeCom webhook adapter (Phase 1: webhook sub-path; no WeCom SDK)
│   └── custom.rs                   # plugin extension point (Phase 1 placeholder)
├── tools/                          # new built-in tools (registered in agent/dispatcher.rs)
│   ├── report_to_user.rs           # completion gate + activity row
│   ├── notify_user.rs              # system / WeCom notifications
│   ├── request_escalation.rs       # writes automation_escalations + terminates run
│   └── memory.rs                   # read/write/append/compact memory.md
├── memory/
│   ├── mod.rs
│   ├── store.rs                    # ~/.uclaw/automation/{spec_id}/memory.md
│   └── compact.rs                  # archive strategy mirroring hello-halo
├── activity.rs                     # rebuilt to match new V20b schema
├── filters.rs                      # structured FilterRule evaluator (zero LLM cost)
├── permissions.rs                  # parse + runtime gate
└── importer/
    └── from_humane_workspace.rs    # Phase 3 placeholder
```

**Change scope**: of the 5 existing files, 1 is preserved (`activity.rs`, rebuilt internally), 4 are deleted or rewritten. New files: ~20, all <300 LOC, no single-file monolith. Total new Rust LOC estimate: ~3000.

**Why extend `automation/` in place** (vs. parallel `halo/` module): matches CLAUDE.md's "flat enumeration over generic dispatchers" guidance, single source of truth, one PR, bisect-friendly. The alternative (sibling `halo/` module + later swap) doubles code volume and splits V20 into two migrations.

---

## 4. Data Model — Rust Types

### 4.1 Top-level spec

```rust
// src-tauri/src/automation/protocol/humane_v1.rs

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HumaneAutomationSpec {
    #[serde(rename = "type")]
    #[garde(custom(must_be_automation))]
    pub kind: String,                          // must equal "automation"

    #[garde(length(min = 1, max = 100))]
    pub name: String,
    #[garde(pattern("^\\d+\\.\\d+\\.\\d+"))]
    pub version: String,
    #[garde(length(min = 1, max = 100))]
    pub author: String,
    #[garde(length(min = 1, max = 500))]
    pub description: String,
    #[garde(length(min = 1))]
    pub system_prompt: String,

    #[garde(dive)]
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[garde(dive)]
    #[serde(default)]
    pub config_schema: Vec<InputDef>,
    #[garde(dive)]
    #[serde(default)]
    pub requires: Requires,
    #[garde(dive)]
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    #[garde(dive)]
    pub memory_schema: Option<MemorySchema>,
    #[garde(dive)]
    pub output: Option<OutputConfig>,
    #[garde(dive)]
    pub escalation: Option<EscalationConfig>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[garde(dive)]
    #[serde(default)]
    pub browser_login: Vec<BrowserLoginEntry>,
    #[serde(default)]
    pub i18n: HashMap<String, I18nLocaleBlock>,
}
```

### 4.2 Subscription discriminated union

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Subscription {
    Schedule(ScheduleSubscription),   // every: "30m" | cron: "0 8 * * *"
    File(FileSubscription),           // pattern: "src/**/*.ts"
    Webhook(WebhookSubscription),     // path + secret
    Webpage(WebpageSubscription),     // url + selector
    Rss(RssSubscription),             // url
    Wecom(WecomSubscription),         // chatId?
    Custom(CustomSubscription),       // provider + key + free-form config
}
```

### 4.3 Permission enum

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    AiBrowser,
    Notification,
    Filesystem,
    Network,
    Shell,
    #[serde(other)]
    Unknown,                          // future hello-halo permissions: stored, runtime denies
}
```

### 4.4 FilterRule

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FilterRule {
    pub field: String,                // JSON Pointer into trigger context: "/event/branch"
    #[garde(custom(valid_op))]
    pub op: String,                   // eq | ne | contains | matches | gt | lt
    pub value: serde_json::Value,
}
```

`InputDef`, `MemorySchema`, `OutputConfig`, `EscalationConfig`, `BrowserLoginEntry`, `Requires`, `I18nLocaleBlock` mirror hello-halo's Zod schema 1:1; each is <30 LOC and lives in `humane_v1.rs`.

### 4.5 Permissive fallback (unknown fields)

Default parse is `#[serde(deny_unknown_fields)]`. On failure, `parse.rs` retries with a tolerant deserializer that:

1. Captures unknown top-level fields into `extra_fields: HashMap<String, Value>`.
2. Stores the row but flags `status = 'needs_review'`.
3. Surfaces a UI warning: "Spec contains Humane-protocol fields not yet recognised by this uClaw version — values preserved."

This satisfies constraint #3 (100% fidelity even for protocol versions newer than ours).

---

## 5. Data Model — DB Schema

### 5.1 V20a — rewrite `automation_specs`

```sql
CREATE TABLE automation_specs_new (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    version             TEXT NOT NULL,
    author              TEXT NOT NULL,
    description         TEXT NOT NULL,
    system_prompt       TEXT NOT NULL,

    spec_format         TEXT NOT NULL DEFAULT 'humane-yaml-v1',
    spec_yaml           TEXT NOT NULL,        -- raw original YAML (round-trip truth)
    spec_json           TEXT NOT NULL,        -- normalized JSON (queries / FTS)

    user_config_values  TEXT NOT NULL DEFAULT '{}',
    permissions_granted TEXT NOT NULL DEFAULT '[]',
    permissions_denied  TEXT NOT NULL DEFAULT '[]',

    status              TEXT NOT NULL DEFAULT 'active',
                        -- active | paused | error | needs_login | waiting_user | needs_review
    enabled             INTEGER NOT NULL DEFAULT 1,
    space_id            TEXT,                 -- FK spaces; NULL = global

    source              TEXT NOT NULL DEFAULT 'local',
                        -- local | marketplace | humane-workspace | toml-migrated
    source_ref          TEXT,                 -- URI by convention (see §5.3)
    source_version      TEXT,                 -- semver or ETag for re-sync (Phase 3)

    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    last_run_at         INTEGER,
    last_run_outcome    TEXT                  -- useful | noop | error | skipped
);
CREATE INDEX idx_specs_status    ON automation_specs_new(status);
CREATE INDEX idx_specs_space     ON automation_specs_new(space_id);
CREATE INDEX idx_specs_enabled   ON automation_specs_new(enabled);
CREATE INDEX idx_specs_source    ON automation_specs_new(source, source_version);
```

Migration: legacy `automation_specs.toml_content` is fed through `migrate_toml_v1` (§7.1) to produce equivalent `spec_yaml + spec_json`. Rows where migration fails get `status = 'error'` plus an audit-log entry; boot continues.

### 5.2 V20b — rewrite `automation_activities`

```sql
CREATE TABLE automation_activities_new (
    id                        TEXT PRIMARY KEY,
    spec_id                   TEXT NOT NULL REFERENCES automation_specs(id) ON DELETE CASCADE,
    subscription_id           TEXT REFERENCES automation_subscriptions(id) ON DELETE SET NULL,

    trigger_source_type       TEXT NOT NULL,                -- schedule|file|webhook|webpage|rss|wecom|custom|manual
    trigger_payload_json      TEXT NOT NULL DEFAULT '{}',

    status                    TEXT NOT NULL,                -- queued|running|completed|failed|cancelled|waiting_user
    error_text                TEXT,

    queued_at                 INTEGER NOT NULL,
    started_at                INTEGER,
    completed_at              INTEGER,
    duration_ms               INTEGER,

    llm_iterations            INTEGER NOT NULL DEFAULT 0,
    llm_tokens_in             INTEGER NOT NULL DEFAULT 0,
    llm_tokens_out            INTEGER NOT NULL DEFAULT 0,
    -- per-LLM-call cost continues to live in V13 cost_records

    tool_calls_json           TEXT NOT NULL DEFAULT '[]',

    report_text               TEXT,
    report_outcome            TEXT,
    escalation_id             TEXT REFERENCES automation_escalations(id) ON DELETE SET NULL,

    resumed_from_activity_id   TEXT REFERENCES automation_activities(id) ON DELETE SET NULL,
    resumed_from_escalation_id TEXT REFERENCES automation_escalations(id) ON DELETE SET NULL
);
CREATE INDEX idx_act_spec      ON automation_activities_new(spec_id);
CREATE INDEX idx_act_status    ON automation_activities_new(status);
CREATE INDEX idx_act_queued_at ON automation_activities_new(queued_at DESC);
CREATE INDEX idx_act_resumed   ON automation_activities_new(resumed_from_activity_id);
CREATE INDEX idx_act_sub       ON automation_activities_new(subscription_id);
```

Migration: legacy rows mapped — `trigger:String` → `trigger_source_type` (heuristic: `"manual"` / `"cron"` / fallback `"manual"`); `created_at` → `queued_at`; `result` → `report_text` plus `report_outcome = 'useful'` if status was `Completed`; the unused `run_id` column is dropped.

### 5.3 V20c — post-migration data fixup

- Set `source = 'toml-migrated'` and `source_ref = NULL` for all rows produced by the V20a migrator.
- Generate equivalent Humane YAML via `migrate_toml_v1` and write to `spec_yaml + spec_json`.
- Idempotent: re-running V20c is a no-op (checks `source_format` and `source` before writing).

### 5.4 V21 — behavior tables

```sql
CREATE TABLE automation_subscriptions (
    id            TEXT PRIMARY KEY,
    spec_id       TEXT NOT NULL REFERENCES automation_specs(id) ON DELETE CASCADE,
    source_type   TEXT NOT NULL,                          -- schedule|file|webhook|webpage|rss|wecom|custom
    config_json   TEXT NOT NULL,                          -- Subscription enum payload
    enabled       INTEGER NOT NULL DEFAULT 1,
    last_fired_at INTEGER,
    created_at    INTEGER NOT NULL
);
CREATE INDEX idx_sub_spec        ON automation_subscriptions(spec_id);
CREATE INDEX idx_sub_source_type ON automation_subscriptions(source_type);

CREATE TABLE automation_memory (
    spec_id                 TEXT PRIMARY KEY REFERENCES automation_specs(id) ON DELETE CASCADE,
    last_updated_at         INTEGER NOT NULL,
    compacted_archives_json TEXT NOT NULL DEFAULT '[]',   -- [{archived_at, path, size}]
    bytes                   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE automation_escalations (
    id           TEXT PRIMARY KEY,
    spec_id      TEXT NOT NULL REFERENCES automation_specs(id) ON DELETE CASCADE,
    activity_id  TEXT NOT NULL REFERENCES automation_activities(id) ON DELETE CASCADE,
    question     TEXT NOT NULL,
    choices_json TEXT NOT NULL,                          -- [{id, label, description?}]
    status       TEXT NOT NULL DEFAULT 'waiting',         -- waiting | resolved | cancelled | expired
    user_choice  TEXT,
    user_note    TEXT,
    created_at   INTEGER NOT NULL,
    responded_at INTEGER
);
CREATE INDEX idx_escalation_spec   ON automation_escalations(spec_id);
CREATE INDEX idx_escalation_status ON automation_escalations(status);
```

### 5.5 `source_ref` URI conventions

| `source` value | `source_ref` format | Example |
|---|---|---|
| `local` | filesystem path or `NULL` | `/Users/ryan/spec.yaml` |
| `marketplace` | `marketplace://{registry_id}/{item_id}` | `marketplace://official/github-pr-monitor` |
| `humane-workspace` | `humane://{workspace_path}/{app_id}` | `humane:///Users/ryan/.halo/apps/abc-123` |
| `toml-migrated` | `NULL` | — |

`source_version` holds a semver or ETag; populated by Phase 3 marketplace sync.

### 5.6 Migration registry update (CLAUDE.md)

| V | What | Status |
|---|---|---|
| V20 | rewrite automation_specs (V20a) + activities (V20b) + data fixup (V20c) | **this PR** |
| V21 | automation_subscriptions + automation_memory + automation_escalations | **this PR** |
| V22 | FTS5 over automation_specs (name + description + system_prompt) | Phase 4 |
| V23 | automation_registries + automation_marketplace_items | Phase 3 |

---

## 6. Runtime Flow

### 6.1 End-to-end lifecycle

```
INSTALL  (manager.rs)
  input: spec_yaml: String, source: Source
  ├─ parse_humane_v1(spec_yaml)          → HumaneAutomationSpec (garde-validated)
  ├─ normalize()                          → spec_json + extract subscriptions[]
  ├─ INSERT automation_specs              (spec_yaml + spec_json + flat columns)
  ├─ INSERT automation_subscriptions      (one row per subscription)
  ├─ ensure_dir(~/.uclaw/automation/{spec_id}/)
  ├─ INSERT automation_memory             (bytes = 0)
  └─ emit InfraEvent::AutomationInstalled{spec_id}

ACTIVATE  (runtime/service.rs::activate)
  input: spec_id
  ├─ load spec + subscriptions
  ├─ verify requires.mcps & requires.skills are present; else status='needs_login', exit
  ├─ for each subscription:
  │     Schedule → register_cron(spec_id, sub_id, expr)
  │     File     → fs_watcher.watch(spec_id, sub_id, pattern)
  │     Webhook  → axum_router.add(/automation/webhook/{spec_id}/{sub_id}/{user_path}, secret)
  │     Webpage  → proactive.attach_poller(spec_id, sub_id, url, selector)
  │     Rss      → same backbone
  │     Wecom    → axum_router.add(/automation/webhook/wecom/{spec_id}/{sub_id}, signature_header?)
  │                  — Phase 1 implements wecom as a specialised webhook sub-path; no WeCom SDK
  │     Custom   → custom_registry.attach(spec_id, sub_id, provider, key, config)
  ├─ keep_alive.register(spec_id)
  └─ emit InfraEvent::AutomationActivated{spec_id}

TRIGGER → RUN  (runtime/execute.rs::execute_run)
  input: spec_id, sub_id?, trigger_payload: Value
  ├─ load spec; check status == 'active'
  ├─ filters.evaluate(trigger_payload)    → zero-LLM pre-filter; on fail record 'skipped', exit
  ├─ semaphore.acquire(per_spec_max_concurrent = 2)
  ├─ INSERT automation_activities (status='queued')
  ├─ build ReasoningContext:
  │     system_prompt     = spec.system_prompt + persona_footer
  │     initial_message   = render_trigger_context(sub, trigger_payload, user_config)
  │     resumption_block  = if prev_escalation: render_escalation_resolution(...)
  │     tools             = builtin + spec.requires.mcps + spec.requires.skills (filtered by skill_tags)
  ├─ delegate = AutomationDelegate { spec_id, activity_id, permission_set, emit_event }
  ├─ UPDATE activity status='running', started_at=now
  ├─ run_agentic_loop(&delegate, &mut ctx, AutoContinueConfig { max_retries: 10 })
  ├─ completion-gate verdict:
  │     report_to_user called      → outcome = report.outcome, status='completed'
  │     request_escalation called  → status='waiting_user', spec.status='waiting_user'
  │     10 retries no report       → status='failed', error='no_report'
  │     LLM/tool error             → status='failed', error_text recorded
  └─ semaphore.release()

ESCALATION RESOLVED  (manager.rs::resolve_escalation)
  input: escalation_id, choice: String, note?: String
  ├─ UPDATE automation_escalations SET status='resolved', user_choice, user_note, responded_at
  ├─ UPDATE automation_specs SET status='active' WHERE id = spec_id
  └─ spawn new run with prev_escalation_id (Humane pattern: escalation = new run + injected context)
```

### 6.2 `AutomationDelegate` relationship to the agent loop

A new delegate, **not** a reuse of [agent/dispatcher.rs](../../../src-tauri/src/agent/dispatcher.rs)'s `ChatDelegate`:

- `ChatDelegate` assumes interactive sessions (user-message replay, streamed UI deltas, SafetyManager pop-ups).
- `AutomationDelegate` runs a stateless completion-gate loop with no user, no SafetyManager prompts (permissions are pre-granted at install), and the four new tools instead of `ask_user`.

**Shared** between the two delegates:
- [agent/agentic_loop.rs::run_agentic_loop](../../../src-tauri/src/agent/agentic_loop.rs)
- Built-in tools `edit / file / search / shell / web` (reused, gated by `permissions`)
- LLM streaming and [llm/stream_error.rs](../../../src-tauri/src/llm/stream_error.rs) classification

The four new tools (§6.4) are registered in [agent/dispatcher.rs](../../../src-tauri/src/agent/dispatcher.rs) but their schemas are only exposed under the `AutomationDelegate` code path — interactive chat does not see them.

### 6.3 Escalation distinguishes from `PendingApprovals`

[SafetyManager](../../../src-tauri/src/safety/)'s `PendingApprovals` is an in-process oneshot-channel map — process restart loses pending state. Automation escalations must survive restart, so they live in SQLite (`automation_escalations`) and flow through `InfraService` events. The two systems are kept orthogonal.

### 6.4 Built-in tool contracts

#### `report_to_user` — completion gate

```jsonc
{
  "name": "report_to_user",
  "input_schema": {
    "text":      "string",
    "outcome":   { "enum": ["useful", "noop", "error", "skipped"] },
    "artifacts": { "type": "array", "items": { "$ref": "#/defs/Artifact" }, "optional": true }
  }
}
```

Sole completion signal. Calling it terminates the auto-continue loop, sets `activity.status = 'completed'`, populates `report_text + report_outcome`, emits `InfraEvent::AutomationRunReported`. No permission gate.

#### `notify_user` — side-channel notification

```jsonc
{
  "name": "notify_user",
  "input_schema": {
    "channels": { "type": "array", "items": { "enum": ["system", "wecom", "email"] } },
    "title":    "string",
    "body":     "string",
    "level":    { "enum": ["info", "important", "critical"] }
  }
}
```

Routes per `spec.output` and global user preferences. **Does not** mark run complete; `report_to_user` is still required. Permission gate: `Permission::Notification`. Phase 1 supports `system` + `wecom`; `email` parsed but runtime returns `unsupported_channel`.

#### `request_escalation` — human-in-the-loop

```jsonc
{
  "name": "request_escalation",
  "input_schema": {
    "question":         "string",
    "choices":          { "type": "array", "minItems": 2, "items": {"id": "string", "label": "string", "description": "string?"} },
    "context_for_user": "string?"
  }
}
```

On invocation: writes `automation_escalations(status='waiting')`, emits `InfraEvent::AutomationEscalationRaised`, **immediately terminates** the run. spec status flips to `waiting_user`. Resolution happens via the `resolve_escalation` Tauri command, which spawns a new run with the escalation context injected.

#### `memory` — per-spec persistent storage

```jsonc
{
  "name": "memory",
  "input_schema": {
    "op":      { "enum": ["read", "write", "append", "compact"] },
    "content": "string?"
  }
}
```

Backed by `~/.uclaw/automation/{spec_id}/memory.md` (see §6.5). No LLM token cost; each op logged to `tool_calls_json`. No permission gate (memory is treated as private spec data).

### 6.5 Memory contract

| op | semantics |
|---|---|
| `read` | returns full file content; empty string if file does not exist |
| `write` | overwrites file; updates `automation_memory.bytes / last_updated_at` |
| `append` | appends; same metadata update |
| `compact` | moves current content to `archives/{ISO8601}.md`, adds entry to `compacted_archives_json`, truncates main file |

Concurrency: each spec has a process-wide async mutex on its memory file (in `MemoryStore`) to prevent torn writes between concurrent runs.

### 6.6 Filter evaluator

[filters.rs](../../../src-tauri/src/automation/filters.rs):

```rust
pub fn evaluate(rules: &[FilterRule], ctx: &Value) -> bool {
    rules.iter().all(|r| match r.op.as_str() {
        "eq"       => ctx.pointer(&r.field) == Some(&r.value),
        "ne"       => ctx.pointer(&r.field) != Some(&r.value),
        "contains" => /* string contains */,
        "matches"  => /* regex compiled lazily, cached per (spec_id, rule_idx) */,
        "gt" | "lt"=> /* numeric compare with NaN-rejection */,
        _          => false,
    })
}
```

Path access uses JSON Pointer (`/event/branch`). Operator set is closed at 6 — **no custom DSL**, deliberately. Matches hello-halo's stance.

### 6.7 Permission gate

[permissions.rs](../../../src-tauri/src/automation/permissions.rs):

```rust
pub fn check(
    spec_perms: &[Permission],
    granted: &[Permission],
    denied: &[Permission],
    tool_name: &str,
) -> Result<(), PermissionError> {
    let required = required_permission_for(tool_name);
    if denied.contains(&required) { return Err(Denied); }
    if granted.contains(&required) || spec_perms.contains(&required) { return Ok(()); }
    Err(NotGranted)
}
```

`required_permission_for` is a static map in code, not in the spec:

| Tool | Required permission |
|---|---|
| `shell` | `Shell` |
| `edit` / `file` | `Filesystem` |
| `web` | `Network` |
| `notify_user` | `Notification` |
| `browser_*` (any) | `AiBrowser` |
| `memory` / `report_to_user` / `request_escalation` | (none — always allowed) |

### 6.8 Streaming telemetry events

| Event | When emitted |
|---|---|
| `AutomationRunStarted{spec_id, activity_id, trigger}` | after filters pass, semaphore acquired |
| `AutomationToolCall{activity_id, tool_name, args_redacted}` | each tool invocation |
| `AutomationLlmChunk{activity_id, delta}` | streamed LLM chunks (opt-in: emitted only if any subscriber exists) |
| `AutomationRunReported{activity_id, outcome, text}` | `report_to_user` called |
| `AutomationEscalationRaised{escalation_id, spec_id, question, choices}` | `request_escalation` called |
| `AutomationRunFinished{activity_id, status}` | run terminates (any reason) |

Phase 1 frontend must subscribe to `AutomationEscalationRaised` (modal); the rest are opt-in optimizations.

---

## 7. Migration & Import

### 7.1 Legacy TOML → Humane YAML migrator

[automation/protocol/migrate_toml_v1.rs](../../../src-tauri/src/automation/protocol/migrate_toml_v1.rs):

```rust
fn migrate_legacy_toml(toml_content: &str) -> Result<MigratedSpec, MigrateError> {
    let legacy: LegacyTomlSpec = toml::from_str(toml_content)?;

    let subscriptions = match legacy.trigger {
        LegacyTrigger::Cron(expr) => vec![Subscription::Schedule(ScheduleSubscription {
            cron: Some(expr), every: None,
        })],
        LegacyTrigger::Once(_) => vec![],   // Once is past-meaningless; drop with warning
        LegacyTrigger::Manual  => vec![],   // manual-only = no subscriptions
    };

    let spec = HumaneAutomationSpec {
        kind: "automation".into(),
        name: legacy.name,
        version: "0.0.0".into(),
        author: "uclaw-migrated".into(),
        description: legacy.description.unwrap_or_else(|| "Migrated from TOML v1".into()),
        system_prompt: legacy.task,           // task → system_prompt
        subscriptions,
        config_schema: vec![],
        requires: Requires::default(),
        filters: vec![],
        memory_schema: None,
        output: None,
        escalation: None,
        permissions: vec![],                  // empty = conservative default
        browser_login: vec![],
        i18n: HashMap::new(),
    };

    let yaml = serde_yaml::to_string(&spec)?;
    Ok(MigratedSpec { spec, yaml, original_toml: toml_content.to_string() })
}
```

V20c iterates legacy rows, runs the migrator, writes `spec_yaml + spec_json`. Failures: mark row `status = 'error'`, write `permission_audit_log` entry, continue. The legacy TOML parse branch is deleted from the source tree immediately after V20c lands — no dual-format runtime.

### 7.2 Phase 1 import entry points

| Tauri command | Purpose |
|---|---|
| `install_humane_spec(yaml: String, source_ref?: String)` | install from raw YAML string; returns `HumaneSpecRow` |
| `import_humane_spec_file(path: String)` | file-picker flow → reads file → calls `install_humane_spec` |

Phase 3 will add: `scan_humane_workspace(path)`, `add_automation_registry`, `sync_automation_registry`, `install_from_marketplace`.

The legacy `install_automation(toml_content)` command is **deleted** in Phase 1. The frontend [AutomationHub.tsx](../../../ui/src/components/automation/AutomationHub.tsx) "paste TOML" text area is rewritten to "paste Humane YAML" at the same position.

### 7.3 Complete Phase 1 Tauri command surface

| Command | Input | Notes |
|---|---|---|
| `install_humane_spec` | `yaml`, `source_ref?` | new install path |
| `import_humane_spec_file` | `path` | reads file → install |
| `list_automations` | — | preserved; response shape upgraded |
| `get_automation_spec` | `spec_id` | **new**; returns spec detail including round-trip YAML |
| `update_user_config` | `spec_id`, `values: Value` | edits `user_config_values` |
| `set_automation_permission` | `spec_id`, `permission`, `granted: bool` | also writes V14 audit |
| `set_automation_enabled` | `spec_id`, `enabled: bool` | pause / resume |
| `uninstall_automation` | `spec_id` | CASCADE deletes activities / subscriptions / memory |
| `trigger_automation_manual` | `spec_id` | preserved |
| `get_automation_activity` | `spec_id`, `limit?` | preserved; response shape upgraded |
| `resolve_escalation` | `escalation_id`, `choice`, `note?` | **new**; spawns resumed run |
| `list_pending_escalations` | `spec_id?` | **new**; frontend pulls on startup |
| `read_automation_memory` | `spec_id` | **new**; UI viewer |
| `compact_automation_memory` | `spec_id` | **new**; manual archive |

All 14 must be added to the `invoke_handler!` macro in [main.rs](../../../src-tauri/src/main.rs).

---

## 8. Testing Strategy

### 8.1 Rust unit tests (inline `#[cfg(test)]`, no integration dir)

| Module | Focus |
|---|---|
| `protocol/humane_v1.rs` | serde+garde round-trip: `parse(yaml) → spec → serialize → parse` is idempotent |
| `protocol/humane_v1.rs` | All 7 subscription discriminators: 1 happy + 1 invalid each |
| `protocol/parse.rs` | Error locations: missing field, wrong type, unknown field — all include path |
| `protocol/migrate_toml_v1.rs` | Cron / Once / Manual cases — verify round-trip YAML |
| `filters.rs` | 6 operators × happy/false × JSON-Pointer paths |
| `runtime/auto_continue.rs` | no `report_to_user` → fails after 10 retries |
| `runtime/execute.rs` | escalation immediately terminates; spec.status flips to waiting_user |
| `memory/store.rs` | read / write / append / compact correctness |
| `memory/compact.rs` | archive paths, `bytes` and `compacted_archives_json` stay in sync |
| `permissions.rs` | denied > granted > spec.permissions precedence (table-driven) |
| `sources/schedule.rs` | cron parsing + mock-clock-driven firing |
| `db/migrations.rs` | V20a/b/c are idempotent on DB with existing legacy rows |

### 8.2 Fixtures directory

`src-tauri/src/automation/protocol/test_fixtures/`:

- `valid/simple.yaml`
- `valid/full_featured.yaml`
- `valid/all_subscription_types.yaml`
- `valid/humane_real_examples/*.yaml` — 3-5 real spec files imported from [../hello-halo/src/main/apps/runtime/](../../../../hello-halo/src/main/apps/runtime/) (with attribution comments)
- `invalid/missing_name.yaml`
- `invalid/bad_subscription.yaml`
- `invalid/unknown_field.yaml` (verifies the permissive-fallback path)

### 8.3 TS unit tests (Vitest + jsdom)

- `AutomationHub.test.tsx` — list / detail / escalation modal renders
- `useAutomationEvents.test.ts` — InfraEvent subscription → atom updates
- `EscalationModal.test.tsx` — choice click → `resolve_escalation` invocation

### 8.4 Manual verification checklist

To run after each Phase 1 PR commit:

1. Copy a real `spec.yaml` from hello-halo → `import_humane_spec_file` → spec appears in list with `source = 'local'`.
2. Manually trigger → activity row appears → AI calls `report_to_user` → status `completed`.
3. Install a spec that only calls `request_escalation` → modal pops → user picks choice → new run starts with `resumed_from_escalation_id` populated.
4. cron + file subscriptions each fire correctly.
5. webhook spec: `curl -X POST http://127.0.0.1:27270/automation/webhook/{spec_id}/{sub_id}/{user_path}` triggers a run.
6. `memory.write` → restart process → `memory.read` returns the written content.
7. After restart, any escalations in `status='waiting'` remain visible in the UI.
8. Run V20 migration against a DB containing a legacy TOML row → row appears in new schema with `source = 'toml-migrated'` and equivalent YAML in `spec_yaml`.

---

## 9. YAGNI — Explicitly Out of Scope for Phase 1

To prevent scope creep, the following are **deferred**:

| Deferred item | Lands in |
|---|---|
| Avatar / TTS / ASR / Live2D / VRM | independent track (Phase 6+, separate PRD) |
| Marketplace registry / sync / store UI | Phase 3 |
| Local hello-halo workspace bulk import | Phase 3 |
| Reverse export uClaw spec → hello-halo (bidirectional) | not planned |
| `memory_graph` integration | post-Phase-1 separate PR (paths kept stable to enable this) |
| Spec store UI (browse / category / rating) | Phase 4 |
| `output.email` / Slack channels | Phase 1 stores config, runtime returns `unsupported_channel` |
| `escalation.timeout` auto-expire | Phase 2 (cron sweeps waiting > N) |
| Per-spec LLM provider override (`modelSourceId`) | Phase 1 stores it, runtime ignores it (uses global) |
| Runtime i18n switching | Phase 1 stores `i18n` block, UI reads `default` locale only |
| Automatic `browser_login` detection | Phase 1 only declares; user manually marks the requirement satisfied |
| Configurable per-spec concurrency (`max_concurrent`) | Phase 2 (Phase 1 hard-codes 2) |
| WeCom SDK / real WeCom callback signature verification | Phase 2 — Phase 1 implements wecom as a webhook sub-path with optional signature header pass-through (no SDK dependency). 100% schema fidelity preserved (enum variant, parsing, storage). |
| DHP marketplace registry pre-flight investigation | Pre-Phase-3 sub-task: verify `openkursar/digital-human-protocol` actually exposes a fetchable registry / has stable schema / has ≥5 real specs. One-hour Explore-agent task. |

---

## 10. Phasing & Migration Plan

Re-sequenced 2026-05-14 after Phase 1 merged; updated again 2026-05-14 after
Phase 3a shipped and the **Kaleidoscope surface** absorbed the automation /
marketplace UI (see §10.1 Frontend surfacing).

| Phase | Scope | Migrations |
|---|---|---|
| **1 (merged)** | Humane spec model + full runtime + file/paste import | V20a, V20b, V20c, V21 |
| **3a (merged — Marketplace UI Port)** | hello-halo store UI ported: `StoreHeader` (search + type + category filters) + `StoreCard`/`StoreGrid` + `StoreDetail` (overview / config / deps / prompts) + `InstallWizard` (scope → config → confirm). Backend local SQLite cache (V23 partial — `automation_marketplace_items` + `registry_sync_state`), `get_marketplace_detail` / paged `query_marketplace` / `check_marketplace_updates`. Single-registry (DHP). Followed by the marketplace-i18n + stable-category-counts fixes. **Surfacing was then superseded** by the Kaleidoscope migration — see §10.1. | V23 (partial) |
| 2 | Hardening: timeouts, configurable concurrency, `memory_graph` indexer, `escalation.timeout` auto-expire, WeCom signature, InfraEvent enum extension | (none expected) |
| 3b (Marketplace completion) | Multi-registry config (5 built-ins + user-added) + proxy adapters (Smithery / MCP Registry / SkillHub) + local hello-halo workspace scan | V23 (complete: `automation_registries` table) |
| 4 (FTS + remaining UI) | FTS over `automation_specs` (escalations / specs / activities) + remaining minor UI polish | V22 |
| 6+ | Optional avatar/voice "skin" layer | separate roadmap |

The 3a slot was created by user demand on 2026-05-14 after Phase 1 surfaced
that the original `MarketplaceModal` was visually inadequate compared to
hello-halo's StoreView. See `docs/superpowers/specs/2026-05-14-marketplace-ui-port-design.md`.

### 10.1 Frontend surfacing — Kaleidoscope migration (2026-05-14)

The 数字人 / 应用商店 / 我的应用 views were **originally** planned (Phase 3a) as a
chat-window 3-tab AppsPage rendered by `AutomationsView` (`automationPanelOpenAtom`).
After Phase 3a shipped, the **Kaleidoscope surface** — a second top-level surface
parallel to the chat/agent workspace — absorbed them. The 3 tabs became 3 separate
rail modules in the Kaleidoscope shell:

- `HumansModule` → `AutomationHub` （数字人）
- `StoreModule` → `StoreView` / `StoreDetail` （应用商店）
- `AppsModule` → `AppsTab` （我的应用）

`AutomationsView` + `automationPanelOpenAtom` are **retired/deleted**; the chat-window
LeftSidebar "Automations" entry now opens the Kaleidoscope surface (`topLevelViewAtom`
= `'kaleidoscope'`, `kaleidoscopeModuleAtom` = `'humans'`). The marketplace UI
components themselves (`StoreView` / `StoreHeader` / `StoreCard`/`Grid` / `StoreDetail`
/ `InstallWizard`) are **unchanged** — only their mount point moved.

**The backend Humane runtime / protocol / DB schema is entirely unaffected** by this
migration — it was purely a frontend re-surfacing. Anything below (§2–§9, §11–§13)
describing the runtime, spec model, tools, and tables still holds verbatim.

See `docs/superpowers/specs/2026-05-14-kaleidoscope-design.md` (esp. §4.1, §7.1–§7.3,
§10.1) and `docs/superpowers/plans/2026-05-14-kaleidoscope-phase1.md` +
`...-phase1-fixes.md`.

---

## 11. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| V20 multi-step migration fails on a real DB → boot fails | Each substep in a transaction; on failure, rollback + log + continue rather than panic; provide `--reset-automation` dev flag |
| `garde` cross-field refinements too cumbersome | Spike the 2 hardest refinements (`escalation.choices ≥ 2`, `subscription.type + config` cross-check) at the start of Phase 1; fallback to hand-rolled `Validate` if blocked |
| Single PR with ~8-10 commits is heavy to review | Strict per-commit independence (each commit compiles & tests); PR description includes a `## Commits (bisectable)` table per CLAUDE.md precedent (#29, #31, #33, #35, #36) |
| 4 new tools further bloat [agent/dispatcher.rs](../../../src-tauri/src/agent/dispatcher.rs) (already 87KB) | Tool implementations live under `automation/tools/`; dispatcher.rs only gains a single `register_automation_tools()` call site |
| Webhook source on `:27270` collides with existing axum routes | Use sub-path `/automation/webhook/{spec_id}/{sub_id}/{user_path}` — orthogonal to existing chat/agent routes |
| hello-halo specs containing protocol fields uClaw doesn't yet recognise → `deny_unknown_fields` fails | Permissive-fallback parse (§4.5): preserve in `extra_fields`, flag `status = 'needs_review'`, UI warns |

---

## 11.5 Pre-implementation Spike — `garde` Viability

Because `serde + garde` underpins every Rust type in this design, validating its viability before writing 10+ types is cheap insurance. The spike is **Task 0** in the writing-plans output.

**Targets** — verify the 4 hardest refinements can be expressed:

1. `escalation.choices.length >= 2` — array-length constraint on a nested optional field.
2. `subscription.type + payload` cross-variant validation — within a discriminated union, ensure the payload struct matches the discriminator (mostly handled by `#[serde(tag)]`, but custom refinement validates e.g. webhook `path` regex when type is `webhook`).
3. `config_schema[].default` type matches `config_schema[].type` — list-item internal cross-field constraint.
4. Error-path readability — `spec.subscriptions[2].config.cron` rather than just `subscriptions[2]`.

**Decision gates**:

| Outcome | Action |
|---|---|
| All 4 expressible cleanly | Adopt `garde` for the full Phase 1 |
| 1–2 hard cases require ad-hoc work | Hybrid: `garde` derive for simple fields; hand-rolled `Validate` trait for the 2 hard struct(s) |
| 3+ fail or error paths are unreadable | Full fallback to hand-rolled `Validate` trait across all types |

**Budget**: half a day. Hard timeout — picks the most conservative viable option at the deadline. Spike code lives at `src-tauri/src/automation/protocol/spike/` and is deleted before the migration commit.

---

## 12. Done Criteria

Phase 1 is complete when:

1. All V20a/V20b/V20c/V21 migrations apply cleanly on a dev DB with legacy TOML data; legacy data is preserved as equivalent Humane YAML.
2. Importing a real hello-halo `spec.yaml` succeeds and the spec activates with all its subscriptions registered.
3. A run that calls `report_to_user` completes successfully and writes a full activity row.
4. A run that calls `request_escalation` pauses; UI modal appears; user resolution spawns a resumed run with `resumed_from_*` populated.
5. Each subscription source type (schedule / file / webhook / webpage / rss / wecom / custom) has at least one passing manual verification.
6. All 14 Tauri commands are reachable from the frontend.
7. The Rust unit-test suite covers each module in §8.1.
8. The CLAUDE.md migration registry is updated to V21.
9. The PR description contains a `## Commits (bisectable)` table.

---

## 13. Open Questions

None at design-doc finalization — all twelve clarifying questions during brainstorming were resolved. Implementation planning (next step via `writing-plans`) will surface task-level uncertainties.
