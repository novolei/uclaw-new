# Agent OS Post-PR14 Codebase Audit

Date: 2026-05-23
Status: audit baseline for the next implementation wave
Scope: current `main` after PR-14 plus the restored Browser Runtime Supervisor
commit set.

## Executive Summary

The PR-1 through PR-14 chain successfully landed many Agent OS foundations:
pure type crates, compatibility adapters, readiness DTOs, soft interrupt
primitives, projection journal helpers, performance scorecard schema, team
runtime hardening scaffolds, tool family cards, browser provider probes,
ambient mapping contracts, harness campaign manifests, and frontend
projection reducers/hydration helpers.

The current codebase has not yet crossed the most important product boundary:
the design spine is not the default runtime path.

Target spine:

```text
IntentSpec -> TaskSpec -> TaskEvent -> WorldProjection -> Harness
```

Current reality:

```text
legacy per-domain runtime paths
  + Agent OS contracts/adapters/tests beside them
  + several pure projection/harness/mesh helpers not yet wired into UI or gates
```

The main risk is not missing types. The risk is believing the runtime truth is
unified because the contracts exist.

## Sources Reviewed

- `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`
- `docs/jcode_comparison/README.md`
- `docs/jcode_comparison/04_backend_reconstruction_blueprint.md`
- `docs/jcode_comparison/05_frontend_integration.md`
- `docs/jcode_comparison/06_adr_gap_audit_and_reference_addenda.md`
- `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `src-tauri/src/runtime/*`
- `src-tauri/src/agent/*`
- `src-tauri/src/automation/*`
- `src-tauri/src/browser/*`
- `src-tauri/src/harness/*`
- `src-tauri/src/registries/*`
- `ui/src/lib/agent-os/*`
- `ui/src/lib/tauri-bridge.ts`
- `ui/src/components/agent/*`
- `ui/src/components/browser/*`
- `ui/src/components/automation/*`
- `ui/src/components/settings/*`

## Audit Method

This audit combines:

- direct source inspection with `rg`, `sed`, and targeted file reads;
- four read-only subagent reviews for backend runtime, frontend wiring,
  browser/tool mesh, and ambient/teams/harness/evolution;
- GitNexus query/detect-change checks where applicable;
- comparison against the North Star ADR and the jcode absorption design.

The audit is intentionally static. It did not run full desktop E2E, full
Tauri startup, or long harness campaigns.

## Severity Model

- P0: contradicts the North Star governance model or can silently make the
  product unsafe/misleading.
- P1: blocks the Agent OS spine from becoming the default product path.
- P2: quality, drift, or UX issues that should be fixed before the next
  runtime expansion.

## P0 Findings

### P0-1: Self-evolution can promote new genes directly to active state

Design promise:

- Evolution must be gated by harness evidence, user review, and a promotion
  registry.
- The North Star forbids direct promotion of learned behavior into durable
  active runtime behavior.

Observed code:

- `src-tauri/src/proactive/service.rs` has a `gene_evolution` branch that
  parses LLM output, validates a gene, checks duplicates, and then calls
  `store_gene`.
- `src-tauri/src/agent/gep/distillation.rs` constructs parsed genes with
  `GeneStatus::Active`.
- The self-improvement harness exposed in Tauri commands is a fixture-style
  command and is not called by the gene promotion path.

Risk:

- A learned gene can become active without passing the intended harness and
  review gates.
- This is a governance conflict, not only an incomplete feature.

Required action:

- Freeze direct active promotion.
- New genes should land as candidate/quarantined status until a promotion
  gate records harness evidence and user approval.

## P1 Findings

### P1-1: TaskEvent is still not the default runtime truth

Design promise:

- Runtime truth is `TaskEvent`.
- UI truth is a `WorldProjection` materialized from runtime truth.

Observed code:

- `src-tauri/src/agent/rollout_integration.rs` gates rollout emission behind
  `UCLAW_ROLLOUT_ENABLED`.
- `src-tauri/src/tauri_commands.rs` still falls back to direct
  `run_agentic_loop` when rollout is disabled.
- Automation and teams also call legacy loop paths directly.

Risk:

- Normal Agent/Chat usage may produce no canonical Agent OS runtime stream.
- Projection and harness cannot become reliable product truth if the default
  path bypasses events.

Required action:

- Make event emission the default for read-only observability first.
- Keep fallback paths explicit and user-visible rather than silent.

### P1-2: PR-14 projection hydration is pure frontend code, not a product path

Design promise:

- `WorldProjection` becomes the product-facing UI state contract.

Observed code:

- `ui/src/lib/agent-os/projection-hydration.ts` can turn backend projection
  payloads into `RuntimeProjection` and `WorldProjection`.
- `src-tauri/src/runtime/projection_journal.rs` can derive projection stubs
  and journal entries.
- No Tauri command currently exposes the backend projection payload to the UI.
- No React/Jotai surface currently consumes `hydrateProjection`.

Risk:

- PR-12 through PR-14 can be mistaken for UI convergence even though no user
  can see the result.

Required action:

- Add a read-only projection ingress command.
- Add a typed frontend bridge and a small visible diagnostics surface.
- Do not rewrite all panels in the first wire-up PR.

### P1-3: Capability Mesh metadata is not yet runtime routing

Design promise:

- Tools, browser providers, workers, memory, providers, and automation should
  appear as capability cards.
- Planner/runtime should resolve needed capability through the mesh.

Observed code:

- `src-tauri/src/registries/tool_families.rs` explicitly says metadata only.
- `src-tauri/src/registries/resolver.rs` is mostly used by tests.
- Real agent runs still build per-session `ToolRegistry` instances directly.
- Some tool family ids such as `search` and `file` do not match real agent
  tool names like `grep`, `glob`, `read_file`, and `write_file`.

Risk:

- Future planner integration can route to nonexistent tool ids.
- The registry can present a capability as available even when runtime cannot
  execute it.

Required action:

- Add a mesh/runtime parity audit before planner routing.
- Align capability ids with real executable tool ids or add explicit adapter
  aliases with tests.

### P1-4: ToolExecutionContext is not yet consumed by tools

Design promise:

- jcode-style ToolContext should carry task identity, cancellation, policy,
  progress, and capability information into tool execution.

Observed code:

- `ToolExecutionContext` exists.
- `execute_tool_with_context` accepts `_ctx` but ignores it and calls the old
  `tool.execute(params)` path.

Risk:

- Safety/capability/provenance fields look present but do not influence tool
  behavior.

Required action:

- Add one narrow context-aware tool path before broad migration.
- Keep existing tool trait stable until the context contract is proven.

### P1-5: BrowserProvider readiness is not a real gate

Design promise:

- Browser tasks should get jcode-style readiness/setup/probe ergonomics with
  clear failure reasons before task start.

Observed code:

- `src-tauri/src/browser/provider.rs` is pure DTO/probe logic.
- It does not launch Chromium, inspect the current browser context manager,
  call CDP, or gate Browser Agent task start.
- The Browser Runtime Supervisor commits restore a stronger foundation, but
  they are still contracts/supervisor shell rather than task-time UI gating.

Risk:

- Browser readiness can appear available while real runtime startup is still
  fragile or invisible.

Required action:

- Treat Browser Runtime Supervisor as the next browser readiness source of
  truth.
- Wire it into BrowserProvider status and projection attention before making
  Playwright provider choices.

### P1-6: Ambient, automation, and scheduled work are not unified

Design promise:

- jcode ambient maps into uClaw automation/scheduled workers, not a second
  scheduler.
- Scheduled work should have pause/cancel/status and durable receipts.

Observed code:

- `automation/ambient_mapping.rs` is a pure contract/test island.
- Real automation runtime still uses its own activity/session semantics.
- Several failure branches update automation state but do not emit a complete
  TaskEvent/projection trace.

Risk:

- Background work remains surprising and poorly observable.

Required action:

- Wire ambient mapping into actual automation scheduling decisions.
- Emit projection-visible events for success, failure, escalation, and
  policy-blocked outcomes.

### P1-7: Teams are still parallel mini-agents

Design promise:

- Subagents and teams become workers under the same TaskSpec/TaskEvent control
  plane.

Observed code:

- `src-tauri/src/workers/spec.rs` has canonical worker spec/lifecycle types.
- `src-tauri/src/agent/teams/worker.rs` defines another local `WorkerSpec` and
  directly calls `run_agentic_loop`.
- Team UI listens for `agent:team-message` but `activeTeamAtom` has no visible
  production starter path and drops messages while null.

Risk:

- Team work can run without entering canonical projection and can be invisible
  to users.

Required action:

- Collapse team workers onto canonical worker spec/events.
- Add a minimal visible team runtime state before raising autonomy.

### P1-8: Harness campaigns are manifests, not promotion gates

Design promise:

- Harness is the promotion gate before autonomy increases.

Observed code:

- Agent OS harness campaigns exist as manifests.
- Settings still exposes older fixture-style harness commands.
- No production runner or UI entry executes PR-11 campaigns as a promotion
  gate.

Risk:

- The product can claim harness-gated autonomy while the gate is not in the
  path.

Required action:

- Add a campaign runner command and a small UI entry.
- Require campaign evidence before evolution or autonomy promotion.

## P2 Findings

### P2-1: Frontend bridge has silent fake-success fallbacks

Examples:

- `createAgentSession` catches backend failure and returns a synthetic local
  id.
- Several bridge calls catch failures and return `[]`, `null`, empty strings,
  or no-op completions.

Risk:

- UI can appear successful when backend state does not exist.

Required action:

- For Agent OS paths, replace fake success with typed recoverable errors and
  visible projection attention.

### P2-2: Browser and automation failures can disappear into console logs

Examples:

- Browser initial navigation and DOM fetch failures are logged but not shown.
- Browser preview rendering can swallow failures.
- Automation spec/activity loading catches errors and leaves empty UI state.

Risk:

- Users see idle/empty surfaces instead of actionable runtime health.

Required action:

- Promote runtime errors into `WorldProjection.attention` and shell status.

### P2-3: Policy models can drift

Observed code:

- Runtime contracts define a `PolicySpec`.
- `policy_eval` also defines policy spec structures.
- HookBus is still partly future wire-up.

Risk:

- Policy semantics can diverge between design, contracts, and enforcement.

Required action:

- Choose a canonical policy DTO boundary and add contract tests before broad
  policy hook wiring.

### P2-4: Large DMZ files remain the highest-risk integration surface

Observed code:

- `src-tauri/src/tauri_commands.rs` remains a very large flat IPC module.
- `src-tauri/src/app.rs` remains a broad DI container.

Risk:

- Future runtime integration PRs will be tempted to add more logic to the
  largest files.

Required action:

- New Agent OS commands should live in focused modules and be re-exported into
  Tauri command registration with minimal wrappers.

## Implemented vs. Not Yet Implemented

| Design Area | Implemented | Not Yet Implemented |
|---|---|---|
| Runtime contracts | Pure crates and TaskEvent variants exist | Default runtime path still bypasses TaskEvent unless rollout env is set |
| Projection journal | Backend stub/journal helpers exist | No Tauri read command and no UI consumer |
| Frontend projection | Reducer/world/hydration helpers exist | Product surfaces do not render from WorldProjection |
| ToolContext | Adapter type exists | Context is ignored by actual tool execution |
| Capability Mesh | Registry/card metadata exists | Planner/dispatcher does not resolve through mesh |
| BrowserProvider | DTO/probe helpers exist | Not task-time readiness gate; not visible in UI |
| Browser Runtime Supervisor | Contracts and shell restored on main | Not yet wired into task start, settings, or projection |
| Ambient mapping | Pure mapping contract exists | Automation scheduler does not consume it |
| Team workers | Worker contracts and team runtime exist | Two worker specs; team events are not TaskEvent |
| Harness | Campaign manifests and scorecards exist | No production promotion runner/UI gate |
| Evolution | GEP can distill genes | Direct active promotion bypasses gate |

## Recommended Recovery Sequence

The next wave should stop adding more scaffolds and close the shortest visible
runtime loop first.

### PR-15: Runtime projection ingress and visible diagnostics

Goal:

- Prove that backend projection data can reach the UI and be inspected.

Scope:

- Read-only Tauri command for projection payloads.
- Typed frontend bridge.
- Jotai/hook wrapper around PR-14 hydration.
- Small diagnostics surface in Agent/System.
- No broad panel rewrite.

### PR-16: Default TaskEvent observability

Goal:

- Make normal Agent/Chat runs emit a canonical read-only event stream by
  default.

Scope:

- Enable rollout emission by default for observability.
- Keep legacy execution behavior.
- Make fallback explicit and visible.

### PR-17: No fake success for Agent OS bridge paths

Goal:

- Remove the most misleading frontend fallbacks before relying on projection.

Scope:

- Replace synthetic sessions and silent empty states with typed errors.
- Surface errors through projection attention.

### PR-18: Browser Runtime Supervisor to BrowserProvider readiness

Goal:

- Connect restored Browser Runtime Supervisor contracts to BrowserProvider
  status and task-time preparation.

Scope:

- Supervisor status command.
- Provider readiness adapter backed by supervisor state.
- Minimal UI readiness line and attention reason.

### PR-19: Harness campaign runner and evolution gate freeze

Goal:

- Stop direct active promotion and add a real promotion evidence path.

Scope:

- Candidate/quarantine gene status path.
- Harness campaign runner command.
- Self-improvement UI verdict fix.

### PR-20: Worker/team convergence

Goal:

- Align teams with canonical worker spec and projection.

Scope:

- Adapter from team worker state into TaskEvent.
- Team UI start/state path that does not drop messages while idle.

## Audit Verdict

The codebase is in a healthy foundation state but an incomplete product state.
The right next move is not another broad rewrite or another metadata layer. The
right next move is one narrow, demonstrable runtime truth loop:

```text
backend projection payload -> typed bridge -> hydrated WorldProjection ->
visible diagnostics -> harness replay test
```

Once that loop exists, later PRs can migrate Agent, Browser, Automation, Teams,
and Evolution into the spine without guessing.
