import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { ScrollPositionManager } from './useScrollPositionMemory'
import { renderWithProviders } from '@/test-utils/render'

// We need a controlled ConversationContext.Provider to verify the hook
// calls scrollToBottom. Mock the module so we can intercept the context.

vi.mock('@/components/ai-elements/conversation', async (importOriginal) => {
  const actual: any = await importOriginal()
  // Replace the hook with a controllable spy
  return {
    ...actual,
    useConversationContext: vi.fn(),
  }
})

import { useConversationContext } from '@/components/ai-elements/conversation'

describe('ScrollPositionManager', () => {
  it('does nothing when context is null', () => {
    ;(useConversationContext as any).mockReturnValue(null)
    // Should render without error and produce nothing
    const { container } = renderWithProviders(
      <ScrollPositionManager id="session-1" ready={true} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('does nothing when ready=false', () => {
    const scrollToBottom = vi.fn()
    ;(useConversationContext as any).mockReturnValue({
      scrollRef: React.createRef<HTMLDivElement>(),
      viewportEl: null,
      scrollToBottom,
    })
    renderWithProviders(<ScrollPositionManager id="session-1" ready={false} />)
    // The hook waits for ready before scrolling.
    expect(scrollToBottom).not.toHaveBeenCalled()
  })

  it('calls scrollToBottom when id is set and ready flips true', async () => {
    const scrollToBottom = vi.fn()
    ;(useConversationContext as any).mockReturnValue({
      scrollRef: React.createRef<HTMLDivElement>(),
      viewportEl: null,
      scrollToBottom,
    })
    const { rerender } = renderWithProviders(
      <ScrollPositionManager id="session-1" ready={false} />,
    )
    // Wait one frame — useEffect with requestAnimationFrame
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).not.toHaveBeenCalled()

    // Flip ready to true → scroll fires on next frame
    rerender(<ScrollPositionManager id="session-1" ready={true} />)
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledWith('auto')
  })

  it('calls scrollToBottom again when id changes', async () => {
    const scrollToBottom = vi.fn()
    ;(useConversationContext as any).mockReturnValue({
      scrollRef: React.createRef<HTMLDivElement>(),
      viewportEl: null,
      scrollToBottom,
    })
    const { rerender } = renderWithProviders(
      <ScrollPositionManager id="session-1" ready={true} />,
    )
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledTimes(1)

    // Same id → no extra call
    rerender(<ScrollPositionManager id="session-1" ready={true} />)
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledTimes(1)

    // New id → another call
    rerender(<ScrollPositionManager id="session-2" ready={true} />)
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledTimes(2)
  })
})
