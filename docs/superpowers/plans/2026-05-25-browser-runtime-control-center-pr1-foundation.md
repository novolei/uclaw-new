# Browser Runtime Control Center PR1 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current Browser Runtime Settings status surface with a config-backed Browser Runtime Control Center foundation that lets users enable CLI/MCP, set desired provider priority, and see Rust-computed active route/fallback truth.

**Architecture:** Add a focused Rust control-center module beside `runtime_status.rs` and keep `tauri_commands.rs` out of the slice. Persist provider enablement and desired priority in `config.json` through `UserSettings`, derive feature flags from that config, and expose one IPC read/write surface. The frontend consumes the new report through typed bridge functions and renders a Control Center first screen without fake clickable MCP integration actions.

**Tech Stack:** Rust/Tauri, serde, existing `UserSettings` JSON persistence, React, TypeScript, Vitest, Testing Library, lucide-react.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/browser/runtime_control_center.rs` | New pure/read-model module: provider config, lane model, active route calculation, labels, tests. |
| `src-tauri/src/browser/runtime_status.rs` | Compose provider readiness using config-derived feature flags instead of hard-coded `safe_defaults()` for user-visible status. |
| `src-tauri/src/browser/runtime_pack_ipc.rs` | Add Tauri commands for control-center report/config mutation; delegate to the new module. |
| `src-tauri/src/browser/mod.rs` | Export new control-center types. |
| `src-tauri/src/settings.rs` | Persist `browser_runtime_provider_config` in `config.json` with serde defaults for legacy files. |
| `src-tauri/src/main.rs` | Register new Tauri commands. |
| `ui/src/lib/startup/startup-doctor.ts` | Add TypeScript report/config/lane types. |
| `ui/src/lib/tauri-bridge.ts` | Add typed bridge functions for Control Center report, enablement, and priority. |
| `ui/src/lib/tauri-bridge.browser-runtime.test.ts` | Cover new command names and payload shapes. |
| `ui/src/lib/browser-runtime/browser-runtime-control-center.ts` | New frontend view model for route summary, lanes, next actions, and status copy. |
| `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts` | Unit tests for no-mock status copy and route/fallback labels. |
| `ui/src/components/settings/BrowserRuntimeSettings.tsx` | Render the Browser Runtime Control Center first screen using the new view model. |
| `ui/src/components/settings/BrowserRuntimeSettings.test.tsx` | UI tests for enable buttons, priority controls, non-clickable MCP configure state, and local fallback copy. |

## Boundaries

- This PR does not run provider probes.
- This PR does not promote provider execution routing.
- This PR does not add Kaleidoscope Playwright MCP integration detail.
- This PR does not execute runtime-pack prepare/repair as side effects.
- `Configure MCP` must not render as a clickable button in this PR because PR3 owns the real route.

## ADR 18 Answers

1. Intent: users can express CLI-first/MCP-second desired browser provider intent from product UI.
2. Autonomy: local settings/read-model only; no browser launch, download, delete, or task execution.
3. Truth source: Rust `BrowserRuntimeControlCenterReport`, persisted provider config, runtime-pack status, and provider readiness.
4. TaskEvent: no persisted TaskEvents; route explanation strings are returned for UI.
5. Context: reads runtime-pack filesystem status, active local Chromium context count, and config.json provider settings.
6. Capability: consumes existing `browser.local_chromium`, `browser.playwright_cli`, `browser.playwright_mcp` provider cards/status.
7. Hooks: GitNexus impact/detect, serde compatibility tests, Rust/frontend tests, pre-commit guardrails.
8. Projection: UI renders desired priority, active route, provider lanes, and fallback reasons.
9. Harness: focused Rust route-decision tests and UI tests for disabled/enabled/fallback states.
10. Rollback: revert this PR; legacy `get_browser_runtime_status` remains compatible.
11. Non-ownership: no probe execution, MCP details page, provider action routing, raw MCP exposure, schema migration, or unrelated settings redesign.

### Task 1: Add Persisted Provider Config

**Files:**
- Create: `src-tauri/src/browser/runtime_control_center.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Modify: `src-tauri/src/settings.rs`

- [ ] **Step 1: Write the config defaults test**

Add this test in `src-tauri/src/browser/runtime_control_center.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
    use crate::browser::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID;
    use crate::browser::provider::LOCAL_CHROMIUM_PROVIDER_ID;

    #[test]
    fn provider_config_defaults_to_cli_mcp_local_priority_with_cli_mcp_off() {
        let config = BrowserRuntimeProviderConfig::default();

        assert!(!config.playwright_cli_enabled);
        assert!(!config.playwright_mcp_enabled);
        assert_eq!(
            config.desired_priority,
            vec![
                PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
                PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
                LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
            ]
        );
        assert_eq!(config.default_fallback_provider, LOCAL_CHROMIUM_PROVIDER_ID);
    }
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center::tests::provider_config_defaults_to_cli_mcp_local_priority_with_cli_mcp_off`

Expected: FAIL because `runtime_control_center` does not exist.

- [ ] **Step 3: Add config types and module export**

Create `src-tauri/src/browser/runtime_control_center.rs` with:

```rust
use serde::{Deserialize, Serialize};

use crate::browser::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
use crate::browser::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID;
use crate::browser::provider::LOCAL_CHROMIUM_PROVIDER_ID;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProviderConfig {
    #[serde(default)]
    pub playwright_cli_enabled: bool,
    #[serde(default)]
    pub playwright_mcp_enabled: bool,
    #[serde(default = "default_provider_priority")]
    pub desired_priority: Vec<String>,
    #[serde(default = "default_fallback_provider")]
    pub default_fallback_provider: String,
    #[serde(default)]
    pub updated_at_ms: i64,
}

impl Default for BrowserRuntimeProviderConfig {
    fn default() -> Self {
        Self {
            playwright_cli_enabled: false,
            playwright_mcp_enabled: false,
            desired_priority: default_provider_priority(),
            default_fallback_provider: default_fallback_provider(),
            updated_at_ms: 0,
        }
    }
}

pub fn default_provider_priority() -> Vec<String> {
    vec![
        PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
    ]
}

fn default_fallback_provider() -> String {
    LOCAL_CHROMIUM_PROVIDER_ID.to_string()
}
```

Modify `src-tauri/src/browser/mod.rs`:

```rust
pub mod runtime_control_center;
pub use runtime_control_center::BrowserRuntimeProviderConfig;
```

Modify `src-tauri/src/settings.rs`:

```rust
use crate::browser::runtime_control_center::BrowserRuntimeProviderConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    pub language: String,
    pub theme: String,
    #[serde(default)]
    pub monthly_budget_usd: Option<f64>,
    #[serde(default)]
    pub memory_recall_config: Option<MemoryRecallConfigDto>,
    #[serde(default)]
    pub browser_runtime_provider_config: BrowserRuntimeProviderConfig,
}
```

Update `Default for UserSettings` to include:

```rust
browser_runtime_provider_config: BrowserRuntimeProviderConfig::default(),
```

- [ ] **Step 4: Run the config tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
cargo test --manifest-path src-tauri/Cargo.toml --lib settings
```

Expected: PASS; legacy `UserSettings` serde tests still pass because the new field has a default.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/browser/runtime_control_center.rs src-tauri/src/browser/mod.rs src-tauri/src/settings.rs
git commit -m "feat(browser-runtime): persist provider control config" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center; cargo test --manifest-path src-tauri/Cargo.toml --lib settings (expected PASS)"
```

### Task 2: Add Rust Control Center Report and Active Route

**Files:**
- Modify: `src-tauri/src/browser/runtime_control_center.rs`
- Modify: `src-tauri/src/browser/runtime_status.rs`

- [ ] **Step 1: Write route tests**

Append these tests:

```rust
#[test]
fn control_center_keeps_desired_priority_but_falls_back_when_cli_mcp_disabled() {
    let runtime_pack = fixture_runtime_pack_status(true);
    let status = crate::browser::runtime_status::compose_browser_runtime_status_with_config(
        runtime_pack,
        Vec::new(),
        BrowserRuntimeProviderConfig::default(),
    );

    assert_eq!(
        status.control_center.active_provider_route.provider_id,
        LOCAL_CHROMIUM_PROVIDER_ID
    );
    assert_eq!(status.control_center.desired_provider_priority[0], PLAYWRIGHT_CLI_PROVIDER_ID);
    assert_eq!(status.control_center.provider_lanes[0].fallback_reason.as_deref(), Some("provider_disabled"));
}

#[test]
fn control_center_marks_enabled_cli_not_routable_until_probe_pr() {
    let mut config = BrowserRuntimeProviderConfig::default();
    config.playwright_cli_enabled = true;
    let runtime_pack = fixture_runtime_pack_status(true);
    let status = crate::browser::runtime_status::compose_browser_runtime_status_with_config(
        runtime_pack,
        Vec::new(),
        config,
    );

    let cli = status.control_center.provider_lanes.iter()
        .find(|lane| lane.provider_id == PLAYWRIGHT_CLI_PROVIDER_ID)
        .expect("cli lane");
    assert!(cli.enabled);
    assert!(!cli.routable);
    assert_eq!(cli.next_action, "run_probe");
    assert_eq!(status.control_center.active_provider_route.provider_id, LOCAL_CHROMIUM_PROVIDER_ID);
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`

Expected: FAIL because `compose_browser_runtime_status_with_config` and `control_center` do not exist.

- [ ] **Step 3: Add report/lane types and route evaluator**

Add to `runtime_control_center.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeRouteRole {
    DesiredFirst,
    Desired,
    Active,
    Fallback,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeActiveProviderRoute {
    pub provider_id: String,
    pub display_name: String,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProviderLane {
    pub provider_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub priority_rank: usize,
    pub readiness: String,
    pub routable: bool,
    pub route_role: BrowserRuntimeRouteRole,
    pub probe_state: String,
    pub fallback_reason: Option<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeControlCenterReport {
    pub feature_flags: crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags,
    pub desired_provider_priority: Vec<String>,
    pub active_provider_route: BrowserRuntimeActiveProviderRoute,
    pub provider_lanes: Vec<BrowserRuntimeProviderLane>,
    pub mcp_integration_summary: BrowserRuntimeMcpIntegrationSummary,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeMcpIntegrationSummary {
    pub built_in: bool,
    pub enabled: bool,
    pub raw_tools_exposed: bool,
    pub configure_route_ready: bool,
}
```

Add a pure builder:

```rust
pub fn build_control_center_report(
    config: BrowserRuntimeProviderConfig,
    runtime_pack_ready: bool,
    providers: &[crate::browser::provider::BrowserProviderStatus],
) -> BrowserRuntimeControlCenterReport {
    let flags = feature_flags_from_provider_config(&config);
    let mut lanes = Vec::new();
    let mut active: Option<BrowserRuntimeActiveProviderRoute> = None;

    for (index, provider_id) in config.desired_priority.iter().enumerate() {
        let status = providers.iter().find(|provider| provider.provider_id == *provider_id);
        let enabled = provider_enabled(provider_id, &config);
        let requires_pack = provider_id == PLAYWRIGHT_CLI_PROVIDER_ID || provider_id == PLAYWRIGHT_MCP_PROVIDER_ID;
        let readiness = status.map(|status| format!("{:?}", status.readiness).to_lowercase()).unwrap_or_else(|| "unavailable".to_string());
        let fallback_reason = if !enabled {
            Some("provider_disabled".to_string())
        } else if requires_pack && !runtime_pack_ready {
            Some("runtime_pack_not_ready".to_string())
        } else if *provider_id == PLAYWRIGHT_CLI_PROVIDER_ID || *provider_id == PLAYWRIGHT_MCP_PROVIDER_ID {
            Some("probe_not_passed".to_string())
        } else {
            None
        };
        let routable = fallback_reason.is_none();
        if routable && active.is_none() {
            active = Some(BrowserRuntimeActiveProviderRoute {
                provider_id: provider_id.clone(),
                display_name: provider_display_name(provider_id).to_string(),
                fallback_reason: None,
            });
        }
        lanes.push(BrowserRuntimeProviderLane {
            provider_id: provider_id.clone(),
            display_name: provider_display_name(provider_id).to_string(),
            enabled,
            priority_rank: index + 1,
            readiness,
            routable,
            route_role: if index == 0 { BrowserRuntimeRouteRole::DesiredFirst } else { BrowserRuntimeRouteRole::Desired },
            probe_state: if provider_id == LOCAL_CHROMIUM_PROVIDER_ID { "passed".to_string() } else { "not_run".to_string() },
            fallback_reason: fallback_reason.clone(),
            next_action: next_action_for_lane(provider_id, enabled, fallback_reason.as_deref()),
        });
    }

    let active_provider_route = active.unwrap_or_else(|| BrowserRuntimeActiveProviderRoute {
        provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
        display_name: "Local Chromium".to_string(),
        fallback_reason: Some("all_preferred_providers_unavailable".to_string()),
    });

    for lane in &mut lanes {
        if lane.provider_id == active_provider_route.provider_id {
            lane.route_role = BrowserRuntimeRouteRole::Active;
        }
    }

    BrowserRuntimeControlCenterReport {
        feature_flags: flags,
        desired_provider_priority: config.desired_priority.clone(),
        active_provider_route,
        provider_lanes: lanes,
        mcp_integration_summary: BrowserRuntimeMcpIntegrationSummary {
            built_in: true,
            enabled: config.playwright_mcp_enabled,
            raw_tools_exposed: false,
            configure_route_ready: false,
        },
        updated_at_ms: config.updated_at_ms,
    }
}
```

Add helpers:

```rust
pub fn feature_flags_from_provider_config(
    config: &BrowserRuntimeProviderConfig,
) -> crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags {
    let mut flags = crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = config.playwright_cli_enabled;
    flags.playwright_mcp = config.playwright_mcp_enabled;
    flags
}

fn provider_enabled(provider_id: &str, config: &BrowserRuntimeProviderConfig) -> bool {
    match provider_id {
        PLAYWRIGHT_CLI_PROVIDER_ID => config.playwright_cli_enabled,
        PLAYWRIGHT_MCP_PROVIDER_ID => config.playwright_mcp_enabled,
        LOCAL_CHROMIUM_PROVIDER_ID => true,
        _ => false,
    }
}

fn provider_display_name(provider_id: &str) -> &'static str {
    match provider_id {
        PLAYWRIGHT_CLI_PROVIDER_ID => "Playwright CLI",
        PLAYWRIGHT_MCP_PROVIDER_ID => "Playwright MCP",
        LOCAL_CHROMIUM_PROVIDER_ID => "Local Chromium",
        _ => "Unknown provider",
    }
}

fn next_action_for_lane(provider_id: &str, enabled: bool, fallback_reason: Option<&str>) -> String {
    if !enabled {
        return if provider_id == PLAYWRIGHT_MCP_PROVIDER_ID { "enable_mcp" } else { "enable_provider" }.to_string();
    }
    match fallback_reason {
        Some("runtime_pack_not_ready") => "prepare_runtime_pack".to_string(),
        Some("probe_not_passed") => "run_probe".to_string(),
        Some(_) => "view_details".to_string(),
        None => "none".to_string(),
    }
}
```

Modify `BrowserRuntimeStatusReport` in `runtime_status.rs`:

```rust
pub control_center: crate::browser::runtime_control_center::BrowserRuntimeControlCenterReport,
```

Add `compose_browser_runtime_status_with_config` and make `compose_browser_runtime_status` call it with default config:

```rust
pub fn compose_browser_runtime_status(
    runtime_pack: BrowserRuntimePackStatusReport,
    active_context_sessions: Vec<String>,
) -> BrowserRuntimeStatusReport {
    compose_browser_runtime_status_with_config(
        runtime_pack,
        active_context_sessions,
        crate::browser::runtime_control_center::BrowserRuntimeProviderConfig::default(),
    )
}
```

Inside the config-aware function, call provider readiness with `feature_flags_from_provider_config(&config)` and build `control_center`.

- [ ] **Step 4: Run route tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/browser/runtime_control_center.rs src-tauri/src/browser/runtime_status.rs
git commit -m "feat(browser-runtime): add control center route report" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center (expected PASS)"
```

### Task 3: Add IPC Commands

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack_ipc.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `ui/src/lib/startup/startup-doctor.ts`
- Modify: `ui/src/lib/tauri-bridge.ts`
- Modify: `ui/src/lib/tauri-bridge.browser-runtime.test.ts`

- [ ] **Step 1: Write bridge tests**

Add to `ui/src/lib/tauri-bridge.browser-runtime.test.ts`:

```ts
it('invokes get_browser_runtime_control_center', async () => {
  const report = { activeProviderRoute: { providerId: 'browser.local_chromium' }, providerLanes: [] }
  mockInvoke.mockResolvedValueOnce(report)

  await expect(getBrowserRuntimeControlCenter()).resolves.toEqual(report)
  expect(mockInvoke).toHaveBeenCalledWith('get_browser_runtime_control_center')
})

it('invokes provider enable and priority commands', async () => {
  mockInvoke.mockResolvedValueOnce({ ok: true })
  await setBrowserRuntimeProviderEnabled('browser.playwright_cli', true)
  expect(mockInvoke).toHaveBeenCalledWith('set_browser_runtime_provider_enabled', {
    providerId: 'browser.playwright_cli',
    enabled: true,
  })

  mockInvoke.mockResolvedValueOnce({ ok: true })
  await setBrowserRuntimeProviderPriority([
    'browser.playwright_cli',
    'browser.playwright_mcp',
    'browser.local_chromium',
  ])
  expect(mockInvoke).toHaveBeenCalledWith('set_browser_runtime_provider_priority', {
    providerIds: [
      'browser.playwright_cli',
      'browser.playwright_mcp',
      'browser.local_chromium',
    ],
  })
})
```

- [ ] **Step 2: Run failing bridge tests**

Run: `cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts`

Expected: FAIL because the bridge functions do not exist.

- [ ] **Step 3: Add Rust commands**

Add commands to `runtime_pack_ipc.rs`:

```rust
#[tauri::command]
pub async fn get_browser_runtime_control_center(
    state: State<'_, AppState>,
) -> Result<crate::browser::runtime_control_center::BrowserRuntimeControlCenterReport, Error> {
    let settings = state.settings.read().await;
    let status = state
        .browser_runtime_status_service
        .inspect_with_provider_config(settings.browser_runtime_provider_config.clone())
        .await?;
    Ok(status.control_center)
}

#[tauri::command]
pub async fn set_browser_runtime_provider_enabled(
    state: State<'_, AppState>,
    provider_id: String,
    enabled: bool,
) -> Result<crate::browser::runtime_control_center::BrowserRuntimeControlCenterReport, Error> {
    {
        let mut settings = state.settings.write().await;
        settings.browser_runtime_provider_config.set_enabled(&provider_id, enabled)?;
        settings.save(&state.config_path)?;
    }
    get_browser_runtime_control_center(state).await
}

#[tauri::command]
pub async fn set_browser_runtime_provider_priority(
    state: State<'_, AppState>,
    provider_ids: Vec<String>,
) -> Result<crate::browser::runtime_control_center::BrowserRuntimeControlCenterReport, Error> {
    {
        let mut settings = state.settings.write().await;
        settings.browser_runtime_provider_config.set_priority(provider_ids)?;
        settings.save(&state.config_path)?;
    }
    get_browser_runtime_control_center(state).await
}
```

Add `set_enabled` and `set_priority` methods on `BrowserRuntimeProviderConfig` that accept only the three known provider ids and always keep Local Chromium in the priority list.

Register the commands in `src-tauri/src/main.rs`.

- [ ] **Step 4: Add TS types and bridge functions**

Add types to `ui/src/lib/startup/startup-doctor.ts`:

```ts
export type BrowserRuntimeProviderId =
  | 'browser.playwright_cli'
  | 'browser.playwright_mcp'
  | 'browser.local_chromium'

export interface BrowserRuntimeProviderLane {
  providerId: BrowserRuntimeProviderId
  displayName: string
  enabled: boolean
  priorityRank: number
  readiness: string
  routable: boolean
  routeRole: string
  probeState: string
  fallbackReason?: string
  nextAction: string
}

export interface BrowserRuntimeControlCenterReport {
  desiredProviderPriority: BrowserRuntimeProviderId[]
  activeProviderRoute: {
    providerId: BrowserRuntimeProviderId
    displayName: string
    fallbackReason?: string
  }
  providerLanes: BrowserRuntimeProviderLane[]
  mcpIntegrationSummary: {
    builtIn: boolean
    enabled: boolean
    rawToolsExposed: boolean
    configureRouteReady: boolean
  }
  updatedAtMs: number
}
```

Add to `ui/src/lib/tauri-bridge.ts`:

```ts
export const getBrowserRuntimeControlCenter = (): Promise<BrowserRuntimeControlCenterReport> =>
  invoke('get_browser_runtime_control_center')

export const setBrowserRuntimeProviderEnabled = (
  providerId: BrowserRuntimeProviderId,
  enabled: boolean,
): Promise<BrowserRuntimeControlCenterReport> =>
  invoke('set_browser_runtime_provider_enabled', { providerId, enabled })

export const setBrowserRuntimeProviderPriority = (
  providerIds: BrowserRuntimeProviderId[],
): Promise<BrowserRuntimeControlCenterReport> =>
  invoke('set_browser_runtime_provider_priority', { providerIds })
```

- [ ] **Step 5: Run IPC/bridge tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status
cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/main.rs ui/src/lib/startup/startup-doctor.ts ui/src/lib/tauri-bridge.ts ui/src/lib/tauri-bridge.browser-runtime.test.ts
git commit -m "feat(browser-runtime): expose control center ipc" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status; cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts (expected PASS)"
```

### Task 4: Render Control Center Foundation UI

**Files:**
- Create: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Create: `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`

- [ ] **Step 1: Write view-model tests**

Create `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { deriveBrowserRuntimeControlCenterViewModel } from './browser-runtime-control-center'
import type { BrowserRuntimeControlCenterReport } from '@/lib/startup/startup-doctor'

function report(): BrowserRuntimeControlCenterReport {
  return {
    desiredProviderPriority: ['browser.playwright_cli', 'browser.playwright_mcp', 'browser.local_chromium'],
    activeProviderRoute: { providerId: 'browser.local_chromium', displayName: 'Local Chromium' },
    providerLanes: [
      { providerId: 'browser.playwright_cli', displayName: 'Playwright CLI', enabled: true, priorityRank: 1, readiness: 'needs_setup', routable: false, routeRole: 'desired_first', probeState: 'not_run', fallbackReason: 'probe_not_passed', nextAction: 'run_probe' },
      { providerId: 'browser.playwright_mcp', displayName: 'Playwright MCP', enabled: false, priorityRank: 2, readiness: 'unavailable', routable: false, routeRole: 'desired', probeState: 'not_run', fallbackReason: 'provider_disabled', nextAction: 'enable_mcp' },
      { providerId: 'browser.local_chromium', displayName: 'Local Chromium', enabled: true, priorityRank: 3, readiness: 'ready', routable: true, routeRole: 'active', probeState: 'passed', nextAction: 'none' },
    ],
    mcpIntegrationSummary: { builtIn: true, enabled: false, rawToolsExposed: false, configureRouteReady: false },
    updatedAtMs: 1,
  }
}

describe('browser runtime control center view model', () => {
  it('separates desired provider priority from active route', () => {
    const model = deriveBrowserRuntimeControlCenterViewModel(report())

    expect(model.routeSummary.desiredLabel).toBe('Playwright CLI > Playwright MCP > Local Chromium')
    expect(model.routeSummary.activeLabel).toBe('Local Chromium')
    expect(model.routeSummary.reasonLabel).toContain('Playwright CLI')
    expect(model.providerRows[1].configureMcpClickable).toBe(false)
  })
})
```

- [ ] **Step 2: Run failing UI model test**

Run: `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts`

Expected: FAIL because the view model does not exist.

- [ ] **Step 3: Add view model**

Create `browser-runtime-control-center.ts`:

```ts
import type { BrowserRuntimeControlCenterReport, BrowserRuntimeProviderLane } from '@/lib/startup/startup-doctor'

export interface BrowserRuntimeControlCenterViewModel {
  routeSummary: {
    desiredLabel: string
    activeLabel: string
    reasonLabel: string
    primaryActionLabel: string
  }
  providerRows: Array<{
    lane: BrowserRuntimeProviderLane
    statusLabel: string
    nextActionLabel: string
    configureMcpClickable: boolean
  }>
}

export function deriveBrowserRuntimeControlCenterViewModel(
  report?: BrowserRuntimeControlCenterReport,
): BrowserRuntimeControlCenterViewModel {
  if (!report) {
    return {
      routeSummary: {
        desiredLabel: '等待 Rust 状态',
        activeLabel: '未检查',
        reasonLabel: '等待 Browser Runtime Control Center 报告。',
        primaryActionLabel: '刷新状态',
      },
      providerRows: [],
    }
  }

  return {
    routeSummary: {
      desiredLabel: report.desiredProviderPriority.map(providerLabel).join(' > '),
      activeLabel: report.activeProviderRoute.displayName,
      reasonLabel: routeReason(report.providerLanes),
      primaryActionLabel: primaryAction(report.providerLanes),
    },
    providerRows: report.providerLanes.map((lane) => ({
      lane,
      statusLabel: laneStatusLabel(lane),
      nextActionLabel: nextActionLabel(lane.nextAction),
      configureMcpClickable: lane.providerId === 'browser.playwright_mcp'
        && report.mcpIntegrationSummary.configureRouteReady,
    })),
  }
}

function providerLabel(providerId: string): string {
  if (providerId === 'browser.playwright_cli') return 'Playwright CLI'
  if (providerId === 'browser.playwright_mcp') return 'Playwright MCP'
  if (providerId === 'browser.local_chromium') return 'Local Chromium'
  return providerId
}

function routeReason(lanes: BrowserRuntimeProviderLane[]): string {
  const skipped = lanes.filter((lane) => lane.fallbackReason && lane.providerId !== 'browser.local_chromium')
  if (skipped.length === 0) return '首选 provider 可用。'
  return skipped.map((lane) => `${lane.displayName}: ${fallbackLabel(lane.fallbackReason)}`).join(' · ')
}

function laneStatusLabel(lane: BrowserRuntimeProviderLane): string {
  if (!lane.enabled) return 'Off'
  if (lane.routeRole === 'active') return 'Active'
  if (lane.fallbackReason === 'runtime_pack_not_ready') return 'Needs runtime pack'
  if (lane.fallbackReason === 'probe_not_passed') return 'Needs probe'
  if (!lane.routable) return 'Not routable'
  return 'Ready'
}

function fallbackLabel(reason?: string): string {
  if (reason === 'provider_disabled') return 'Off'
  if (reason === 'runtime_pack_not_ready') return 'Needs runtime pack'
  if (reason === 'probe_not_passed') return 'Needs probe'
  return reason ? reason : 'Ready'
}

function nextActionLabel(nextAction: string): string {
  if (nextAction === 'enable_mcp') return 'Enable MCP'
  if (nextAction === 'enable_provider') return 'Enable provider'
  if (nextAction === 'prepare_runtime_pack') return 'Prepare runtime pack'
  if (nextAction === 'run_probe') return 'Run probe'
  if (nextAction === 'view_details') return 'View details'
  return 'No action'
}

function primaryAction(lanes: BrowserRuntimeProviderLane[]): string {
  if (lanes.some((lane) => lane.nextAction === 'run_probe')) return 'Run probes'
  if (lanes.some((lane) => lane.nextAction === 'prepare_runtime_pack')) return 'Prepare runtime pack'
  return 'Refresh status'
}
```

- [ ] **Step 4: Update Settings UI**

In `BrowserRuntimeSettings.tsx`, add state for `controlCenter`, load it with `getBrowserRuntimeControlCenter()`, and render these sections before the runtime pack section:

```tsx
<SettingsSection title="Browser Runtime Control Center" description="CLI first · MCP second · Local Chromium fallback">
  <SettingsCard>
    <SettingsRow label="Desired route" description={controlModel.routeSummary.desiredLabel} />
    <SettingsRow label="Active route" description={controlModel.routeSummary.reasonLabel}>
      <Badge variant="outline">{controlModel.routeSummary.activeLabel}</Badge>
    </SettingsRow>
  </SettingsCard>
</SettingsSection>

<SettingsSection title="Provider Priority">
  <SettingsCard divided={false}>
    <div className="divide-y divide-border">
      {controlModel.providerRows.map((row) => (
        <div key={row.lane.providerId} className="grid gap-3 p-4 md:grid-cols-[minmax(0,1fr)_auto]">
          <div>
            <div className="text-sm font-medium">{row.lane.displayName}</div>
            <div className="text-xs text-muted-foreground">{row.statusLabel}</div>
          </div>
          <div className="flex min-h-11 flex-wrap items-center gap-2">
            <Button type="button" variant="outline" size="sm" disabled={row.nextActionLabel === 'Run probe'}>
              {row.nextActionLabel}
            </Button>
            {row.lane.providerId === 'browser.playwright_mcp' && !row.configureMcpClickable ? (
              <span className="text-xs text-muted-foreground">Kaleidoscope integration wires in PR3</span>
            ) : null}
          </div>
        </div>
      ))}
    </div>
  </SettingsCard>
</SettingsSection>
```

Wire `Enable CLI`, `Enable MCP`, and `Set first` buttons to the new bridge commands. Render `Run probe` as disabled in PR1 with copy `Probe gates wire in PR2`.

- [ ] **Step 5: Run frontend tests**

Run:

```bash
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/lib/browser-runtime/browser-runtime-control-center.ts ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx
git commit -m "feat(browser-runtime): render control center foundation" -m "Verification: cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx (expected PASS)"
```

## Final Verification

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status
cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
npm --prefix ui run build
git diff --check
```

Expected:

- Rust tests PASS.
- Vitest files PASS.
- UI build exits 0.
- `git diff --check` exits 0.
