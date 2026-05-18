import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import React from 'react'
import {
  internetOnlineAtom,
  backendOnlineAtom,
  memuOnlineAtom,
} from '@/atoms/dock-atoms'
import { useConnectionStatus } from './useConnectionStatus'

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/core — must be hoisted before the module is imported.
// ---------------------------------------------------------------------------
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

// Pull the mocked invoke after vi.mock has run.
import { invoke } from '@tauri-apps/api/core'
const mockInvoke = vi.mocked(invoke)

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

/** Set navigator.onLine synchronously; must be reset between tests. */
function setOnline(value: boolean) {
  Object.defineProperty(navigator, 'onLine', {
    value,
    configurable: true,
    writable: true,
  })
}

/** Default mock: get_app_health resolves, get_memu_status returns { online: true } */
function setupDefaultMocks() {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'get_app_health') return Promise.resolve(undefined)
    if (cmd === 'get_memu_status') return Promise.resolve({ online: true })
    return Promise.resolve(undefined)
  })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('useConnectionStatus', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    // Ensure navigator.onLine starts as true for each test.
    setOnline(true)
    setupDefaultMocks()
  })

  afterEach(() => {
    vi.useRealTimers()
    // Restore navigator.onLine to the default (true).
    setOnline(true)
  })

  // -------------------------------------------------------------------------
  // 1. Initial sync of navigator.onLine
  // -------------------------------------------------------------------------
  it('sets internetOnlineAtom to true on mount when navigator.onLine is true', async () => {
    const store = createStore()
    // Atom starts at true (default), confirm the hook writes it on mount.
    store.set(internetOnlineAtom, false) // manually flip to false first
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    // useEffect fires synchronously in renderHook; flush microtasks.
    await act(async () => { await Promise.resolve() })
    expect(store.get(internetOnlineAtom)).toBe(true)
  })

  it('sets internetOnlineAtom to false on mount when navigator.onLine is false', async () => {
    setOnline(false)
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    expect(store.get(internetOnlineAtom)).toBe(false)
  })

  // -------------------------------------------------------------------------
  // 2. online/offline window events update internetOnlineAtom
  // -------------------------------------------------------------------------
  it('sets internetOnlineAtom to false when window fires the offline event', async () => {
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => {
      window.dispatchEvent(new Event('offline'))
    })
    expect(store.get(internetOnlineAtom)).toBe(false)
  })

  it('sets internetOnlineAtom to true when window fires the online event', async () => {
    setOnline(false)
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    // Confirm we're offline first.
    expect(store.get(internetOnlineAtom)).toBe(false)
    act(() => {
      window.dispatchEvent(new Event('online'))
    })
    expect(store.get(internetOnlineAtom)).toBe(true)
  })

  // -------------------------------------------------------------------------
  // 3. Initial poll runs on mount
  // -------------------------------------------------------------------------
  it('calls invoke on mount (initial poll)', async () => {
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    // Flush the async poll microtasks.
    await act(async () => { await Promise.resolve() })
    expect(mockInvoke).toHaveBeenCalledWith('get_app_health')
    expect(mockInvoke).toHaveBeenCalledWith('get_memu_status')
  })

  it('sets backendOnlineAtom to true when get_app_health resolves', async () => {
    const store = createStore()
    store.set(backendOnlineAtom, false)
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    expect(store.get(backendOnlineAtom)).toBe(true)
  })

  it('sets backendOnlineAtom to false when get_app_health rejects', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_app_health') return Promise.reject(new Error('offline'))
      if (cmd === 'get_memu_status') return Promise.resolve({ online: true })
      return Promise.resolve(undefined)
    })
    const store = createStore()
    store.set(backendOnlineAtom, true)
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    expect(store.get(backendOnlineAtom)).toBe(false)
  })

  it('sets memuOnlineAtom from get_memu_status result', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_app_health') return Promise.resolve(undefined)
      if (cmd === 'get_memu_status') return Promise.resolve({ online: false })
      return Promise.resolve(undefined)
    })
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    expect(store.get(memuOnlineAtom)).toBe(false)
  })

  it('sets memuOnlineAtom to false when get_memu_status rejects', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_app_health') return Promise.resolve(undefined)
      if (cmd === 'get_memu_status') return Promise.reject(new Error('memu down'))
      return Promise.resolve(undefined)
    })
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    expect(store.get(memuOnlineAtom)).toBe(false)
  })

  // -------------------------------------------------------------------------
  // 4. Interval poll fires after 30s
  // -------------------------------------------------------------------------
  it('calls invoke again after 30s interval', async () => {
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    // Flush initial poll.
    await act(async () => { await Promise.resolve() })
    const callCountAfterMount = mockInvoke.mock.calls.length
    // Advance 30s to fire the interval.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000)
    })
    // At least one more round of invoke calls (2 commands per poll).
    expect(mockInvoke.mock.calls.length).toBeGreaterThan(callCountAfterMount)
    expect(mockInvoke).toHaveBeenCalledWith('get_app_health')
    expect(mockInvoke).toHaveBeenCalledWith('get_memu_status')
  })

  // -------------------------------------------------------------------------
  // 5. Offline guard skips poll
  // -------------------------------------------------------------------------
  it('does not call invoke when navigator.onLine is false during polling', async () => {
    setOnline(false)
    const store = createStore()
    renderHook(() => useConnectionStatus(), { wrapper: wrapper(store) })
    // Flush initial poll attempt (poll() returns early because !navigator.onLine).
    await act(async () => { await Promise.resolve() })
    expect(mockInvoke).not.toHaveBeenCalled()
    // Advance 30s — interval fires but poll should still skip.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000)
    })
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  // -------------------------------------------------------------------------
  // 6. Cleanup removes listeners and clears interval
  // -------------------------------------------------------------------------
  it('stops polling after unmount (interval is cleared)', async () => {
    const store = createStore()
    const { unmount } = renderHook(() => useConnectionStatus(), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    const callCountAfterMount = mockInvoke.mock.calls.length
    unmount()
    // Advance past the interval — no further invokes should fire.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000)
    })
    expect(mockInvoke.mock.calls.length).toBe(callCountAfterMount)
  })

  it('stops reacting to online/offline events after unmount', async () => {
    const store = createStore()
    const { unmount } = renderHook(() => useConnectionStatus(), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    unmount()
    // Dispatch offline — atom should not change since listener was removed.
    act(() => {
      window.dispatchEvent(new Event('offline'))
    })
    expect(store.get(internetOnlineAtom)).toBe(true)
  })
})
