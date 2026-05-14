import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { ActivityRow } from './AutomationHub'
import type { AutomationActivity } from '@/lib/tauri-bridge'

const baseActivity: AutomationActivity = {
  id: 'act-1', specId: 's1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status: 'completed', errorText: null,
  queuedAt: 1, startedAt: 1, completedAt: 2, durationMs: 1000,
  llmIterations: 3, llmTokensIn: 100, llmTokensOut: 50,
  sessionId: 'sess-1', reportArtifactsJson: '[]',
  reportText: 'done', reportOutcome: 'useful',
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
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
})
