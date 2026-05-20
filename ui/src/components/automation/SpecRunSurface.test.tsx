import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { SpecRunSurface } from './SpecRunSurface'
import { automationActivitiesAtom, humaneSpecsAtom } from '@/atoms/automation'
import type { AutomationActivity, HumaneSpecRow } from '@/lib/tauri-bridge'
import {
  getAutomationActivity,
  stopAutomationRuns,
} from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  triggerAutomationManualHumane: vi.fn().mockResolvedValue(undefined),
  getAutomationActivity: vi.fn().mockResolvedValue([]),
  stopAutomationRuns: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('./HomeThreadView', () => ({ HomeThreadView: () => <div data-testid="home-thread" /> }))
vi.mock('./ActivityHistoryView', () => ({ ActivityHistoryView: () => <div data-testid="activity-history" /> }))
vi.mock('./ChatThreadsTab', () => ({ ChatThreadsTab: () => <div data-testid="chat-threads" /> }))
vi.mock('./SpecSettingsView', () => ({ SpecSettingsView: () => <div data-testid="spec-settings" /> }))
vi.mock('./AutomationRightPanel', () => ({ AutomationRightPanel: () => <div data-testid="right-panel" /> }))

const spec: HumaneSpecRow = {
  id: 'spec-1',
  name: 'Douyin Live Moderator',
  version: '0.1.0',
  author: 'uClaw',
  description: 'Live room spec',
  systemPrompt: '',
  specFormat: 'humane_v1',
  specYaml: '',
  specJson: JSON.stringify({
    type: 'automation',
    x_uclaw_runtime: { kind: 'live_room_moderator' },
    config: { platform: 'douyin', room_id: 'room-1' },
  }),
  userConfigValues: '{}',
  permissionsGranted: '[]',
  permissionsDenied: '[]',
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
}

const runningActivity: AutomationActivity = {
  id: 'act-1',
  specId: 'spec-1',
  subscriptionId: null,
  triggerSourceType: 'manual',
  triggerPayloadJson: '{}',
  status: 'running',
  errorText: null,
  queuedAt: 1,
  startedAt: 1,
  completedAt: null,
  durationMs: 0,
  llmIterations: 0,
  llmTokensIn: 0,
  llmTokensOut: 0,
  sessionId: 'sess-1',
  reportArtifactsJson: '[]',
  reportText: null,
  reportOutcome: null,
  escalationId: null,
  resumedFromActivityId: null,
  resumedFromEscalationId: null,
  workingDir: '',
}

describe('SpecRunSurface stop control', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('stops active automation runs and refreshes activity', async () => {
    const store = createStore()
    store.set(humaneSpecsAtom, [spec])
    store.set(automationActivitiesAtom, { 'spec-1': [runningActivity] })
    vi.mocked(getAutomationActivity).mockResolvedValue([{ ...runningActivity, status: 'cancelled' }])

    render(
      <Provider store={store}>
        <SpecRunSurface specId="spec-1" />
      </Provider>
    )

    fireEvent.click(screen.getByRole('button', { name: /停止/ }))

    await waitFor(() => {
      expect(stopAutomationRuns).toHaveBeenCalledWith('spec-1')
      expect(getAutomationActivity).toHaveBeenCalledWith('spec-1', 50)
    })
  })
})
