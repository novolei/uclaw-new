import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import * as React from 'react'
import { useMemuConsolidation } from './useMemuConsolidation'
import { memuConsolidatingAtom } from '@/atoms/dock-atoms'

// Capture the latest listen callbacks per event name.
const listeners = new Map<string, (e: { payload: unknown }) => void>()

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(event, cb)
    return Promise.resolve(() => {
      listeners.delete(event)
    })
  },
}))

function setup() {
  const store = createStore()
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <JotaiProvider store={store}>{children}</JotaiProvider>
  )
  renderHook(() => useMemuConsolidation(), { wrapper })
  return { store }
}

describe('useMemuConsolidation', () => {
  beforeEach(() => {
    listeners.clear()
  })

  it('subscribes to both started and finished events on mount', async () => {
    setup()
    await Promise.resolve() // drain promise queue
    expect(listeners.has('memu:consolidation_started')).toBe(true)
    expect(listeners.has('memu:consolidation_finished')).toBe(true)
  })

  it('flips memuConsolidatingAtom to true on started', async () => {
    const { store } = setup()
    await Promise.resolve()
    expect(store.get(memuConsolidatingAtom)).toBe(false)
    act(() => {
      listeners.get('memu:consolidation_started')?.({ payload: { id: 'a' } })
    })
    expect(store.get(memuConsolidatingAtom)).toBe(true)
  })

  it('flips memuConsolidatingAtom back to false after matching finished', async () => {
    const { store } = setup()
    await Promise.resolve()
    act(() => {
      listeners.get('memu:consolidation_started')?.({ payload: { id: 'a' } })
    })
    expect(store.get(memuConsolidatingAtom)).toBe(true)
    act(() => {
      listeners.get('memu:consolidation_finished')?.({ payload: { id: 'a' } })
    })
    expect(store.get(memuConsolidatingAtom)).toBe(false)
  })

  it('stays true while any consolidation is in flight (concurrent dedup)', async () => {
    const { store } = setup()
    await Promise.resolve()
    act(() => {
      listeners.get('memu:consolidation_started')?.({ payload: { id: 'a' } })
      listeners.get('memu:consolidation_started')?.({ payload: { id: 'b' } })
      listeners.get('memu:consolidation_started')?.({ payload: { id: 'c' } })
    })
    expect(store.get(memuConsolidatingAtom)).toBe(true)
    // First two finish — atom still true because 'c' is in flight.
    act(() => {
      listeners.get('memu:consolidation_finished')?.({ payload: { id: 'a' } })
      listeners.get('memu:consolidation_finished')?.({ payload: { id: 'b' } })
    })
    expect(store.get(memuConsolidatingAtom)).toBe(true)
    // Last one finishes — atom flips false.
    act(() => {
      listeners.get('memu:consolidation_finished')?.({ payload: { id: 'c' } })
    })
    expect(store.get(memuConsolidatingAtom)).toBe(false)
  })

  it('ignores an unmatched finished event (defensive)', async () => {
    const { store } = setup()
    await Promise.resolve()
    expect(store.get(memuConsolidatingAtom)).toBe(false)
    act(() => {
      listeners.get('memu:consolidation_finished')?.({ payload: { id: 'never-started' } })
    })
    // Set was empty, still empty — atom stays false.
    expect(store.get(memuConsolidatingAtom)).toBe(false)
  })
})
