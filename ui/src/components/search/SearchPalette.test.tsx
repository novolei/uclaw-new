import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { TooltipProvider } from '@/components/ui/tooltip'
import { render, fireEvent } from '@testing-library/react'
import { SearchPalette } from './SearchPalette'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { searchPaletteOpenAtom, searchPaletteScopeAtom } from '@/atoms/search-atoms'
import { appModeAtom } from '@/atoms/app-mode'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { workspacesAtom, activeWorkspaceIdAtom, type WorkspaceInfo } from '@/atoms/workspace'
import { invoke } from '@tauri-apps/api/core'

// cmdk uses scrollIntoView for keyboard nav; jsdom doesn't implement it.
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = vi.fn()
}

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

vi.mock('@/lib/tauri-bridge', () => ({
  listRecentThreads: vi.fn(async () => [
    {
      id: 'sess-1',
      kind: 'chat',
      title: '记住我最喜欢的fps',
      titleEmoji: '🎨',
      workspaceName: 'Workaround',
      workspaceId: 'ws-1',
      messageCount: 4,
      updatedAt: new Date(Date.now() - 42 * 60_000).toISOString(),
    },
    {
      id: 'sess-2',
      kind: 'agent',
      title: '新对话',
      workspaceName: 'Downloads',
      workspaceId: 'ws-2',
      messageCount: 2,
      updatedAt: new Date(Date.now() - 6 * 86_400_000).toISOString(),
    },
  ]),
  listSpaces: vi.fn(async () => [
    { id: 'ws-1', name: 'Workaround', icon: '📁', conversationCount: 6 },
    { id: 'ws-2', name: 'Downloads', conversationCount: 1 },
    { id: 'ws-3', name: 'me', icon: '👤', conversationCount: 3 },
  ]),
  searchFragments: vi.fn(async () => []),
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
    expect(await screen.findByPlaceholderText('搜索线程、项目...')).toBeInTheDocument()
  })

  it('opens via ⌘K keyboard shortcut', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }),
    )
    await waitFor(() => expect(store.get(searchPaletteOpenAtom)).toBe(true))
  })

  // ===== BROWSE MODE (empty input) =====

  it('shows the three browse sections when input is empty', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('最近线程')
    expect(screen.getByText('最近线程')).toBeInTheDocument()
    expect(screen.getByText('设置与命令')).toBeInTheDocument()
    expect(screen.getByText('项目')).toBeInTheDocument()
  })

  it('renders recent threads with workspace badge + relative time', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('记住我最喜欢的fps')
    const rows = screen.getAllByRole('option')
    expect(rows.length).toBeGreaterThan(0)
    expect(screen.getAllByText('Workaround').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText(/分钟前|刚刚/)).toBeInTheDocument()
  })

  it('renders settings shortcuts with hint text', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('设置与命令')
    expect(screen.getByText('服务商配置')).toBeInTheDocument()
    expect(screen.getByText('Provider / API Key / Base URL')).toBeInTheDocument()
    expect(screen.getByText('浏览器运行时')).toBeInTheDocument()
    expect(screen.getByText('Runtime pack / Startup Doctor / repair')).toBeInTheDocument()
  })

  it('renders workspaces with thread count pill', async () => {
    const { store } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('项目')
    expect(screen.getAllByText(/Workaround/).length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText(/6 个线程/)).toBeInTheDocument()
  })

  // ===== TYPING MODE =====

  it('client-filters recent threads when typing', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    await screen.findByText('最近线程')
    const input = screen.getByPlaceholderText('搜索线程、项目...')
    await user.type(input, 'fps')
    await waitFor(() => {
      expect(screen.getByText('记住我最喜欢的fps')).toBeInTheDocument()
      expect(screen.queryByText('新对话')).not.toBeInTheDocument()
    })
  })

  it('queries the FTS backend and renders the search-hits section', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText('搜索线程、项目...')
    await user.type(input, 'gomoku')
    await waitFor(
      () => {
        // Hits with no workspaceId render under the fallback "默认工作区" group
        expect(screen.getByText(/默认工作区 · 1/)).toBeInTheDocument()
        expect(screen.getByText('Game session')).toBeInTheDocument()
      },
      { timeout: 1000 },
    )
  })

  it('calls onSelect with thread payload when a recent thread is clicked', async () => {
    const onSelect = vi.fn()
    const { store, user } = renderWithProviders(<SearchPalette onSelect={onSelect} />)
    store.set(searchPaletteOpenAtom, true)
    const item = await screen.findByText('记住我最喜欢的fps')
    await user.click(item)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'thread',
      thread: expect.objectContaining({ id: 'sess-1', kind: 'chat' }),
    }))
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
  })

  it('calls onSelect with search_hit payload when an FTS hit is clicked', async () => {
    const onSelect = vi.fn()
    const { store, user } = renderWithProviders(<SearchPalette onSelect={onSelect} />)
    store.set(searchPaletteOpenAtom, true)
    const input = await screen.findByPlaceholderText('搜索线程、项目...')
    await user.type(input, 'gomoku')
    const hit = await screen.findByText('Game session')
    await user.click(hit)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'search_hit',
      hit: expect.objectContaining({ messageId: 'msg-1', sourceId: 'sess-1' }),
    }))
  })

  it('calls onSelect with browser runtime settings deep-link payload', async () => {
    const onSelect = vi.fn()
    const { store, user } = renderWithProviders(<SearchPalette onSelect={onSelect} />)
    store.set(searchPaletteOpenAtom, true)
    const item = await screen.findByText('浏览器运行时')
    await user.click(item)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'settings',
      settings: expect.objectContaining({
        id: 'settings:browser-runtime',
        settingsTab: 'browserRuntime',
      }),
    }))
    expect(store.get(searchPaletteOpenAtom)).toBe(false)
  })

  // ===== SCOPE BEHAVIOR =====

  it('Tab toggles scope chip when an active session exists', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(appModeAtom, 'chat')
    store.set(currentConversationIdAtom, 'conv-1')
    store.set(searchPaletteOpenAtom, true)
    await screen.findByPlaceholderText('搜索线程、项目...')
    expect(store.get(searchPaletteScopeAtom)).toBe('all')
    await user.keyboard('{Tab}')
    await waitFor(() => {
      const s = store.get(searchPaletteScopeAtom)
      expect(s).not.toBe('all')
      if (s !== 'all') expect(s.id).toBe('conv-1')
    })
  })

  it('first Esc clears scope, second Esc closes the palette', async () => {
    const { store, user } = renderWithProviders(<SearchPalette />)
    store.set(appModeAtom, 'chat')
    store.set(currentConversationIdAtom, 'conv-1')
    store.set(searchPaletteOpenAtom, true)
    await screen.findByPlaceholderText('搜索线程、项目...')
    store.set(searchPaletteScopeAtom, { kind: 'session', id: 'conv-1', label: '当前聊天' })
    await user.keyboard('{Escape}')
    await waitFor(() => expect(store.get(searchPaletteScopeAtom)).toBe('all'))
    expect(store.get(searchPaletteOpenAtom)).toBe(true)
    await user.keyboard('{Escape}')
    await waitFor(() => expect(store.get(searchPaletteOpenAtom)).toBe(false))
  })
})

// ===== Cross-workspace grouped render =====

function ws(id: string, name: string): WorkspaceInfo {
  return {
    id, name, icon: 'Folder', path: `/${id}`, attachedDirs: [],
    sortOrder: 0, createdAt: '', updatedAt: '',
  }
}

function renderWithStore(store: ReturnType<typeof createStore>) {
  return render(
    <Provider store={store}>
      <TooltipProvider>
        <SearchPalette />
      </TooltipProvider>
    </Provider>
  )
}

describe('SearchPalette — cross-workspace grouped render', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
    document.body.innerHTML = ''
  })

  it('renders per-workspace section headers with workspace name + hit count', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { id: 'h1', title: 'A1', snippet: 's1', source: 'agent_message',
        sourceId: 's1', workspaceId: 'ws-a', createdAt: '0' },
      { id: 'h2', title: 'A2', snippet: 's2', source: 'agent_message',
        sourceId: 's2', workspaceId: 'ws-a', createdAt: '0' },
      { id: 'h3', title: 'B1', snippet: 's3', source: 'agent_message',
        sourceId: 's3', workspaceId: 'ws-b', createdAt: '0' },
    ])
    const store = createStore()
    store.set(workspacesAtom, [ws('ws-a', 'Alpha'), ws('ws-b', 'Beta')])
    store.set(activeWorkspaceIdAtom, 'ws-a')
    store.set(searchPaletteOpenAtom, true)
    renderWithStore(store)

    const input = screen.getByPlaceholderText('搜索线程、项目...')
    fireEvent.change(input, { target: { value: 'hello' } })

    await waitFor(() => expect(screen.getByText(/Alpha · 2/)).toBeInTheDocument())
    expect(screen.getByText(/Beta · 1/)).toBeInTheDocument()
  })

  it('puts the active workspace section first', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { id: 'h1', title: 'A1', snippet: '', source: 'agent_message',
        sourceId: 's1', workspaceId: 'ws-a', createdAt: '0' },
      { id: 'h2', title: 'B1', snippet: '', source: 'agent_message',
        sourceId: 's2', workspaceId: 'ws-b', createdAt: '0' },
    ])
    const store = createStore()
    store.set(workspacesAtom, [ws('ws-a', 'Alpha'), ws('ws-b', 'Beta')])
    store.set(activeWorkspaceIdAtom, 'ws-b')
    store.set(searchPaletteOpenAtom, true)
    renderWithStore(store)

    fireEvent.change(screen.getByPlaceholderText('搜索线程、项目...'),
      { target: { value: 'hello' } })

    await waitFor(() => expect(screen.getByText(/Beta · 1/)).toBeInTheDocument())
    const headers = screen.getAllByText(/(Alpha|Beta) · /)
    expect(headers[0].textContent).toMatch(/Beta/)
    expect(headers[1].textContent).toMatch(/Alpha/)
  })

  it('shows an overflow chip when a group has more than 5 hits', async () => {
    const hits = Array.from({ length: 8 }, (_, i) => ({
      id: `h${i}`, title: `T${i}`, snippet: '', source: 'agent_message',
      sourceId: `s${i}`, workspaceId: 'ws-a', createdAt: '0',
    }))
    vi.mocked(invoke).mockResolvedValue(hits)
    const store = createStore()
    store.set(workspacesAtom, [ws('ws-a', 'Alpha')])
    store.set(activeWorkspaceIdAtom, 'ws-a')
    store.set(searchPaletteOpenAtom, true)
    renderWithStore(store)

    fireEvent.change(screen.getByPlaceholderText('搜索线程、项目...'),
      { target: { value: 'hello' } })

    await waitFor(() => expect(screen.getByText(/Alpha · 8/)).toBeInTheDocument())
    expect(screen.getByText('T0')).toBeInTheDocument()
    expect(screen.getByText('T4')).toBeInTheDocument()
    expect(screen.queryByText('T5')).not.toBeInTheDocument()
    expect(screen.getByText(/还有 3 条/)).toBeInTheDocument()
  })
})
