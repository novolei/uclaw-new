# Automation Phase 2a — 打通执行墙 (Design)

> Status: approved (brainstorming complete, refined via grill-me — 10 design-tree branches)
> Date: 2026-05-14
> Next: `superpowers:writing-plans`

## Goal

Wire `execute_run` to actually invoke the agent loop. Today uClaw's automation runtime is ~80% built — spec/manager/sources/scheduler all run in the background — but every run dead-ends at the `deferred_phase_2` stub in `automation/runtime/service.rs` and never calls `run_agentic_loop`. Phase 2a breaks that execution wall: a triggered run executes a real agent loop, remembers, produces artifacts, reports back, and is cost-bounded.

Phase 2a also establishes the **run ownership model** ("Run = Session + provenance", a.k.a. Option 3): an automation run *is* an `agent_session`, so uClaw's entire agent-session substrate (transcript storage, FTS, the Agent view, retention) is reused instead of re-built.

## Background

uClaw's `automation/` module is a Rust port of hello-halo's `src/main/apps/` layer — same Zod-derived spec schema, same "Humane v1" protocol, same DHP marketplace. halo is Electron and runs its agent loop via the Claude Code SDK subprocess; **uClaw runs a pure-Rust agent loop**, so halo's loop wiring cannot be copy-ported — it must be re-implemented against uClaw's `LoopDelegate` trait.

Relevant existing surfaces:

- `agent/agentic_loop.rs` — `run_agentic_loop(delegate, reason_ctx, config) -> LoopOutcome`.
- `agent/types.rs` — the `LoopDelegate` trait (8 methods, 3 with defaults). `ChatDelegate` (`agent/dispatcher.rs`) is the only impl and is `tauri::AppHandle`-bound (emits IPC). `AutomationDelegate` will be the first headless delegate.
- `automation/runtime/service.rs` — `execute_run` with the `deferred_phase_2` stub; `AppRuntimeService` has no `provider_service` field yet.
- `automation/runtime/execute.rs` — partial `AutomationDelegate` struct (7 fields); `call_llm` stub; `execute_tool_calls` has 4 humane tools real + an `other =>` fall-through stub.
- `automation/runtime/prompt.rs` — `build_system_prompt` / `build_initial_message`; does **not** pre-load memory.
- `automation/runtime/auto_continue.rs` — `AutoContinueConfig`, `CompletionGate { Reported, Escalated, LoopExhausted, ErrorTerminal }` (last two never constructed).
- `automation/tools/report_to_user.rs` + `notify_user.rs` — humane tools, both marked "wired in Task 15 (AutomationDelegate)", currently unwired.
- `automation/memory/store.rs` — per-spec file `MemoryStore` (`<root>/<spec_id>/memory.md` + `archives/`), wired into the `memory` tool but **not** pre-loaded into prompts.
- `automation/memory/compact.rs` — archive cleanup, currently a Phase 1 no-op.
- `channels.rs` — a 153-line **outbound-only** `ChannelManager` (`broadcast`).
- DB: `agent_sessions` (`metadata_json` is free-form JSON, `space_id` already exists), `agent_messages` (FK CASCADE), `automation_specs` (has `space_id`), `automation_activities` (Humane schema, V20).

## Architecture

Phase 2a has four pillars. §0 is the ownership model (the conceptual spine — refined branch-by-branch via grill-me). §1–§3 are the execution-wall mechanics. §4 is the migration. §5 fixes the scope boundary; §6 covers testing.

The unifying idea: **everything an automation does lives on the existing agent-session substrate.** A run is an `agent_session`; its transcript is `agent_messages`; `automation_activities` becomes a thin ledger that *links to* the session rather than duplicating it. This is deliberately more convergent than halo, which keeps three parallel stores (`activity_entries` SQLite + `chat.jsonl` + per-IM JSONL).

---

## §0 Automation Run Ownership Model (Option 3)

### §0.1 Home space (grill Q1)

A spec is bound to a **home space** at install time.

- Default: an auto-created shared space named **"Automations"** (created on first automation install if absent; `path = NULL`).
- May be pointed at **any existing space**, including a user's project space — this enables "the automation works inside my project."
- `automation_specs.space_id` already exists (V20); no schema change for this branch.
- Every run-session this spec produces is created with `agent_sessions.space_id = <spec home space>`.

### §0.2 Session granularity (grill Q2)

- A **triggered run** = exactly one `agent_session` (`origin = automation:<trigger>`).
- Run history is a navigable chain: each run-session stores `metadata.prev_run_session_id` pointing at the previous run-session of the same spec.
- (Chat / IM threads use a *different* granularity — one long-lived session per thread — but those are Phase 2b; see §0.10 and §5.)

### §0.3 Ledger ↔ session relationship (grill Q3)

`automation_activities` becomes a **thin ledger** that links to the run-session instead of re-storing the transcript.

- **Add** `session_id TEXT` (nullable) — `1:0..1`. Runs that never reach the loop (filtered out by a source, deduped, rejected by the permission gate) keep an activity row for "why didn't this run" observability but have **no** session.
- **Keep** the denormalized summary columns (`status`, `error_text`, timing, `llm_iterations`, `llm_tokens_in/out`, `report_text`, `report_outcome`) — the AutomationHub list view needs to scan these without loading a transcript.
- **Drop** `tool_calls_json` — the per-tool-call breakdown belongs in `agent_messages` (rendered by the Agent UI). This is the one genuinely redundant fat column.
- **Chain links:** the session chain (`metadata.prev_run_session_id`, §0.2) is canonical for UI navigation. `automation_activities.resumed_from_activity_id` is **retained** — it covers runs that have no session, and the escalation-resume path already depends on it. The two are not duplicates; they cover different lifecycle slices.

### §0.4 Visibility & retention (grill Q4)

**Visibility.** The agent session sidebar filters by `origin`: it shows `origin = human` sessions by default. Automation run-sessions are reached through the AutomationHub activity list, not the sidebar. (A "show automation runs" toggle, parallel to the existing `archived` toggle, may be added but defaults off.) This keeps a project-scoped spec (Q1) from flooding that project's sidebar.

**Retention.** Per-spec, keep the most recent **N ≈ 50** run-session transcripts (N configurable via `memubot_config`). When a spec exceeds N:

- Delete the oldest run-session's `agent_messages` rows and the `agent_session` row.
- Set the corresponding `automation_activities.session_id` back to `NULL`.
- The `automation_activities` ledger row is **never** deleted — it retains `report_text` / `report_outcome` / metrics, so "what happened" survives forever; only the verbose transcript is pruned.

N is **per-spec**, not global, so a chatty 5-minute-cadence spec cannot evict a daily spec's history.

**Where it runs.** Inline, after a run completes — count this spec's run-sessions, prune oldest beyond N. No new background service. This fills in `automation/memory/compact.rs` (currently a Phase 1 no-op).

**Adjacent tech debt (in scope).** `agent_sessions` has an `archived` column and the sidebar has an `active | archived` view mode, but the archive *action* and any retention are no-ops in both backend and frontend. Phase 2a closes this debt: wire a working archive action and the retention plumbing end-to-end, since the automation retention work touches exactly this code (`compact.rs`, sidebar origin/archive filtering).

### §0.5 Working directory (grill Q5)

A run-session is an `agent_session`, but a session has no filesystem root — the root comes from the space. Resolution chain:

```
spec.space_id → space.path  →  if NULL → ~/Documents/workground/automations/<spec_id>/
```

- `<spec_id>` keys the scratch dir — matching the existing `automation/memory/store.rs` convention (`<root>/<spec_id>/memory.md`).
- The auto-created "Automations" space has `path = NULL` → runs fall through to a per-spec scratch dir under `workground/`, created on first run.
- A project-scoped spec inherits the project's `path` directly.
- **Per-spec persistent directory, not per-run isolation.** All runs of a spec share the same directory — this matches the "spec is a persistent worker with memory" mental model: a file produced by one run can be built on by the next. Per-run attribution is provided by artifact provenance (§0.7), not by directory isolation.
- This `workspace_root` is what file/edit/search tools resolve relative paths against and is the default cwd for the shell tool.
- Blast-radius note: a project-scoped spec + full base tools (§1) + spec-permission-only safety (§2) means an automation can mutate the user's real project directory. This is the accepted halo `bypassPermissions` model; cwd resolution simply defines its boundary.

`space.attached_dirs` is **not** consumed in Phase 2a (grill Q5c — out of scope).

### §0.6 Run viewing UI (grill Q6)

Because a run *is* an `agent_session`, the entire agent viewing stack is reused with zero new viewer components.

- **Entry point:** clicking a run in `AutomationHub` switches `topLevelView = 'workspace'` + `appMode = 'agent'` and loads that session — exactly like opening any agent session. `WorkspaceShell` hosts it, so `PreviewPanel` and `RightSidePanel` (files / plan / trajectory / teams / browser tabs) come for free.
- **Run banner:** a single "automation run context" banner at the top of the Agent view when the loaded session has `origin = automation:*` — shows spec, trigger, run #N, prev/next-run navigation (the §0.2 chain), and a link back to AutomationHub.
- **Tab visibility by capability:** for an automation run, `files` / `plan` / `trajectory` are always shown; `teams` / `browser` are shown only if the run actually used them (determined from the run's tool-call record or the spec's capability map). This is a small, localized change to the tab-bar render — `RightSidePanel` itself is untouched.

Rejected alternative: a dedicated run-detail page inside Kaleidoscope — it would require refactoring `RightSidePanel` from Agent-mode-exclusive to surface-agnostic. Not worth it.

### §0.7 Artifact provenance (grill Q7)

The Humane protocol already has a products concept: `report_to_user` (the only way to end a run) carries an `artifacts` field. Phase 2a uses it directly.

- **Products = `report_to_user.artifacts`** — the agent *declares* its products when it ends the run. No auto-tracking of every file write (grill Q7c — the protocol already chose declared products; match it).
- **Storage:** the declared artifact list is persisted as a JSON column (`report_artifacts_json`) on the `automation_activities` row. It rides along with the ledger summary (§0.3). No sidecar manifest file, no new table.
- **Provenance is automatic:** artifact → activity row → `session_id` (§0.3) + `spec_id`. Option 3's "products carry `produced_by`" is satisfied without a dedicated `produced_by` column.
- **Typed shape** for an artifact entry (replacing the current bare `serde_json::Value`):

  ```rust
  struct ReportArtifact {
      kind: ArtifactKind,        // "file" | "text" | "url"
      path: Option<String>,      // relative to the spec dir, for kind = "file"
      title: String,
  }
  ```

  Phase 2a's common case is `kind = "file"`.

### §0.8 Two-tier memory (grill Q8)

**Tier 1 — private spec memory.** Already exists and is wired: the per-spec file `MemoryStore` (`memory.md` + `archives/`), reachable via the `memory` tool. The one Phase 2a gap: it is **not pre-loaded into the prompt** — the agent has to spend a tool call to remember itself. Fix: `prompt.rs` pre-loads `memory.md` into a `## Memory` section of the run's initial message. The `memory` tool stays, for *writing* and for re-reading after `compact`. This is the only Tier 1 work in Phase 2a.

**Tier 2 — promotable shared memory.** Target: a `memory_graph` node with `MemoryVisibility::Shared`, kind `Curated` / `Reference`, scoped to the spec's home space. Rationale: `memory_graph` is the store uClaw's recall/reflection actually consults; the KV `memories` table would be a dead-end silo. A useful emergent property: **promotion visibility = home-space visibility** — a spec in the shared "Automations" space promotes knowledge only other automations see; a project-scoped spec promotes into that project, where the human chat agent sees it too.

**Scope split:** Phase 2a does Tier 1 pre-loading and *defines* the Tier 2 target and node shape. The Tier 2 **promotion mechanism** (trigger timing, the `memory_graph` write, dedup) is deferred to Phase 2b — cross-spec knowledge sharing is not a "is the execution wall broken" criterion, and the promotion path needs the run path to exist first.

**Dead-asset cleanup:** the `automation_memory` DB table (V21, currently unread) becomes the home for compaction bookkeeping (`compacted_archives_json`, `bytes`, `last_updated_at`) — a small wire-up alongside §0.4's `compact.rs` work. The spec's `memory_schema` field stays dormant (Tier 1 structuring is a separate concern, deferred).

### §0.9 Continuation semantics (grill Q9)

- A triggered run-session is the **immutable record** of that run. A human does not reopen and type into it — that would require switching the session's driving delegate from headless `AutomationDelegate` to IPC-bound `ChatDelegate` mid-session. Instead, conversing with the agent happens in a persistent **chat thread** (Phase 2b; §0.10).
- **Escalation is the one structured human-in-the-loop path, and it already exists** — `CompletionGate::Escalated` + `automation_escalations` (V21) + `resolveEscalation` + `EscalationModal` + `resumed_from_escalation_id`. The agent pauses (`status = waiting_user`), the human answers in the modal, the run resumes **as the same `AutomationDelegate`** (no delegate switch). Phase 2a uses this as-is.
- A spec's next scheduled tick is a **new session** (§0.2), not a continuation. Cross-run continuity flows through Tier 1 memory (§0.8), not through the transcript.

### §0.10 Unified messaging substrate (grill Q10)

**Conceptual model (written into the spec as forward-looking architecture).** halo splits agent messaging across three parallel stores. uClaw, having decided run = `agent_session`, converges all of it onto one substrate — `agent_session` + `agent_messages`, distinguished by `metadata.origin`:

| `origin` | Granularity | What it is |
|---|---|---|
| `automation:scheduled` / `:file` / `:webhook` / `:manual` … | one per run (§0.2) | a triggered run record |
| `automation:chat` | one per spec, long-lived | the user's native chat thread with the spec agent |
| `automation:im` | one per (spec, channel, chatId), long-lived | an IM-bound conversation thread |

Payoffs, all from existing infrastructure: unified FTS (`agent_messages` FTS already exists), unified rendering (the Agent view renders any session), unified retention (§0.4 applies to all origins), the report "feed" is just a query over run-sessions (no `activity_entries`-equivalent table needed), and inbound/outbound symmetry (`sources/` = inbound triggers, `channels.rs` = outbound — a "close loop" is wiring a source-triggered run's reply back through the matching channel).

**Phase 2a scope of this branch:** only the **run → report** direction.

- Wire `report_to_user` — it is the *only* way a run terminates ("THIS IS THE ONLY WAY TO END A RUN — without calling this, the run will retry up to 10 times"). Without it wired, runs cannot end. Its output lands in the ledger (§0.3) + artifacts (§0.7).
- Wire `notify_user` to the **existing outbound `channels.rs` `broadcast`** — a thin call for side-channel notifications.

The full messaging system — the `automation:chat` long-lived thread, IM inbound close-loop, and the full unified-substrate implementation — is **Phase 2b** ("真实触达"). The delegate-identity question for an interactive chat thread (which delegate drives it) is a Phase 2b design item, recorded but not solved here.

---

## §1 AutomationDelegate drives `run_agentic_loop`

Replace the `deferred_phase_2` stub in `automation/runtime/service.rs::execute_run` with a real `run_agentic_loop` call driven by `AutomationDelegate`.

**`AutomationDelegate` — the first headless `LoopDelegate`.** Existing 7 fields (`spec_id`, `activity_id`, `permissions`, `memory`, `db`, `gate`, `auto_continue`) plus:

- `llm: Arc<dyn LlmProvider>`
- `model: String`
- `tools: Arc<ToolRegistry>` — full base tool set (grill pre-lock: option C)
- `session_id: String` — the run-session created per §0.2
- `cost_cap: CostCapConfig` and `cumulative_cost_usd: Arc<Mutex<f64>>` (see §2)
- `continue_count: AtomicU32`

Method wiring:

- `call_llm` → the shared `agent/llm_stream.rs` helper (§3), passing `on_delta = None` (headless).
- `execute_tool_calls` → keep the 4 real humane tools; replace the `other =>` fall-through stub with real dispatch against `tools` (the full base set); wire `report_to_user` (§0.7, §0.10) and `notify_user` (§0.10).
- `handle_text_response` / `after_iteration` → persist turns to `agent_messages` under `session_id`; update the `automation_activities` ledger summary (§0.3).
- The run-session row is created up front (with `space_id` per §0.1, `metadata.origin` + `metadata.prev_run_session_id` per §0.2/§0.10) and `automation_activities.session_id` is set to point at it.

**`AppRuntimeService`** gains a `provider_service` field (constructed from `AppState`) so `execute_run` can resolve the `LlmProvider` + model for the delegate.

`CompletionGate::LoopExhausted` and `ErrorTerminal` — currently never constructed — get constructed on the corresponding terminal paths (retry budget exhausted; cost cap or fatal error).

## §2 Cost guardrails

Two hard caps (distinct from the observational `cost_records` / V13 — these *stop* the run):

- **Per-run cap** — `cumulative_cost_usd` is accumulated across loop iterations from `on_usage`. When it exceeds the per-run cap, the run terminates as `CompletionGate::ErrorTerminal` with a cost-limit reason.
- **Per-day cap** — checked *before* a run starts (sum of the day's `cost_records` for automation-origin sessions). Over the cap → the run does not start; the activity row records the skip reason.

`CostCapConfig` (per-run + per-day limits) comes from `memubot_config`. Safety remains the accepted halo `bypassPermissions` model: spec-declared permissions only, no `SafetyManager` hard-block in the automation path.

## §3 `agent/llm_stream.rs` shared helper

Extract the streaming-completion logic currently inline in `ChatDelegate::call_llm` into a reusable helper so both delegates share it:

```rust
pub async fn stream_completion(
    llm: &dyn LlmProvider,
    messages: ...,
    tools: ...,
    config: &CompletionConfig,
    retry_budget: RetryBudget,
    on_delta: Option<&(dyn Fn(&StreamDelta) + Send + Sync)>,
) -> Result<RespondOutput, Error>
```

- Tiered timeouts (`connect_timeout` 15s, `STREAM_STALL_TIMEOUT` 45s/chunk, `COMPLETE_TIMEOUT` 120s) and the `classify_stream_error` retry-vs-fail decision move into this helper.
- `ChatDelegate` passes `Some(on_delta)` (IPC streaming to the frontend); `AutomationDelegate` passes `None` (headless).
- `ChatDelegate::call_llm` is refactored to call the helper — behavior-preserving for the chat path.

## §4 Migration (V24)

Latest live migration is V23a; V20/V21 are merged. Phase 2a adds a clean **V24** (claim the number in the CLAUDE.md *Active migration registry* in the PR):

- `automation_activities`: add `session_id TEXT` (nullable), add `report_artifacts_json TEXT NOT NULL DEFAULT '[]'`, drop `tool_calls_json` (SQLite table-rebuild; the column is for a not-yet-shipped feature so no data preservation needed).
- `agent_sessions`: add `archived_at INTEGER` (nullable) — the existing `archived` boolean cannot order archived sessions or drive time-based cleanup; §0.4's archive action and retention plumbing need a timestamp.
- Idempotent, following the existing `pragma_table_info` guard pattern used by V20.

## §5 Scope boundary

| In Phase 2a | Deferred to 2b / 2c |
|---|---|
| `AutomationDelegate` + `run_agentic_loop` wiring (§1) | `automation:chat` long-lived chat thread (§0.10) |
| Per-run + per-day cost caps (§2) | IM inbound close-loop (§0.10) |
| `agent/llm_stream.rs` shared helper (§3) | Tier 2 promotion mechanism (§0.8) |
| Full §0 ownership model — schema, session, ledger, cwd, retention | `memory_schema` structured Tier 1 (§0.8) |
| Run opens in Agent view + banner + capability tab filter (§0.6) | "Continue in a new session" fork affordance (§0.9) |
| Tier 1 memory pre-loading (§0.8) | Full unified-substrate implementation (§0.10) |
| `report_to_user` + `notify_user` wiring — run → report loop (§0.10) | |
| `agent_sessions` archive/retention tech-debt closure (§0.4) | |
| V24 migration (§4) | |

## §6 Testing

**Rust `#[cfg(test)]` (inline, per CLAUDE.md — no integration dir):**

- `AutomationDelegate` `LoopDelegate` methods — `call_llm` via a stub `LlmProvider`, `execute_tool_calls` dispatch (humane + base tools), turn persistence to `agent_messages`.
- Cost-cap paths — per-run cap → `ErrorTerminal`; per-day cap → run skipped before start.
- `llm_stream::stream_completion` — `classify_stream_error` retry classification, timeout tiers, `on_delta` present vs `None`.
- Ownership model — run-session created with correct `space_id` / `origin` / `prev_run_session_id`; `automation_activities.session_id` linkage; `report_artifacts_json` round-trip.
- Retention — per-spec prune keeps N, deletes oldest transcript + `agent_session`, nulls `session_id`, leaves the ledger row.
- cwd resolution — `space.path` set vs `NULL` → per-spec scratch dir.

**Frontend Vitest (jsdom, `renderWithProviders`):**

- Agent view automation run banner renders for `origin = automation:*` sessions, hidden otherwise.
- `RightSidePanel` tab visibility filtered by run capability.
- Agent session sidebar filters out `origin = automation` by default.

**Verification commands** (per CLAUDE.md):

- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
- `cd src-tauri && cargo test --lib automation` / `... llm_stream`
- `cd ui && npx tsc --noEmit 2>&1 | head -10`
- `cd ui && npm test -- --run 2>&1 | tail -10`

## Adjacent edits (call out in commit bodies, not scope creep)

- New Tauri command(s) for the AutomationHub → Agent-view run-open flow → define in `tauri_commands.rs` **and** register in the `invoke_handler!` macro in `main.rs`.
- `AppRuntimeService` gains `provider_service` → constructed in `app.rs`, passed at the `runtime_service` construction site.
- `notify_user` wiring touches `channels.rs` (outbound `broadcast`).
- Composer behavior is **not** touched (run-sessions are view-only in 2a, §0.9) — so the two-parallel-composers rule does not apply here.
