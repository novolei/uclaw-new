import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@/test-utils/render'
import { renderWithProviders } from '@/test-utils/render'
import { mockInvoke, resetTauriMocks } from '@/test-utils/mock-tauri'
import { SystemTab } from './SystemTab'

vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }))

const diagnostics = {
  app_version: '0.1.0',
  platform: 'macos',
  arch: 'aarch64',
  memory_used_mb: 512,
  memory_total_mb: 1024,
  uptime_secs: 60,
  consecutive_failures: 0,
  recovery_attempts: 0,
  active_processes: 1,
  orphan_processes: 0,
  services: [],
  memu: {
    running: true,
    pid: 123,
    reason: null,
    python_path: '/python',
    script_path: '/memu_bridge.py',
    db_path: '/memu.db',
  },
  gbrain: {
    connected: true,
    tool_count: 6,
    pgdata_ready: true,
    error: null,
    status: 'connected',
    error_kind: null,
    suggested_action: null,
    home_path: '/gbrain',
    launcher_path: '/bun',
    pgdata_path: '/pgdata',
    config_command: '/bun',
    config_entry_path: '/cli.ts',
    config_command_exists: true,
    config_entry_exists: true,
    config_gbrain_home: '/gbrain',
    path_stale: false,
  },
  gbrain_init: { status: 'skipped_already_initialized', at_ms: 1 },
}

const suite = {
  passed: true,
  averageScore: 1,
  runIds: ['run-1'],
  scorecards: [
    {
      caseId: 'agent_loop.tool_result_pairing',
      title: 'Tool calls are paired with tool results',
      passed: true,
      score: 1,
      checks: [{ id: 'tool_results_paired', passed: true, score: 1, message: 'ok' }],
    },
  ],
}

describe('SystemTab harness reporting', () => {
  beforeEach(() => {
    resetTauriMocks()
  })

  it('runs the agent control-plane harness and renders the scorecard', async () => {
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'get_system_diagnostics') return diagnostics
      if (command === 'run_agent_control_plane_harness') return suite
      throw new Error(`unexpected command ${command}`)
    })

    const { user } = renderWithProviders(<SystemTab />)
    await user.click(screen.getByRole('button', { name: /运行诊断/ }))
    await screen.findByText('自治回归套件')

    await user.click(screen.getByRole('button', { name: /Agent/ }))

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('run_agent_control_plane_harness')
    })
    expect(await screen.findByText('agent control-plane')).toBeInTheDocument()
    expect(screen.getByText('Tool calls are paired with tool results')).toBeInTheDocument()
  })
})
