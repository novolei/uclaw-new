import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@/test-utils/render'
import { renderWithProviders } from '@/test-utils/render'
import { BrowserRuntimeSettings } from './BrowserRuntimeSettings'
import type {
  BrowserRuntimePackExecutionReport,
  StartupRuntimePackStatusReport,
} from '@/lib/startup/startup-doctor'
import {
  dryRunBrowserRuntimeAction,
  getBrowserRuntimeStatus,
  listBrowserIdentities,
  revokeBrowserIdentity,
  type BrowserIdentityStatusReport,
} from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  dryRunBrowserRuntimeAction: vi.fn(),
  getBrowserRuntimeStatus: vi.fn(),
  listBrowserIdentities: vi.fn(),
  revokeBrowserIdentity: vi.fn(),
}))

function runtimeReport(manifestPackVersion = '1.48.2-uclaw.1'): StartupRuntimePackStatusReport {
  return {
    manifestPackVersion,
    runtimeRoot: '/uclaw/browser-runtime',
    currentPackDir: '/uclaw/browser-runtime/current',
    ready: true,
    canRunBrowserTasks: true,
    primaryAction: 'keep_current',
    eventNames: ['browser.runtime.doctor.completed'],
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Browser runtime is ready.',
      actions: ['keep_current'],
      manifestPackVersion,
      rollbackAvailable: true,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'ready',
      summary: 'Runtime pack is ready.',
      eventNames: ['browser.runtime.keep_current.ready'],
    },
    supervisor: {
      providerId: 'browser.local_chromium',
      selectedSessionId: 'startup-runtime-status',
      runtimeState: 'ready',
      doctorStatus: 'ready',
      activeContextCount: 0,
      activeContextSessions: [],
      detail: 'Local Chromium supervisor is ready.',
    },
    providerReadiness: {
      localChromium: {
        providerId: 'browser.local_chromium',
        displayName: 'Local Chromium',
        readiness: 'ready',
        ready: true,
        setupComplete: true,
        activeContexts: 0,
        remediation: [],
        notes: [],
      },
      playwrightCli: {
        providerId: 'browser.playwright_cli',
        displayName: 'Playwright CLI',
        readiness: 'needs_setup',
        ready: false,
        setupComplete: false,
        activeContexts: 0,
        remediation: ['Prepare the runtime pack.'],
        notes: [],
      },
      playwrightMcp: {
        providerId: 'browser.playwright_mcp',
        displayName: 'Playwright MCP',
        readiness: 'needs_setup',
        ready: false,
        setupComplete: false,
        activeContexts: 0,
        remediation: ['Prepare the runtime pack.'],
        notes: [],
      },
    },
    supervisorEventNames: ['browser.startup_doctor.ready'],
  }
}

function dryRunReport(): BrowserRuntimePackExecutionReport {
  return {
    operation: 'repair',
    mode: 'dry_run',
    status: 'succeeded',
    summary: 'Dry run succeeded: Repair Browser runtime pack after policy checks.',
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
    manifestPackVersion: '1.48.2-uclaw.1',
    runtimeRoot: '/uclaw/browser-runtime',
    currentPackDir: '/uclaw/browser-runtime/current',
    usesNetwork: false,
    destructive: false,
    requiresConfirmation: false,
    keepsCurrentPack: true,
  }
}

function identityReport(
  overrides: Partial<BrowserIdentityStatusReport> = {},
): BrowserIdentityStatusReport {
  return {
    profiles: [],
    authorizedCount: 0,
    revokedCount: 0,
    activeTaskCount: 0,
    activeTasks: [],
    ...overrides,
  }
}

function authorizedIdentityReport(): BrowserIdentityStatusReport {
  return identityReport({
    profiles: [
      {
        id: 'auth-example',
        label: 'Example',
        originPattern: 'https://*.example.com',
        kind: 'storage_state',
        provider: 'playwright',
        scope: 'global',
        createdAtMs: 1_770_000_000_000,
        lastUsedAtMs: 1_770_000_010_000,
        lastVerifiedAtMs: null,
        expiresAtMs: null,
        revokedAtMs: null,
        status: 'live',
        revoked: false,
      },
    ],
    authorizedCount: 1,
    revokedCount: 0,
  })
}

function revokedIdentityReport(): BrowserIdentityStatusReport {
  return identityReport({
    profiles: [
      {
        ...authorizedIdentityReport().profiles[0],
        status: 'revoked',
        revoked: true,
        revokedAtMs: 1_770_000_020_000,
      },
    ],
    authorizedCount: 0,
    revokedCount: 1,
  })
}

function activeIdentityTaskReport(): BrowserIdentityStatusReport {
  return identityReport({
    ...authorizedIdentityReport(),
    activeTaskCount: 2,
    activeTasks: [
      {
        profileId: 'auth-example',
        runId: 'run-active-1',
        sessionId: 'session-active-1',
        task: 'Use an authorized dashboard',
        status: 'running',
        startedAtMs: 1_770_000_010_000,
        updatedAtMs: 1_770_000_020_000,
        drainDeadlineMs: null,
      },
      {
        profileId: 'auth-example',
        runId: 'run-draining-2',
        sessionId: 'session-draining-2',
        task: 'Finish a revoked identity action',
        status: 'paused_checkpointed',
        startedAtMs: 1_770_000_030_000,
        updatedAtMs: 1_770_000_040_000,
        drainDeadlineMs: 1_770_000_045_000,
      },
    ],
  })
}

describe('BrowserRuntimeSettings', () => {
  beforeEach(() => {
    vi.mocked(dryRunBrowserRuntimeAction).mockReset()
    vi.mocked(getBrowserRuntimeStatus).mockReset()
    vi.mocked(listBrowserIdentities).mockReset()
    vi.mocked(revokeBrowserIdentity).mockReset()
    vi.mocked(listBrowserIdentities).mockReturnValue(
      new Promise<BrowserIdentityStatusReport>(() => {}),
    )
  })

  it('renders a readonly default surface while live status is pending', () => {
    vi.mocked(getBrowserRuntimeStatus).mockReturnValue(
      new Promise<StartupRuntimePackStatusReport>(() => {}),
    )

    renderWithProviders(<BrowserRuntimeSettings />)

    expect(screen.getByText('运行时 Supervisor')).toBeInTheDocument()
    expect(screen.getByText('Playwright runtime pack')).toBeInTheDocument()
    expect(screen.getAllByText('未检查').length).toBeGreaterThan(1)
    expect(screen.getAllByText('等待运行时状态').length).toBeGreaterThan(1)
    expect(screen.getByRole('button', { name: '预览准备' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '运行诊断' })).toBeDisabled()
  })

  it('loads browser identity status through the dedicated bridge', async () => {
    vi.mocked(listBrowserIdentities).mockResolvedValueOnce(authorizedIdentityReport())

    renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(listBrowserIdentities).toHaveBeenCalledTimes(1)
    })
    expect(screen.getByText('浏览器身份')).toBeInTheDocument()
    expect(screen.getByText('Example')).toBeInTheDocument()
    expect(screen.getByText('https://*.example.com · Playwright · Global')).toBeInTheDocument()
    expect(screen.getByText('1 可用 / 0 已撤销')).toBeInTheDocument()
    expect(screen.getByText('0 个任务')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '撤销 Example' })).toBeEnabled()
  })

  it('revokes a browser identity and refreshes status', async () => {
    vi.mocked(listBrowserIdentities)
      .mockResolvedValueOnce(authorizedIdentityReport())
      .mockResolvedValueOnce(revokedIdentityReport())
    vi.mocked(revokeBrowserIdentity).mockResolvedValueOnce({
      profile: revokedIdentityReport().profiles[0],
      revoked: true,
      activeTaskCount: 0,
      activeTasks: [],
      drainDeadlineMs: null,
    })

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '撤销 Example' })).toBeEnabled()
    })

    await user.click(screen.getByRole('button', { name: '撤销 Example' }))

    await waitFor(() => {
      expect(revokeBrowserIdentity).toHaveBeenCalledWith('auth-example')
    })
    await waitFor(() => {
      expect(listBrowserIdentities).toHaveBeenCalledTimes(2)
    })
    expect(screen.getByRole('button', { name: '已撤销 Example' })).toBeDisabled()
    expect(screen.getByText('0 可用 / 1 已撤销')).toBeInTheDocument()
  })

  it('renders browser identity active task details', async () => {
    vi.mocked(listBrowserIdentities).mockResolvedValueOnce(activeIdentityTaskReport())

    renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByText('Use an authorized dashboard')).toBeInTheDocument()
    })
    expect(screen.getByText('Finish a revoked identity action')).toBeInTheDocument()
    expect(screen.getByText('2 个任务')).toBeInTheDocument()
    expect(screen.getByText('运行中')).toBeInTheDocument()
    expect(screen.getByText('已检查点暂停')).toBeInTheDocument()
    expect(screen.getByText('session-active-1 · run-active-1')).toBeInTheDocument()
    expect(screen.getByText('session-draining-2 · run-draining-2')).toBeInTheDocument()
    expect(screen.getByText(/撤销 drain 至/)).toBeInTheDocument()
  })

  it('loads live runtime status through the dedicated read-only bridge', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce(runtimeReport())

    renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(getBrowserRuntimeStatus).toHaveBeenCalledTimes(1)
    })
    await waitFor(() => {
      expect(screen.getByText('1.48.2-uclaw.1')).toBeInTheDocument()
    })
    expect(screen.getByText('Rust Browser Runtime Supervisor')).toBeInTheDocument()
    expect(screen.getByText('Local Chromium: 可用, setup 完成, 0 个上下文')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '预览保持当前' })).toBeEnabled()
  })

  it('refreshes live runtime status from the run-doctor action', async () => {
    vi.mocked(getBrowserRuntimeStatus)
      .mockResolvedValueOnce(runtimeReport('1.48.2-uclaw.1'))
      .mockResolvedValueOnce(runtimeReport('1.49.0-uclaw.1'))

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByText('1.48.2-uclaw.1')).toBeInTheDocument()
    })

    await user.click(screen.getByRole('button', { name: '运行诊断' }))

    await waitFor(() => {
      expect(getBrowserRuntimeStatus).toHaveBeenCalledTimes(2)
    })
    await waitFor(() => {
      expect(screen.getByText('1.49.0-uclaw.1')).toBeInTheDocument()
    })
    expect(screen.getByText('刷新 Startup Doctor / Browser Runtime 状态，只读取本地运行时状态。')).toBeInTheDocument()
  })

  it('keeps explicit status previews from invoking run-doctor refreshes', async () => {
    const { user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: runtimeReport(),
        }}
      />,
    )

    await user.click(screen.getByRole('button', { name: '运行诊断' }))

    expect(getBrowserRuntimeStatus).not.toHaveBeenCalled()
    expect(screen.getByText('刷新 Startup Doctor / Browser Runtime 状态，只读取本地运行时状态。')).toBeInTheDocument()
  })

  it('keeps the last live status when run-doctor refresh fails', async () => {
    vi.mocked(getBrowserRuntimeStatus)
      .mockResolvedValueOnce(runtimeReport('1.48.2-uclaw.1'))
      .mockRejectedValueOnce(new Error('runtime status unavailable'))

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByText('1.48.2-uclaw.1')).toBeInTheDocument()
    })

    await user.click(screen.getByRole('button', { name: '运行诊断' }))

    await waitFor(() => {
      expect(getBrowserRuntimeStatus).toHaveBeenCalledTimes(2)
    })
    expect(screen.getByText('1.48.2-uclaw.1')).toBeInTheDocument()
  })

  it('renders runtime metadata from the Phase 2 status report adapter', () => {
    renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: runtimeReport(),
          artifactSizeBytes: 734 * 1024 * 1024,
          runtimePackPath: '/uclaw/browser-runtime/v1',
          releaseChannel: 'stable',
          updateState: 'current',
          developerFallbackEnabled: false,
          autoPrepareEnabled: true,
        }}
      />,
    )

    expect(screen.getAllByText('可用').length).toBeGreaterThan(1)
    expect(screen.getByText('1.48.2-uclaw.1')).toBeInTheDocument()
    expect(screen.getByText('更新状态')).toBeInTheDocument()
    expect(screen.getByText('当前版本')).toBeInTheDocument()
    expect(screen.getByText('开发者回退')).toBeInTheDocument()
    expect(screen.getByText('未启用')).toBeInTheDocument()
    expect(screen.getByText('734 MiB')).toBeInTheDocument()
    expect(screen.getByText('运行时根目录')).toBeInTheDocument()
    expect(screen.getByText('/uclaw/browser-runtime')).toBeInTheDocument()
    expect(screen.getByText('当前 pack')).toBeInTheDocument()
    expect(screen.getByText('/uclaw/browser-runtime/current')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '预览保持当前' })).toBeEnabled()
    expect(screen.getByRole('button', { name: '关闭自动准备' })).toBeEnabled()
    expect(screen.queryByText('操作预览')).not.toBeInTheDocument()
  })

  it('selects runtime action intents without invoking side effects', async () => {
    const { user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: {
            ...runtimeReport(),
            ready: false,
            canRunBrowserTasks: false,
            primaryAction: 'repair',
            doctor: {
              status: 'needs_repair',
              ready: false,
              issue: 'corrupt_cache',
              remediation: 'Runtime cache is corrupt.',
              actions: ['repair', 'rollback'],
              manifestPackVersion: '1.48.2-uclaw.1',
              rollbackAvailable: true,
              activeTasks: 0,
            },
            operationPlan: {
              status: 'planned',
              summary: 'Repair Browser runtime pack after policy checks.',
              eventNames: ['browser.runtime.repair.planned'],
            },
          },
        }}
      />,
    )

    await user.click(screen.getByRole('button', { name: '预览回滚' }))

    expect(screen.getByText('回滚到上一个可用 Browser runtime pack，需要明确确认并等待后端执行边界接入。')).toBeInTheDocument()
    expect(screen.getByText('本地预估 · 需确认')).toBeInTheDocument()
    expect(screen.getByText('无副作用')).toBeInTheDocument()
  })

  it('requests backend dry-run evidence for live runtime action controls', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce({
      ...runtimeReport(),
      ready: false,
      canRunBrowserTasks: false,
      primaryAction: 'repair',
      doctor: {
        status: 'needs_repair',
        ready: false,
        issue: 'corrupt_cache',
        remediation: 'Runtime cache is corrupt.',
        actions: ['repair'],
        manifestPackVersion: '1.48.2-uclaw.1',
        rollbackAvailable: true,
        activeTasks: 0,
      },
      operationPlan: {
        status: 'planned',
        summary: 'Repair Browser runtime pack after policy checks.',
        eventNames: ['browser.runtime.repair.planned'],
      },
    })
    vi.mocked(dryRunBrowserRuntimeAction).mockResolvedValueOnce(dryRunReport())

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '预览修复' })).toBeEnabled()
    })

    await user.click(screen.getByRole('button', { name: '预览修复' }))

    await waitFor(() => {
      expect(dryRunBrowserRuntimeAction).toHaveBeenCalledWith('repair')
    })
    expect(screen.getByText('Dry run succeeded: Repair Browser runtime pack after policy checks.')).toBeInTheDocument()
    expect(screen.getByText('browser.runtime.repair.dry_run_succeeded')).toBeInTheDocument()
    expect(screen.getByText('browser-runtime-repair-dry_run_succeeded')).toBeInTheDocument()
    expect(screen.getByText('1 steps')).toBeInTheDocument()
  })

  it('clears stale dry-run evidence when a later dry-run request fails', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce({
      ...runtimeReport(),
      ready: false,
      canRunBrowserTasks: false,
      primaryAction: 'repair',
      doctor: {
        status: 'needs_repair',
        ready: false,
        issue: 'corrupt_cache',
        remediation: 'Runtime cache is corrupt.',
        actions: ['repair'],
        manifestPackVersion: '1.48.2-uclaw.1',
        rollbackAvailable: true,
        activeTasks: 0,
      },
      operationPlan: {
        status: 'planned',
        summary: 'Repair Browser runtime pack after policy checks.',
        eventNames: ['browser.runtime.repair.planned'],
      },
    })
    vi.mocked(dryRunBrowserRuntimeAction)
      .mockResolvedValueOnce(dryRunReport())
      .mockRejectedValueOnce(new Error('dry-run unavailable'))

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '预览修复' })).toBeEnabled()
    })

    await user.click(screen.getByRole('button', { name: '预览修复' }))

    await waitFor(() => {
      expect(screen.getByText('Dry run succeeded: Repair Browser runtime pack after policy checks.')).toBeInTheDocument()
    })

    await user.click(screen.getByRole('button', { name: '预览修复' }))

    await waitFor(() => {
      expect(dryRunBrowserRuntimeAction).toHaveBeenCalledTimes(2)
    })
    expect(screen.queryByText('Dry run succeeded: Repair Browser runtime pack after policy checks.')).not.toBeInTheDocument()
    expect(screen.getByText('dry-run unavailable')).toBeInTheDocument()
  })

  it('keeps retry-when-online as a local preview until it has distinct dry-run evidence', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce({
      ...runtimeReport(),
      ready: false,
      canRunBrowserTasks: false,
      primaryAction: 'retry_when_online',
      doctor: {
        status: 'deferred',
        ready: false,
        issue: 'offline_download',
        remediation: 'Browser runtime can retry when network returns.',
        actions: ['retry_when_online'],
        manifestPackVersion: '1.48.2-uclaw.1',
        rollbackAvailable: false,
        activeTasks: 0,
      },
      operationPlan: {
        status: 'deferred',
        summary: 'Runtime pack preparation is deferred while offline.',
        eventNames: ['browser.runtime.prepare.deferred'],
      },
    })

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '联网后重试' })).toBeEnabled()
    })

    await user.click(screen.getByRole('button', { name: '联网后重试' }))

    expect(dryRunBrowserRuntimeAction).not.toHaveBeenCalled()
    expect(screen.getAllByText('Runtime pack preparation is deferred while offline.').length).toBeGreaterThan(0)
  })

  it('keeps explicit status previews from invoking action dry runs', async () => {
    const { user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: {
            ...runtimeReport(),
            ready: false,
            canRunBrowserTasks: false,
            primaryAction: 'repair',
            doctor: {
              status: 'needs_repair',
              ready: false,
              issue: 'corrupt_cache',
              remediation: 'Runtime cache is corrupt.',
              actions: ['repair'],
              manifestPackVersion: '1.48.2-uclaw.1',
              rollbackAvailable: true,
              activeTasks: 0,
            },
            operationPlan: {
              status: 'planned',
              summary: 'Repair Browser runtime pack after policy checks.',
              eventNames: ['browser.runtime.repair.planned'],
            },
          },
        }}
      />,
    )

    await user.click(screen.getByRole('button', { name: '预览修复' }))

    expect(dryRunBrowserRuntimeAction).not.toHaveBeenCalled()
    expect(screen.getAllByText('Repair Browser runtime pack after policy checks.').length).toBeGreaterThan(0)
  })

  it('previews auto-prepare control without disabling browser capability', async () => {
    const { user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: runtimeReport(),
          autoPrepareEnabled: true,
        }}
      />,
    )

    await user.click(screen.getByRole('button', { name: '关闭自动准备' }))

    expect(screen.getByText('关闭启动/后台自动准备；浏览器任务仍可在使用时请求准备运行时。')).toBeInTheDocument()
    expect(screen.getByText('browser.runtime.auto_prepare.disable.requested')).toBeInTheDocument()
    expect(screen.getByText('本地预估')).toBeInTheDocument()
  })
})
