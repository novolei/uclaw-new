import { describe, it, expect, beforeEach, vi } from 'vitest'
import * as React from 'react'
import { Provider, createStore, useAtomValue } from 'jotai'
import { act, render } from '@testing-library/react'
import { useOpenSession } from './useOpenSession'
import { tabsAtom, type TabItem } from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { agentSessionsAtom } from '@/atoms/agent-atoms'

vi.mock('@/lib/tauri-bridge', () => ({}))

function makeAgentSession(id: string, workspaceId: string) {
  return {
    id,
    workspaceId,
    title: id,
    titleEmoji: '💬',
    titlePending: false,
    archived: false,
    createdAt: '2026-05-11T00:00:00Z',
    updatedAt: '2026-05-11T00:00:00Z',
  }
}

interface HarnessHandle {
  open: ReturnType<typeof useOpenSession>
  tabs: TabItem[]
}

function Harness({ onReady }: { onReady: (h: HarnessHandle) => void }): null {
  const open = useOpenSession()
  const tabs = useAtomValue(tabsAtom)
  React.useEffect(() => {
    onReady({ open, tabs })
  }, [open, tabs, onReady])
  return null
}

describe('useOpenSession — workspace tagging', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('tags an agent tab with the SESSION\'s workspaceId, not the active workspace', () => {
    // Repro for "标签页不存在" bug: user is viewing workspace A, clicks a
    // session from a context that shows cross-workspace results (e.g. a
    // search hit). The session belongs to workspace B. Without the fix
    // the tab would be tagged A and visibleTabsAtom would filter it out.
    const store = createStore()
    store.set(activeWorkspaceIdAtom, 'ws-A')
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    store.set(agentSessionsAtom, [makeAgentSession('s-in-B', 'ws-B')] as any)

    let handle: HarnessHandle | null = null
    render(
      <Provider store={store}>
        <Harness onReady={(h) => { handle = h }} />
      </Provider>,
    )
    act(() => { handle!.open('agent', 's-in-B', 'Title') })

    const tab = store.get(tabsAtom).find((t) => t.id === 's-in-B')
    expect(tab?.workspaceId).toBe('ws-B')
  })

  it('does not get stale on workspace switch (the original bug)', () => {
    // Repro: open a session in A → switch to B → open a session in B.
    // Before the fix, useCallback retained the stale activeWorkspaceId
    // = A in its closure (missing dep), so the second open tagged the
    // tab with A even though we're now in B and the session belongs
    // to B.
    const store = createStore()
    store.set(activeWorkspaceIdAtom, 'ws-A')
    store.set(agentSessionsAtom, [
      makeAgentSession('s-A', 'ws-A'),
      makeAgentSession('s-B', 'ws-B'),
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ] as any)

    let handle: HarnessHandle | null = null
    const { rerender } = render(
      <Provider store={store}>
        <Harness onReady={(h) => { handle = h }} />
      </Provider>,
    )
    act(() => { handle!.open('agent', 's-A', 'A') })
    expect(store.get(tabsAtom).find((t) => t.id === 's-A')?.workspaceId).toBe('ws-A')

    // Switch active workspace and re-render so the hook re-evaluates.
    act(() => { store.set(activeWorkspaceIdAtom, 'ws-B') })
    rerender(
      <Provider store={store}>
        <Harness onReady={(h) => { handle = h }} />
      </Provider>,
    )

    act(() => { handle!.open('agent', 's-B', 'B') })
    expect(store.get(tabsAtom).find((t) => t.id === 's-B')?.workspaceId).toBe('ws-B')
  })

  it('falls back to activeWorkspaceId when the session is not in agentSessions', () => {
    // Chat tabs, browser tabs, or sessions that haven't been loaded
    // yet should still receive a sensible workspaceId.
    const store = createStore()
    store.set(activeWorkspaceIdAtom, 'ws-A')
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    store.set(agentSessionsAtom, [] as any)

    let handle: HarnessHandle | null = null
    render(
      <Provider store={store}>
        <Harness onReady={(h) => { handle = h }} />
      </Provider>,
    )
    act(() => { handle!.open('chat', 'c-unknown', 'Chat') })

    const tab = store.get(tabsAtom).find((t) => t.id === 'c-unknown')
    expect(tab?.workspaceId).toBe('ws-A')
  })
})
