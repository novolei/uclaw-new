import { describe, it, expect, vi } from 'vitest'
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderWithProviders } from '@/test-utils/render'
import { ActivityHistoryView } from './ActivityHistoryView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

const makeActivity = (overrides: Partial<AutomationActivity> = {}): AutomationActivity => ({
  id: 'act-1',
  specId: 'spec-1',
  subscriptionId: null,
  triggerSourceType: 'schedule',
  triggerPayloadJson: '{}',
  status: 'completed',
  errorText: null,
  queuedAt: 1715741400000,
  startedAt: 1715741400000,
  completedAt: 1715741412400,
  durationMs: 12400,
  llmIterations: 3,
  llmTokensIn: 0,
  llmTokensOut: 0,
  sessionId: 'session-run-1',
  reportArtifactsJson: '[]',
  reportText: '3 指标正常，1 待确认',
  reportOutcome: 'completed',
  escalationId: null,
  resumedFromActivityId: null,
  resumedFromEscalationId: null,
  ...overrides,
})

describe('ActivityHistoryView', () => {
  it('renders activity report text', () => {
    renderWithProviders(
      <ActivityHistoryView specId="spec-1" activities={[makeActivity()]} />
    )
    expect(screen.getByText('3 指标正常，1 待确认')).toBeInTheDocument()
  })

  it('renders 查看进程 button for activity with sessionId', () => {
    renderWithProviders(
      <ActivityHistoryView specId="spec-1" activities={[makeActivity({ sessionId: 'run-sess' })]} />
    )
    expect(screen.getByRole('button', { name: /查看进程/i })).toBeInTheDocument()
  })

  it('calls onOpenRunSession when 查看进程 is clicked', async () => {
    const onOpen = vi.fn()
    renderWithProviders(
      <ActivityHistoryView
        specId="spec-1"
        activities={[makeActivity({ sessionId: 'run-sess' })]}
        onOpenRunSession={onOpen}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /查看进程/i }))
    expect(onOpen).toHaveBeenCalledWith('run-sess')
  })

  it('shows empty state when no activities', () => {
    renderWithProviders(<ActivityHistoryView specId="spec-1" activities={[]} />)
    expect(screen.getByText(/还没有运行记录/i)).toBeInTheDocument()
  })

  it('highlights escalation status', () => {
    const act = makeActivity({ status: 'waiting_user', escalationId: 'esc-1' })
    renderWithProviders(<ActivityHistoryView specId="spec-1" activities={[act]} />)
    const row = screen.getByTestId('activity-row-act-1')
    expect(row.className).toMatch(/border-orange|border-amber|ring-orange/)
  })
})
