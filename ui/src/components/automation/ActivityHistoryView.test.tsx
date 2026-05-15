import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ActivityHistoryView } from './ActivityHistoryView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  toggleArchiveAgentSession: vi.fn().mockResolvedValue(undefined),
  openFile: vi.fn(),
  openExternal: vi.fn(),
}))
vi.mock('./RunSessionSubView', () => ({
  RunSessionSubView: () => <div data-testid="run-session" />,
}))

const makeAct = (id: string, status = 'completed'): AutomationActivity => ({
  id, specId: 'spec-1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status, errorText: null,
  queuedAt: 1_700_000_000_000, startedAt: null, completedAt: null,
  durationMs: 0, llmIterations: 0, llmTokensIn: 0, llmTokensOut: 0,
  sessionId: `sess-${id}`, reportArtifactsJson: '[]',
  reportText: null, reportOutcome: null,
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
  workingDir: '/workdir',
})

describe('ActivityHistoryView', () => {
  it('renders each activity as a timeline row', () => {
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={[makeAct('a1'), makeAct('a2')]}
      />
    )
    expect(screen.getByTestId('activity-row-a1')).toBeTruthy()
    expect(screen.getByTestId('activity-row-a2')).toBeTruthy()
  })

  it('shows empty-state when activities array is empty', () => {
    render(<ActivityHistoryView specId="spec-1" activities={[]} />)
    expect(screen.getByText(/还没有运行记录/)).toBeTruthy()
  })

  it('shows RunSessionSubView when activeRunSessionId matches a session', () => {
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={[makeAct('a1')]}
        activeRunSessionId="sess-a1"
        onCloseRunSession={() => {}}
      />
    )
    expect(screen.getByTestId('run-session')).toBeTruthy()
  })
})
