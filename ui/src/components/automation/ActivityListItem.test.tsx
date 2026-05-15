import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react'
import { ActivityListItem } from './ActivityListItem'
import type { AutomationActivity } from '@/lib/tauri-bridge'

const { openFileMock, openExternalMock } = vi.hoisted(() => ({
  openFileMock: vi.fn().mockResolvedValue(undefined),
  openExternalMock: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@/lib/tauri-bridge', () => ({
  toggleArchiveAgentSession: vi.fn().mockResolvedValue(undefined),
  openFile: openFileMock,
  openExternal: openExternalMock,
}))

// react-markdown renders async in jsdom; wrap in act where needed
const baseActivity: AutomationActivity = {
  id: 'act-1', specId: 'spec-1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status: 'completed', errorText: null,
  queuedAt: 1_700_000_000_000, startedAt: 1_700_000_000_000,
  completedAt: 1_700_000_042_000,
  durationMs: 42_000, llmIterations: 1, llmTokensIn: 100, llmTokensOut: 50,
  sessionId: 'sess-1', reportArtifactsJson: '[]',
  reportText: null, reportOutcome: null,
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
  workingDir: '/test/workdir',
}

beforeEach(() => {
  openFileMock.mockClear()
  openExternalMock.mockClear()
})

describe('ActivityListItem', () => {
  it('renders the testid and status label', () => {
    render(<ActivityListItem activity={baseActivity} />)
    expect(screen.getByTestId('activity-row-act-1')).toBeTruthy()
    expect(screen.getByText('已完成')).toBeTruthy()
  })

  it('shows outcome badge only when reportOutcome is set', async () => {
    const { rerender } = render(<ActivityListItem activity={baseActivity} />)
    expect(screen.queryByText('有效')).toBeNull()

    rerender(<ActivityListItem activity={{ ...baseActivity, reportOutcome: 'useful' }} />)
    expect(screen.getByText('有效')).toBeTruthy()
  })

  it('maps all outcome values to correct labels', () => {
    const cases: [string, string][] = [
      ['useful', '有效'], ['noop', '无操作'], ['skipped', '跳过'], ['error', '错误'],
    ]
    for (const [outcome, label] of cases) {
      const { unmount } = render(
        <ActivityListItem activity={{ ...baseActivity, reportOutcome: outcome }} />
      )
      expect(screen.getByText(label)).toBeTruthy()
      unmount()
    }
  })

  it('shows running placeholder when status is running and no reportText', () => {
    render(
      <ActivityListItem
        activity={{ ...baseActivity, status: 'running', reportText: null }}
      />
    )
    expect(screen.getByText(/运行中，暂无报告/)).toBeTruthy()
  })

  it('does not show body when status is completed and reportText is null', () => {
    render(<ActivityListItem activity={baseActivity} />)
    expect(screen.queryByText(/运行中，暂无报告/)).toBeNull()
  })

  it('renders reportText via ActivityMarkdown', async () => {
    await act(async () => {
      render(
        <ActivityListItem activity={{ ...baseActivity, reportText: '**bold result**' }} />
      )
    })
    const el = document.querySelector('strong')
    expect(el).toBeTruthy()
  })

  it('renders file artifact chip and calls openFile on click', async () => {
    const artifacts = JSON.stringify([{ kind: 'file', path: 'report.md', title: 'Report' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('Report', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openFileMock).toHaveBeenCalledWith('/test/workdir/report.md')
  })

  it('renders url artifact chip and calls openExternal on click', async () => {
    const artifacts = JSON.stringify([{ kind: 'url', path: 'https://example.com', title: 'Results' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('Results', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openExternalMock).toHaveBeenCalledWith('https://example.com')
  })

  it('renders url artifact chip using title as fallback URL when path is absent', async () => {
    const artifacts = JSON.stringify([{ kind: 'url', title: 'https://fallback.com' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('https://fallback.com', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openExternalMock).toHaveBeenCalledWith('https://fallback.com')
  })

  it('renders text artifact chip as non-clickable (no openFile or openExternal)', async () => {
    const artifacts = JSON.stringify([{ kind: 'text', title: 'Summary' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('Summary', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openFileMock).not.toHaveBeenCalled()
    expect(openExternalMock).not.toHaveBeenCalled()
  })

  it('calls onArchived after archiving', async () => {
    const onArchived = vi.fn()
    render(<ActivityListItem activity={baseActivity} onArchived={onArchived} />)
    const btn = screen.getByLabelText('归档')
    await act(async () => { fireEvent.click(btn) })
    await waitFor(() => expect(onArchived).toHaveBeenCalledWith('sess-1'))
  })

  it('calls onOpenRunSession when 查看进程 is clicked', () => {
    const onOpen = vi.fn()
    render(<ActivityListItem activity={baseActivity} onOpenRunSession={onOpen} />)
    fireEvent.click(screen.getByText(/查看进程/))
    expect(onOpen).toHaveBeenCalledWith('sess-1')
  })

  it('escalation ring applied when status is waiting_user', () => {
    const { container } = render(
      <ActivityListItem activity={{ ...baseActivity, status: 'waiting_user' }} />
    )
    expect(container.firstElementChild?.className).toContain('ring-warning')
  })
})
