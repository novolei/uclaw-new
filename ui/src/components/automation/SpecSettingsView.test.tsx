import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { SpecSettingsView } from './SpecSettingsView'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'
import {
  setAutomationPermission,
  updateAutomationUserConfig,
} from '@/lib/tauri-bridge'
import { openAutomationLoginWindow } from '@/lib/automation-login-window'

vi.mock('@/lib/tauri-bridge', () => ({
  setAutomationEnabled: vi.fn().mockResolvedValue(undefined),
  setAutomationPermission: vi.fn().mockResolvedValue(undefined),
  updateAutomationUserConfig: vi.fn().mockResolvedValue(undefined),
  listSpecChannelBindings: vi.fn(() => new Promise(() => {})),
  updateSpecChannelBindings: vi.fn().mockResolvedValue(undefined),
  updateSpecImSettings: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@/lib/automation-login-window', () => ({
  openAutomationLoginWindow: vi.fn().mockResolvedValue(undefined),
}))

function liveSpec(overrides: Partial<HumaneSpecRow> = {}): HumaneSpecRow {
  return {
    id: 'spec-1',
    name: 'Douyin Live Moderator',
    version: '0.1.0',
    author: 'uClaw',
    description: 'Live room spec',
    systemPrompt: '',
    specFormat: 'humane_v1',
    specYaml: 'type: automation',
    specJson: JSON.stringify({
      type: 'automation',
      permissions: ['ai_browser', 'notification'],
      browser_login: [{ url: 'https://www.douyin.com/', label: 'Douyin' }],
      x_uclaw_runtime: {
        kind: 'live_room_moderator',
        poll_interval_seconds: 30,
        action_mode_default: 'real',
      },
      config: {
        platform: 'douyin',
        room_id: '',
        live_url: '',
        action_mode: 'real',
      },
    }),
    userConfigValues: JSON.stringify({
      room_id: 'room-a',
      live_url: 'https://live.douyin.com/room-a',
      poll_interval_seconds: 15,
    }),
    permissionsGranted: '["ai_browser"]',
    permissionsDenied: '["shell"]',
    status: 'active',
    enabled: true,
    spaceId: null,
    source: 'builtin',
    sourceRef: null,
    sourceVersion: null,
    createdAt: 1,
    updatedAt: 1,
    lastRunAt: null,
    lastRunOutcome: null,
    triggerPhrase: '',
    systemPromptOverride: '',
    ...overrides,
  }
}

describe('SpecSettingsView Halo-compatible controls', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders real automation permission ids and toggles grant/deny', async () => {
    const onSpecChange = vi.fn()
    render(<SpecSettingsView spec={liveSpec()} onSpecChange={onSpecChange} />)

    expect(screen.getByText('ai_browser')).toBeInTheDocument()
    expect(screen.getByText('shell')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /拒绝 notification/ }))

    await waitFor(() => {
      expect(setAutomationPermission).toHaveBeenCalledWith('spec-1', 'notification', false)
      expect(onSpecChange).toHaveBeenCalledWith(expect.objectContaining({
        permissionsDenied: expect.stringContaining('notification'),
      }))
    })
  })

  it('saves live-room user config overrides without editing the spec yaml', async () => {
    const onSpecChange = vi.fn()
    render(<SpecSettingsView spec={liveSpec()} onSpecChange={onSpecChange} />)

    fireEvent.change(screen.getByLabelText('room_id'), { target: { value: 'room-b' } })
    fireEvent.change(screen.getByLabelText('poll_interval_seconds'), { target: { value: '20' } })
    fireEvent.click(screen.getByRole('button', { name: /保存直播间配置/ }))

    await waitFor(() => {
      expect(updateAutomationUserConfig).toHaveBeenCalledWith('spec-1', expect.objectContaining({
        room_id: 'room-b',
        poll_interval_seconds: 20,
      }))
      expect(onSpecChange).toHaveBeenCalledWith(expect.objectContaining({
        userConfigValues: expect.stringContaining('room-b'),
        specYaml: 'type: automation',
      }))
    })
  })

  it('shows browser login requirement without credential fields', () => {
    render(<SpecSettingsView spec={liveSpec()} onSpecChange={() => {}} />)

    expect(screen.getByText('Douyin')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /AI Browser.*登录/ })).toBeInTheDocument()
    expect(screen.queryByLabelText(/password|密码/i)).not.toBeInTheDocument()
  })

  it('opens browser login in a dedicated Halo-style window', async () => {
    render(<SpecSettingsView spec={liveSpec()} onSpecChange={() => {}} />)

    fireEvent.click(screen.getByRole('button', { name: /AI Browser.*登录/ }))

    await waitFor(() => {
      expect(openAutomationLoginWindow).toHaveBeenCalledWith({
        specId: 'spec-1',
        label: 'Douyin',
        url: 'https://www.douyin.com/',
      })
    })
  })
})
