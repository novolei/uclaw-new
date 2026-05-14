import { describe, it, expect, beforeEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { homeOfficeStateAtom } from '@/atoms/home-office-atoms'
import { useHomeOfficeAgentSync } from './useHomeOfficeAgentSync'

type Listener = (event: { payload: unknown }) => void
const listeners = new Map<string, Listener>()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (channel: string, handler: Listener) => {
    listeners.set(channel, handler)
    return () => { listeners.delete(channel) }
  }),
}))

function fire(channel: string, payload: unknown = {}) {
  const h = listeners.get(channel)
  if (h) h({ payload })
}

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

describe('useHomeOfficeAgentSync', () => {
  beforeEach(() => {
    listeners.clear()
    vi.clearAllMocks()
  })

  it('maps chat:stream-chunk → typing', async () => {
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-chunk'))
    expect(store.get(homeOfficeStateAtom)).toBe('typing')
  })

  it('maps chat:stream-tool-activity → tool_activity', async () => {
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-tool-activity'))
    expect(store.get(homeOfficeStateAtom)).toBe('tool_activity')
  })

  it('maps chat:stream-error → error', async () => {
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-error'))
    expect(store.get(homeOfficeStateAtom)).toBe('error')
  })

  it('chat:stream-complete → success then idle after 4s', async () => {
    vi.useFakeTimers()
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-complete'))
    expect(store.get(homeOfficeStateAtom)).toBe('success')
    act(() => { vi.advanceTimersByTime(4000) })
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
    vi.useRealTimers()
  })

  it('agent:stream-reset → idle (cancels pending success timer)', async () => {
    vi.useFakeTimers()
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-complete'))
    expect(store.get(homeOfficeStateAtom)).toBe('success')
    act(() => fire('agent:stream-reset'))
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
    // even if the 4s success timer fires later, state should not flip
    act(() => { vi.advanceTimersByTime(5000) })
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
    vi.useRealTimers()
  })
})
