# Browser Automation Official Playwright Runtime Design

Date: 2026-05-26
Status: Draft for implementation planning
Owner: Codex
Related ADRs:
- `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`

## 1. Purpose

uClaw should stop treating its app-managed Browser Runtime pack as the default
truth source for Playwright CLI and Playwright MCP readiness. The product goal is
now simpler:

> Browser Automation uses official Playwright integration paths by default:
> global `@playwright/cli@latest` for the fast CLI lane and
> `npx @playwright/mcp@latest` for the MCP lane. uClaw owns discovery, setup,
> policy, routing, artifact capture, Agent skill discovery, and diagnostics.

This keeps Browser Runtime Supervisor as the policy and routing boundary while
removing duplicated package/runtime ownership. It also aligns uClaw with the
mainstream Agent app pattern: configure official tools, then manage them through
one capability layer.

## 2. Inputs and Decisions

The design is based on:

- Architecture review:
  `/private/var/folders/h_/z21cg38x3xz6z1ppwjcz_8qc0000gn/T/browser-runtime-architecture-review-20260526-103310.html`
- Official Playwright CLI README:
  https://github.com/microsoft/playwright-cli#installation
- Official Playwright MCP docs:
  https://playwright.dev/docs/getting-started-mcp
- Official Playwright MCP capabilities docs:
  https://playwright.dev/mcp/capabilities
- Existing uClaw MCP manager:
  `src-tauri/src/mcp.rs`
- Existing Browser Runtime modules:
  `src-tauri/src/browser/`
- Comparison reference:
  `/Users/ryanliu/Documents/cc-haha`

Resolved decisions:

1. Playwright CLI is the default primary fast lane.
2. Playwright MCP is a visible secondary/backup lane, not a hidden-only detail.
3. Users can reorder CLI and MCP priority.
4. Browser Runtime can override the user order only for explicit capability or
   failure reasons, and must record route evidence.
5. Playwright MCP is an internal built-in MCP server managed through uClaw's
   existing `McpManager`, not a parallel Browser Runtime sidecar.
6. Raw Playwright MCP tools do not enter the normal Agent tool pool by default.
7. Playwright CLI skills are a uClaw built-in skill pack, sourced from official
   `playwright-cli install --skills`, synced to a uClaw-managed skills directory,
   and gated by uClaw compatibility mapping.
8. Browser actions described by Playwright skills execute through the uClaw
   Browser Runtime Adapter, not arbitrary shell commands.
9. The old runtime-pack product concept is removed from the default path. It is
   not kept as a parallel fallback truth.
10. App startup performs lightweight detection only. Mutating setup runs when
    the Browser Automation Control Center opens or when browser automation is
    first needed.
11. If Node/npm/npx are missing, uClaw shows a clear blocked setup state. The
    experimental Node bootstrap supports only macOS Homebrew installation and
    does not run `sudo`, install Homebrew, modify shell profiles, or support
    Linux/Windows in this round.

## 3. User Experience

### 3.1 Browser Automation Control Center

The Settings page becomes a Browser Automation Control Center with product
language, not runtime-pack internals.

This is a UI/UX restructure, not a label-only change. The current Browser
Runtime settings page should be replaced with a scannable control surface for
Browser Automation. The first viewport must answer four questions:

1. Can uClaw automate the browser right now?
2. Which provider will run first: Playwright CLI, Playwright MCP, or Local
   Chromium?
3. Is setup currently installing, blocked, degraded, or ready?
4. What is the one next action the user can take?

Primary states:

- `ready`: Playwright CLI, skills, and MCP are ready enough for the selected
  priority order.
- `setting_up`: uClaw is installing or upgrading official Playwright services.
- `needs_node`: Node/npm/npx are missing.
- `needs_permission`: npm global install cannot write to the configured prefix.
- `network_unavailable`: npm/npx package fetch failed because the network is
  offline or blocked.
- `degraded`: at least one lane is ready but another is unavailable.
- `failed`: setup/probe failed with actionable diagnostics.
- `disabled`: the user disabled a provider or auto setup.

Provider lane states:

- CLI: `ready`, `needs_setup`, `unavailable`, `disabled`.
- MCP: `ready`, `needs_setup`, `unavailable`, `disabled`.
- Skills: `installed`, `missing`, `refreshing`, `incompatible`.

Normal users see:

- Browser Automation status.
- Provider priority: Playwright CLI, Playwright MCP, Local Chromium fallback.
- One primary setup/repair action.
- Optional diagnostics drawer.

Required layout:

- Header band: "Browser Automation" title, readiness badge, selected provider,
  last checked time, and one primary action.
- Provider priority section: three rows for CLI, MCP, and Local Chromium with
  clear status, reason, and compact controls. Rows must use buttons and toggles,
  not static pill labels that look clickable.
- Setup progress section: visible only while setup runs or has failed. Shows
  current step, progress timeline, stdout/stderr artifact links, retry, and
  cancel when cancellation is supported.
- Built-in Playwright skills section: installed/missing/incompatible count,
  sync action, and Agent discovery status.
- Diagnostics drawer: command paths, versions, MCP config, tool allowlist,
  route evidence, and compatibility report.

UI requirements:

- Use existing settings primitives and lucide icons.
- Keep information density moderate and operational; no marketing hero.
- Avoid nested cards; use full-width sections and row groups.
- Every async action needs disabled/loading/success/error states.
- Every icon-only control needs an accessible label.
- Provider rows must fit on mobile without horizontal scroll.
- Diagnostics are progressive disclosure; normal users should not see raw JSON by
  default.

Diagnostics show command paths, versions, setup logs, MCP server status, tool
allowlist, skill compatibility, route evidence, and failure details. Diagnostics
do not reintroduce "runtime pack" as a product concept.

### 3.2 Setup Flow

Startup:

- Detect `node`, `npm`, `npx`, global `playwright-cli`, installed skills, and
  built-in MCP config status.
- Do not install packages or mutate system state during ordinary startup.

Control Center open or first browser automation request:

- If auto setup is enabled and Node/npm are available, run setup with visible
  progress.
- Install or upgrade global `@playwright/cli@latest`.
- Run `playwright-cli install --skills`.
- Sync official skills into uClaw's built-in Playwright skill pack directory.
- Seed or refresh the built-in Playwright MCP server config using
  `npx @playwright/mcp@latest`.
- Probe MCP with uClaw's existing MCP manager.

Node missing:

- Show `needs_node`.
- Offer install guide and re-check.
- Experimental macOS-only button: "Install Node.js with Homebrew".
- The experimental button may run `brew install node` only when Homebrew is
  already present.
- It must not run `sudo`, install Homebrew, change shell profiles, or alter npm
  global prefix.

## 4. Architecture

### 4.1 New Modules

`src-tauri/src/browser/playwright_discovery.rs`

- Detects Node/npm/npx.
- Detects global `playwright-cli`.
- Detects official CLI version.
- Detects npm global prefix writability.
- Detects Playwright skills install/sync state.
- Produces a read-only discovery report.

`src-tauri/src/browser/playwright_setup.rs`

- Plans and executes controlled setup actions.
- Runs official commands with bounded timeout and captured stdout/stderr.
- Supports install/upgrade CLI, refresh skills, probe MCP, and macOS Homebrew
  Node bootstrap.
- Emits setup events and artifacts.

`src-tauri/src/browser/playwright_skills.rs`

- Tracks the uClaw built-in Playwright skill pack.
- Syncs official skill content into a uClaw-managed built-in skills directory
  outside the user-editable skills tree, for example
  `<uclaw_data_dir>/builtin-skills/playwright-cli/`.
- Stores metadata: source command, package version, install timestamp, hash, and
  compatibility report.
- Exposes only skills compatible with uClaw Browser Runtime Adapter actions.

`src-tauri/src/browser/playwright_mcp_adapter.rs`

- Seeds/updates the internal built-in Playwright MCP server config through
  `McpManager`.
- Applies tool allowlist.
- Calls MCP tools through `McpManager::call_tool`.
- Maps MCP results and artifacts into Browser Runtime provider results.

`src-tauri/src/browser/playwright_cli_adapter.rs`

- Executes official `playwright-cli` actions through controlled commands or
  supported adapter actions.
- Does not let skills execute arbitrary shell commands.
- Maps outputs into Browser Runtime provider results.

### 4.2 Existing Modules to Change

`src-tauri/src/browser/runtime_status.rs`

- Stops composing provider readiness from runtime-pack readiness.
- Composes Browser Automation setup/discovery status.
- Keeps Local Chromium status as fallback.

`src-tauri/src/browser/runtime_control_center.rs`

- Replaces runtime-pack lane reasons with setup/readiness reasons.
- Keeps provider priority and route evidence.

`src-tauri/src/browser/provider_execution.rs`

- Routes CLI actions through `playwright_cli_adapter`.
- Routes MCP actions through `playwright_mcp_adapter`.
- Applies user priority and capability override.

`src-tauri/src/browser/playwright_mcp_sidecar.rs`

- Deleted after the new MCP manager path is runnable.

`src-tauri/src/browser/runtime_pack*.rs`

- Runtime-pack product semantics are removed from provider readiness and UI.
- Reusable step runner/report primitives can move into `playwright_setup.rs`.
- Pack manifest/current pack/rollback/prepare actions are deleted or made
  unreachable by this program.

`src-tauri/src/mcp.rs`

- Gains a built-in Playwright MCP seed/update path, parallel to the existing
  bundled gbrain pattern but without exposing raw tools by default.

`src-tauri/src/app.rs`

- Registers the managed Playwright CLI skills directory with
  `SkillProvenance::Bundled` before user/project skill directories, so official
  Playwright skills appear as uClaw built-ins while user-authored skills can
  still shadow them intentionally.

`src-tauri/src/skills_manifest.rs`

- Filters Playwright built-in skills through the compatibility report before
  adding them to the Agent injected skills manifest.

`ui/src/components/settings/BrowserRuntimeSettings.tsx`

- Becomes Browser Automation Control Center.
- Renders a simplified read model.
- Moves technical details into diagnostics.

## 5. Routing Policy

Default priority:

1. Playwright CLI
2. Playwright MCP
3. Local Chromium

Users can reorder CLI and MCP. Guardrails override ordering:

- unavailable providers cannot be selected;
- disabled providers cannot be selected;
- identity/profile policy can block a lane;
- sensitive/payment boundaries can pause execution;
- raw MCP tools cannot be selected outside the Adapter path.

Capability override can select MCP when:

- the task needs accessibility snapshot or locator discovery;
- the task needs trace exploration;
- CLI returns a failure code explicitly marked `retryable_with_mcp`;
- the user selected "Use MCP for this task".

Fallback from CLI to MCP is allowed only for explicit transferable failure
codes. Runtime/setup failures do not silently fall through to MCP; they surface
as setup diagnostics.

Every route decision records evidence:

- requested provider or user priority source;
- selected provider;
- skipped providers with reasons;
- capability override reason;
- failure fallback reason;
- action id and provider result artifact id.

## 6. Playwright CLI Skills

uClaw treats Playwright CLI skills as a built-in skill pack.

Source:

- Official CLI command: `playwright-cli install --skills`.
- Official package: `@playwright/cli@latest`.

Storage:

- uClaw-managed skills directory, for example
  `~/.uclaw/skills/playwright-cli/`.
- Metadata file records source version, install time, hashes, and compatibility.

Discovery:

- uClaw Agent discovery sees Playwright skills as built-in skills.
- Skills are tagged as Browser Automation capabilities.

Execution:

- Browser actions described by skills map to Browser Runtime Adapter actions.
- Setup/diagnostic commands can run through controlled setup executor.
- Arbitrary shell commands from skills are not executed by ordinary Agent
  paths.

Compatibility:

- Each skill/action must map to a supported uClaw capability.
- Unsupported or incompatible skills are hidden from normal Agent use and shown
  in diagnostics with reason.

## 7. MCP Integration

Playwright MCP is a built-in uClaw MCP server.

Default command:

```json
{
  "id": "playwright",
  "name": "Playwright MCP",
  "transportType": "stdio",
  "command": "npx",
  "args": ["@playwright/mcp@latest"],
  "enabled": true,
  "autoApprove": false,
  "toolAllowlist": [
    "browser_snapshot",
    "browser_navigate",
    "browser_click",
    "browser_type",
    "browser_take_screenshot",
    "browser_start_tracing",
    "browser_stop_tracing"
  ]
}
```

The exact allowlist may change only through tests and explicit review. Raw MCP
tools remain hidden from the ordinary Agent tool pool by default. Advanced raw
tool exposure is out of scope for implementation PRs in this program unless a
later spec approves it.

## 8. Error Handling

Setup errors:

- Missing Node/npm/npx -> `needs_node`.
- npm prefix not writable -> `needs_permission`.
- package fetch failed -> `network_unavailable` or `failed` with stderr.
- old CLI version -> setup runs upgrade to latest.
- skills incompatible -> `degraded` and compatibility diagnostics.
- MCP cannot connect -> `degraded` if CLI ready, `failed` if no lane ready.

Execution errors:

- unsupported CLI action -> route to MCP only if MCP is ready and the failure is
  marked transferable.
- MCP tool error -> surface provider result and route evidence.
- sensitive action -> policy boundary pauses before provider action.
- raw shell request from skill -> blocked outside setup/diagnostic executor.

## 9. Testing and Harness

Rust unit tests:

- discovery state for Node/npm/npx present/missing;
- global CLI present/old/missing;
- setup plan for install/upgrade/skills/MCP;
- no sudo command in default or experimental flow;
- Homebrew-only Node bootstrap planning;
- MCP config seed/update and allowlist;
- provider route with user priority and capability override;
- route evidence for skipped providers;
- skill compatibility mapping.

Frontend tests:

- Control Center ready/degraded/needs_node/setting_up states;
- provider priority reordering;
- diagnostics drawer;
- setup progress and failure messages;
- built-in Playwright skills installed/missing/incompatible states;
- mobile layout without horizontal overflow for provider rows;
- no "runtime pack" primary state copy.

Integration/manual checks:

- macOS with Node/npm/npx installed: setup installs/upgrades CLI, refreshes
  skills, seeds MCP, and routes a simple browser action.
- macOS without Node: Control Center shows `needs_node` and Homebrew experiment
  path if brew is present.
- MCP raw tools are not in ordinary Agent tool pool.
- uClaw Agent discovers Playwright built-in skills after sync.

## 10. Migration Strategy

This is a breaking architecture simplification but should be staged safely.

1. Add the official-runtime spec and tracker update.
2. Add discovery/setup while old runtime pack still exists.
3. Remove runtime-pack readiness gates and UI copy.
4. Seed Playwright MCP through `McpManager`.
5. Wire provider adapters and delete the sidecar.
6. Redesign Control Center and Agent skill integration.

At every stage, Local Chromium remains available as fallback.

## 11. ADR Section 18 Answers

1. **What user intent does this support?**
   Users want browser automation to work without understanding runtime packs,
   package manifests, sidecars, or manual terminal setup. They also want CLI and
   MCP to be easy to choose and reliable for Agent tasks.

2. **What autonomy level can it run at?**
   Read-only discovery can run automatically at startup. Mutating setup can run
   automatically when Control Center opens or browser automation is first needed,
   with visible progress. Experimental Node bootstrap is user-triggered only.

3. **What is the canonical truth source?**
   Browser Automation setup/discovery status from Rust is the truth for provider
   readiness. `McpManager` is the truth for MCP connection/tool state. The
   runtime-pack manifest is no longer a provider readiness truth.

4. **What TaskEvent entries does it emit?**
   Setup events for detection, CLI install/upgrade, skills sync, MCP seed/probe,
   setup failure, setup completion, provider route selection, capability
   override, and provider fallback. Existing event names can be extended through
   focused implementation PRs.

5. **What context does it read, and how is it cited?**
   It reads local command availability, npm/global CLI state, uClaw skills
   metadata, MCP config/status, user provider priority, and Browser Runtime
   policy. Diagnostics cite command paths, versions, stderr/stdout artifacts,
   skill metadata, and route evidence.

6. **What capability cards does it add or consume?**
   It consumes Browser Runtime provider cards and adds/updates Browser
   Automation capability states for Playwright CLI, Playwright MCP, and
   Playwright CLI skills.

7. **What policy hooks can block it?**
   Missing Node/npm/npx, npm prefix permission failures, network failure,
   disabled provider, sensitive action boundary, identity/profile policy,
   incompatible skill, raw shell request, raw MCP exposure, and unsupported
   elevated setup.

8. **What world projection does the UI render?**
   Browser Automation Control Center renders ready/setting_up/needs_node/
   needs_permission/network_unavailable/degraded/failed/disabled with provider
   priority, route evidence, setup progress, and diagnostics.

9. **What harness cases prove it works?**
   Discovery/setup unit tests, MCP manager seed/probe tests, provider route
   tests, skill compatibility tests, Control Center UI tests, and a manual E2E
   setup route on macOS with and without Node.

10. **What is the rollback or disable path?**
    Users can disable auto setup, disable CLI or MCP providers, reorder priority
    to Local Chromium, and remove/refresh built-in Playwright MCP config.
    Implementation PRs can rollback to Local Chromium-only routing.

11. **What does it deliberately not own?**
    It does not own Node.js distribution by default, Homebrew installation,
    Linux/Windows Node bootstrap, raw MCP tool exposure, Playwright package
    forks, official skill content edits, or hosted browser providers.

## 12. Non-Goals

- No silent `sudo`.
- No Homebrew installation.
- No Linux/Windows Node bootstrap in this round.
- No vendored Playwright MCP server.
- No forked official Playwright CLI skills.
- No raw Playwright MCP tools in ordinary Agent tool pool.
- No retained runtime-pack product truth.
- No hosted provider changes.
- No browser identity redesign.

## 13. PR Plan Overview

Implementation is split into six PRs:

1. PR0 - Spec and tracker alignment.
2. PR1 - Playwright system discovery and setup.
3. PR2 - Runtime-pack product truth removal.
4. PR3 - Playwright MCP via existing `McpManager`.
5. PR4 - Browser Runtime Adapter routing and sidecar deletion.
6. PR5 - Control Center and built-in Playwright skills integration.
