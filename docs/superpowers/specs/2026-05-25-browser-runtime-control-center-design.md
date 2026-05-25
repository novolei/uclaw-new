# Browser Runtime Control Center Design

Date: 2026-05-25
Status: Proposed
Owner: Browser Runtime Supervisor track

## Summary

Replace the current browser runtime settings surface with a real Browser Runtime Control Center. The Control Center must let users enable Playwright CLI and Playwright MCP, express provider priority, understand the actual active browser route, and recover from probe failures without confusing "desired provider" with "currently routable provider."

The default desired priority is:

1. `browser.playwright_cli`
2. `browser.playwright_mcp`
3. `browser.local_chromium`

Playwright CLI is the first-class app-managed browser execution lane. Playwright MCP is the second-priority provider, marked advanced, with stronger guardrails and detailed configuration in Kaleidoscope Integrations. Local Chromium remains the fallback lane and must stay available unless a later design explicitly removes that guarantee.

## Current Truth

The provider implementation base already exists:

- Playwright CLI has feature-flagged readiness, request envelopes, worker execution, timeout/kill behavior, action state diff evidence, and provider execution adapters.
- Playwright MCP has feature-flagged readiness, sidecar specs, stdio action boundary, artifact/error routing, and MCP-vs-CLI selection guardrails.
- Runtime status currently builds provider readiness with `BrowserRuntimeFeatureFlags::safe_defaults()`, so CLI/MCP remain disabled unless tests manually construct enabled flags.
- The current Settings page exposes raw readiness wording such as "feature flag disabled" and "No active local Chromium context exists for this session", which is true but product-hostile.

This design turns those backend capabilities into a user-operable control surface.

## Goals

- Enable Playwright CLI and Playwright MCP from product UI.
- Let users reorder desired provider priority from the Browser Runtime Control Center.
- Show both desired route and active route, with explicit fallback reasons.
- Require probe gates before CLI/MCP become routable.
- Keep Playwright CLI configuration in the Control Center.
- Put Playwright MCP detailed configuration in Kaleidoscope > Integrations as an app-built-in integration, similar to Gbrain MCP.
- Keep raw MCP tools hidden from the model; route only uClaw-wrapped browser actions.
- Preserve Local Chromium as a safe fallback.
- Avoid scary or misleading statuses for feature-off or no-active-session states.

## Non-Goals

- Do not remove Local Chromium.
- Do not expose raw Playwright MCP tools to the model.
- Do not make provider enablement equivalent to provider routability.
- Do not force CLI/MCP as active route when probes fail.
- Do not put all MCP configuration in Settings.
- Do not redesign unrelated Settings pages.

## Product Model

The UI separates user intent from runtime truth:

- `desiredPriority`: the order the user wants the app to try.
- `activeRoute`: the provider Rust would actually choose for the next browser task now.
- `enabled`: whether a provider is allowed to participate.
- `routable`: whether a provider has passed required gates and can execute browser actions.
- `fallbackReason`: why a higher-priority provider was skipped.

Example:

```text
Desired priority:
1. Playwright CLI
2. Playwright MCP
3. Local Chromium

Active route:
Local Chromium

Why:
Playwright CLI probe failed: worker startup timed out.
Playwright MCP not routable: sidecar probe has not passed.
```

This lets users insist on CLI/MCP priority without making the app brittle.

## Backend Read Model

Add a Rust read model for the Control Center:

```rust
BrowserRuntimeControlCenterReport {
    runtime_pack,
    feature_flags,
    desired_provider_priority,
    active_provider_route,
    provider_lanes,
    mcp_integration_summary,
    updated_at_ms,
}
```

Each provider lane includes:

```rust
BrowserRuntimeProviderLane {
    provider_id,
    display_name,
    enabled,
    priority_rank,
    readiness,
    routable,
    route_role,
    probe_state,
    fallback_reason,
    next_action,
    last_probe_artifact,
}
```

`active_provider_route` is calculated by Rust from current readiness, probes, and priority. The frontend must not infer it.

## Persistent Configuration

Add a small provider config model:

```rust
BrowserRuntimeProviderConfig {
    playwright_cli_enabled: bool,
    playwright_mcp_enabled: bool,
    desired_priority: Vec<String>,
    default_fallback_provider: String,
    updated_at_ms: i64,
}
```

Default config:

```text
playwright_cli_enabled = false
playwright_mcp_enabled = false
desired_priority = [
  "browser.playwright_cli",
  "browser.playwright_mcp",
  "browser.local_chromium"
]
default_fallback_provider = "browser.local_chromium"
```

Implementation should prefer an existing settings/config persistence path. If a database migration is required, claim the next V-number in `CONTEXT.md` before implementation.

## IPC

Add focused Tauri commands:

- `get_browser_runtime_control_center`
- `set_browser_runtime_provider_enabled(provider_id, enabled)`
- `set_browser_runtime_provider_priority(provider_ids)`
- `run_browser_runtime_provider_probe(provider_id)`
- `set_browser_runtime_provider_config(input)` if batching proves simpler

Commands should delegate to focused browser runtime modules and keep `tauri_commands.rs` thin.

## Routing Semantics

Runtime route evaluation:

```text
for provider in desiredPriority:
  if provider is disabled:
    record fallback reason
    continue
  if provider requires runtime pack and runtime pack is not ready:
    record fallback reason
    continue
  if provider probe has not passed:
    record fallback reason
    continue
  if provider-specific guardrail blocks it:
    record fallback reason
    continue
  return provider

return browser.local_chromium
```

Provider-specific rules:

### Playwright CLI

- Configured in Browser Runtime Control Center.
- Default first desired priority.
- Requires runtime pack ready.
- Requires CLI worker probe.
- Requires real page/action smoke.
- Probe failure preserves desired priority but prevents active routing.

### Playwright MCP

- Configured in Kaleidoscope > Integrations.
- Default second desired priority.
- Marked advanced in Control Center.
- Requires runtime pack ready.
- Requires MCP sidecar probe.
- Raw MCP tools remain hidden.
- Only uClaw-wrapped browser actions are routable.
- Probe failure shows `Enabled / Not routable`.

### Local Chromium

- Default fallback.
- Does not require runtime pack.
- `No active sessions` is informational, not an error.
- A browser task can create a supervised context when needed.

## Status Vocabulary

Use these labels:

- `Off`
- `Enabled`
- `Needs runtime pack`
- `Needs probe`
- `Probe failed`
- `Ready`
- `Active`
- `Fallback active`
- `Advanced`
- `Not routable`

Avoid these misleading patterns:

- Do not use `unavailable` to mean feature flag off.
- Do not show `needs setup` without a next action.
- Do not elevate `No active local Chromium context` into a global warning.
- Do not present desired provider priority as the active route.

## Control Center UI

The first screen answers three questions:

1. What do I want to use?
2. What will the app actually use now?
3. What should I do next?

Structure:

```text
Browser Runtime Control Center

[Route Summary]
Desired: CLI > MCP > Local Chromium
Active: Local Chromium
Reason: CLI enabled but worker probe has not passed.
Primary action: Run probes

[Provider Priority]
1. Playwright CLI       Enabled   Needs probe    [Run probe] [Set first]
2. Playwright MCP       Enabled   Advanced       [Configure] [Run probe] [Set first]
3. Local Chromium       Fallback  Ready          [View sessions]

[Runtime Pack]
Pack version / root / current pack / prepare status
Actions: Prepare / Repair / Run doctor

[Probe Evidence]
Last CLI probe
Last MCP probe
Last route decision
Artifact refs / event names

[Advanced]
Feature flags
Raw JSON report
Developer fallback / auto-prepare
```

UI rules:

- One primary action at the top.
- Provider lanes show status plus next action.
- Raw diagnostics stay collapsed.
- Buttons are explicit: `Enable CLI`, `Run CLI probe`, `Set first`, `Configure MCP`.
- Static text must not look like disabled buttons.
- Use existing theme tokens and Settings primitives where possible.
- Touch targets must be at least 44px high.
- Status is not conveyed by color alone; include text and icons.

## MCP in Kaleidoscope Integrations

Playwright MCP detailed configuration belongs in Kaleidoscope > Integrations as a built-in integration, similar to Gbrain MCP.

Playwright MCP integration detail:

```text
Playwright MCP
Built-in integration

Status
- Built in
- Enabled / Off
- Sidecar ready / Not ready
- Raw MCP tools hidden
- Wrapped browser actions available

Configuration
- Enable Playwright MCP provider
- Allow Control Center to route browser actions through MCP
- Sidecar startup mode: app-managed
- Runtime pack source: uClaw-managed Browser Runtime Pack
- Raw MCP exposure: locked off

Diagnostics
- Last sidecar probe
- Last action envelope
- Last artifact/error route
- Version / package path
```

Control Center MCP lane displays only summary and links:

- `Configure MCP`
- `Run probe`
- `Set first`

First enablement should show a confirmation sheet explaining that MCP is an advanced provider, raw MCP tools stay hidden, and probe gates are required before routing.

## Probe UX

Provider probe states:

- `not_run`
- `running`
- `passed`
- `failed`
- `stale`
- `blocked`

CLI probe includes:

- runtime pack check
- worker startup
- real page probe
- minimal action smoke
- artifact capture

MCP probe includes:

- runtime pack check
- sidecar spec check
- sidecar startup
- wrapped action envelope check
- raw MCP hidden check
- artifact/error routing check

Probe failure display:

```text
Playwright CLI
Enabled · Probe failed · Not routable

Reason:
Worker startup timed out after 15s.

Next:
[Run probe again] [View artifact] [Fallback details]
```

Top-level route summary aggregates skipped-provider reasons.

## Error Handling

- Feature off: show `Off` with `Enable CLI` or `Enable MCP`.
- Runtime pack missing: show `Needs runtime pack` with `Prepare runtime pack`.
- Probe not run: show `Needs probe` with `Run probe`.
- Probe failed: show cause and artifact.
- Probe stale: keep desired priority but mark provider not routable until refreshed.
- Active fallback: always show which provider is actually active and why.
- Local Chromium no active context: show `No active sessions. A browser task will create one.`

## PR Slices

### PR 1: Control Center foundation

Scope:

- Add config-backed CLI/MCP enablement and desired priority.
- Add Control Center report/read model.
- Stop using hard-coded `safe_defaults()` in user-visible status paths.
- Redesign Settings into the first Control Center surface.
- CLI can be enabled from Control Center.
- MCP enabled/off is visible; `Configure MCP` is not shown as a clickable action
  until PR 3 wires the real Kaleidoscope integration route.

Verification:

- Rust config tests.
- Runtime status/read-model tests.
- Settings UI tests.
- `npm run build`.
- Focused Rust tests for provider defaults / runtime status.

### PR 2: Provider probe gates

Scope:

- Add CLI probe command.
- Add MCP probe command.
- Persist or expose last probe summary.
- Compute routable gates and active route explanations.
- Show probe failures and fallback in UI.

Verification:

- CLI probe pass/fail tests.
- MCP probe pass/fail tests.
- Route decision tests.
- UI tests for failed probe and fallback active.

### PR 3: Kaleidoscope Playwright MCP built-in integration

Scope:

- Add Playwright MCP integration detail next to Gbrain MCP.
- Built-in integration card.
- Enable MCP provider.
- Show sidecar status/config.
- Keep raw MCP exposure locked off.
- Wire `Configure MCP` from Control Center.

Verification:

- Kaleidoscope integration UI tests.
- MCP config IPC tests.
- Navigation/deeplink tests if route support exists.

### PR 4: Default provider execution promotion

Scope:

- Browser task routing consumes config-backed desired priority.
- CLI first, MCP second, Local Chromium fallback.
- Real browser action execution follows active route.
- TaskEvent/artifact explanation includes selected provider and skipped providers.

Verification:

- Provider execution tests.
- Browser task smoke.
- Regression: Local Chromium fallback still works.
- Fresh reviewer required because this changes real browser execution routing.

### PR 5: Polish and diagnostics

Scope:

- Better artifact links.
- Last probe history.
- Raw JSON diagnostics.
- UX polish after real use.

Verification:

- UI regression tests.
- Manual Control Center validation.

## ADR 18 Answers

1. Intent: make CLI/MCP genuinely user-enableable and routable through a real Control Center.
2. Autonomy boundary: user sets desired priority; Rust decides active route from gates.
3. Truth source: Rust Control Center report, provider config, runtime pack status, provider probes.
4. TaskEvent: provider selection and fallback should emit route/probe/fallback events in later execution PRs.
5. Context: Control Center consumes current runtime context and active browser sessions without launching browsers during read-only status.
6. Capability: consumes existing Local Chromium, Playwright CLI, and Playwright MCP provider capability cards.
7. Hooks: no new hooks in PR 1; future PRs may hook probe/task events.
8. Projection: expose desired route, active route, provider lanes, and fallback explanation.
9. Harness: probe tests and browser task smoke cover CLI/MCP promotion.
10. Rollback: disable CLI/MCP flags or fall back to Local Chromium; provider priority remains editable.
11. Non-ownership: this does not expose raw MCP tools, remove Local Chromium, or redesign unrelated Settings.

## Open Implementation Notes

- Prefer existing settings persistence to avoid a migration. If unavailable, update the migration registry before adding a schema version.
- Keep `tauri_commands.rs` thin by delegating to a browser runtime control module.
- Use a separate worktree and PR per slice.
- Run GitNexus impact before modifying provider execution or runtime routing symbols.
