import { describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { dryRunBrowserRuntimeAction, getBrowserRuntimeStatus } from './tauri-bridge'
import type {
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
})
