# ADR - Browser Runtime Supervisor and Playwright Provider Strategy

- **Status:** Partially implemented, then superseded for Playwright runtime distribution by `docs/superpowers/specs/2026-05-26-browser-automation-official-playwright-runtime-design.md`.
- **Date:** 2026-05-23
- **Deciders:** Ryan Liu + Codex research session with two browser-agent research subagents.
- **Scope:** Browser automation runtime stability, provider strategy, Playwright CLI/MCP integration, recovery, observability, and harness gates.
- **Related code:** `src-tauri/src/browser/`, `src-tauri/src/harness/`, `src-tauri/src/automation/`, `src-tauri/src/agent/`, `ui/src/components/browser/`, `ui/src/hooks/useBrowserScreencast.ts`
- **Related docs:** `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`, `docs/superpowers/specs/2026-05-18-ai-browser-agent-v2-design.md`, `docs/superpowers/specs/2026-05-19-browser-agent-v2-rendering-features-design.md`
- **External references:** Playwright MCP, Playwright Agent CLI, Playwright locators, Playwright tracing, Chrome DevTools Protocol, browser-use/browser-harness, Stagehand observe/cache, Browserbase, Browser Use, Steel, Hyperbrowser, Agent S2.

---

> **Supersession note, 2026-05-26:** The Browser Runtime Supervisor, provider
> policy, identity, artifact, and routing principles remain active. The default
> Playwright runtime distribution strategy changed: uClaw now targets official
> `@playwright/cli@latest` plus `playwright-cli install --skills` for the CLI
> lane, and official `npx @playwright/mcp@latest` through the existing uClaw
> `McpManager` for MCP. The app-managed runtime pack is no longer the default
> CLI/MCP readiness truth.

---

## 1. Executive Decision

uClaw should not directly replace the existing browser runtime with an
unsupervised Playwright CLI process.

The durable direction is:

> Build a first-class `BrowserRuntimeSupervisor`, then run the current
> chromiumoxide runtime, Playwright CLI thin lane, Playwright MCP, and future
> browser backends behind a `BrowserProvider` boundary.

Playwright is a provider strategy, not the browser kernel. The browser kernel must own session lifecycle, health, recovery, artifacts, policy, capability selection, and world projection. Provider implementations only perform browser actions and observations under that contract.

The recommended first new provider is **Playwright CLI Thin Lane** for fast,
bounded, low-token browser actions. **Playwright MCP** is second: use it for
capability discovery, exploratory automation, richer managed tooling, and
ecosystem reach. Raw CDP remains a guarded escape hatch. Hosted providers remain
optional. The important browser-harness lesson is not "let the model run any
browser script"; it is the topology: short foreground command, long-lived
browser runtime, one-request JSON-line IPC, persistent browser connection, event
buffer, real liveness probe, and bounded stale-session repair.

Normal uClaw users should not manually install Playwright CLI, Node packages, or
browser binaries. uClaw owns the Playwright runtime: pinned package versions,
worker scripts, browser revisions, cache paths, one-click install/repair, and
optional offline/prebundled runtime modes.

The primary app bundle should keep using the existing bundled Bun runtime. Do
not add a second full Node runtime directly to the main app bundle for v1.
Instead, Playwright uses an optional uClaw-managed runtime pack that can be
prepared during Startup Splash / Startup Doctor and repaired later from
Settings.

This preserves the Agent OS v2 rule from the north-star ADR:

> Keep the kernel small. Make context queryable. Make capabilities replaceable. Make state observable. Make learning gated. Make autonomy resumable.

---

## 2. Context

The current AI Browser can already navigate, observe DOM, interact with pages, stream screencast frames, keep per-session browser profiles, record browser task runs, and checkpoint browser tasks. The problem is not absence of browser functionality.

The observed product pain is runtime quality:

1. Browser automation is unstable and the toolchain is too long.
2. Browser runs take too long.
3. Tool failures can make the run appear dead or stuck.

The current architecture is strong enough to evolve, but it lacks a first-class browser supervisor:

- `BrowserContextManager` lazily creates per-session Chrome processes and profile directories.
- `BrowserContext` launches Chrome through chromiumoxide and drives a CDP handler.
- `BrowserAgentLoop` observes, decides, executes actions, checkpoints, and retries.
- `dom_state.rs` builds a custom DOM index through injected JavaScript.
- `recovery.rs` classifies errors into a small set of string-based recovery kinds.
- The screencast path streams CDP frames, but first-frame deadlines, heartbeats, and recovery state live outside a single browser lifecycle model.

That shape explains the failure mode: a process can be alive while the page, CDP target, screencast, action future, or tool-result delivery path is unhealthy.

---

## 3. Current Code Truth

The following facts anchor this ADR in the current codebase:

| Area | Current truth | Risk |
|---|---|---|
| Session lifecycle | `BrowserContextManager` keeps live contexts keyed by session/profile and launches Chrome lazily. | No explicit runtime state machine or health ledger. |
| Chrome/CDP ownership | `BrowserContext::launch` starts chromiumoxide and spawns a CDP event loop. Handler exit is logged as a warning. | Crash/detach does not automatically transition the session into recovery. |
| Observation | `dom_state.rs` injects JS, marks interactive nodes with `data-uclaw-index`, and returns indexed elements plus truncated page text. | Custom DOM snapshots are brittle compared with accessibility snapshots and role/test-id locators. |
| Recovery | `recovery.rs` classifies stale tab, stale element, timeout, detached target, and unknown errors. | Recovery is reactive and string-based; it does not own deadlines, process restarts, or artifacts. |
| Screencast | CDP screencast frames are acked and emitted to the frontend. | No single Rust-side first-frame/no-frame deadline or health state owns blank preview recovery. |
| Agent loop | Browser task runs have observe/decide/act/recover/checkpoint flow. | The loop can still wait behind a hung provider call unless deadlines are enforced below it. |

This ADR intentionally avoids code changes. It defines the target boundary that future implementation plans should use.

---

## 4. Options Considered

### Option A - Harden only the existing chromiumoxide runtime

This is low disruption and should be part of the first implementation phase. It does not fully solve long-term provider flexibility. uClaw would still own every locator, trace, context, and browser-action detail itself.

### Option B - Use Playwright CLI as a supervised thin lane

This is recommended as the first new provider when it is behind
`BrowserRuntimeSupervisor`. The CLI lane must be bounded, declarative, and
artifact-producing. Rust owns timeouts, kill/retry, profile state, policy,
TaskEvent emission, and recovery. Node/Playwright is a managed child worker, not
a second control plane.

### Option C - Add Playwright MCP as a provider behind a supervisor

This remains recommended, but as the second lane rather than the first.
Playwright MCP gives uClaw accessibility snapshots, locators, browser contexts,
traces, network/console tools, and a structured action surface while letting
uClaw keep one canonical browser task lifecycle.

### Option D - Adopt a hosted browser-agent platform as the core runtime

Browserbase, Browser Use Cloud, Steel, and Hyperbrowser are useful as optional providers for difficult websites, remote browser pools, replay, or scaling. They should not become the local-first default runtime because uClaw's product identity depends on a local, inspectable, recoverable desktop browser kernel.

---

## 5. Decision

uClaw will treat browser automation as a supervised capability mesh area:

1. Add a `BrowserRuntimeSupervisor` concept before making Playwright the default execution path.
2. Introduce a `BrowserProvider` boundary for browser execution backends.
3. Keep chromiumoxide as the first provider and harden it through the supervisor.
4. Add `PlaywrightCliProvider` as the first new provider for fast bounded actions.
5. Add Playwright MCP as the second provider for richer structured automation,
   discovery, and ecosystem integrations.
6. Add deterministic recipe/action caching for repeated workflows.
7. Emit artifacts on failure, timeout, recovery, and user-requested inspection.
8. Promote any provider only through harness scorecards.

The runtime must never create a second browser truth source. Browser task state remains canonical in uClaw's task/run/event model; providers are replaceable executors.

The first `PlaywrightCliProvider` protocol is deliberately simple:

- uClaw launches its app-managed Playwright runtime, not an arbitrary global
  user install.
- Rust starts a short-lived child process for each bounded action.
- The worker receives one stdin JSON envelope and emits one stdout JSON result
  envelope plus artifact refs.
- The v1 envelope exposes declarative actions by default: `navigate`, `click`,
  `type`, `screenshot`, `extract`, and `wait`.
- Arbitrary Playwright script execution is a policy-controlled escape hatch,
  disabled by default and limited to development or allowlisted profiles.
- Rust keeps warm browser/session metadata and owns per-action timeout, kill,
  retry, and error classification.
- A long-lived JSON-RPC worker may be introduced later only after the
  short-command path is stable.

---

## 6. Target Architecture

```text
Agent / Automation / User Intent
          |
          v
BrowserRuntimeSupervisor
  - session lifecycle state machine
  - deadlines and watchdogs
  - app-managed Playwright runtime install/repair
  - provider selection
  - profile locks and auth scope
  - browser identity grants and revocation
  - artifact collection
  - recovery and restart
  - TaskEvent emission
          |
          v
BrowserProvider trait
  - ChromiumoxideProvider
  - PlaywrightCliProvider
  - PlaywrightMcpProvider
  - StagehandProvider (future)
  - HostedBrowserProvider (future)
          |
          v
Observation / Action / Artifact contracts
          |
          v
World Projection + Harness Scorecards
```

### 6.1 BrowserRuntimeSupervisor

The supervisor owns browser session truth:

- `starting`
- `ready`
- `acting`
- `idle`
- `recovering`
- `degraded`
- `stopped`

A provider process being alive is not sufficient. A healthy session requires:

- provider process alive;
- CDP or provider transport attached;
- active page responds to a heartbeat probe;
- the current action has not exceeded its deadline;
- the provider has emitted progress, output, or a no-output heartbeat inside the configured window;
- Playwright package, worker, and browser revisions match the pinned runtime
  manifest when the Playwright CLI lane is selected;
- profile/auth locks are not shared unsafely across concurrent runs.
- `browser_runtime_doctor` can prove the session with a real browser operation,
  not only a process/socket ping.

### 6.2 BrowserProvider

The provider boundary should expose product-level operations, not every low-level Playwright or CDP command:

- start or attach session;
- stop session;
- list tabs;
- navigate;
- observe;
- act;
- collect artifacts;
- recover;
- manage identity/profile state through the policy-approved provider contract.

The agent should not see raw Playwright MCP tools by default. uClaw should expose a smaller capability card surface with explicit costs, permissions, and harness scores.

### 6.3 Browser Identity and Profile Scope

Browser identities are global user-level resources in v1. Do not introduce
Space/Workspace association for browser identity grants yet; keep the design
simple. A task may choose an authorized identity through policy and runtime
selection, but the identity itself belongs to the local user, not to a project.

Default automation uses a uClaw-isolated profile. The Settings surface may
create a uClaw-managed browser identity through an OAuth-style in-app
authorization flow:

- the user clicks a connect button in Settings;
- uClaw opens a dedicated in-app WebView/Browser authorization window or wizard;
- the user chooses or logs into the browser identity there;
- uClaw stores only the authorization association and profile metadata needed to
  reconnect and audit the grant;
- the authorization window uses the same `BrowserProvider`,
  `BrowserRuntimeSupervisor`, trace, profile metadata, and revocation model as
  normal browser automation.

The v1 authorization flow creates a uClaw-managed browser identity, not a
binding to an external Chrome profile. External Chrome real-profile attach is an
advanced or later mode. Once a uClaw-managed identity is authorized, it does not
require per-domain or per-task reauthorization by default. Payment-related
sensitive actions still trigger the agent `ask_user` confirmation banner.

Settings must show authorized identity status, last-used time, active tasks, and
a one-click revoke action. Revocation blocks new actions, allows the current
action only a short bounded drain window, then moves affected tasks to
`paused_checkpointed`, emits a user-boundary event, and asks whether to switch
to an isolated profile, reauthorize, or end the task.

Browser identity authorization state and profile metadata live in uClaw config
or DB. Local uClaw-managed identities should not require storing cookies,
storage state, or long-lived attach secrets outside their managed profile store.
System keychain is reserved for external provider credentials and other true
secrets, read outside the per-action hot path and cached in memory for the
active session.

### 6.4 App-Managed Playwright Runtime

`BrowserRuntimeSupervisor` owns the Playwright runtime lifecycle. Normal users
should never need `npm install -g`, `npx playwright install`, or terminal setup
commands before using browser automation.

Runtime requirements:

- pin the Playwright package/CLI version, worker script version, and browser
  revision in a uClaw runtime manifest;
- keep the main app bundle on the already-bundled Bun runtime; do not ship a
  second full Node runtime directly in the primary app bundle for v1;
- install the optional runtime pack into uClaw-managed directories, such as
  `~/.uclaw/browser-runtime/playwright/` and
  `~/.uclaw/browser-runtime/ms-playwright/`;
- include a pinned Node runtime, pinned Playwright package, worker scripts, and
  the v1 Chromium browser binary in that managed pack;
- treat Firefox and WebKit as later optional browser targets, not part of the
  default v1 pack;
- set the worker browser cache path explicitly, for example through
  `PLAYWRIGHT_BROWSERS_PATH`, so global user caches do not become product
  state;
- support first-use lazy installation with visible progress UI and automatic
  preparation by default;
- fetch runtime packs through a uClaw-controlled manifest that declares runtime
  pack version, pinned Node version, Playwright package version, worker version,
  Chromium revision, download URLs, artifact size, sha256, minimum compatible
  app version, rollback version, and release channel;
- download signed/hashed uClaw-managed artifacts by default in production, with
  no silent fallback to upstream Playwright install paths;
- allow an explicit upstream fallback only in development builds, labeled as
  developer mode and outside normal product runtime state;
- schedule runtime-pack updates non-disruptively: security updates may prompt
  and take priority, ordinary updates defer to idle time or the next launch, and
  no update replaces a runtime pack while a browser task is using it;
- keep the previous working pack version for rollback until the new version
  passes doctor and harness gates;
- expose Settings and `browser_runtime_doctor` actions for install, repair,
  reinstall, version check, cache cleanup, disabling automatic preparation, and
  offline-mode diagnosis;
- require lightweight user confirmation before downloading when the runtime pack
  is unusually large or the network appears metered, cellular, captive,
  restricted, or offline;
- support offline or enterprise distribution through prebundled runtime
  artifacts;
- treat global Playwright CLI or user npm installs as developer fallback only.

The doctor should classify missing package, missing browser binary, version
mismatch, corrupt cache, worker startup failure, and failed real page probe, then
return a one-click repair path when possible.

### 6.5 Startup Splash and Startup Doctor

uClaw should add a polished Startup Splash / Startup Doctor surface for launch
self-checks. The splash is brand-first: beautiful, elegant,
attention-grabbing, and deliberately designed as a strong uClaw launch moment
before it becomes a diagnostic surface. It should use refined typography,
deliberate motion, depth, excellent spacing, and a premium visual system
tailored to the app's identity. Its visual language should fuse a professional,
trustworthy agent workbench with a restrained sense of futuristic intelligence:
stable and production-grade, but alive through motion, depth, and responsive
state. Motion may carry a strong brand memory, but it must be bounded: first
launch may use a fuller 1.5-2.5 second brand sequence, daily launch should stay
short at roughly 300-800 ms, it must be interruptible/skippable, and reduced
motion settings must fall back to a static or subtle fade-in experience.
Diagnostics should appear as a progressive secondary layer, not as the first
thing the user sees. The default splash should not show a checklist. It should
show one polished status line and minimal progress, then reveal detailed checks
only after the time budget is exceeded, a failure occurs, or the user explicitly
expands diagnostics. This makes browser runtime preparation feel like a
first-class app capability, not a hidden terminal setup step.

In v1, this surface should be the first route rendered by the main Tauri
WebView, with an extremely early render path. A native splash may serve only as a
very short blank-window placeholder or later optimization; it should not be the
primary interactive Startup Doctor in v1. The WebView route can integrate with
React state, World Projection, settings, confirmations, and runtime progress.
Splash visuals must be fully local and bundled; the first impression must not
depend on network access. Use local lightweight shader/CSS/canvas/image assets
as needed. Remote visual assets are allowed only for later optional themes or
updates, never for the v1 launch-critical path.
Generated image or video assets are allowed and should be prepared ahead of time
into the frontend asset tree, such as `ui/src/assets/startup-splash/` for
build-imported assets or `ui/public/startup-splash/` for static public assets.
Generated assets must be optimized for startup, include source prompt/metadata
and license/provenance notes, support reduced-motion fallback, and avoid making
launch dependent on a remote generation service. The v1 primary visual path
should use local static or short-loop WebP/AVIF assets plus CSS/canvas
lightweight motion. Video may be an enhancement, but not the only primary path;
if the desired effect can be achieved with image assets plus CSS/canvas, prefer
that over video. Video may be an enhancement only when image/CSS cannot deliver
the intended effect with acceptable quality or complexity; it must not be the
only primary path and requires a static first frame and reduced-motion fallback.
Splash asset performance is a product requirement: launch-critical visual assets
should stay under a small startup budget, with a v1 target under roughly 2 MB
total for first-screen critical assets; enhancement video and high-resolution
alternates lazy-load after the first frame; the first static frame renders
before any Runtime Doctor or browser-runtime work completes. v1 does not support
multiple splash themes or skinning; it ships one canonical uClaw brand
experience. Seasonal or optional themes can be considered only after the
canonical experience proves quality, performance, and reliability.

Startup Doctor checks:

- local configuration readability;
- DB/migration readiness;
- bundled Bun runtime availability;
- basic permissions needed by the app;
- network availability for optional runtime downloads;
- Playwright runtime manifest status;
- runtime-pack path existence;
- last-known runtime status.

Every launch should run only lightweight runtime checks by default. Heavy checks
such as Chromium binary hashing, Playwright worker smoke test, and bounded
real-page probe run only on first install, version upgrade, previous failure,
user-requested repair, or immediately before a browser task needs the lane.

The splash may start first-use runtime-pack installation automatically by
default. It must show clear visible state and Settings must expose controls for
disabling automatic preparation, clearing the runtime pack, reinstalling, and
repairing it. If the download is unusually large or the network appears metered,
cellular, captive, restricted, or offline, the splash should ask for lightweight
confirmation before downloading.

Disabling automatic preparation only disables startup/background runtime-pack
download. It does not disable Browser automation capability. If the user later
starts a task that needs the browser lane and the runtime pack is absent, uClaw
should show a clear "prepare Browser runtime" confirmation with actions to
prepare now, defer until later, or continue through no-browser lanes when
possible. If the user defers and the task cannot proceed without browser
automation, the task enters `paused_waiting_for_browser_runtime` with a
checkpoint instead of failing. If a no-browser lane can satisfy the request,
uClaw may continue in that mode with visible capability limits.

Runtime preparation must not block the whole app indefinitely. If it exceeds a
short budget, such as 5-8 seconds, uClaw should enter the main UI and continue
preparation in the background. Normal chat, project browsing, settings, and
no-browser lanes remain usable. Browser automation tasks wait on
`preparing_browser_runtime` only when they actually need the browser lane.

Failure is recoverable state, not app-launch failure. A failed setup becomes
`browser_runtime_setup_failed` with retry, repair, reinstall, cleanup, offline
diagnosis, and no-browser fallback options.

Startup Doctor state enters World Projection and emits lightweight TaskEvents
for launch diagnostics, support, and developer visibility. It must not create a
normal user-visible task or pollute the user's task list.

### 6.5.1 Adjacent Product Shell UX Requirements

The browser runtime strategy depends on broader shell UX quality. A codebase
audit found useful foundations and clear upgrade needs:

- `ui/src/App.tsx` still has a generic spinner loading state. Browser runtime
  preparation should land through the branded Startup Splash route instead.
- `ui/src/main.tsx` uses a raw root render-error screen with inline colors and
  stack output. Runtime and startup failures should use branded recovery states
  with expandable developer details.
- `ui/src/components/app-shell/AppShell.tsx` coordinates many global overlays.
  Browser runtime prompts, identity boundaries, Startup Doctor, and repair
  states should consume shared World Projection facts instead of adding another
  isolated overlay truth source.
- `ui/src/components/settings/SettingsPanel.tsx` has a strong navigation and
  primitives foundation. Browser Runtime, Startup Doctor, Browser Identity, and
  provider health should become a first-class Settings destination with deep
  links from runtime prompts and search.
- `ui/src/styles/globals.css` provides theme variables, but operational modules
  still use many hard-coded status colors and shadows. Browser runtime UI should
  use shared semantic status, elevation, and motion tokens from the start.
- Placeholder and debug surfaces should not appear as normal product states.
  Missing runtime, failed doctor, deferred preparation, revoked identity, and
  unavailable provider states need branded recovery or unavailable components.

UX requirements:

- Startup Splash and runtime prompts are brand-first, not engineering-first.
- Normal users see concise status, one primary action, one defer/continue path,
  and optional details.
- Settings shows status, last check, version, artifact size, runtime pack path,
  install/repair/cleanup/rollback, auto-prepare state, and developer fallback
  state.
- All new runtime UI has keyboard access, focus states, contrast across themes,
  reduced-motion behavior, and screenshot coverage.
- Browser runtime surfaces must share World Projection state with shell banners,
  status bars, settings, and task checkpoints.

### 6.6 Execution Priority Ladder

The default browser action ladder is:

1. **No-browser lane:** use HTTP, API, static fetch, or existing structured data
   when the task is read-only or does not require browser state.
2. **Playwright CLI thin lane:** run a bounded declarative action against a warm
   browser context and return a structured result envelope plus artifact refs.
3. **Playwright warm session lane:** reuse browser/page/context state through
   `BrowserRuntimeSupervisor` for multi-step flows.
4. **Guarded raw CDP lane:** use direct CDP only for mechanics that need it:
   compositor-level coordinate actions, target/session repair, dialogs,
   downloads, uploads, iframes, shadow DOM, screenshots, and low-level events.
5. **MCP exploratory lane:** use MCP when a third-party server provides a useful
   capability, when discovery matters more than speed, or when a provider has a
   strong managed integration.
6. **Cloud/remote provider lane:** use Browser Use Cloud, Browserbase,
   Firecrawl, Steel, Hyperbrowser, or another provider only when local execution
   cannot satisfy isolation, scaling, proxy, hostile-site, or deployment
   constraints.

This ordering is part of the provider contract. Implementations should not
promote MCP, raw CDP, or hosted providers ahead of the CLI thin lane unless the
task requires their specific capabilities.

### 6.7 Observation Contract

The standard browser observation should combine:

- URL, title, tab list, active tab;
- accessibility snapshot where available;
- selected DOM snapshot for missing a11y details;
- stable locator candidates;
- current visual screenshot only when needed;
- console tail;
- network/request-failure tail;
- page error tail;
- page fingerprint for recipe cache validation.

This reduces the need for expensive full screenshots and long page-text payloads on every step.

### 6.8 Action Recipe and Domain-Skill Cache

Repeated browser workflows should become cheaper over time:

1. First run uses AI observation, locator generation, and recovery.
2. Successful actions are normalized into a recipe.
3. The recipe is keyed by site, route, DOM/a11y fingerprint, instruction family, and provider version.
4. Later runs validate the fingerprint and replay deterministic locators/scripts.
5. On validation failure, uClaw falls back to observation and updates the recipe only after harness/user approval.

This borrows the strongest idea from Stagehand's `observe()` and action caching without making Stagehand the only runtime.

Browser learning produces candidates, not production mutation. Domain-skill
candidates may capture stable URL patterns, selectors, private API shapes, wait
conditions, iframe/shadow-DOM notes, auth boundaries, and known traps. They must
not store secrets, task diaries, transient pixel coordinates, or private user
data. Promotion requires evidence from the run, harness regression coverage,
redaction review, and rollback.

### 6.9 Runtime Doctor

`browser_runtime_doctor` is both a user-visible diagnosis action and a harness
subject. It should report:

- provider install/version status;
- app-managed Playwright runtime status: package version, worker version,
  browser revision, cache path, install/repair availability, and offline mode;
- Startup Doctor phase and progress: config, DB/migration, Bun runtime,
  permissions, network, runtime manifest, runtime-pack path, last-known runtime
  status, and any deferred heavy check reason;
- runtime pack state: absent, preparing, ready, repairing, failed, offline, or
  background-installing;
- runtime download gate: auto-allowed, awaiting lightweight confirmation,
  metered network, restricted network, offline, disabled in Settings, or
  cleanup requested;
- runtime manifest trust state: channel, artifact URL, size, sha256, minimum app
  version, rollback version, signature/hash validation, and whether any
  developer-only upstream fallback is active;
- runtime update state: none, ordinary deferred, security update available,
  updating at idle, update failed, rollback available, or rollback active;
- browser process and profile status;
- profile mode: isolated uClaw profile, authorized uClaw-managed identity,
  advanced real-profile attach, or remote/cloud profile;
- active context/page/tab identity;
- real page operation probe, such as title, URL, target list, or screenshot;
- stale target/session status and bounded reattach result;
- pending dialog, download, file picker, beforeunload, and auth-wall state;
- active-session event tail and whether background tab noise is filtered;
- timeout class: startup, connect, action, wait, network idle, policy block,
  user boundary, provider crash;
- artifact refs for screenshots, traces, logs, action envelopes, and event tail.

A frozen or half-dead browser runtime is a recoverable runtime state, not an
opaque tool failure.

### 6.10 Artifact Pack

Every timeout, failed action, recovery, user intervention, and provider crash should produce an artifact pack:

- `session_meta.json`
- `last_snapshot.yml`
- `last_screenshot.png`
- `console_tail.jsonl`
- `network_tail.jsonl`
- `page_errors.jsonl`
- `trace.zip` when tracing is enabled or failure requires it
- `provider_log.jsonl`

Artifacts should live under a uClaw-managed home path, not ad hoc temp directories.

---

## 7. Playwright Integration Strategy

### 7.1 Playwright CLI Thin Lane

Playwright CLI Thin Lane is the preferred first new product-provider path
because it can reduce tool-token load, keep actions bounded, and run fast
deterministic browser actions under Rust supervision.

Use it for:

- bounded navigation, click, type, wait, screenshot, and extraction actions;
- deterministic recipe replay;
- locator validation;
- local harness fixture reproduction;
- fast action loops where provider startup/session metadata is already warm.

Addressing order:

1. semantic Playwright addressing: role, label, text, test id, locator;
2. uClaw DOM index or element id when structured observation has identified the
   target;
3. coordinate/compositor fallback for cross-origin iframes, shadow DOM, canvas,
   virtualized lists, rich text editors, and unstable locators.

Screenshots are risk-based, not mandatory after every action. Capture screenshots
for navigation, submit, upload, login/user-boundary events, cross iframe/shadow
DOM work, coordinate fallback, failed retry, and final state. Ordinary `type`,
`wait`, and stable locator clicks may rely on action results plus DOM/state
diffs.

### 7.2 Playwright MCP

Playwright MCP is the second product-provider path because it offers structured
browser automation, snapshots, tracing, profile handling, and configurable
capabilities. It should be spawned as a supervised sidecar, with pinned
package/browser versions, controlled `outputDir`, explicit profile directories,
and a provider capability card.

Use it for:

- exploratory browser automation;
- accessibility-snapshot driven observation;
- reliable locators;
- network/console/page-error artifact collection;
- trace capture on failure;
- local web app testing and harness cases.

Do not expose every MCP tool directly to the model. The supervisor should translate uClaw actions into provider calls.

### 7.3 Playwright Agent CLI / Developer Tooling

Playwright Agent CLI and broader Playwright tooling should still be useful for
developer diagnostics, codegen, locator generation, recipe capture, and
debug-mode experiments. These paths are not the normal user-facing automation
surface and must not bypass `BrowserRuntimeSupervisor`.

### 7.4 Existing chromiumoxide provider

The existing runtime remains valuable:

- local-first and already integrated;
- direct Rust ownership;
- existing task storage, UI projection, and screencast path;
- useful fallback while Playwright MCP matures.

The first implementation phase should wrap and harden the existing path rather than rip it out.

### 7.5 Future hosted providers

Browserbase, Browser Use Cloud, Steel, and Hyperbrowser should be optional adapters for:

- hostile websites;
- remote browser pools;
- scalable parallel browser sessions;
- replay and observability;
- captcha/manual takeover workflows where policy allows.

They should not replace the local-first default until harness data proves a clear reliability win.

---

## 8. Pain Point Mapping

| Pain point | Root cause | Decision |
|---|---|---|
| Browser automation unstable | No single supervisor owns health, CDP detach, page response, action deadlines, and recovery. | Add `BrowserRuntimeSupervisor` with state machine and watchdogs. |
| Toolchain too long | Agent can see or trigger too many low-level browser operations. | Expose a small product-level browser tool surface; keep provider details internal. |
| Runtime too long | Repeated full observation and AI planning; screenshots/DOM scans used too broadly. | Use no-browser lane where possible, a11y/locator-first observations, warm sessions, managed identity/profile reuse, locator/action cache, and scripts for known paths. |
| Manual runtime install would hurt UX | Requiring users to run npm/npx/CLI setup makes browser automation feel unfinished. | Use an app-managed pinned runtime with first-use install, progress UI, and doctor repair. |
| App package could become too large | Shipping Bun plus a second full Node runtime plus all browser binaries in the main bundle would bloat the app. | Keep Bun in the primary app bundle; install a uClaw-managed optional Playwright runtime pack with pinned Node, Playwright, worker scripts, and v1 Chromium. |
| Splash could become a startup trap | Runtime downloads or repair can hang or be slow on poor networks. | Startup Doctor has a short time budget, then enters the main UI while browser runtime preparation continues in the background. |
| Automatic download can surprise users | Large downloads or metered/restricted networks create cost, speed, or enterprise-policy concerns. | Default to automatic preparation, but require lightweight confirmation for unusually large or metered/cellular/captive/restricted/offline downloads and expose Settings disable/cleanup controls. |
| Upstream runtime fallback could drift | Falling back silently to upstream Playwright install paths can change versions, artifacts, or supply-chain evidence outside uClaw control. | Production uses a uClaw-controlled runtime manifest and signed/hashed artifacts; upstream fallback is explicit developer mode only. |
| Runtime update could interrupt automation | Replacing Node/Playwright/browser binaries during a browser task can break sessions or corrupt artifacts. | Security updates may prompt and take priority; ordinary updates defer to idle time or next launch, and previous working packs remain available for rollback. |
| "Disable auto-prepare" could be misunderstood | Users may think Browser automation is fully disabled. | Treat the setting as disabling startup/background downloads only; task-time browser use asks for explicit runtime preparation if the pack is absent. |
| Runtime UI becomes another overlay island | AppShell already hosts many global overlays; adding isolated runtime prompts would fragment truth and focus behavior. | Browser runtime prompts, banners, settings rows, and task pauses consume World Projection and shared recovery primitives. |
| Settings cannot explain runtime state | Browser runtime, identity, doctor, cleanup, rollback, and developer fallback controls may scatter across implementation tabs. | Add a first-class Browser Runtime / Startup Doctor / Browser Identity settings destination with deep links and scannable status groups. |
| Browser runtime states look like debug output | Generic spinners, raw stack traces, placeholders, or hard-coded red/green statuses reduce trust. | Use branded recovery states, semantic status tokens, progressive details, and screenshot coverage across themes. |
| CLI could become a hidden side channel | Playwright scripts could mutate pages without policy, TaskEvent, or artifacts. | Route CLI lane through `BrowserProvider`, `BrowserRuntimeSupervisor`, declarative envelopes, and policy hooks. |
| Tool errors cause fake-dead state | Hung futures or stalled transports do not always turn into user-visible failure artifacts. | Enforce per-action, no-output, heartbeat, and first-frame deadlines; collect artifact packs before retry/restart. |
| Runtime appears alive while unusable | Socket/process ping passes but target/page/action calls hang. | `browser_runtime_doctor` must execute real browser operations and classify stale sessions, dialogs, profile locks, and timeout class. |
| Blank/stale preview failure modes | Screencast and frontend state can drift from tab/provider state. | Supervisor owns preview health and emits world projection events, not just raw frames. |

---

## 9. TaskEvent and World Projection

Future implementation should emit browser-specific TaskEvents rather than private logs only:

- `browser_session_starting`
- `browser_session_ready`
- `browser_provider_selected`
- `browser_observation_captured`
- `browser_action_started`
- `browser_action_progress`
- `browser_action_succeeded`
- `browser_action_failed`
- `browser_recovery_started`
- `browser_recovery_succeeded`
- `browser_recovery_failed`
- `browser_artifact_pack_written`
- `browser_session_degraded`
- `browser_session_stopped`
- `startup_doctor_started`
- `startup_doctor_progress`
- `startup_doctor_completed`
- `startup_doctor_degraded`
- `browser_runtime_preparing`
- `browser_runtime_ready`
- `browser_runtime_setup_failed`
- `browser_runtime_download_confirmation_required`
- `browser_runtime_auto_prepare_disabled`
- `browser_runtime_prepare_prompted`
- `browser_runtime_prepare_deferred`
- `browser_runtime_task_paused_waiting`
- `browser_runtime_no_browser_fallback_selected`
- `browser_runtime_manifest_checked`
- `browser_runtime_artifact_verified`
- `browser_runtime_upstream_fallback_blocked`
- `browser_runtime_update_available`
- `browser_runtime_update_deferred`
- `browser_runtime_updated`
- `browser_runtime_rollback_available`
- `browser_runtime_rollback_performed`
- `browser_runtime_doctor_run`
- `browser_identity_authorized`
- `browser_identity_revoked`
- `browser_identity_boundary`
- `browser_domain_skill_candidate_created`
- `browser_domain_skill_candidate_promoted`
- `browser_domain_skill_candidate_rejected`

The UI should render a world projection that answers:

- which browser session is active;
- which provider is running;
- which URL/tab is current;
- whether the page is healthy;
- what action is in progress;
- what artifact pack explains the last failure;
- whether Startup Doctor is checking, preparing, degraded, or continuing in the
  background;
- the latest Startup Doctor status without creating a normal task-list item;
- whether browser automation is waiting on `preparing_browser_runtime`;
- whether the runtime is retrying, waiting for user input, or stopped.
- which browser identity/profile mode is in use;
- whether an identity grant can be revoked or needs reauthorization.
- whether a domain-skill or recipe candidate was created and whether it is
  pending, promoted, or rejected.

---

## 10. Policy and Safety

Browser automation must remain policy-controlled:

- uClaw-managed browser identities require one-time explicit Settings
  authorization and visible revocation;
- external Chrome real-profile attach is advanced/later-mode only;
- shared profile reuse must be locked or disallowed across concurrent sessions;
- file upload/download operations require policy hooks;
- payment-related sensitive actions require the agent `ask_user` confirmation
  banner;
- posting, account changes, and irreversible actions require higher autonomy gates;
- provider secrets must be redacted from logs and artifact packs;
- keychain reads must stay out of the per-action hot path;
- hosted providers require a separate capability card and data-boundary policy;
- domain-skill candidates must be redacted and harness-gated before promotion;
- arbitrary provider-side code execution must be disabled unless the user grants a debug/developer capability.

The supervisor should downgrade autonomy or ask the user when provider confidence is low, auth is missing, the site changed, or a recipe fingerprint no longer matches.

---

## 11. Harness and Verification Strategy

Provider promotion requires browser harness cases, not anecdotal demos.

Minimum fixture set:

- Startup Doctor self-check success, degraded background continuation, and
  `browser_runtime_setup_failed` repair flow;
- Startup Splash first-frame render, reduced-motion fallback, details-expanded
  diagnostics, and branded recovery states across core themes;
- Browser Runtime / Startup Doctor / Browser Identity Settings status, deep
  links, cleanup, repair, rollback, and developer fallback controls;
- Playwright CLI declarative action envelope success and failure;
- hung CLI worker timeout, kill, and retry classification;
- no-browser lane for read-only/static extraction;
- `browser_runtime_doctor` real-operation probe and stale session classification;
- slow navigation;
- popup/dialog handling;
- target closed / page crash;
- stale tab id;
- stale element / detached DOM;
- uClaw-managed browser identity authorization, reuse, revocation, and resume;
- file upload/download policy gate;
- payment-related `ask_user` confirmation boundary;
- domain-skill candidate redaction, harness gate, promotion, and rejection;
- screencast first-frame timeout;
- network failure and request-failed logging;
- known repeatable recipe flow;
- hosted-provider disabled fallback;
- user intervention checkpoint and resume.

Metrics:

- success rate;
- mean/median/p95 task duration;
- per-step latency;
- number of model calls;
- number of provider calls;
- recovery success rate;
- artifact completeness;
- false-positive recovery rate;
- cost per successful task.

Promotion rule:

> A provider can become default only after it beats the current default on reliability without regressing observability, policy, or local-first behavior.

---

## 12. Phased Plan

The implementation must land as a sequence of reversible slices. Each phase
keeps uClaw's task/run/event model as canonical truth and avoids exposing raw
provider tools directly to the model.

### Phase 0 - Contracts, flags, and projection skeleton

Define the product-level contracts before adding new runtime behavior:

- `BrowserProvider` interface shape for start/attach, stop, tabs, navigate,
  observe, act, collect artifacts, recover, and identity/profile management;
- `BrowserRuntimeSupervisor` state names and state-transition rules;
- browser-specific `TaskEvent` names from section 9;
- capability cards for Chromiumoxide, Playwright CLI, Playwright MCP, and future
  hosted providers;
- feature flags for Playwright CLI, Playwright MCP, hosted providers, runtime
  auto-prepare, developer upstream fallback, and external real-profile attach;
- initial World Projection model for startup, runtime, provider, identity,
  task pause/resume, and degraded states.

Expected outcome: later phases add behavior behind stable contracts and visible
projection fields, not ad hoc UI or provider state.

### Phase 1 - Supervisor around the current chromiumoxide runtime

Wrap the existing chromiumoxide path with the supervisor before introducing
Playwright:

- session lifecycle state machine: `starting`, `ready`, `acting`, `idle`,
  `recovering`, `degraded`, `stopped`;
- deadlines for startup, connect, action, wait, network idle, first frame, and
  no-output heartbeat;
- `browser_runtime_doctor` real-operation probe, not only process/socket ping;
- stale target/session detection and one bounded reattach retry;
- dialog/download/file picker/beforeunload/auth-wall detection;
- artifact packs for timeout, failed action, recovery, user intervention, and
  provider crash;
- screencast first-frame/no-frame health projection;
- no-browser lane for read-only/static/API extraction where browser state is not
  needed.

Expected outcome: today's fake-dead states become explicit `degraded`, `failed`,
`recovering`, or `paused_checkpointed` states with artifacts and user-visible
projection.

Gate: current browser harness cases still pass through chromiumoxide, and stale
tab/page/action hangs produce classified events plus artifact packs.

### Phase 2 - App-managed Playwright runtime pack

Build the runtime manager before Playwright CLI becomes a product lane:

- optional runtime pack outside the primary app bundle: pinned Node runtime,
  pinned Playwright package, worker scripts, and v1 Chromium binary;
- uClaw-managed install/cache directories and explicit `PLAYWRIGHT_BROWSERS_PATH`;
- uClaw-controlled runtime manifest with pack version, Node version, Playwright
  version, worker version, Chromium revision, URL, size, sha256, minimum app
  version, rollback version, and release channel;
- signed/hashed production artifacts with no silent upstream fallback;
- explicit developer-mode upstream fallback only for local iteration;
- install, repair, reinstall, version check, cleanup, rollback, and offline
  diagnosis actions;
- default automatic preparation, but lightweight confirmation for unusually
  large, metered, cellular, captive, restricted, or offline downloads;
- non-disruptive update policy: security updates may prompt and take priority,
  ordinary updates defer to idle/next launch, and active tasks keep their current
  pack version;
- previous working pack retained until the new pack passes doctor and harness
  gates.

Expected outcome: users never manually install Node, npm, Playwright CLI, or
browser binaries, and production runtime state is auditable and rollbackable.

Gate: runtime pack install/repair/cleanup/rollback works without a browser task,
and doctor can classify missing package, missing browser binary, corrupt cache,
version mismatch, worker startup failure, offline download, and failed real-page
probe.

### Phase 3 - Startup Splash, Startup Doctor, and shell UX

Replace generic initialization with the branded startup experience and shell
state plumbing:

- main Tauri WebView first route with extremely early first frame;
- fully local bundled visual assets under `ui/src/assets/startup-splash/` or
  `ui/public/startup-splash/`;
- canonical uClaw brand experience only, no v1 theme/skinning system;
- static or short-loop WebP/AVIF plus CSS/canvas motion as primary path; video
  only as optional enhancement with static first frame and reduced-motion
  fallback;
- first-screen critical assets target under roughly 2 MB;
- first launch brand sequence 1.5-2.5 seconds, daily launch 300-800 ms,
  interruptible/skippable, reduced-motion friendly;
- default status line plus minimal progress, with detailed checks revealed only
  on timeout, failure, or user expansion;
- lightweight checks every launch: config, DB/migration, Bun runtime,
  permissions, network status, runtime manifest, runtime-pack path, last-known
  runtime status;
- heavy checks only on first install, upgrade, previous failure, repair, or
  task-time browser use;
- Splash enters main UI after the time budget while background preparation
  continues;
- Startup Doctor emits lightweight TaskEvents into World Projection without
  creating a normal user-visible task;
- branded recovery surfaces for root render error, runtime setup failure,
  offline mode, deferred preparation, and unavailable provider states.

Expected outcome: startup feels premium and fast while runtime preparation is
visible, recoverable, and never an indefinite app-launch trap.

Gate: screenshot checks cover first frame, daily launch, details-expanded
diagnostics, failure/recovery, reduced motion, and core themes.

### Phase 4 - Browser Runtime settings and task-time preparation UX

Make runtime state controllable before broad provider rollout:

- first-class Browser Runtime / Startup Doctor / Browser Identity Settings
  destination;
- visible status, last check, version, artifact size, runtime pack path, release
  channel, update state, rollback availability, developer fallback state, and
  auto-prepare state;
- install, repair, reinstall, cleanup, rollback, disable-auto-prepare, and
  run-doctor controls;
- Settings deep links from SearchPalette, Startup Doctor, task-time runtime
  prompts, and error/recovery surfaces;
- "disable automatic preparation" disables only startup/background downloads,
  not Browser automation capability;
- task-time "prepare Browser runtime" confirmation with actions to prepare now,
  defer, or continue with no-browser lanes when possible;
- deferral checkpoints tasks as `paused_waiting_for_browser_runtime` unless a
  no-browser fallback can satisfy the request.

Expected outcome: users can see, control, repair, defer, and resume browser
runtime setup without understanding CLI tooling.

Gate: settings harness covers status, deep links, cleanup, repair, rollback,
auto-prepare disabled, task-time prompt, defer, and no-browser fallback.

### Phase 5 - Playwright CLI thin lane behind a feature flag

Add the first new provider only after runtime management and user-facing setup
are ready:

- `PlaywrightCliProvider` using the app-managed runtime pack, not global npm or
  global browser caches;
- short-lived child process per bounded action;
- stdin/stdout JSON envelope: one request, one result, artifact refs, structured
  error classification;
- declarative v1 actions: `navigate`, `click`, `type`, `screenshot`, `extract`,
  and `wait`;
- arbitrary Playwright script escape hatch disabled by default and available
  only in dev/allowlisted policy profiles;
- Rust supervisor owns warm browser/session metadata, action timeout, kill,
  retry, and recovery;
- addressing order: semantic Playwright locator first, uClaw DOM index/element id
  second, coordinate/compositor fallback last;
- risk-based screenshot capture, not screenshots after every action;
- action result plus DOM/state diff for stable locator clicks, `type`, and
  `wait`;
- no promotion ahead of chromiumoxide until harness data proves reliability.

Expected outcome: selected local fixture flows run faster and with fewer model
tokens through the CLI thin lane while preserving policy, artifacts, projection,
and canonical task truth.

Gate: CLI fixtures cover success/failure envelopes, hung worker timeout/kill,
locator fallback, coordinate fallback, risk screenshot policy, artifact refs,
and no raw script by default.

### Phase 6 - Browser identity authorization and profile UX

Add consented identity only after isolated-profile automation is supervised:

- default isolated uClaw profile remains the baseline;
- Settings connect flow creates a global user-level uClaw-managed browser
  identity, with no Space/Workspace association in v1;
- in-app WebView/Browser authorization window uses the same `BrowserProvider`,
  `BrowserRuntimeSupervisor`, trace, profile metadata, and revocation model as
  normal automation;
- Settings shows authorized identity status, last-used time, active tasks, and
  one-click revoke;
- revocation blocks new actions, allows a short bounded current-action drain,
  moves affected tasks to `paused_checkpointed`, emits a user-boundary event,
  and asks whether to switch to isolated profile, reauthorize, or end the task;
- system keychain is reserved for external provider credentials/true secrets,
  not per-action hot-path reads;
- payment-related sensitive actions use the agent `ask_user` confirmation banner;
- external Chrome real-profile attach remains advanced/later-mode only.

Expected outcome: users get a visible, revocable, global browser identity
without repeated per-domain prompts or hidden profile attachment.

Gate: identity harness covers authorize, reuse, active task display, revoke,
bounded drain, paused checkpoint, isolated-profile fallback, reauthorize, end
task, and payment confirmation.

### Phase 7 - Playwright MCP sidecar behind a feature flag

Add MCP as the second provider lane, not as the main execution path:

- spawn Playwright MCP as a supervised provider with pinned package/browser
  versions, explicit output directory, controlled profile/storage state, and
  provider-level timeouts;
- expose a small uClaw browser capability card surface rather than raw MCP tools
  to the model;
- use MCP for exploratory automation, accessibility-snapshot observation,
  locator discovery, trace capture, and ecosystem integrations;
- route MCP artifacts and errors through the same supervisor, TaskEvent,
  artifact, policy, and projection model;
- ensure MCP does not outrank CLI thin lane unless the task requires MCP-specific
  capability.

Expected outcome: uClaw gains richer exploratory browser automation without
creating a second browser truth source or raw tool surface.

Gate: MCP harness covers snapshot/locator discovery, trace output, timeout,
profile isolation, disabled fallback, and no raw MCP exposure by default.

### Phase 8 - Provider abstraction, parity harness, and default selection

Unify provider routing and compare providers with evidence:

- route chromiumoxide, Playwright CLI, and Playwright MCP through
  `BrowserProvider`;
- maintain provider capability cards with permissions, costs, supported actions,
  observation modes, auth/profile behavior, runtime/install behavior, artifacts,
  harness score, and disable path;
- compare providers on the same fixture suite and metrics from section 11;
- emit provider selection, provider degradation, and provider rollback events;
- make provider default selection data-driven and reversible.

Expected outcome: provider choice becomes a runtime policy decision backed by
scorecards, not a code fork or preference.

Gate: the same browser harness case can run against chromiumoxide, Playwright
CLI, Playwright MCP where appropriate, and a mock hosted provider; disabling a
provider falls back without losing artifacts.

### Phase 9 - Recipes, locator cache, and domain-skill candidates

Reduce repeated-task runtime only after provider behavior is observable:

- normalize successful browser actions into deterministic recipes;
- key recipes by site, route, DOM/a11y fingerprint, instruction family, and
  provider version;
- validate fingerprint before replay and fall back to observation on mismatch;
- generate domain-skill candidates with stable URL patterns, selectors, private
  API shapes, wait conditions, iframe/shadow-DOM notes, auth boundaries, and
  known traps;
- reject secrets, task diaries, transient pixel coordinates, and private user
  data from candidates;
- require evidence, redaction review, harness regression coverage, promotion
  state, and rollback before production use.

Expected outcome: repeated browser tasks become faster and cheaper without
silently mutating production behavior.

Gate: recipe/domain-skill harness covers replay success, fingerprint mismatch,
redaction, promotion, rejection, rollback, and provider-version invalidation.

### Phase 10 - Optional hosted providers and hard-site escape hatches

Add hosted browser systems only as opt-in provider adapters:

- Browserbase, Browser Use Cloud, Steel, Hyperbrowser, or similar providers live
  behind `BrowserProvider` and capability cards;
- hosted providers require explicit data-boundary policy, profile/storage
  policy, artifact handling, cost visibility, and disable path;
- use hosted providers only for isolation, scaling, proxy, hostile-site,
  CAPTCHA/manual takeover, or deployment constraints that local providers cannot
  satisfy;
- never make hosted infrastructure the default local runtime.

Expected outcome: uClaw gains hard-site escape hatches while preserving
local-first defaults and reversible provider routing.

Gate: hosted-provider disabled fallback, data-boundary prompt, artifact capture,
cost visibility, and local-provider fallback are covered by harness.

### Phase Coverage Check

This phased plan covers the ADR's browser implementation surface:

- sections 6.1-6.2: Phases 0, 1, 5, 7, and 8;
- section 6.3: Phase 6;
- section 6.4: Phase 2;
- sections 6.5 and 6.5.1: Phases 3 and 4;
- sections 6.6-6.7: Phases 1 and 5;
- section 6.8: Phase 9;
- section 6.9: Phases 1, 2, 3, 4, 5, 7, and 8;
- section 6.10: Phases 1, 5, 7, 8, 9, and 10;
- section 7.1: Phase 5;
- section 7.2: Phase 7;
- section 7.3: Phases 2, 5, and 7;
- section 7.4: Phases 1 and 8;
- section 7.5: Phase 10;
- sections 8-11: gates across all phases;
- section 13 rollback/disable answers: Phases 2, 4, 6, 8, 9, and 10;
- section 14 non-goals: enforced by phase gates and feature flags.

---

## 13. ADR Section 18 Checklist

### 1. What user intent does this support?

It supports user requests that require browser automation: inspect a site, log in with consent, operate web apps, run UI workflows, collect evidence, automate repeated browser tasks, and recover from long-running browser failures.

### 2. What autonomy level can it run at?

Default browser automation should run at L1-L3. L4+ requires high harness scores, explicit policy approval, and no irreversible user-impacting actions. Payment, posting, account modification, and external side effects require user confirmation unless a future policy profile explicitly permits them.

### 3. What is the canonical truth source?

The canonical truth source is uClaw's task/run/event model: browser task runs, browser task steps, checkpoints, artifact packs, and TaskEvent streams. Provider-internal state is execution detail, not product truth.

### 4. What TaskEvent entries does it emit?

It emits the browser lifecycle events listed in section 9, including session start, provider selection, observations, actions, failures, recoveries, runtime doctor checks, artifact writes, identity grants/revocations, domain-skill candidate lifecycle, degraded state, and stop.

Startup Doctor emits lightweight launch-diagnostic TaskEvents into World
Projection, but does not create a normal user-visible task.

### 5. What context does it read, and how is it cited?

It reads active task intent, browser session state, auth/profile policy, current page observations, prior browser task memory, recipe cache entries, provider capability cards, and harness results. Any model-visible observation should cite the artifact or snapshot id that produced it.

### 6. What capability cards does it add or consume?

It consumes provider cards for chromiumoxide, Playwright CLI, Playwright MCP, and future hosted providers. Each card must declare permissions, cost, supported actions, supported observation modes, auth/profile behavior, app-managed runtime/install behavior where relevant, artifact support, harness score, and disable path.

### 7. What policy hooks can block it?

Policy hooks can block profile use, identity grant/revocation edge cases, external posting, purchases, credentials entry, file upload/download, hosted-provider data egress, unsafe code execution, untrusted domains, and actions above the task's approved autonomy level.

### 8. What world projection does the UI render?

The UI renders provider, session health, browser identity/profile mode, active
tab, current URL, in-progress action, recovery state, last artifact pack,
user-intervention boundary, and whether the session is running locally or
through a hosted provider. Startup, runtime preparation, doctor, identity, and
provider health states appear consistently across Splash, shell banners,
settings, browser panels, and task checkpoints.

### 9. What harness cases prove it works?

The harness cases in section 11 prove provider reliability, recovery behavior,
artifact completeness, timeout handling, Startup Doctor behavior, runtime
doctor behavior, screencast health, uClaw-managed browser identity
authorization/revocation, recipe/domain-skill replay, and rollback.

### 10. What is the rollback or disable path?

Each provider must be feature-flagged and individually disabled. Playwright CLI and Playwright MCP can be turned off without removing chromiumoxide. The app-managed Playwright runtime cache can be repaired, reinstalled, or cleaned independently of provider enablement. Hosted providers can be disabled without affecting local runtime. A failed provider promotion rolls back to the previous default provider and keeps artifact evidence. Browser identity grants can be revoked independently of provider enablement.

### 11. What does it deliberately not own?

It does not own general agent planning, long-term memory, unrelated MCP server
management, automation scheduling, the full chat UI redesign, or hosted browser
vendor selection beyond the provider interface. It does not replace the Agent OS
runtime kernel. It does own the browser-runtime-adjacent UX contract: startup
runtime status, Browser Runtime settings, browser identity visibility, runtime
repair/recovery states, and browser task pause/resume affordances.

---

## 14. Non-Goals

- Do not expose raw Playwright tools directly as the main agent browser surface.
- Do not make Playwright CLI the browser kernel or a shell-like script runner.
- Do not make Playwright MCP the first execution path when the CLI thin lane can
  satisfy the task faster under supervision.
- Do not require normal users to install Playwright CLI, Node packages, or
  browser binaries manually.
- Do not depend on global npm packages or global Playwright browser caches for
  production app behavior.
- Do not ship a second full Node runtime in the primary app bundle for v1; Node
  belongs to the optional uClaw-managed Playwright runtime pack.
- Do not block app launch indefinitely on Playwright runtime download or repair.
- Do not silently perform unusually large or metered/restricted runtime
  downloads without lightweight confirmation.
- Do not treat "disable automatic preparation" as disabling Browser automation;
  it only disables startup/background runtime downloads.
- Do not silently fall back to upstream Playwright install paths in production.
  Upstream fallback is explicit developer mode only.
- Do not replace a runtime pack while an active browser task is using it.
- Do not delete the existing chromiumoxide runtime before parity and harness data exist.
- Do not introduce a second browser task database.
- Do not introduce Space/Workspace-scoped browser identity in v1.
- Do not bind the v1 authorization window to an external Chrome profile.
- Do not self-promote domain skills, helper patches, or site playbooks into
  production behavior.
- Do not make hosted browser infrastructure the default local runtime.
- Do not enable arbitrary browser-side code execution for normal users.
- Do not always record trace/video; capture them by policy, failure, or debug mode.

---

## 15. Sources and Research Notes

Official references:

- [Playwright MCP introduction](https://playwright.dev/mcp/introduction)
- [Playwright MCP configuration](https://playwright.dev/mcp/configuration/options)
- [Playwright MCP user profile modes](https://playwright.dev/mcp/configuration/user-profile)
- [Playwright MCP tracing](https://playwright.dev/mcp/tools/tracing)
- [Playwright Agent CLI introduction](https://playwright.dev/agent-cli/introduction)
- [Playwright Agent CLI installation](https://playwright.dev/agent-cli/installation)
- [Playwright Agent CLI capabilities](https://playwright.dev/agent-cli/capabilities)
- [Playwright CLI and test CLI](https://playwright.dev/docs/test-cli)
- [Playwright browser binaries and cache path](https://playwright.dev/docs/browsers)
- [Playwright codegen](https://playwright.dev/docs/codegen)
- [Playwright locators](https://playwright.dev/docs/locators)
- [Playwright browser contexts](https://playwright.dev/docs/browser-contexts)
- [Playwright trace viewer](https://playwright.dev/docs/trace-viewer-intro)
- [Playwright timeouts](https://playwright.dev/docs/test-timeouts)
- [Playwright retries](https://playwright.dev/docs/test-retries)
- [Chrome DevTools Protocol Target domain](https://chromedevtools.github.io/devtools-protocol/tot/Target/)
- [browser-use/browser-harness](https://github.com/browser-use/browser-harness)
- [browser-harness SKILL.md](https://github.com/browser-use/browser-harness/blob/main/SKILL.md)
- [browser-harness install.md](https://github.com/browser-use/browser-harness/blob/main/install.md)

Advanced browser-agent references:

- [Stagehand observe](https://docs.stagehand.dev/v3/basics/observe)
- [Stagehand caching](https://docs.stagehand.dev/v3/best-practices/caching)
- [Browserbase](https://www.browserbase.com/)
- [Browser Use](https://browser-use.com/)
- [Steel](https://steel.dev/)
- [Hyperbrowser](https://www.hyperbrowser.ai/)
- [Agent S2 paper](https://arxiv.org/abs/2504.00906)

Research conclusions:

- Playwright CLI thin lane is the first new provider path when it is supervised
  by Rust, bounded by declarative envelopes, and artifact-producing.
- Playwright browser binaries are version-coupled to the Playwright runtime and
  have explicit install/cache-path mechanics. uClaw should hide those mechanics
  behind an app-managed runtime manager, not require normal users to run CLI
  setup commands.
- Global `@playwright/cli` and global browser caches are acceptable developer
  fallbacks, but they should not be production app state.
- Because uClaw already bundles Bun, v1 should not increase the primary app
  bundle by embedding a second full Node runtime there. Node belongs inside the
  optional Playwright runtime pack.
- Runtime-pack delivery should use a uClaw-controlled manifest with artifact
  size, sha256, compatibility bounds, rollback version, and release channel.
  Production should use signed/hashed uClaw-managed artifacts; upstream
  Playwright install fallback is developer-mode only.
- Startup Splash / Startup Doctor should make first-use preparation visible and
  repairable while preserving graceful app launch when downloads are slow or
  unavailable.
- MCP is the second lane for discovery, ecosystem reach, and richer exploratory
  capability.
- Browser-harness's useful lesson is the runtime topology: short foreground
  command, persistent browser runtime, one-request IPC, persistent browser
  connection, event buffer, real liveness probe, and bounded stale-session
  repair.
- Semantic locators should be tried first, uClaw DOM index/element id second,
  and coordinate/compositor input as a first-class fallback.
- The no-browser lane is part of the browser strategy: read-only/static pages
  and APIs should bypass browser automation when possible.
- `browser_runtime_doctor` should be implemented as a real browser-operation
  probe, not a process/socket ping.
- Browser providers should be swappable, but provider truth should not become product truth.
- Session health requires more than process liveness.
- Artifact-first recovery is more valuable than blind retries.
- Deterministic recipes are the main path to shorter runtime on repeated tasks.
- Domain skills and site playbooks are executable-knowledge candidates and must
  go through redaction, harness gates, and rollback.
- uClaw-managed browser identity is global user-level in v1; keep it simple and
  do not introduce Space/Workspace identity association yet.
- Hosted browser systems are useful provider adapters, not the local-first default.

---

## 16. Verification for this ADR

This ADR is documentation-only. It introduces no code, schema, or runtime behavior.

Suggested verification command for the documentation PR:

```bash
git diff --check
```

Expected output:

```text
<no output>
```

Future implementation plans should add symbol-level GitNexus impact analysis before modifying any runtime code.
