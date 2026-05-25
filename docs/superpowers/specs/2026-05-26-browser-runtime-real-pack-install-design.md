# Browser Runtime Real Pack Install Design

Date: 2026-05-26
Status: Proposed
Owner: Browser Runtime Supervisor track

## Summary

Make the Browser Runtime Control Center install and validate a real
uClaw-managed Playwright runtime pack instead of stopping at dry-run previews.
The first productized path is an app-managed complete macOS arm64 runtime pack
installed into `~/.uclaw/browser-runtime/packs/browser-runtime-pack-v1`.

The selected approach is Product Installer plus Dev Generator:

- Runtime pack source resolver finds a complete source pack by priority:
  explicit env override, app bundle resource, then dev staging source.
- Rust installer reuses the existing local step runner to execute confirmed
  `prepare` and `repair` actions.
- Dev tooling generates and validates a full macOS arm64 pack without checking
  large Node or Chromium artifacts into git.
- The Settings Control Center changes `Needs runtime pack` from a dead-end
  status into a dry-run -> confirm -> execute flow.

This design keeps Local Chromium fallback available and does not add remote
signed downloads in the first implementation.

## Current Truth

The Browser Runtime Control Center now correctly shows Playwright CLI first,
Playwright MCP second, and Local Chromium fallback. CLI and MCP still show
`Needs runtime pack` on a fresh machine because the runtime doctor probes real
files under the uClaw runtime home and those files are not present.

Required files currently include:

- `runtime-pack.manifest.json`
- `node/bin/node`
- `node_modules/playwright`
- `node_modules/@playwright/mcp`
- `worker/uclaw-playwright-worker.mjs`
- `ms-playwright/chromium-1181/.../Chromium`

Existing Rust pieces already cover much of the installer shape:

- `BrowserRuntimePackManifest::v1_default()` defines the pinned versions.
- `probe_runtime_pack_filesystem` checks the real filesystem.
- `diagnose_runtime_pack` returns ready/prepare/repair states.
- `plan_runtime_pack_operation` models prepare, repair, cleanup, rollback, and
  keep-current.
- `BrowserRuntimePackLocalStepRunner` can install from an app-managed staging
  source, but no product IPC currently executes it from Settings.
- `dry_run_browser_runtime_action` is no-side-effect only.

## Goals

- Install a complete real macOS arm64 runtime pack from an app-managed source.
- Make Settings `Prepare runtime pack` perform a confirmed real install.
- Preserve the existing dry-run preview before any file-writing action.
- Provide a dev-only generator and validator for local pack creation.
- Keep real pack artifacts out of git.
- Turn CLI/MCP from `Needs runtime pack` to `Needs probe` or `Ready` after
  prepare succeeds.
- Let CLI become active route after its provider probe passes.
- Keep Playwright MCP second priority, advanced, and raw-tool-guarded.
- Keep Local Chromium as fallback.

## Non-Goals

- Do not implement signed remote runtime-pack download in this slice.
- Do not commit Node, Playwright, MCP, or Chromium binaries to git.
- Do not remove Local Chromium fallback.
- Do not expose raw Playwright MCP tools to the model.
- Do not enable destructive `reinstall`, `cleanup`, or `rollback` execution in
  the first real installer.
- Do not support Windows or Linux packs in the first implementation.
- Do not use global npm, global Playwright caches, or user-managed browser
  installs as silent product dependencies.

## Platform Scope

First implementation is macOS arm64 only.

On unsupported platforms, resolver and UI should return a clear unsupported or
pack-unavailable state. They must not claim CLI/MCP are routable. Future work
can add macOS x64, Windows, and Linux packs with the same resolver and
installer interfaces.

## Runtime Pack Layout

The complete source and installed pack use the same layout:

```text
browser-runtime-pack-v1/
  runtime-pack.manifest.json
  node/
    bin/node
  node_modules/
    playwright/
    @playwright/mcp/
  worker/
    uclaw-playwright-worker.mjs
  ms-playwright/
    chromium-1181/
      chrome-mac/Chromium.app/Contents/MacOS/Chromium
```

The manifest must match `BrowserRuntimePackManifest::v1_default()` for:

- `pack_version = browser-runtime-pack-v1`
- `node_version = 22.16.0`
- `playwright_version = 1.53.0`
- `playwright_mcp_version = 0.0.75`
- `worker_version = 0.1.0`
- `chromium_revision = 1181`

Doctor readiness requires manifest match plus required paths. The validator
also checks executable/runtime behavior where practical.

## Source Resolution

Add a focused Rust resolver module, for example
`src-tauri/src/browser/runtime_pack_source.rs`.

Resolution priority:

1. Explicit env override:
   `UCLAW_BROWSER_RUNTIME_PACK_SOURCE=/path/to/browser-runtime-pack-v1`
2. App bundle resource:
   `browser-runtime-pack/browser-runtime-pack-v1`
3. Dev staging source:
   `src-tauri/.runtime-pack-staging/browser-runtime-pack-v1`

Env override comes first because it is an explicit operator/developer choice.
Bundle resource remains the product path. Dev staging is the local development
path generated by repo tooling.

The resolver returns structured data:

```rust
BrowserRuntimePackSourceResolution {
    status: Found | Missing | Invalid | UnsupportedPlatform,
    source_kind: EnvOverride | BundleResource | DevStaging,
    source_dir,
    manifest,
    validation_errors,
}
```

Installer code only proceeds when resolution is `Found` and validation passes.
Missing or invalid source is surfaced to UI with concrete missing paths or
version mismatches.

## Dev Generator And Validator

Add dev-only scripts:

```text
scripts/browser-runtime/generate-runtime-pack.mjs
scripts/browser-runtime/validate-runtime-pack.mjs
src-tauri/.runtime-pack-staging/browser-runtime-pack-v1/   # gitignored
```

Default generator flow:

1. Create a temporary build directory.
2. Download pinned Node `22.16.0` for macOS arm64.
3. Install `playwright@1.53.0` and `@playwright/mcp@0.0.75` into pack
   `node_modules`.
4. Install Playwright Chromium into pack-local `ms-playwright`.
5. Copy or generate `worker/uclaw-playwright-worker.mjs`.
6. Write `runtime-pack.manifest.json`.
7. Run validator.
8. Publish the staging source to
   `src-tauri/.runtime-pack-staging/browser-runtime-pack-v1`.

Explicit dev escape hatch:

```bash
node scripts/browser-runtime/generate-runtime-pack.mjs --from-local-toolchain
```

This mode may copy from local toolchain assets, but it must be explicit in the
command and logs. It is not a silent fallback and is not the release path.

Validator command:

```bash
node scripts/browser-runtime/validate-runtime-pack.mjs \
  src-tauri/.runtime-pack-staging/browser-runtime-pack-v1
```

Validator checks:

- manifest versions
- required files and directories
- `node --version`
- `node -e "require('playwright')"`
- `node -e "require('@playwright/mcp/package.json')"`
- Chromium binary exists and is executable on macOS
- worker script exists

## Execution IPC

Keep existing dry-run command:

```rust
dry_run_browser_runtime_action(action)
```

Add a real execution command:

```rust
execute_browser_runtime_action(
    action: BrowserRuntimePackAction,
    confirmed: bool,
) -> BrowserRuntimePackExecutionReport
```

Execution rules:

- `confirmed = false` refuses real file writes and returns a confirmation
  required result or error.
- `prepare` and `repair` are executable in the first implementation.
- `reinstall`, `cleanup`, and `rollback` remain dry-run or unsupported for real
  execution in the first implementation.
- `run_doctor` and `keep_current` may refresh/read status without destructive
  side effects.

Execution flow:

```text
load BrowserRuntimePackManifest::v1_default()
paths = BrowserRuntimePackPaths::from_uclaw_home()
resolution = BrowserRuntimePackSourceResolver::resolve()
validate source
probe current filesystem
diagnose current pack
plan_runtime_pack_operation(... user_confirmed=true)
runner = BrowserRuntimePackLocalStepRunner::new(...)
  .with_staging_source_dir(resolution.source_dir)
  .with_post_install_smoke_probe(real Node/worker/Chromium smoke)
execute_runtime_pack_plan_with_runner(policy, runner)
return execution report with source and target evidence
```

Security boundaries:

- Target path must stay under `uclaw_home()/browser-runtime`.
- Source path must come from resolver.
- No global npm or global Playwright cache may be used by installer.
- Production execution must not force `worker_startup_ok` or
  `real_page_probe_ok`; it must derive them from a real smoke probe using the
  installed pack. Unit tests may inject fixture probe outcomes.
- Report includes source kind, source dir, target dir, step reports, and
  artifact id.
- UI must dry-run before calling confirmed execution.

## Control Center UI

Provider Priority behavior:

- If CLI/MCP has `fallbackReason = runtime_pack_not_ready`:
  - status badge: `Needs runtime pack`
  - main action: `Prepare runtime pack`
  - `Run probe` hidden or disabled
  - copy: install app-managed Playwright runtime pack before probing.
- If runtime pack is ready and provider probe is not passed:
  - status badge: `Needs probe`
  - main action: `Run probe`
- If CLI probe passes and CLI is first priority:
  - active route: `Playwright CLI`
- MCP remains advanced and configured in Kaleidoscope Integrations.

Runtime Pack section behavior:

1. User clicks `Prepare runtime pack`.
2. UI requests dry-run and shows source, target, steps, and confirmation need.
3. User clicks `Confirm install`.
4. UI calls `execute_browser_runtime_action(prepare, confirmed=true)`.
5. UI shows succeeded/failed report and step evidence.
6. UI refreshes `get_browser_runtime_status` and
   `get_browser_runtime_control_center`.
7. CLI/MCP lanes transition from `Needs runtime pack` to `Needs probe` or
   `Ready`.

Error copy:

- source missing: generate the dev pack or install an app bundle that includes
  it.
- invalid source: list missing required paths.
- version mismatch: show expected versus actual.
- execution failed: show failing step and artifact id.

## Testing

Rust tests:

- resolver env override priority
- bundle resource fallback
- dev staging fallback
- unsupported platform response
- source missing and invalid source validation errors
- confirmed=false refuses writes
- prepare installs from small staging fixture into temp runtime root
- repair replaces current pack from staging fixture
- production path does not mark worker/page probes ready without a smoke probe
- reinstall/cleanup/rollback do not execute in first implementation
- status after prepare reports ready with test probe options

Node/script tests:

- validator passes a small fixture
- validator fails missing manifest
- validator fails missing node/playwright/mcp/worker/chromium paths
- generator writes manifest and expected layout in a temp output directory
- `--from-local-toolchain` is explicit and logged

UI tests:

- `Needs runtime pack` shows `Prepare runtime pack`, not `Run probe`
- dry-run preview appears before confirm
- confirm calls `execute_browser_runtime_action`
- success refreshes status and control center
- execution failure surfaces source or step error

Manual validation:

```bash
node scripts/browser-runtime/generate-runtime-pack.mjs
node scripts/browser-runtime/validate-runtime-pack.mjs \
  src-tauri/.runtime-pack-staging/browser-runtime-pack-v1
cargo tauri dev
```

Then in Settings:

1. open Browser Runtime Control Center
2. click `Prepare runtime pack`
3. confirm install
4. verify `~/.uclaw/browser-runtime/packs/browser-runtime-pack-v1` exists
5. refresh status
6. run CLI probe
7. verify active route becomes Playwright CLI

## PR Slices

### PR1: Resolver And Execute IPC

- Add source resolver and source validator.
- Add `execute_browser_runtime_action`.
- Wire command in Tauri invoke handler.
- Keep executable actions limited to `prepare` and `repair`.
- Use small fixture tests, not real Chromium artifacts.

### PR2: Dev Generator And Validator

- Add generator and validator scripts.
- Add gitignore entry for `src-tauri/.runtime-pack-staging/`.
- Document local generation commands.
- Keep real generated pack out of git.

### PR3: UI Integration And E2E Validation

- Change Control Center provider actions for `Needs runtime pack`.
- Add dry-run -> confirm -> execute UI.
- Refresh status/control center after execution.
- Validate local full pack generation and CLI probe on macOS arm64.

## ADR 18 Answers

1. Intent: install a real app-managed Playwright runtime pack so CLI/MCP
   providers can become routable from the Control Center.
2. Autonomy: controlled local installation and repair of browser runtime assets
   under uClaw-managed storage.
3. Truth source: Rust runtime doctor and source resolver, not frontend guesses
   or mock data.
4. TaskEvent: execution reports and step events describe source, target,
   install, doctor, and probe outcomes.
5. Context: runtime manifest, source resolution result, filesystem probe,
   provider config, and user confirmation.
6. Capability: Playwright CLI first, Playwright MCP second, Local Chromium
   fallback after real pack readiness and provider probes.
7. Hooks: dry-run preview, explicit confirmation, resolver validation,
   installer step reports, focused Rust/UI/script tests, GitNexus detect.
8. Projection: Control Center displays route status, source/target evidence,
   install artifacts, and provider probe transition.
9. Harness: unit tests plus manual macOS arm64 E2E from generator to CLI active
   route.
10. Rollback: disable CLI/MCP, keep Local Chromium fallback, remove generated
    dev staging, or revert installer PRs. Destructive rollback execution is not
    opened in the first implementation.
11. Non-ownership: no remote signed download, no raw MCP tool exposure, no
    Windows/Linux pack, no global runtime dependency, no unrelated Settings
    redesign.

## Open Follow-Ups

- Signed remote runtime-pack download and update channel.
- macOS x64, Windows, and Linux pack generation.
- Release bundle packaging and notarization checks for embedded Node/Chromium.
- Pack size optimization and delta updates.
- Runtime-pack artifact retention and telemetry.
