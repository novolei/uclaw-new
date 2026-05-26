# Browser Automation Official Runtime PR5 Control Center And Agent Skills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the Settings browser runtime page as a user-facing Browser Automation Control Center and make official Playwright CLI skills discoverable as a uClaw built-in skill pack while enforcing Browser Runtime Adapter execution.

**Architecture:** The frontend renders a simplified backend read model through a redesigned Settings surface, not a runtime-pack diagnostic table. Playwright skills are synchronized into a uClaw-managed built-in skill pack with compatibility metadata; ordinary Agent execution never runs arbitrary shell commands from skills.

**Tech Stack:** React/TypeScript settings UI, Rust setup/skills DTOs, uClaw skill discovery.

---

## File Structure

- Create: `src-tauri/src/browser/playwright_skills.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/skills_manifest.rs`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Create: `ui/src/components/settings/browser-runtime/BrowserAutomationHeader.tsx`
- Create: `ui/src/components/settings/browser-runtime/ProviderPriorityList.tsx`
- Create: `ui/src/components/settings/browser-runtime/PlaywrightSetupProgress.tsx`
- Create: `ui/src/components/settings/browser-runtime/PlaywrightSkillsPanel.tsx`
- Create: `ui/src/components/settings/browser-runtime/BrowserAutomationDiagnostics.tsx`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- Modify: `ui/src/lib/tauri-bridge.ts`
- Test: `src-tauri/src/browser/playwright_skills_tests.rs`
- Test: `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- Test: `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts`

## Task 1: Add Built-In Playwright Skills Metadata

**Files:**
- Create: `src-tauri/src/browser/playwright_skills.rs`
- Modify: `src-tauri/src/browser/mod.rs`

- [ ] **Step 1: Add tests**

Create `src-tauri/src/browser/playwright_skills_tests.rs`:

```rust
use super::playwright_skills::*;

#[test]
fn compatible_skill_is_enabled() {
    let skill = PlaywrightSkillManifest {
        name: "playwright-navigate".to_string(),
        source_version: "1.0.0".to_string(),
        required_capabilities: vec!["navigate".to_string()],
        hash: "abc".to_string(),
    };

    let report = classify_playwright_skill(&skill);
    assert_eq!(report.status, PlaywrightSkillCompatibilityStatus::Enabled);
}

#[test]
fn raw_shell_skill_is_unavailable() {
    let skill = PlaywrightSkillManifest {
        name: "playwright-raw-shell".to_string(),
        source_version: "1.0.0".to_string(),
        required_capabilities: vec!["raw_shell".to_string()],
        hash: "abc".to_string(),
    };

    let report = classify_playwright_skill(&skill);
    assert_eq!(report.status, PlaywrightSkillCompatibilityStatus::Unavailable);
    assert_eq!(report.reason.as_deref(), Some("unsupported_capability:raw_shell"));
}
```

- [ ] **Step 2: Implement module**

Create `src-tauri/src/browser/playwright_skills.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSkillManifest {
    pub name: String,
    pub source_version: String,
    pub required_capabilities: Vec<String>,
    pub hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightSkillCompatibilityStatus {
    Enabled,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSkillCompatibilityReport {
    pub name: String,
    pub status: PlaywrightSkillCompatibilityStatus,
    pub reason: Option<String>,
}

pub fn classify_playwright_skill(
    skill: &PlaywrightSkillManifest,
) -> PlaywrightSkillCompatibilityReport {
    let supported = ["navigate", "click", "type", "snapshot", "screenshot", "trace"];
    for capability in &skill.required_capabilities {
        if !supported.contains(&capability.as_str()) {
            return PlaywrightSkillCompatibilityReport {
                name: skill.name.clone(),
                status: PlaywrightSkillCompatibilityStatus::Unavailable,
                reason: Some(format!("unsupported_capability:{capability}")),
            };
        }
    }
    PlaywrightSkillCompatibilityReport {
        name: skill.name.clone(),
        status: PlaywrightSkillCompatibilityStatus::Enabled,
        reason: None,
    }
}

#[cfg(test)]
#[path = "playwright_skills_tests.rs"]
mod playwright_skills_tests;
```

Update `mod.rs`:

```rust
pub mod playwright_skills;
```

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_skills
```

Expected: PASS.

## Task 2: Wire Skills Into uClaw Discovery

**Files:**
- Modify: `src-tauri/src/browser/playwright_skills.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/skills_manifest.rs`

- [ ] **Step 1: Add managed built-in skills directory helper**

Extend `src-tauri/src/browser/playwright_skills.rs`:

```rust
use std::path::{Path, PathBuf};

pub const PLAYWRIGHT_BUILTIN_SKILLS_DIR_NAME: &str = "playwright-cli";

pub fn managed_playwright_skills_dir(data_dir: &Path) -> PathBuf {
    data_dir
        .join("builtin-skills")
        .join(PLAYWRIGHT_BUILTIN_SKILLS_DIR_NAME)
}
```

Rationale: do not place managed official skills under `data_dir/skills`, because
that tree is user-owned and currently scanned as `SkillProvenance::User`.

- [ ] **Step 2: Register managed Playwright skills as built-ins**

In `src-tauri/src/app.rs`, after resource-dir bundled skills and before
`user_skills_dir`, register the managed Playwright directory:

```rust
let managed_playwright_skills =
    crate::browser::playwright_skills::managed_playwright_skills_dir(&data_dir);
if managed_playwright_skills.exists() {
    tracing::info!(
        skills_dir = %managed_playwright_skills.display(),
        "Registering managed Playwright built-in skills scan dir"
    );
    skills_reg.add_scan_dir(
        managed_playwright_skills,
        crate::skills::SkillProvenance::Bundled,
    );
}
```

This makes official Playwright CLI skills act like built-ins in the Agent
manifest and Settings skill surfaces, while keeping user forks in
`data_dir/skills` separate.

- [ ] **Step 3: Add discovery provenance test**

In `src-tauri/src/skills.rs`, extend the existing tests near
`provenance_propagates_from_scan_dir_to_loaded_skill`:

```rust
#[test]
fn managed_playwright_skills_can_register_as_bundled() {
    let tmp = tempfile::TempDir::new().unwrap();
    let managed = tmp.path().join("builtin-skills/playwright-cli/navigate");
    std::fs::create_dir_all(&managed).unwrap();
    std::fs::write(
        managed.join("SKILL.md"),
        "---\nname: playwright-navigate\ndescription: Navigate with Playwright\n---\nbody",
    ).unwrap();

    let mut reg = SkillsRegistry::new();
    reg.add_scan_dir(tmp.path().join("builtin-skills/playwright-cli"), SkillProvenance::Bundled);
    reg.discover();

    let loaded = reg.get_loaded("playwright-navigate").expect("registered");
    assert_eq!(loaded.provenance, SkillProvenance::Bundled);
}
```

- [ ] **Step 4: Gate Agent-injected Playwright skills by compatibility**

In `src-tauri/src/skills_manifest.rs`, when collecting registry skills, detect
managed Playwright built-ins by loaded skill path or metadata and call:

```rust
classify_playwright_skill(&manifest)
```

Only return `Enabled` Playwright skills to ordinary Agent prompt injection.
Store unavailable reports for the Browser Automation diagnostics view added in
Task 3. Non-Playwright skills keep the existing behavior.

- [ ] **Step 5: Run skill tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib skills
```

Expected: PASS.

## Task 3: Build Browser Automation Control Center Structure

**Files:**
- Modify: `ui/src/lib/browser-runtime/browser-runtime-control-center.ts`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Create: `ui/src/components/settings/browser-runtime/BrowserAutomationHeader.tsx`
- Create: `ui/src/components/settings/browser-runtime/ProviderPriorityList.tsx`
- Create: `ui/src/components/settings/browser-runtime/PlaywrightSetupProgress.tsx`
- Create: `ui/src/components/settings/browser-runtime/PlaywrightSkillsPanel.tsx`
- Create: `ui/src/components/settings/browser-runtime/BrowserAutomationDiagnostics.tsx`

- [ ] **Step 1: Add frontend view-model test**

In `ui/src/lib/browser-runtime/browser-runtime-control-center.test.ts`, add:

```ts
it('labels Node setup as actionable setup instead of runtime pack prep', () => {
  const view = deriveBrowserRuntimeControlCenterViewModel({
    featureFlags: { playwrightCli: true, playwrightMcp: true },
    desiredProviderPriority: ['browser.playwright_cli', 'browser.playwright_mcp', 'browser.local_chromium'],
    activeProviderRoute: { providerId: 'browser.local_chromium', displayName: 'Local Chromium' },
    providerLanes: [
      {
        providerId: 'browser.playwright_cli',
        displayName: 'Playwright CLI',
        enabled: true,
        priorityRank: 1,
        readiness: 'needssetup',
        routable: false,
        routeRole: 'desired_first',
        probeState: 'blocked',
        fallbackReason: 'playwright_setup_not_ready',
        nextAction: 'run_playwright_setup',
      },
    ],
    mcpIntegrationSummary: { builtIn: true, enabled: true, rawToolsExposed: false, configureRouteReady: true },
    updatedAtMs: 1,
  })

  expect(view.providerRows[0].statusLabel).toBe('Needs setup')
  expect(view.providerRows[0].nextActionLabel).toBe('Set up')
})
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts
```

Expected: FAIL until labels are updated.

- [ ] **Step 3: Update labels and view-model fields**

Replace runtime-pack labels with Browser Automation setup labels:

```ts
if (lane.fallbackReason === 'playwright_setup_not_ready') return 'Needs setup'
if (lane.nextAction === 'run_playwright_setup') return 'Set up'
```

Remove or hide:

```ts
'Needs runtime pack'
'Prepare runtime pack'
```

- [ ] **Step 4: Create BrowserAutomationHeader**

Create `ui/src/components/settings/browser-runtime/BrowserAutomationHeader.tsx`:

```tsx
import { Activity, CheckCircle2, Download, RefreshCw, TriangleAlert } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

interface BrowserAutomationHeaderProps {
  title: string
  statusLabel: string
  statusTone: 'ready' | 'setup' | 'blocked' | 'degraded' | 'failed'
  activeProviderLabel: string
  lastCheckedLabel?: string
  primaryActionLabel: string
  primaryActionPending?: boolean
  onPrimaryAction: () => void
}

export function BrowserAutomationHeader({
  title,
  statusLabel,
  statusTone,
  activeProviderLabel,
  lastCheckedLabel,
  primaryActionLabel,
  primaryActionPending,
  onPrimaryAction,
}: BrowserAutomationHeaderProps) {
  const Icon = statusTone === 'ready' ? CheckCircle2 : statusTone === 'failed' || statusTone === 'blocked' ? TriangleAlert : Activity

  return (
    <section className="grid gap-4 border-b border-border pb-5 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <Icon className="size-5" aria-hidden="true" />
          <h2 className="text-xl font-semibold">{title}</h2>
          <Badge variant={statusTone === 'ready' ? 'default' : 'secondary'}>{statusLabel}</Badge>
        </div>
        <p className="mt-2 text-sm text-muted-foreground">
          Active provider: {activeProviderLabel}
          {lastCheckedLabel ? ` · Last checked ${lastCheckedLabel}` : ''}
        </p>
      </div>
      <Button type="button" onClick={onPrimaryAction} disabled={primaryActionPending}>
        {primaryActionPending ? <RefreshCw className="animate-spin" /> : <Download />}
        {primaryActionPending ? 'Working' : primaryActionLabel}
      </Button>
    </section>
  )
}
```

- [ ] **Step 5: Create ProviderPriorityList**

Create `ui/src/components/settings/browser-runtime/ProviderPriorityList.tsx`:

```tsx
import { Bug, Settings2, RotateCw } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { BrowserRuntimeProviderId } from '@/lib/startup/startup-doctor'

export interface ProviderPriorityItem {
  providerId: BrowserRuntimeProviderId
  displayName: string
  statusLabel: string
  detailLabel: string
  isActive: boolean
  canSetFirst: boolean
  canConfigureMcp: boolean
}

interface ProviderPriorityListProps {
  rows: ProviderPriorityItem[]
  onSetFirst: (providerId: BrowserRuntimeProviderId) => void
  onRunProbe: (providerId: BrowserRuntimeProviderId) => void
  onConfigureMcp: () => void
}

export function ProviderPriorityList({
  rows,
  onSetFirst,
  onRunProbe,
  onConfigureMcp,
}: ProviderPriorityListProps) {
  return (
    <section className="space-y-3">
      <h3 className="text-base font-semibold">Provider Priority</h3>
      <div className="divide-y divide-border rounded-lg border border-border">
        {rows.map((row) => (
          <div key={row.providerId} className="grid gap-3 p-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">{row.displayName}</span>
                <Badge variant={row.isActive ? 'default' : 'outline'}>{row.statusLabel}</Badge>
              </div>
              <p className="mt-1 text-sm text-muted-foreground">{row.detailLabel}</p>
            </div>
            <div className="flex flex-wrap gap-2 md:justify-end">
              {row.canConfigureMcp ? (
                <Button type="button" variant="outline" size="sm" onClick={onConfigureMcp}>
                  <Settings2 />
                  Configure
                </Button>
              ) : null}
              <Button type="button" variant="outline" size="sm" onClick={() => onRunProbe(row.providerId)}>
                <Bug />
                Check
              </Button>
              <Button type="button" variant="outline" size="sm" disabled={!row.canSetFirst} onClick={() => onSetFirst(row.providerId)}>
                <RotateCw />
                Set first
              </Button>
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}
```

- [ ] **Step 6: Create setup, skills, and diagnostics panels**

Create `PlaywrightSetupProgress.tsx`:

```tsx
import { RefreshCw, TriangleAlert } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface PlaywrightSetupProgressProps {
  visible: boolean
  state: 'setting_up' | 'failed' | 'needs_node' | 'network_unavailable'
  currentStepLabel?: string
  errorMessage?: string
  onRetry: () => void
  onRecheck: () => void
}

export function PlaywrightSetupProgress({
  visible,
  state,
  currentStepLabel,
  errorMessage,
  onRetry,
  onRecheck,
}: PlaywrightSetupProgressProps) {
  if (!visible) return null

  return (
    <section className="rounded-lg border border-border p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="font-semibold">{state === 'setting_up' ? 'Setting up Playwright' : 'Setup needs attention'}</h3>
          <p className="mt-1 text-sm text-muted-foreground">{currentStepLabel ?? errorMessage ?? 'Check the setup diagnostics and try again.'}</p>
        </div>
        <Button type="button" variant="outline" onClick={state === 'needs_node' ? onRecheck : onRetry}>
          {state === 'setting_up' ? <RefreshCw className="animate-spin" /> : <TriangleAlert />}
          {state === 'needs_node' ? 'Re-check' : 'Retry'}
        </Button>
      </div>
    </section>
  )
}
```

Create `PlaywrightSkillsPanel.tsx`:

```tsx
import { RefreshCw } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

interface PlaywrightSkillsPanelProps {
  installedCount: number
  incompatibleCount: number
  agentDiscoveryEnabled: boolean
  refreshing?: boolean
  onRefresh: () => void
}

export function PlaywrightSkillsPanel({
  installedCount,
  incompatibleCount,
  agentDiscoveryEnabled,
  refreshing,
  onRefresh,
}: PlaywrightSkillsPanelProps) {
  return (
    <section className="rounded-lg border border-border p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="font-semibold">Built-in Playwright Skills</h3>
          <p className="mt-1 text-sm text-muted-foreground">
            {installedCount} installed · {incompatibleCount} incompatible · Agent discovery {agentDiscoveryEnabled ? 'enabled' : 'disabled'}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={agentDiscoveryEnabled ? 'default' : 'secondary'}>{agentDiscoveryEnabled ? 'Discoverable' : 'Hidden'}</Badge>
          <Button type="button" variant="outline" size="sm" onClick={onRefresh} disabled={refreshing}>
            <RefreshCw className={refreshing ? 'animate-spin' : undefined} />
            Refresh
          </Button>
        </div>
      </div>
    </section>
  )
}
```

Create `BrowserAutomationDiagnostics.tsx`:

```tsx
import { Bug } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface BrowserAutomationDiagnosticsProps {
  open: boolean
  rawReport: string
  onToggle: () => void
}

export function BrowserAutomationDiagnostics({ open, rawReport, onToggle }: BrowserAutomationDiagnosticsProps) {
  return (
    <section className="space-y-3">
      <Button type="button" variant="outline" onClick={onToggle}>
        <Bug />
        {open ? 'Hide diagnostics' : 'Show diagnostics'}
      </Button>
      {open ? (
        <pre className="max-h-80 overflow-auto rounded-lg border border-border bg-muted p-3 text-xs">
          {rawReport}
        </pre>
      ) : null}
    </section>
  )
}
```

- [ ] **Step 7: Update BrowserRuntimeSettings composition**

In `BrowserRuntimeSettings.tsx`, replace prepare runtime pack button action with:

```tsx
<Button type="button" variant="outline" size="sm" onClick={() => void runPlaywrightSetup()}>
  <Download />
  Set up
</Button>
```

Use the actual bridge function added by PR1/PR2. If it is not yet implemented, add a disabled UI state with a test and wire the bridge in the same PR.

- [ ] **Step 8: Run frontend tests**

Run:

```bash
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

## Task 4: Add Setup Progress And Diagnostics UI

**Files:**
- Modify: `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- Modify: `ui/src/lib/browser-runtime/browser-runtime-settings.ts`

- [ ] **Step 1: Add test for needs_node state**

In `BrowserRuntimeSettings.test.tsx`, add:

```tsx
it('shows Node install guidance when Playwright setup needs Node', () => {
  render(<BrowserRuntimeSettings status={fixtureStatus({ setupStatus: 'needs_node' })} />)
  expect(screen.getByText(/Node.js required/i)).toBeInTheDocument()
  expect(screen.getByRole('button', { name: /Re-check/i })).toBeInTheDocument()
})
```

Adapt `fixtureStatus` to the existing test helpers.

- [ ] **Step 2: Add UI copy**

Render a status block:

```tsx
{view.setupStatus === 'needs_node' ? (
  <SettingsCard>
    <h3>Node.js required</h3>
    <p>Install Node.js/npm/npx, then re-check. uClaw uses official Playwright CLI and MCP packages.</p>
    <Button type="button" onClick={refreshLiveStatus}>Re-check</Button>
  </SettingsCard>
) : null}
```

- [ ] **Step 3: Add experimental Homebrew action only when available**

Render:

```tsx
{view.experimentalNodeBootstrapAvailable ? (
  <Button type="button" variant="outline" onClick={() => void runNodeBootstrap()}>
    Install Node.js with Homebrew
  </Button>
) : null}
```

Never render sudo copy or command.

- [ ] **Step 4: Run tests**

Run:

```bash
cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

## Task 5: Verify And Commit

- [ ] **Step 1: Run backend tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_skills browser::playwright_discovery browser::playwright_setup
```

Expected: PASS.

- [ ] **Step 2: Run frontend tests**

Run:

```bash
cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx
```

Expected: PASS.

- [ ] **Step 3: Search for forbidden UI copy**

Run:

```bash
rg -n "Prepare runtime pack|Needs runtime pack|sudo|Install Homebrew" ui/src src-tauri/src/browser
```

Expected: no matches in new product UI. Historical docs/tests must be intentionally reviewed if they match.

- [ ] **Step 4: Commit**

Run:

```bash
git add src-tauri/src/browser ui/src
git commit -m "feat(browser-runtime): ship Browser Automation control center and Playwright skills"
```

Expected: commit succeeds.
