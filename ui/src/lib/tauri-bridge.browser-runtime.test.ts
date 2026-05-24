import { describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { getBrowserRuntimeStatus } from './tauri-bridge'
import type { StartupRuntimePackStatusReport } from './startup/startup-doctor'

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
})
