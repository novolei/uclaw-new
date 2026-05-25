import { describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import {
  dryRunBrowserRuntimeAction,
  getBrowserRuntimeControlCenter,
  getBrowserRuntimeStatus,
  runBrowserRuntimeProviderProbe,
  setBrowserRuntimeProviderEnabled,
  setBrowserRuntimeProviderPriority,
} from './tauri-bridge'
import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimePackExecutionReport,
  StartupRuntimePackStatusReport,
} from './startup/startup-doctor'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}))

describe('browser runtime tauri bridge', () => {
  it('queries the dedicated read-only Browser Runtime status command', async () => {
    const report: StartupRuntimePackStatusReport = {
      manifestPackVersion: 'browser-runtime-pack-v1',
      runtimeRoot: '/uclaw/browser-runtime',
      currentPackDir: '/uclaw/browser-runtime/current',
      ready: false,
      canRunBrowserTasks: false,
      primaryAction: 'prepare',
      eventNames: ['browser.runtime.doctor.completed'],
      doctor: {
        status: 'needs_prepare',
        ready: false,
        issue: 'missing_manifest',
        remediation: 'Prepare the Browser runtime pack before running Playwright providers.',
        actions: ['prepare'],
        manifestPackVersion: 'browser-runtime-pack-v1',
        rollbackAvailable: false,
        activeTasks: 0,
      },
      operationPlan: {
        status: 'planned',
        summary: 'Prepare the pinned Browser runtime pack in uClaw-managed storage.',
        eventNames: ['browser.runtime.prepare.planned'],
      },
    }
    vi.mocked(invoke).mockResolvedValueOnce(report)

    await expect(getBrowserRuntimeStatus()).resolves.toEqual(report)
    expect(invoke).toHaveBeenCalledWith('get_browser_runtime_status')
  })

  it('requests a no-side-effect Browser Runtime action dry run', async () => {
    const report: BrowserRuntimePackExecutionReport = {
      operation: 'repair',
      mode: 'dry_run',
      status: 'succeeded',
      summary: 'Dry run succeeded: Repair Browser runtime pack.',
      artifactId: 'browser-runtime-repair-dry_run_succeeded',
      eventNames: ['browser.runtime.repair.dry_run_succeeded'],
      stepReports: [
        {
          step: 'run_doctor',
          status: 'would_run',
          label: 'Run Browser runtime doctor after repair.',
          usesNetwork: false,
          destructive: false,
          requiresConfirmation: false,
        },
      ],
      manifestPackVersion: 'browser-runtime-pack-v1',
      runtimeRoot: '/uclaw/browser-runtime',
      currentPackDir: '/uclaw/browser-runtime/current',
      usesNetwork: false,
      destructive: false,
      requiresConfirmation: false,
      keepsCurrentPack: true,
    }
    vi.mocked(invoke).mockResolvedValueOnce(report)

    await expect(dryRunBrowserRuntimeAction('repair')).resolves.toEqual(report)
    expect(invoke).toHaveBeenCalledWith('dry_run_browser_runtime_action', {
      action: 'repair',
    })
  })

  it('invokes get_browser_runtime_control_center', async () => {
    const report: BrowserRuntimeControlCenterReport = {
      featureFlags: {
        playwrightCli: false,
        playwrightMcp: false,
        hostedBrowser: false,
        forceLegacyLocalChromium: false,
      },
      desiredProviderPriority: [
        'browser.playwright_cli',
        'browser.playwright_mcp',
        'browser.local_chromium',
      ],
      activeProviderRoute: {
        providerId: 'browser.local_chromium',
        displayName: 'Local Chromium',
      },
      providerLanes: [],
      mcpIntegrationSummary: {
        builtIn: true,
        enabled: false,
        rawToolsExposed: false,
        configureRouteReady: false,
      },
      updatedAtMs: 0,
    }
    vi.mocked(invoke).mockResolvedValueOnce(report)

    await expect(getBrowserRuntimeControlCenter()).resolves.toEqual(report)
    expect(invoke).toHaveBeenCalledWith('get_browser_runtime_control_center')
  })

  it('invokes provider enable and priority commands', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ ok: true })
    await setBrowserRuntimeProviderEnabled('browser.playwright_cli', true)
    expect(invoke).toHaveBeenCalledWith('set_browser_runtime_provider_enabled', {
      providerId: 'browser.playwright_cli',
      enabled: true,
    })

    vi.mocked(invoke).mockResolvedValueOnce({ ok: true })
    await setBrowserRuntimeProviderPriority([
      'browser.playwright_cli',
      'browser.playwright_mcp',
      'browser.local_chromium',
    ])
    expect(invoke).toHaveBeenCalledWith('set_browser_runtime_provider_priority', {
      providerIds: [
        'browser.playwright_cli',
        'browser.playwright_mcp',
        'browser.local_chromium',
      ],
    })
  })

  it('invokes run_browser_runtime_provider_probe', async () => {
    const summary = {
      providerId: 'browser.playwright_cli',
      state: 'passed',
      checkedAtMs: 1,
      artifactId: 'browser-runtime-provider-probe-passed',
      message: 'Provider probe passed.',
      eventNames: ['browser.runtime.provider.probe.passed'],
    }
    vi.mocked(invoke).mockResolvedValueOnce(summary)

    await expect(runBrowserRuntimeProviderProbe('browser.playwright_cli')).resolves.toEqual(summary)
    expect(invoke).toHaveBeenCalledWith('run_browser_runtime_provider_probe', {
      providerId: 'browser.playwright_cli',
    })
  })
})
