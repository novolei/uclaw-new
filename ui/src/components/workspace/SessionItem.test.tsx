import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, fireEvent } from '@/test-utils/render'
import { createStore } from 'jotai'
import { SessionItem } from './SessionItem'
import { dockOrderAtom } from '@/atoms/dock-atoms'

describe('SessionItem — pin menu label', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    localStorage.clear()
    vi.clearAllMocks()
  })

  it('shows "固定" when not pinned', async () => {
    const { user } = renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        isPinned={false}
        onClick={() => {}}
        onTogglePin={() => {}}
      />
    )
    await user.click(screen.getByTitle('更多'))
    expect(await screen.findByText('固定')).toBeInTheDocument()
    expect(screen.queryByText('取消固定')).not.toBeInTheDocument()
  })

  it('shows "取消固定" when pinned', async () => {
    const { user } = renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        isPinned
        onClick={() => {}}
        onTogglePin={() => {}}
      />
    )
    await user.click(screen.getByTitle('更多'))
    expect(await screen.findByText('取消固定')).toBeInTheDocument()
    expect(screen.queryByText('固定')).not.toBeInTheDocument()
  })

  it('clicking the menu item invokes onTogglePin', async () => {
    const onTogglePin = vi.fn()
    const { user } = renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        isPinned={false}
        onClick={() => {}}
        onTogglePin={onTogglePin}
      />
    )
    await user.click(screen.getByTitle('更多'))
    await user.click(await screen.findByText('固定'))
    expect(onTogglePin).toHaveBeenCalledTimes(1)
  })

  it('always renders the 3-dot trigger because dock-pin is always available', () => {
    renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        onClick={() => {}}
      />
    )
    expect(screen.getByTitle('更多')).toBeInTheDocument()
  })

  it('shows "固定到 Dock" when session is not in dock', async () => {
    const { user } = renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        onClick={() => {}}
      />
    )
    await user.click(screen.getByTitle('更多'))
    expect(await screen.findByText('固定到 Dock')).toBeInTheDocument()
    expect(screen.queryByText('从 Dock 移除')).not.toBeInTheDocument()
  })

  it('shows "从 Dock 移除" when session is already in dock', async () => {
    const store = createStore()
    store.set(dockOrderAtom, [
      { kind: 'pinned-conversation', sessionId: 's1', type: 'agent' },
    ])
    const { user } = renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        onClick={() => {}}
      />,
      { store },
    )
    await user.click(screen.getByTitle('更多'))
    expect(await screen.findByText('从 Dock 移除')).toBeInTheDocument()
    expect(screen.queryByText('固定到 Dock')).not.toBeInTheDocument()
  })

  it('clicking "固定到 Dock" appends a pinned-conversation spec to dockOrder', async () => {
    const store = createStore()
    store.set(dockOrderAtom, [])
    const { user } = renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        onClick={() => {}}
      />,
      { store },
    )
    await user.click(screen.getByTitle('更多'))
    await user.click(await screen.findByText('固定到 Dock'))
    expect(store.get(dockOrderAtom)).toContainEqual({
      kind: 'pinned-conversation',
      sessionId: 's1',
      type: 'agent',
    })
  })

  it('clicking "从 Dock 移除" removes the pinned-conversation entry', async () => {
    const store = createStore()
    store.set(dockOrderAtom, [
      { kind: 'pinned-conversation', sessionId: 's1', type: 'agent' },
    ])
    const { user } = renderWithProviders(
      <SessionItem
        id="s1"
        title="Hi"
        titleEmoji="💬"
        titlePending={false}
        isActive={false}
        onClick={() => {}}
      />,
      { store },
    )
    await user.click(screen.getByTitle('更多'))
    await user.click(await screen.findByText('从 Dock 移除'))
    expect(store.get(dockOrderAtom)).toEqual([])
  })
})
