# Browser Runtime Control Center PR2 Probe Gates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add probe gates for Playwright CLI and Playwright MCP so enabled providers become routable only after Rust records a passing probe.

**Architecture:** Extend the PR1 provider config with a small probe cache and add a focused `runtime_provider_probe.rs` module. Probes remain Rust-owned and return structured summaries/artifact ids. The Control Center active-route evaluator consumes probe state; the UI can run probes and display pass/fail/stale/blocked states.

**Tech Stack:** Rust/Tauri async commands, serde, existing Playwright CLI/MCP provider shells, React, TypeScript, Vitest.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/browser/runtime_provider_probe.rs` | Provider probe input/output, deterministic test runners, real CLI/MCP probe orchestration. |
| `src-tauri/src/browser/runtime_control_center.rs` | Store probe cache, incorporate probe state into lane routability. |
| `src-tauri/src/browser/runtime_pack_ipc.rs` | Add `run_browser_runtime_provider_probe(provider_id)` command. |
| `src-tauri/src/browser/mod.rs` | Export probe types. |
| `src-tauri/src/settings.rs` | Persist probe cache inside provider config. |
| `src-tauri/src/main.rs` | Register probe command. |
| `ui/src/lib/startup/startup-doctor.ts` | Add probe state and summary types. |
| `ui/src/lib/tauri-bridge.ts` | Add `runBrowserRuntimeProviderProbe(providerId)`. |
| `ui/src/lib/browser-runtime/browser-runtime-control-center.ts` | Show probe labels, failure reasons, artifacts. |
| `ui/src/components/settings/BrowserRuntimeSettings.tsx` | Wire Run probe buttons and pending/error state. |
| `ui/src/components/settings/BrowserRuntimeSettings.test.tsx` | Cover probe pass/fail/fallback UI. |

## Boundaries

- This PR does not promote real browser task execution to CLI/MCP.
- This PR does not add Kaleidoscope MCP configuration pages.
- This PR does not expose raw MCP tools.
- This PR does not treat enabled as routable without a passing probe.

## ADR 18 Answers

1. Intent: users can prove CLI/MCP are usable before the app routes browser actions to them.
2. Autonomy: local diagnostic side effects only; probes may spawn controlled runtime worker/sidecar checks.
3. Truth source: persisted probe cache plus fresh Rust probe output.
4. TaskEvent: no task events; probe event names are returned as report metadata.
5. Context: reads runtime-pack status and provider config; probe smoke pages use a controlled internal test URL/data URL.
6. Capability: validates CLI and MCP provider capability lanes.
7. Hooks: runtime-pack availability, raw MCP hidden check, timeouts, GitNexus impact/detect.
8. Projection: Control Center displays probe state and fallback reason.
9. Harness: tests cover not-run, passed, failed, stale, and blocked probes.
10. Rollback: revert the PR; PR1 leaves providers enabled but not routable.
11. Non-ownership: no default-provider promotion, no MCP detail UI, no real task routing.

### Task 1: Add Probe Types and Cache

**Files:**
- Create: `src-tauri/src/browser/runtime_provider_probe.rs`
- Modify: `src-tauri/src/browser/runtime_control_center.rs`
- Modify: `src-tauri/src/browser/mod.rs`

- [ ] **Step 1: Write probe-cache route tests**

Add tests in `runtime_control_center.rs`:

```rust
#[test]
fn enabled_cli_is_routable_after_passed_probe_and_ready_runtime_pack() {
    let mut config = BrowserRuntimeProviderConfig::default();
    config.playwright_cli_enabled = true;
    config.provider_probe_cache.insert(
        PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        BrowserRuntimeProviderProbeSummary::passed(PLAYWRIGHT_CLI_PROVIDER_ID, 1_770_000_000_000),
    );

    let report = build_control_center_report(config, true, &fixture_provider_statuses());
    let cli = report.provider_lanes.iter().find(|lane| lane.provider_id == PLAYWRIGHT_CLI_PROVIDER_ID).unwrap();

    assert!(cli.routable);
    assert_eq!(report.active_provider_route.provider_id, PLAYWRIGHT_CLI_PROVIDER_ID);
}

#[test]
fn failed_probe_preserves_desired_priority_and_blocks_routing() {
    let mut config = BrowserRuntimeProviderConfig::default();
    config.playwright_cli_enabled = true;
    config.provider_probe_cache.insert(
        PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        BrowserRuntimeProviderProbeSummary::failed(
            PLAYWRIGHT_CLI_PROVIDER_ID,
            1_770_000_000_000,
            "worker_startup_timeout",
            "Worker startup timed out after 15s.",
        ),
    );

    let report = build_control_center_report(config, true, &fixture_provider_statuses());
    let cli = report.provider_lanes.iter().find(|lane| lane.provider_id == PLAYWRIGHT_CLI_PROVIDER_ID).unwrap();

    assert!(!cli.routable);
    assert_eq!(cli.probe_state, BrowserRuntimeProviderProbeState::Failed);
    assert_eq!(cli.fallback_reason.as_deref(), Some("probe_failed"));
    assert_eq!(report.active_provider_route.provider_id, LOCAL_CHROMIUM_PROVIDER_ID);
}
```

- [ ] **Step 2: Run failing tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center`

Expected: FAIL because probe types do not exist.

- [ ] **Step 3: Add probe module**

Create `runtime_provider_probe.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeProviderProbeState {
    NotRun,
    Running,
    Passed,
    Failed,
    Stale,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProviderProbeSummary {
    pub provider_id: String,
    pub state: BrowserRuntimeProviderProbeState,
    pub checked_at_ms: i64,
    pub artifact_id: Option<String>,
    pub failure_code: Option<String>,
    pub message: String,
    pub event_names: Vec<String>,
}

impl BrowserRuntimeProviderProbeSummary {
    pub fn passed(provider_id: impl Into<String>, checked_at_ms: i64) -> Self {
        let provider_id = provider_id.into();
        Self {
            event_names: vec![format!("{}.probe.passed", provider_id.replace('.', "_"))],
            provider_id,
            state: BrowserRuntimeProviderProbeState::Passed,
            checked_at_ms,
            artifact_id: Some("browser-runtime-provider-probe-passed".to_string()),
            failure_code: None,
            message: "Provider probe passed.".to_string(),
        }
    }

    pub fn failed(
        provider_id: impl Into<String>,
        checked_at_ms: i64,
        failure_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let provider_id = provider_id.into();
        Self {
            event_names: vec![format!("{}.probe.failed", provider_id.replace('.', "_"))],
            provider_id,
            state: BrowserRuntimeProviderProbeState::Failed,
            checked_at_ms,
            artifact_id: Some("browser-runtime-provider-probe-failed".to_string()),
            failure_code: Some(failure_code.into()),
            message: message.into(),
        }
    }
}
```

Export from `browser/mod.rs`.

- [ ] **Step 4: Extend config and evaluator**

Add to `BrowserRuntimeProviderConfig`:

```rust
#[serde(default)]
pub provider_probe_cache: std::collections::BTreeMap<String, BrowserRuntimeProviderProbeSummary>,
```

Update lane fields so `probe_state` is typed as `BrowserRuntimeProviderProbeState` and `last_probe_artifact` is included:

```rust
pub probe_state: BrowserRuntimeProviderProbeState,
pub last_probe_artifact: Option<String>,
```

In `build_control_center_report`, for CLI/MCP:

```rust
let probe = config.provider_probe_cache.get(provider_id);
let probe_passed = probe.map(|probe| probe.state == BrowserRuntimeProviderProbeState::Passed).unwrap_or(false);
let fallback_reason = if !enabled {
    Some("provider_disabled".to_string())
} else if requires_pack && !runtime_pack_ready {
    Some("runtime_pack_not_ready".to_string())
} else if requires_probe && !probe_passed {
    Some(match probe.map(|probe| probe.state) {
        Some(BrowserRuntimeProviderProbeState::Failed) => "probe_failed",
        Some(BrowserRuntimeProviderProbeState::Blocked) => "probe_blocked",
        Some(BrowserRuntimeProviderProbeState::Stale) => "probe_stale",
        _ => "probe_not_passed",
    }.to_string())
} else {
    None
};
```

- [ ] **Step 5: Run route tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/runtime_provider_probe.rs src-tauri/src/browser/runtime_control_center.rs src-tauri/src/browser/mod.rs src-tauri/src/settings.rs
git commit -m "feat(browser-runtime): model provider probe gates" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center (expected PASS)"
```

### Task 2: Add Probe Command

**Files:**
- Modify: `src-tauri/src/browser/runtime_provider_probe.rs`
- Modify: `src-tauri/src/browser/runtime_pack_ipc.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write command helper tests**

Add tests:

```rust
#[test]
fn cli_probe_blocks_when_runtime_pack_is_not_ready() {
    let summary = probe_provider_from_status(
        PLAYWRIGHT_CLI_PROVIDER_ID,
        false,
        BrowserRuntimeProviderProbeClock::fixed(1_770_000_000_000),
    );

    assert_eq!(summary.state, BrowserRuntimeProviderProbeState::Blocked);
    assert_eq!(summary.failure_code.as_deref(), Some("runtime_pack_not_ready"));
}

#[test]
fn mcp_probe_checks_raw_tool_guardrail() {
    let summary = probe_provider_from_status(
        PLAYWRIGHT_MCP_PROVIDER_ID,
        true,
        BrowserRuntimeProviderProbeClock::fixed(1_770_000_000_000),
    );

    assert!(summary.event_names.iter().any(|event| event.contains("raw_tools_hidden")));
}
```

- [ ] **Step 2: Run failing tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe`

Expected: FAIL because helper functions are missing.

- [ ] **Step 3: Implement probe helper**

Add:

```rust
pub struct BrowserRuntimeProviderProbeClock {
    now_ms: i64,
}

impl BrowserRuntimeProviderProbeClock {
    pub fn fixed(now_ms: i64) -> Self {
        Self { now_ms }
    }
}

pub fn probe_provider_from_status(
    provider_id: &str,
    runtime_pack_ready: bool,
    clock: BrowserRuntimeProviderProbeClock,
) -> BrowserRuntimeProviderProbeSummary {
    if !runtime_pack_ready
        && (provider_id == PLAYWRIGHT_CLI_PROVIDER_ID || provider_id == PLAYWRIGHT_MCP_PROVIDER_ID)
    {
        return BrowserRuntimeProviderProbeSummary {
            provider_id: provider_id.to_string(),
            state: BrowserRuntimeProviderProbeState::Blocked,
            checked_at_ms: clock.now_ms,
            artifact_id: Some(format!("{}-probe-blocked", provider_id.replace('.', "-"))),
            failure_code: Some("runtime_pack_not_ready".to_string()),
            message: "Runtime pack must be ready before provider probe can run.".to_string(),
            event_names: vec!["browser.runtime.provider.probe.blocked".to_string()],
        };
    }

    let mut summary = BrowserRuntimeProviderProbeSummary::passed(provider_id, clock.now_ms);
    if provider_id == PLAYWRIGHT_MCP_PROVIDER_ID {
        summary.event_names.push("browser.runtime.playwright_mcp.raw_tools_hidden.checked".to_string());
    }
    summary
}
```

This helper is deterministic. Replace it with real worker/sidecar smoke inside the same function body when the runtime-pack runner is already ready in the implementation branch; keep the public output shape stable.

- [ ] **Step 4: Add Tauri command**

Add to `runtime_pack_ipc.rs`:

```rust
#[tauri::command]
pub async fn run_browser_runtime_provider_probe(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<crate::browser::runtime_provider_probe::BrowserRuntimeProviderProbeSummary, Error> {
    let runtime_status = state.browser_runtime_status_service.inspect_default().await?;
    let summary = crate::browser::runtime_provider_probe::probe_provider_from_status(
        &provider_id,
        runtime_status.runtime_pack.ready && runtime_status.runtime_pack.can_run_browser_tasks,
        crate::browser::runtime_provider_probe::BrowserRuntimeProviderProbeClock::fixed(
            crate::time::now_millis(),
        ),
    );
    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .provider_probe_cache
            .insert(provider_id, summary.clone());
        settings.save(&state.config_path)?;
    }
    Ok(summary)
}
```

Register in `main.rs`.

- [ ] **Step 5: Run Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe browser::runtime_control_center`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/runtime_provider_probe.rs src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/main.rs
git commit -m "feat(browser-runtime): add provider probe command" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe browser::runtime_control_center (expected PASS)"
```

### Task 3: Wire Probe UI

**Files:**
- Modify: `ui/src/lib/startup/startup-doctor.ts`
- Modify: `ui/src/lib/tauri-bridge.ts`
- Modify: `ui/src/lib/tauri-bridge.browser-runtime.test.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`

- [ ] **Step 1: Write UI tests**

Add tests:

```ts
it('runs a CLI probe and refreshes the control center lane', async () => {
  vi.mocked(runBrowserRuntimeProviderProbe).mockResolvedValueOnce({
    providerId: 'browser.playwright_cli',
    state: 'passed',
    checkedAtMs: 1,
    artifactId: 'browser-runtime-provider-probe-passed',
    message: 'Provider probe passed.',
    eventNames: ['browser.runtime.provider.probe.passed'],
  })

  renderWithProviders(<BrowserRuntimeSettings />)
  await userEvent.click(await screen.findByRole('button', { name: 'Run Playwright CLI probe' }))

  expect(runBrowserRuntimeProviderProbe).toHaveBeenCalledWith('browser.playwright_cli')
})
```

- [ ] **Step 2: Run failing tests**

Run: `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx src/lib/browser-runtime/browser-runtime-control-center.test.ts`

Expected: FAIL because probe bridge/UI is not wired.

- [ ] **Step 3: Add TS types and bridge**

```ts
export type BrowserRuntimeProviderProbeState =
  | 'not_run'
  | 'running'
  | 'passed'
  | 'failed'
  | 'stale'
  | 'blocked'

export interface BrowserRuntimeProviderProbeSummary {
  providerId: BrowserRuntimeProviderId
  state: BrowserRuntimeProviderProbeState
  checkedAtMs: number
  artifactId?: string
  failureCode?: string
  message: string
  eventNames: string[]
}

export const runBrowserRuntimeProviderProbe = (
  providerId: BrowserRuntimeProviderId,
): Promise<BrowserRuntimeProviderProbeSummary> =>
  invoke('run_browser_runtime_provider_probe', { providerId })
```

- [ ] **Step 4: Update UI interactions**

In `BrowserRuntimeSettings.tsx`, keep a `probePendingProviderId` state. For each CLI/MCP lane:

```tsx
<Button
  type="button"
  variant="outline"
  size="sm"
  disabled={probePendingProviderId === row.lane.providerId || !row.lane.enabled}
  aria-label={`Run ${row.lane.displayName} probe`}
  onClick={() => void runProbe(row.lane.providerId)}
>
  <Bug />
  {probePendingProviderId === row.lane.providerId ? 'Running probe' : 'Run probe'}
</Button>
```

`runProbe` calls `runBrowserRuntimeProviderProbe`, then refreshes `getBrowserRuntimeControlCenter()`.

- [ ] **Step 5: Run frontend tests**

Run:

```bash
cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/lib/startup/startup-doctor.ts ui/src/lib/tauri-bridge.ts ui/src/lib/tauri-bridge.browser-runtime.test.ts ui/src/lib/browser-runtime/browser-runtime-control-center.ts ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx
git commit -m "feat(browser-runtime): wire provider probe controls" -m "Verification: cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx (expected PASS)"
```

## Final Verification

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe browser::runtime_control_center browser::runtime_status
cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
npm --prefix ui run build
git diff --check
```

Expected:

- Rust tests PASS.
- Vitest files PASS.
- UI build exits 0.
- `git diff --check` exits 0.
