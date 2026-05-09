import { describe, it, expect, beforeEach } from 'vitest'
import { ChatAppearancePopover } from './ChatAppearancePopover'
import { renderWithProviders, screen } from '@/test-utils/render'
import { chatFontSizeAtom, chatSerifAtom } from '@/atoms/chat-appearance'

describe('ChatAppearancePopover', () => {
  beforeEach(() => {
    // Clear any data-chat-* state left on <html> by previous tests.
    document.documentElement.removeAttribute('data-chat-font-size')
    document.documentElement.removeAttribute('data-chat-serif')
  })

  it('renders a trigger button by default (popover closed)', () => {
    renderWithProviders(<ChatAppearancePopover />)
    // Popover trigger is a button; the content panel is not yet in the DOM.
    expect(screen.getByRole('button')).toBeInTheDocument()
    // Font-size buttons inside the panel are not visible until trigger is clicked.
    expect(screen.queryByText('小')).not.toBeInTheDocument()
  })

  it('opens the popover on trigger click and shows three font-size choices', async () => {
    const { user } = renderWithProviders(<ChatAppearancePopover />)
    await user.click(screen.getByRole('button'))
    // Three size buttons inside the panel
    expect(screen.getByText('小')).toBeInTheDocument()
    expect(screen.getByText('中')).toBeInTheDocument()
    expect(screen.getByText('大')).toBeInTheDocument()
  })

  it('clicking a size button updates the atom and the html data attribute', async () => {
    const { user, store } = renderWithProviders(<ChatAppearancePopover />)
    await user.click(screen.getByRole('button'))   // open popover
    await user.click(screen.getByText('大'))
    expect(store.get(chatFontSizeAtom)).toBe('lg')
    expect(document.documentElement.getAttribute('data-chat-font-size')).toBe('lg')
  })

  it('toggling serif switch updates the atom and the html data attribute', async () => {
    const { user, store } = renderWithProviders(<ChatAppearancePopover />)
    await user.click(screen.getByRole('button'))   // open popover
    // The serif Switch is a Radix component — find it by accessible name.
    const switchEl = screen.getByRole('switch')
    expect(store.get(chatSerifAtom)).toBe(false)
    await user.click(switchEl)
    expect(store.get(chatSerifAtom)).toBe(true)
    expect(document.documentElement.getAttribute('data-chat-serif')).toBe('true')
  })
})
