import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ActivityHistoryView } from './ActivityHistoryView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

// Mock tauri-bridge so toggleArchiveAgentSession resolves immediately.
vi.mock('@/lib/tauri-bridge', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/tauri-bridge')>()
  return {
    ...actual,
    toggleArchiveAgentSession: vi.fn().mockResolvedValue(1_700_000_000_000),
  }
})

function makeActivity(overrides: Partial<AutomationActivity> = {}): AutomationActivity {
  return {
    id: 'act-1',
    specId: 'spec-1',
    subscriptionId: null,
    triggerSourceType: 'manual',
    triggerPayloadJson: '{}',
    status: 'completed',
    errorText: null,
    queuedAt: Date.now(),
    startedAt: Date.now(),
    completedAt: Date.now(),
    durationMs: 1000,
    llmIterations: 1,
    llmTokensIn: 100,
    llmTokensOut: 50,
    sessionId: 'sess-1',
    reportArtifactsJson: '[]',
    reportText: 'done',
    reportOutcome: 'success',
    escalationId: null,
    resumedFromActivityId: null,
    resumedFromEscalationId: null,
    ...overrides,
  }
}

describe('ActivityHistoryView', () => {
  const activities = [makeActivity()]

  it('shows archive button on hover and hides item after archive (default: show-archived off)', async () => {
    const user = userEvent.setup()
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={activities}
        onOpenRunSession={vi.fn()}
        activeRunSessionId={null}
        onCloseRunSession={vi.fn()}
      />
    )

    // Item visible initially.
    expect(screen.getByTestId('activity-row-act-1')).toBeInTheDocument()

    // Hover to reveal archive button.
    await user.hover(screen.getByTestId('activity-row-act-1'))
    const archiveBtn = await screen.findByRole('button', { name: /归档/i })
    expect(archiveBtn).toBeInTheDocument()

    // Click archive.
    await user.click(archiveBtn)

    // Item disappears (filtered because show-archived is off by default).
    await waitFor(() => {
      expect(screen.queryByTestId('activity-row-act-1')).not.toBeInTheDocument()
    })
  })

  it('shows archived items when show-archived toggle is on', async () => {
    const user = userEvent.setup()
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={activities}
        onOpenRunSession={vi.fn()}
        activeRunSessionId={null}
        onCloseRunSession={vi.fn()}
      />
    )

    // Archive the item.
    await user.hover(screen.getByTestId('activity-row-act-1'))
    await user.click(await screen.findByRole('button', { name: /归档/i }))

    // Toggle "show archived".
    const toggle = screen.getByRole('button', { name: /显示已归档/i })
    await user.click(toggle)

    // Item reappears.
    expect(await screen.findByTestId('activity-row-act-1')).toBeInTheDocument()
  })

  it('renders escalation ring with theme tokens', () => {
    const escalation = makeActivity({ status: 'waiting_user' })
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={[escalation]}
        onOpenRunSession={vi.fn()}
        activeRunSessionId={null}
        onCloseRunSession={vi.fn()}
      />
    )
    const row = screen.getByTestId('activity-row-act-1')
    expect(row.className).toMatch(/border-warning|ring-warning/)
  })
})
