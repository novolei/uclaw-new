import * as React from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { browserTaskRunAtom } from '@/atoms/browser-atoms'
import { useBrowserTaskEvents } from './useBrowserTaskEvents'

const bridge = vi.hoisted(() => ({
  listenBrowserTaskRun: vi.fn(() => Promise.resolve(vi.fn())),
  listenBrowserTaskStep: vi.fn(() => Promise.resolve(vi.fn())),
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
  vi.clearAllMocks()
})

describe('useBrowserTaskEvents', () => {
  it('stores task run events for the matching session', async () => {
    const store = createStore()
    let onRun: Parameters<typeof bridge.listenBrowserTaskRun>[0] | null = null
    bridge.listenBrowserTaskRun.mockImplementationOnce((handler) => {
      onRun = handler
      return Promise.resolve(vi.fn())
    })

    renderHook(() => useBrowserTaskEvents('sess-1'), { wrapper: wrapper(store) })
    await flush()

    act(() => {
      onRun?.({
        runId: 'run-1',
        sessionId: 'sess-1',
        task: 'Search',
        status: 'running',
        steps: [],
      })
    })

    expect(store.get(browserTaskRunAtom).get('sess-1')?.task).toBe('Search')
  })

  it('upserts task steps in step order', async () => {
    const store = createStore()
    let onStep: Parameters<typeof bridge.listenBrowserTaskStep>[0] | null = null
    bridge.listenBrowserTaskStep.mockImplementationOnce((handler) => {
      onStep = handler
      return Promise.resolve(vi.fn())
    })

    renderHook(() => useBrowserTaskEvents('sess-1'), { wrapper: wrapper(store) })
    await flush()

    const step = (stepIndex: number, actionName: string) => ({
      stepIndex,
      phase: 'act' as const,
      observationSummary: '',
      reasoning: actionName,
      actionName,
      actionArgs: {},
      ok: true,
      message: null,
      error: null,
      timestampMs: stepIndex,
    })

    act(() => {
      onStep?.({ runId: 'run-1', sessionId: 'sess-1', status: 'running', step: step(2, 'browser_click') })
      onStep?.({ runId: 'run-1', sessionId: 'sess-1', status: 'running', step: step(1, 'decide') })
    })

    const steps = store.get(browserTaskRunAtom).get('sess-1')?.steps
    expect(steps?.map((s) => s.actionName)).toEqual(['decide', 'browser_click'])
  })
})
