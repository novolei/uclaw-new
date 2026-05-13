import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { usePreviewRefresh } from './usePreviewRefresh'
import { bumpPreviewRefreshAtom } from '@/atoms/preview-atoms'
import { setDirtyBufferAction, clearDirtyBufferAction } from '@/atoms/preview-editor-atoms'

// Tauri event subscription is mocked to capture the registered handler so the
// test can synthesize events without touching the real bus. The shared
// `mockListen` helper in ui/src/test-utils/mock-tauri.ts is a no-op and
// cannot drive the event-driven assertions below, so this file uses a
// richer event-capturing mock instead.
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>()
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (e: { payload: unknown }) => void) => {
    if (!listeners.has(name)) listeners.set(name, new Set())
    listeners.get(name)!.add(handler)
    return () => listeners.get(name)!.delete(handler)
  }),
}))

function emit(name: string, payload: unknown): void {
  listeners.get(name)?.forEach((h) => h({ payload }))
}

function wrapper(store: ReturnType<typeof createStore>) {
  return function W({ children }: { children: React.ReactNode }) {
    return <Provider store={store}>{children}</Provider>
  }
}

describe('usePreviewRefresh', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    listeners.clear()
    store = createStore()
  })

  it('returns 0 by default', () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    expect(result.current).toBe(0)
  })

  it('re-renders when the atom is bumped', () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    act(() => store.set(bumpPreviewRefreshAtom, '/x/a.ts'))
    expect(result.current).toBe(1)
  })

  it('bumps version when an agent:file-written event matches the path', async () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    // Allow the effect that registers the listener to run
    await act(async () => { await Promise.resolve() })
    act(() => emit('agent:file-written', { path: '/x/a.ts' }))
    expect(result.current).toBe(1)
  })

  it('ignores agent:file-written for unrelated paths', async () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    act(() => emit('agent:file-written', { path: '/x/other.ts' }))
    expect(result.current).toBe(0)
  })

  it('bumps version on tauri://focus regardless of path', async () => {
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    act(() => emit('tauri://focus', undefined))
    expect(result.current).toBe(1)
  })

  // Dirty-guard regression suite (if2Ai port, 2026-05-13).
  // The user's in-progress draft must never be silently overwritten by a
  // refetch — neither from the agent's own writes nor from window-focus
  // refreshes. Without these guards the editor would clobber unsaved
  // markdown edits the moment the watcher / focus event arrived.

  it('skips agent:file-written bump when the path is dirty', async () => {
    store.set(setDirtyBufferAction, {
      filePath: '/x/a.ts',
      content: 'draft',
      baselineMtimeMs: 1,
    })
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    act(() => emit('agent:file-written', { path: '/x/a.ts' }))
    expect(result.current).toBe(0)
  })

  it('skips tauri://focus bump when the path is dirty', async () => {
    store.set(setDirtyBufferAction, {
      filePath: '/x/a.ts',
      content: 'draft',
      baselineMtimeMs: 1,
    })
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    act(() => emit('tauri://focus', undefined))
    expect(result.current).toBe(0)
  })

  it('resumes bumping once the path is cleaned', async () => {
    store.set(setDirtyBufferAction, {
      filePath: '/x/a.ts',
      content: 'draft',
      baselineMtimeMs: 1,
    })
    const { result } = renderHook(() => usePreviewRefresh('/x/a.ts'), {
      wrapper: wrapper(store),
    })
    await act(async () => { await Promise.resolve() })
    // Dirty: bump skipped.
    act(() => emit('agent:file-written', { path: '/x/a.ts' }))
    expect(result.current).toBe(0)
    // Cleaned (e.g. after a successful save): bump fires.
    act(() => store.set(clearDirtyBufferAction, '/x/a.ts'))
    act(() => emit('agent:file-written', { path: '/x/a.ts' }))
    expect(result.current).toBe(1)
  })
})
