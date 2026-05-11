import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, fireEvent } from '@/test-utils/render'
import { SessionItem } from './SessionItem'

describe('SessionItem — pin menu label', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
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

  it('hides menu trigger entirely when no actions provided', () => {
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
    expect(screen.queryByTitle('更多')).not.toBeInTheDocument()
  })
})
