import { describe, it, expect } from 'vitest'
import { ChatToolActivityIndicator } from './ChatToolActivityIndicator'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { ChatToolActivity } from '@/lib/chat-types'

describe('ChatToolActivityIndicator', () => {
  it('renders nothing for an empty array', () => {
    const { container } = renderWithProviders(
      <ChatToolActivityIndicator activities={[]} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders one row per merged toolCallId, not per event', () => {
    // Two tool calls each with start + result → 4 events, 2 visible rows.
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: { command: 'ls' } },
      { toolCallId: 'tc-2', type: 'start', toolName: 'bash', input: { command: 'pwd' } },
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: { command: 'ls' }, result: 'a b c', isError: false },
      { toolCallId: 'tc-2', type: 'result', toolName: 'bash', input: { command: 'pwd' }, result: '/home', isError: false },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    // Each ChatToolBlock is a button — count rows.
    expect(screen.getAllByRole('button')).toHaveLength(2)
  })

  it('result event marks the row done (becomes expandable)', () => {
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: {} },
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: {}, result: 'output', isError: false },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    expect(screen.getByRole('button')).not.toBeDisabled()
  })

  it('start-only event yields a still-running row (button disabled)', () => {
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: {} },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    expect(screen.getByRole('button')).toBeDisabled()
  })

  it('out-of-order result-before-start still merges by id', () => {
    // Defensive: if for some reason result arrives first (e.g. dropped start),
    // the row should still render and be done.
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: { command: 'foo' }, result: 'ok', isError: false },
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: { command: 'foo' } },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    // Single row, and the most recent event was 'start' which sets done: false
    expect(screen.getAllByRole('button')).toHaveLength(1)
    expect(screen.getByRole('button')).toBeDisabled()
  })

  it('isError flag from result event propagates to the row tint', () => {
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: {} },
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: {}, result: 'err', isError: true },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    const button = screen.getByRole('button')
    expect(button.className).toMatch(/bg-destructive/)
  })
})
