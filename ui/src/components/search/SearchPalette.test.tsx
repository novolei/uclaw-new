import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { SearchPalette } from './SearchPalette'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'

// Mock cmdk to disable its built-in fuzzy filtering so items we render are always visible
// in jsdom tests regardless of whether the search term matches the item value prop.
// The SearchPalette does server-side search via debounce, so cmdk's client-side filter
// would hide our fixture results (e.g. "Game session" doesn't match the query "gomoku").
vi.mock('cmdk', async (importOriginal) => {
  const original = await importOriginal<typeof import('cmdk')>()
  const OriginalCommand = original.Command
  // Wrap the root Command with shouldFilter=false
  const PatchedCommand = React.forwardRef<
    React.ElementRef<typeof OriginalCommand>,
    React.ComponentPropsWithoutRef<typeof OriginalCommand>
  >((props, ref) => {
    return React.createElement(OriginalCommand, { ...props, shouldFilter: false, ref })
  })
  PatchedCommand.displayName = 'Command'
  // Copy over sub-components
  Object.assign(PatchedCommand, {
    Input: OriginalCommand.Input,
    List: OriginalCommand.List,
    Item: OriginalCommand.Item,
    Empty: OriginalCommand.Empty,
    Group: OriginalCommand.Group,
    Separator: OriginalCommand.Separator,
    Dialog: OriginalCommand.Dialog,
    Loading: OriginalCommand.Loading,
  })
  return { ...original, Command: PatchedCommand }
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args?: any) => {
    if (cmd === 'search_conversations') {
      const q: string = args?.input?.query ?? ''
      if (q === 'gomoku') {
        return [
          {
            id: 'chat:msg-1',
            title: 'Game session',
            snippet: '... <b>gomoku</b> rules ...',
            source: 'chat_message',
            sourceId: 'sess-1',
            messageId: 'msg-1',
            createdAt: '2026-05-09',
          },
        ]
      }
      return []
    }
    return []
  }),
}))

describe('SearchPalette', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders nothing when closed', () => {
    const { container } = renderWithProviders(<SearchPalette />)
    expect(container.querySelector('input')).toBeNull()
  })

  it('opens when the atom is set true', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    expect(await screen.findByPlaceholderText(/Search conversations/i)).toBeInTheDocument()
  })

  it('opens via ⌘K keyboard shortcut', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
    // simulate Cmd+K
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }),
    )
    await waitFor(() => expect(store.get(searchPaletteOpenAtom)).toBe(true))
  })

  it('queries the backend and renders results', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText(/Search conversations/i)
    await user.type(input, 'gomoku')
    // Wait for debounce + result render
    await waitFor(() => {
      expect(screen.getByText('Game session')).toBeInTheDocument()
    }, { timeout: 1000 })
  })

  it('calls onSelect when a result is clicked', async () => {
    const onSelect = vi.fn()
    const { store, user } = renderWithProviders(<SearchPalette onSelect={onSelect} />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText(/Search conversations/i)
    await user.type(input, 'gomoku')
    const item = await screen.findByText('Game session')
    await user.click(item)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({
      source: 'chat_message',
      messageId: 'msg-1',
      sourceId: 'sess-1',
    }))
    // Palette should close after selection
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
  })

  it('shows "No results" when the backend returns empty', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText(/Search conversations/i)
    await user.type(input, 'no_match_query')
    await waitFor(() => {
      expect(screen.getByText('No results')).toBeInTheDocument()
    }, { timeout: 1000 })
  })
})
