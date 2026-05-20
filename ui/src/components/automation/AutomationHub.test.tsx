import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { ActivityRow, AutomationHub } from './AutomationHub'
import type { AutomationActivity, HumaneSpecRow } from '@/lib/tauri-bridge'

const baseActivity: AutomationActivity = {
  id: 'act-1', specId: 's1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status: 'completed', errorText: null,
  queuedAt: 1, startedAt: 1, completedAt: 2, durationMs: 1000,
  llmIterations: 3, llmTokensIn: 100, llmTokensOut: 50,
  sessionId: 'sess-1', reportArtifactsJson: '[]',
  reportText: 'done', reportOutcome: 'useful',
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
  workingDir: '',
}

function liveSpec(id: string, name: string, roomId: string): HumaneSpecRow {
  return {
    id,
    name,
    version: '0.1.0',
    author: 'uClaw',
    description: 'Live room spec',
    systemPrompt: 'moderate',
    specFormat: 'humane_v1',
    specYaml: '',
    specJson: JSON.stringify({
      type: 'automation',
      x_uclaw_runtime: { kind: 'live_room_moderator' },
      config: { platform: 'douyin', room_id: roomId },
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
}

describe('ActivityRow', () => {
  it('calls onOpen with the session id when a linked row is clicked', () => {
    const onOpen = vi.fn()
    render(<ActivityRow a={baseActivity} onOpen={onOpen} />)
    fireEvent.click(screen.getByText('manual'))
    expect(onOpen).toHaveBeenCalledWith('sess-1')
  })

  it('is not clickable when sessionId is null', () => {
    const onOpen = vi.fn()
    render(<ActivityRow a={{ ...baseActivity, sessionId: null }} onOpen={onOpen} />)
    fireEvent.click(screen.getByText('manual'))
    expect(onOpen).not.toHaveBeenCalled()
  })

  it('opens on Enter key when focused', () => {
    const onOpen = vi.fn()
    const { container } = render(<ActivityRow a={baseActivity} onOpen={onOpen} />)
    const row = container.querySelector('[role="button"]')
    expect(row).not.toBeNull()
    fireEvent.keyDown(row!, { key: 'Enter' })
    expect(onOpen).toHaveBeenCalledWith('sess-1')
  })
})

describe('AutomationHub live room specs', () => {
  it('distinguishes concurrent live room specs by platform and room', () => {
    render(<AutomationHub initialSpecs={[
      liveSpec('a', 'Room A', 'room-a'),
      liveSpec('b', 'Room B', 'room-b'),
    ]} />)
    expect(screen.getAllByText(/room-a/i).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/room-b/i).length).toBeGreaterThan(0)
  })
})
