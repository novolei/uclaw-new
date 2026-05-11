import { describe, it, expect, beforeEach, vi } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render } from '@testing-library/react'
import { TabSessionSyncer } from './TabSessionSyncer'
import {
  tabsAtom, workspaceActiveTabIdMapAtom, type TabItem,
} from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { appModeAtom } from '@/atoms/app-mode'

function mk(id: string, type: 'agent' | 'chat', ws: string): TabItem {
  return { id, type, sessionId: id, title: id, workspaceId: ws }
}

vi.mock('@/lib/tauri-bridge', () => ({}))

describe('TabSessionSyncer', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('rewrites currentAgentSessionIdAtom when workspace switch flips the active tab', () => {
    const store = createStore()
    store.set(tabsAtom, [mk('a1', 'agent', 'ws-1'), mk('b1', 'agent', 'ws-2')])
    store.set(workspaceActiveTabIdMapAtom, new Map([['ws-1', 'a1'], ['ws-2', 'b1']]))
    store.set(activeWorkspaceIdAtom, 'ws-1')

    const { rerender } = render(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(currentAgentSessionIdAtom)).toBe('a1')

    store.set(activeWorkspaceIdAtom, 'ws-2')
    rerender(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(currentAgentSessionIdAtom)).toBe('b1')
  })

  it('sets appMode and currentConversationIdAtom for chat tabs', () => {
    const store = createStore()
    store.set(tabsAtom, [mk('c1', 'chat', 'ws-1')])
    store.set(workspaceActiveTabIdMapAtom, new Map([['ws-1', 'c1']]))
    store.set(activeWorkspaceIdAtom, 'ws-1')

    render(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(appModeAtom)).toBe('chat')
    expect(store.get(currentConversationIdAtom)).toBe('c1')
  })

  it('clears session atoms when active workspace has no tabs', () => {
    const store = createStore()
    store.set(tabsAtom, [])
    store.set(activeWorkspaceIdAtom, 'ws-empty')
    store.set(currentAgentSessionIdAtom, 'stale')
    render(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(currentAgentSessionIdAtom)).toBeNull()
  })
})
