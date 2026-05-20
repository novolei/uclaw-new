import { clearMocks, mockConvertFileSrc, mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import type { InvokeArgs } from '@tauri-apps/api/core'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>
    __UCLAW_DEV_TAURI_MOCK__?: boolean
  }
}

type MockHandler = (cmd: string, payload?: InvokeArgs) => unknown

const settingsFixture = {
  language: 'zh-CN',
  theme: 'system',
  theme_style: 'default',
  provider: null,
  model: null,
  safety_mode: 'yolo',
}

const diagnosticsFixture = {
  app_version: 'dev-mock',
  platform: 'browser',
  arch: 'mock',
  memory_used_mb: 256,
  memory_total_mb: 1024,
  uptime_secs: 1,
  consecutive_failures: 0,
  recovery_attempts: 0,
  active_processes: 1,
  orphan_processes: 0,
  services: [
    { name: 'AppRuntimeService', status: 'Running', detail: 'mocked browser runtime' },
  ],
  memu: {
    running: true,
    pid: 1,
    reason: null,
    python_path: '/mock/python',
    script_path: '/mock/memu_bridge.py',
    db_path: '/mock/memu.db',
  },
  gbrain: {
    connected: true,
    tool_count: 6,
    pgdata_ready: true,
    error: null,
    status: 'connected',
    error_kind: null,
    suggested_action: null,
    home_path: '/mock/gbrain',
    launcher_path: '/mock/bun',
    pgdata_path: '/mock/pgdata',
    config_command: '/mock/bun',
    config_entry_path: '/mock/gbrain/src/cli.ts',
    config_command_exists: true,
    config_entry_exists: true,
    config_gbrain_home: '/mock/gbrain',
    path_stale: false,
  },
  gbrain_init: { status: 'skipped_already_initialized', at_ms: 1 },
}

const harnessSuiteFixture = {
  passed: true,
  averageScore: 1,
  runIds: ['mock-run'],
  scorecards: [
    {
      caseId: 'mock.browser.ui_debug',
      title: 'Mock bridge keeps browser UI debuggable',
      passed: true,
      score: 1,
      checks: [{ id: 'mock_bridge_installed', passed: true, score: 1, message: 'ok' }],
    },
  ],
}

const selfImprovementFixture = [
  {
    candidateId: 'candidate.mock.ui_debug_loop',
    verdict: 'promote',
    score: 1,
    checks: [{ id: 'rollback_ref', passed: true, message: 'ok' }],
  },
]

export function shouldInstallDevTauriMock(): boolean {
  return import.meta.env.VITE_UCLAW_MOCK_TAURI === '1'
    && typeof window !== 'undefined'
    && !window.__TAURI_INTERNALS__?.invoke
}

export function createUclawMockIpcHandler(): MockHandler {
  return (cmd: string, payload?: InvokeArgs): unknown => {
    console.info('[uClaw mock Tauri IPC]', cmd, payload ?? {})

    switch (cmd) {
      case 'get_settings':
      case 'patch_settings':
        return settingsFixture
      case 'get_platform':
        return { platform: 'browser', arch: 'mock' }
      case 'get_version':
        return { version: 'dev-mock', commit: null, build_time: null }
      case 'get_bootstrap_status':
        return { complete: true, steps: [] }
      case 'get_active_model':
        return null
      case 'list_conversations':
      case 'list_spaces':
      case 'list_notifications':
      case 'list_background_tasks':
      case 'list_mcp_servers':
      case 'list_skills':
      case 'list_channels':
      case 'list_pending_escalations':
      case 'automation_list_specs':
      case 'automation_list_activities':
        return []
      case 'get_system_diagnostics':
        return diagnosticsFixture
      case 'run_browser_parity_harness':
      case 'run_memory_gbrain_eval_harness':
      case 'run_agent_control_plane_harness':
        return harnessSuiteFixture
      case 'run_self_improvement_gate_harness':
        return selfImprovementFixture
      case 'restart_memu_bridge':
      case 'restart_gbrain_mcp':
      case 'reset_ai_engine':
        return { ok: true, mocked: true }
      case 'get_safety_policy':
        return { mode: 'yolo', tool_overrides: [] }
      case 'get_default_prompts':
        return { prompts: [] }
      default:
        console.warn(`[uClaw mock Tauri IPC] unhandled command: ${cmd}`)
        return null
    }
  }
}

export function installDevTauriMock(): void {
  if (!shouldInstallDevTauriMock() || window.__UCLAW_DEV_TAURI_MOCK__) return

  clearMocks()
  mockWindows('main')
  mockConvertFileSrc('macos')
  mockIPC(createUclawMockIpcHandler(), { shouldMockEvents: true })
  window.__UCLAW_DEV_TAURI_MOCK__ = true

  console.info('[uClaw mock Tauri IPC] installed for browser-only UI debugging')
}

installDevTauriMock()
