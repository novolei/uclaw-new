import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import * as React from 'react'
import { useDockBounce } from './useDockBounce'
import { dockBounceKeysAtom } from '@/atoms/dock-atoms'
import type { BottomDockHoverRegionHandle } from '@/components/dock/BottomDockHoverRegion'

// Capture the latest onNeedApproval callback so tests can drive it.
let needApprovalCb: ((p: unknown) => void) | null = null

vi.mock('@/lib/tauri-bridge', () => ({
  onNeedApproval: (cb: (p: unknown) => void) => {
    needApprovalCb = cb
    return Promise.resolve(() => {
      needApprovalCb = null
    })
  },
}))

describe('useDockBounce', () => {
  beforeEach(() => {
    needApprovalCb = null
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  function setup() {
    const store = createStore()
    const forceReveal = vi.fn()
    const holdRevealed = vi.fn()
    const ref: React.RefObject<BottomDockHoverRegionHandle> = {
      current: { forceReveal, holdRevealed },
    }
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <JotaiProvider store={store}>{children}</JotaiProvider>
    )
    renderHook(() => useDockBounce(ref), { wrapper })
    return { store, forceReveal, holdRevealed }
  }

  it('subscribes to onNeedApproval on mount', async () => {
    setup()
    await Promise.resolve()
    expect(needApprovalCb).not.toBeNull()
  })

  it('on approval event: forceReveal + holdRevealed(1500) + bumps mode-agent counter', async () => {
    const { store, forceReveal, holdRevealed } = setup()
    await Promise.resolve()
    act(() => {
      needApprovalCb?.({ id: 'req-1', tool_name: 'shell', params: {} })
    })
    expect(forceReveal).toHaveBeenCalledTimes(1)
    expect(holdRevealed).toHaveBeenCalledWith(1500)
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 1 })
  })

  it('multiple events accumulate the counter', async () => {
    const { store } = setup()
    await Promise.resolve()
    act(() => { needApprovalCb?.({ id: 'a' }) })
    act(() => { needApprovalCb?.({ id: 'b' }) })
    act(() => { needApprovalCb?.({ id: 'c' }) })
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 3 })
  })
})
