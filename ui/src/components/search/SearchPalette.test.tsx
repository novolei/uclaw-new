import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { SearchPalette } from './SearchPalette'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'

// SearchPalette passes shouldFilter={false} to cmdk's <Command> at the source
// because the backend already does FTS filtering — cmdk's built-in fuzzy filter
// operates on item `value` (ids) which would hide server-returned hits whose
// content matched but whose id didn't. So no cmdk mock is needed here.

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
