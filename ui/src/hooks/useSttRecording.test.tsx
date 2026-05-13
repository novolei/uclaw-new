import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider as JotaiProvider, createStore } from 'jotai'
import * as React from 'react'
import { installAudioStubs, type InstalledStubs } from '@/test-utils/stt-mocks'
import { useSttRecording } from './useSttRecording'
import {
  activeComposerAtom,
  modelStatusAtom,
} from '@/atoms/stt-atoms'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, _args?: unknown) => {
    if (cmd === 'stt_model_status') {
      return { openflow_ready: true, openflow_model_dir: '/tmp/sensevoice' }
    }
    if (cmd === 'stt_transcribe') {
      return {
        text: '你好世界',
        language: 'zh',
        elapsed_seconds: 0.42,
        provider: 'openflow',
      }
    }
    return null
  }),
}))

let stubs: InstalledStubs

beforeEach(() => {
  stubs = installAudioStubs()
})
afterEach(() => {
  stubs.cleanup()
  vi.clearAllTimers()
})

function makeWrapper(store: ReturnType<typeof createStore>) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <JotaiProvider store={store}>{children}</JotaiProvider>
  }
}

describe('useSttRecording', () => {
  it('starts in idle state', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp' })
    const { result } = renderHook(() => useSttRecording('chat'), {
      wrapper: makeWrapper(store),
    })
    expect(result.current.state.kind).toBe('idle')
  })

  it('transitions idle → recording on start()', async () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp' })
    const { result } = renderHook(() => useSttRecording('chat'), {
      wrapper: makeWrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    expect(result.current.state.kind).toBe('recording')
    expect(store.get(activeComposerAtom)).toBe('chat')
  })

  it('blocks second composer while another is recording', async () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp' })
    const wrapper = makeWrapper(store)
    const { result: chat } = renderHook(() => useSttRecording('chat'), { wrapper })
    const { result: agent } = renderHook(() => useSttRecording('agent'), { wrapper })
    await act(async () => {
      await chat.current.start()
    })
    let blocked: string | undefined
    await act(async () => {
      blocked = await agent.current.start()
    })
    expect(blocked).toBe('busy')
    expect(agent.current.state.kind).toBe('idle')
  })

  it('returns "needs-download" when model not ready', async () => {
    const store = createStore()
    store.set(modelStatusAtom, {
      kind: 'not-downloaded',
      expectedDir: '/tmp/sensevoice',
    })
    const { result } = renderHook(() => useSttRecording('chat'), {
      wrapper: makeWrapper(store),
    })
    let outcome: string | undefined
    await act(async () => {
      outcome = await result.current.start()
    })
    expect(outcome).toBe('needs-download')
    expect(result.current.state.kind).toBe('idle')
  })

  it('stop() transitions recording → transcribing → done with onTranscribe called', async () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp' })
    const onTranscribe = vi.fn()
    const { result } = renderHook(
      () => useSttRecording('chat', { onTranscribe }),
      { wrapper: makeWrapper(store) },
    )
    await act(async () => {
      await result.current.start()
    })
    stubs.emitData(new Blob([new Uint8Array([1, 2, 3, 4])], { type: 'audio/webm' }))
    await act(async () => {
      await result.current.stop()
    })
    expect(onTranscribe).toHaveBeenCalledWith('你好世界')
    expect(store.get(activeComposerAtom)).toBeNull()
  })

  it('cancel() returns idle without invoking transcribe', async () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp' })
    const onTranscribe = vi.fn()
    const { result } = renderHook(
      () => useSttRecording('chat', { onTranscribe }),
      { wrapper: makeWrapper(store) },
    )
    await act(async () => {
      await result.current.start()
    })
    await act(async () => {
      result.current.cancel()
    })
    expect(result.current.state.kind).toBe('idle')
    expect(onTranscribe).not.toHaveBeenCalled()
    expect(store.get(activeComposerAtom)).toBeNull()
  })

  it('records elapsed time accurately', async () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp' })
    const { result } = renderHook(() => useSttRecording('chat'), {
      wrapper: makeWrapper(store),
    })
    const before = Date.now()
    await act(async () => {
      await result.current.start()
    })
    const state = result.current.state
    if (state.kind === 'recording') {
      expect(state.startedAtMs).toBeGreaterThanOrEqual(before)
    } else {
      throw new Error('expected recording state')
    }
  })

  it('exposes start/stop/cancel as callable methods', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp' })
    const { result } = renderHook(() => useSttRecording('chat'), {
      wrapper: makeWrapper(store),
    })
    expect(typeof result.current.start).toBe('function')
    expect(typeof result.current.stop).toBe('function')
    expect(typeof result.current.cancel).toBe('function')
  })
})
