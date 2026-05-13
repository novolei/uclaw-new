import * as React from 'react'
import { act, renderHook } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  composerFocusedAtom,
  composerHasTextAtom,
} from '@/atoms/agent-atoms'
import { petPrimaryStateAtom } from '@/atoms/pet-atoms'

const listeners = new Map<string, (event: { payload: unknown }) => void>()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (eventName: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(eventName, cb)
    return () => {
      listeners.delete(eventName)
    }
  }),
}))

import { usePetStateSync } from './usePetStateSync'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) => (
    <Provider store={store}>{children}</Provider>
  )
}

describe('usePetStateSync', () => {
  beforeEach(() => {
    listeners.clear()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('sets typing on chat:stream-chunk (agent producing tokens)', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-chunk')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('typing')
  })

  it('sets thinking on chat:stream-tool-activity (agent using tools)', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-tool-activity')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('thinking')
  })

  it('alternates typing ↔ thinking as chunks and tool activity interleave', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-chunk')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('typing')
    await act(async () => {
      listeners.get('chat:stream-tool-activity')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('thinking')
    await act(async () => {
      listeners.get('chat:stream-chunk')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('typing')
  })

  it('sets success then auto-returns to idle after 4000ms (full animation)', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-complete')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('success')
    await act(async () => {
      vi.advanceTimersByTime(4000)
    })
    expect(store.get(petPrimaryStateAtom)).toBe('idle')
  })

  it('sets error on chat:stream-error', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-error')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('error')
  })

  it('sets typing when composer is focused with text', async () => {
    const store = createStore()
    const { rerender } = renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      store.set(composerFocusedAtom, true)
      store.set(composerHasTextAtom, true)
    })
    rerender()
    expect(store.get(petPrimaryStateAtom)).toBe('typing')
  })

  it('does not override thinking/success/error with composer typing', async () => {
    const store = createStore()
    store.set(petPrimaryStateAtom, 'thinking')
    const { rerender } = renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      store.set(composerFocusedAtom, true)
      store.set(composerHasTextAtom, true)
    })
    rerender()
    expect(store.get(petPrimaryStateAtom)).toBe('thinking')
  })

  it('cancels the success linger timer when stream-chunk arrives mid-linger', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })

    // Fire stream-complete → starts the 1500ms success → idle timer
    await act(async () => {
      listeners.get('chat:stream-complete')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('success')

    // Mid-linger (after 500ms), stream-chunk arrives → state switches to typing
    await act(async () => {
      vi.advanceTimersByTime(500)
      listeners.get('chat:stream-chunk')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('typing')

    // Advance well past the original 1500ms — state must stay typing, not snap to idle
    await act(async () => {
      vi.advanceTimersByTime(2000)
    })
    expect(store.get(petPrimaryStateAtom)).toBe('typing')
  })
})
