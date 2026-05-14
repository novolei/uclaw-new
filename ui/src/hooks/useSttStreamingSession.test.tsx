import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import {
  sttModalStateAtom,
  activeComposerAtom,
  modelStatusAtom,
} from '@/atoms/stt-atoms'
import { useSttStreamingSession } from './useSttStreamingSession'

// mock streaming-capture
const mockCapture = {
  start: vi.fn().mockResolvedValue(undefined),
  stop: vi.fn(),
  getSegmentPcmBase64: vi.fn().mockReturnValue('AAAA'),
  resetSegment: vi.fn(),
  getVolume: vi.fn().mockReturnValue(0),
}
vi.mock('@/lib/stt/streaming-capture', () => ({
  createStreamingCapture: vi.fn(async () => mockCapture),
}))

// mock Tauri invoke
const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

function readyStore() {
  const store = createStore()
  store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
  return store
}

describe('useSttStreamingSession — skeleton', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockCapture.getSegmentPcmBase64.mockReturnValue('AAAA')
    mockCapture.getVolume.mockReturnValue(0)
    invokeMock.mockResolvedValue({ text: '' })
  })
  afterEach(() => { vi.useRealTimers() })

  it('start() transitions idle → listening and starts capture', async () => {
    const store = readyStore()
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    let res: string | undefined
    await act(async () => {
      res = await result.current.start()
    })
    expect(res).toBe('started')
    expect(store.get(sttModalStateAtom).kind).toBe('listening')
    expect(store.get(activeComposerAtom)).toBe('chat')
    expect(mockCapture.start).toHaveBeenCalledTimes(1)
  })

  it('start() returns needs-download when model is not ready', async () => {
    const store = createStore()
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    let res: string | undefined
    await act(async () => {
      res = await result.current.start()
    })
    expect(res).toBe('needs-download')
    expect(store.get(sttModalStateAtom).kind).toBe('idle')
  })

  it('start() returns busy when another composer holds the lock', async () => {
    const store = readyStore()
    store.set(activeComposerAtom, 'agent')
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    let res: string | undefined
    await act(async () => {
      res = await result.current.start()
    })
    expect(res).toBe('busy')
  })

  it('permission denial transitions to permission-denied and releases lock', async () => {
    const store = readyStore()
    mockCapture.start.mockRejectedValueOnce(
      Object.assign(new Error('denied'), { name: 'NotAllowedError' }),
    )
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    expect(store.get(sttModalStateAtom).kind).toBe('permission-denied')
    expect(store.get(activeComposerAtom)).toBeNull()
  })

  it('cancel() stops capture, releases lock, returns to idle', async () => {
    const store = readyStore()
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    act(() => {
      result.current.cancel()
    })
    expect(store.get(sttModalStateAtom).kind).toBe('idle')
    expect(store.get(activeComposerAtom)).toBeNull()
    expect(mockCapture.stop).toHaveBeenCalled()
  })

  it('end() with empty interim text just closes (idle, lock released)', async () => {
    const store = readyStore()
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    await act(async () => {
      await result.current.end()
    })
    expect(store.get(sttModalStateAtom).kind).toBe('idle')
    expect(store.get(activeComposerAtom)).toBeNull()
    expect(mockCapture.stop).toHaveBeenCalled()
  })
})
