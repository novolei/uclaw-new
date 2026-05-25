import * as React from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, renderHook } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { browserScreencastActiveAtom, browserScreencastFrameAtom } from '@/atoms/browser-atoms'
import { useBrowserScreencast } from './useBrowserScreencast'

const bridge = vi.hoisted(() => ({
  browserCaptureScreenshot: vi.fn(() => Promise.resolve('png-b64')),
  browserStartScreencast: vi.fn(() => Promise.resolve()),
  browserStopScreencast: vi.fn(() => Promise.resolve()),
  listenScreencastFrames: vi.fn(() => Promise.resolve(vi.fn())),
}))

vi.mock('@/lib/tauri-bridge', () => bridge)

function wrapper(store = createStore()): React.FC<{ children: React.ReactNode }> {
  return ({ children }) => <Provider store={store}>{children}</Provider>
}

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve()
  })
}

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.clearAllMocks()
})

describe('useBrowserScreencast', () => {
  it('ignores pseudo new tab ids', async () => {
    const store = createStore()
    renderHook(() => useBrowserScreencast('sess-1', 'new'), { wrapper: wrapper(store) })
    await flush()

    expect(bridge.listenScreencastFrames).not.toHaveBeenCalled()
    expect(bridge.browserStartScreencast).not.toHaveBeenCalled()
    expect(store.get(browserScreencastActiveAtom).has('sess-1')).toBe(false)
  })

  it('starts after the frame listener is registered', async () => {
    const store = createStore()
    renderHook(() => useBrowserScreencast('sess-1', 'tab-1'), { wrapper: wrapper(store) })

    expect(bridge.listenScreencastFrames).toHaveBeenCalledTimes(1)
    expect(bridge.browserStartScreencast).not.toHaveBeenCalled()

    await flush()

    expect(bridge.browserStartScreencast).toHaveBeenCalledWith('sess-1', 'tab-1')
    expect(store.get(browserScreencastActiveAtom).has('sess-1')).toBe(true)
  })

  it('shares one backend screencast across multiple mounted consumers', async () => {
    const store = createStore()
    const first = renderHook(() => useBrowserScreencast('sess-1', 'tab-1'), { wrapper: wrapper(store) })
    const second = renderHook(() => useBrowserScreencast('sess-1', 'tab-1'), { wrapper: wrapper(store) })
    await flush()

    expect(bridge.browserStartScreencast).toHaveBeenCalledTimes(1)

    first.unmount()
    expect(bridge.browserStopScreencast).not.toHaveBeenCalled()

    second.unmount()
    expect(bridge.browserStopScreencast).toHaveBeenCalledWith('sess-1', 'tab-1')
  })

  it('routes screencast frames into the session frame atom', async () => {
    const store = createStore()
    let onFrame: Parameters<typeof bridge.listenScreencastFrames>[0] | null = null
    bridge.listenScreencastFrames.mockImplementationOnce((handler) => {
      onFrame = handler
      return Promise.resolve(vi.fn())
    })

    renderHook(() => useBrowserScreencast('sess-1', 'tab-1'), { wrapper: wrapper(store) })
    await flush()

    act(() => {
      onFrame?.({
        sessionId: 'sess-1',
        tabId: 'tab-1',
        dataB64: 'abc',
        pageWidth: 1280,
        pageHeight: 800,
      })
    })

    expect(store.get(browserScreencastFrameAtom).get('sess-1')?.dataB64).toBe('abc')
  })

  it('uses screenshot fallback only once while waiting for the first live frame', async () => {
    vi.useFakeTimers()
    const store = createStore()
    renderHook(() => useBrowserScreencast('sess-fallback', 'tab-fallback'), { wrapper: wrapper(store) })
    await flush()

    expect(bridge.browserCaptureScreenshot).not.toHaveBeenCalled()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_500)
    })
    expect(bridge.browserCaptureScreenshot).toHaveBeenCalledTimes(1)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(6_000)
    })
    expect(bridge.browserCaptureScreenshot).toHaveBeenCalledTimes(1)
  })
})
