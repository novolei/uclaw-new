import * as React from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'
import { Provider, createStore } from 'jotai'

type Handler = (event: { payload: unknown }) => void

const handlers = vi.hoisted(() => new Map<string, Handler[]>())

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: Handler) => {
    const list = handlers.get(name) ?? []
    list.push(cb)
    handlers.set(name, list)
    return Promise.resolve(() => {})
  }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve(null)),
}))

const toastMock = vi.hoisted(() => {
  const fn = vi.fn() as any
  fn.success = vi.fn()
  fn.info = vi.fn()
  fn.warning = vi.fn()
  fn.error = vi.fn()
  fn.message = vi.fn()
  return fn
})

vi.mock('sonner', () => ({ toast: toastMock }))

const AUTO_COPY = '已自动压缩较早的对话，保留了最近几轮原文'

function emit(name: string, payload: unknown): void {
  for (const cb of handlers.get(name) ?? []) cb({ payload })
}

function totalToastCalls(): number {
  return (
    toastMock.mock.calls.length +
    toastMock.success.mock.calls.length +
    toastMock.info.mock.calls.length +
    toastMock.warning.mock.calls.length +
    toastMock.error.mock.calls.length +
    toastMock.message.mock.calls.length
  )
}

function allToastTexts(): string[] {
  return [
    toastMock,
    toastMock.success,
    toastMock.info,
    toastMock.warning,
    toastMock.error,
    toastMock.message,
  ].flatMap((f: any) => f.mock.calls.map((c: unknown[]) => String(c[0])))
}

async function setup() {
  // Fresh module per test — the hook keeps a module-level `initialized` singleton.
  vi.resetModules()
  const atoms = await import('@/atoms/agent-atoms')
  const { useGlobalAgentListeners } = await import('./useGlobalAgentListeners')
  const store = createStore()
  store.set(atoms.currentAgentSessionIdAtom, 'sess-1')
  const wrapper: React.FC<{ children: React.ReactNode }> = ({ children }) => (
    <Provider store={store}>{children}</Provider>
  )
  renderHook(() => useGlobalAgentListeners(), { wrapper })
  await act(async () => {
    await Promise.resolve()
  })
  return { store, atoms }
}

const compactedPayload = (sessionId: string, trigger: 'auto' | 'manual' | 'overflow' = 'auto') => ({
  sessionId,
  compactedMessages: 12,
  summaryTokens: 800,
  thresholdTokens: 128000,
  trigger,
})

beforeEach(() => {
  handlers.clear()
})

afterEach(() => {
  vi.clearAllMocks()
})

describe('useGlobalAgentListeners — agent:context_compacted', () => {
  it('context_compacted refreshes the session messages', async () => {
    const { store, atoms } = await setup()
    expect(store.get(atoms.agentMessageRefreshAtom).get('sess-1')).toBeUndefined()

    act(() => emit('agent:context_compacted', compactedPayload('sess-1')))

    expect(store.get(atoms.agentMessageRefreshAtom).get('sess-1')).toBe(1)
  })

  it('handler leaves agentStreamingStatesAtom untouched (refresh is only a version bump)', async () => {
    const { store, atoms } = await setup()
    const streaming = new Map([
      ['sess-1', { running: true, content: 'partial', toolActivities: [], teammates: [] }],
    ])
    store.set(atoms.agentStreamingStatesAtom, streaming)

    act(() => emit('agent:context_compacted', compactedPayload('sess-1')))

    expect(store.get(atoms.agentStreamingStatesAtom)).toBe(streaming)
  })

  it('shows the toast "已自动压缩较早的对话，保留了最近几轮原文"', async () => {
    await setup()

    act(() => emit('agent:context_compacted', compactedPayload('sess-1')))

    expect(allToastTexts()).toContain(AUTO_COPY)
    expect(totalToastCalls()).toBe(1)
  })

  it('ignores events for other sessions (no toast for an unfocused session)', async () => {
    await setup()

    act(() => emit('agent:context_compacted', compactedPayload('sess-other')))

    expect(totalToastCalls()).toBe(0)
  })

  it('auto event for an unfocused open session bumps its refresh but shows no toast', async () => {
    const { store, atoms } = await setup()

    act(() => emit('agent:context_compacted', compactedPayload('sess-other')))

    expect(store.get(atoms.agentMessageRefreshAtom).get('sess-other')).toBe(1)
    expect(store.get(atoms.agentMessageRefreshAtom).has('sess-1')).toBe(false)
    expect(totalToastCalls()).toBe(0)
  })

  it('manual /compact shows exactly one toast', async () => {
    const { store, atoms } = await setup()
    // AgentView's /compact path optimistically sets these flags.
    store.set(atoms.agentStreamingStatesAtom, new Map([
      ['sess-1', {
        running: true, content: '', toolActivities: [], teammates: [],
        isCompacting: true, compactInFlight: true,
      }],
    ]))

    act(() => emit('agent:context_compacted', compactedPayload('sess-1', 'manual')))
    act(() => emit('chat:stream-complete', {
      conversationId: 'sess-1',
      text: '',
      compact: { removed: 12, remaining: 4, before: 16 },
    }))

    expect(totalToastCalls()).toBe(1)
    expect(allToastTexts()).not.toContain(AUTO_COPY)
  })

  it('manual /compact during a running turn (cancel first) shows exactly one toast', async () => {
    const { store, atoms } = await setup()
    store.set(atoms.agentStreamingStatesAtom, new Map([
      ['sess-1', {
        running: true, content: 'partial', toolActivities: [], teammates: [],
        isCompacting: true, compactInFlight: true,
      }],
    ]))

    // The cancelled turn completes first and clears compactInFlight …
    act(() => emit('chat:stream-complete', { conversationId: 'sess-1', text: '' }))
    // … then the manual compaction runs and reports itself.
    act(() => emit('agent:context_compacted', compactedPayload('sess-1', 'manual')))
    act(() => emit('chat:stream-complete', {
      conversationId: 'sess-1',
      text: '',
      compact: { removed: 12, remaining: 4, before: 16 },
    }))

    expect(allToastTexts()).not.toContain(AUTO_COPY)
    expect(totalToastCalls()).toBe(1)
  })

  it('overflow trigger (compact + retry after a provider overflow) refreshes like auto', async () => {
    const { store, atoms } = await setup()

    act(() => emit('agent:context_compacted', compactedPayload('sess-1', 'overflow')))

    expect(store.get(atoms.agentMessageRefreshAtom).get('sess-1')).toBe(1)
    expect(allToastTexts()).toContain(AUTO_COPY)
  })

  it('manual trigger does not bump the refresh (the /compact stream-complete reloads)', async () => {
    const { store, atoms } = await setup()

    act(() => emit('agent:context_compacted', compactedPayload('sess-1', 'manual')))

    expect(store.get(atoms.agentMessageRefreshAtom).has('sess-1')).toBe(false)
  })

  it('a no-op /compact says there was nothing to compact (no compaction toast or marker)', async () => {
    const { store, atoms } = await setup()
    store.set(atoms.agentStreamingStatesAtom, new Map([
      ['sess-1', {
        running: true, content: '', toolActivities: [], teammates: [],
        isCompacting: true, compactInFlight: true,
      }],
    ]))
    store.set(atoms.liveMessagesMapAtom, new Map([
      ['sess-1', [{ type: 'system', subtype: 'compacting', uuid: 'c1' }]],
    ]))

    act(() => emit('chat:stream-complete', {
      conversationId: 'sess-1',
      text: '',
      compact: { compacted: false, removed: 0, remaining: 4, before: 4 },
    }))

    expect(totalToastCalls()).toBe(1)
    expect(allToastTexts()).toEqual(['没有可压缩的内容'])
    const live = store.get(atoms.liveMessagesMapAtom).get('sess-1') ?? []
    expect(live.some((m: any) => m.subtype === 'compact_boundary')).toBe(false)
    expect(live.some((m: any) => m.subtype === 'compacting')).toBe(false)
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-1')?.isCompacting).toBe(false)
  })
})

describe('useGlobalAgentListeners — agent:tool_results_cleared', () => {
  const cleared = (sessionId: string, ids: string[], freedTokens = 23456) => ({
    sessionId,
    toolCallIds: ids,
    freedTokens,
  })

  it('merges cleared ids into clearedToolCallIdsMapAtom per session', async () => {
    const { store, atoms } = await setup()

    act(() => emit('agent:tool_results_cleared', cleared('sess-1', ['a', 'b'])))
    act(() => emit('agent:tool_results_cleared', cleared('sess-1', ['b', 'c'])))
    act(() => emit('agent:tool_results_cleared', cleared('sess-other', ['z'])))

    const map = store.get(atoms.clearedToolCallIdsMapAtom)
    expect([...(map.get('sess-1') ?? [])].sort()).toEqual(['a', 'b', 'c'])
    expect([...(map.get('sess-other') ?? [])]).toEqual(['z'])
  })

  it('toasts the freed tokens only for the current session', async () => {
    await setup()

    act(() => emit('agent:tool_results_cleared', cleared('sess-other', ['z'])))
    expect(totalToastCalls()).toBe(0)

    act(() => emit('agent:tool_results_cleared', cleared('sess-1', ['a'], 23456)))
    expect(toastMock.info).toHaveBeenCalledTimes(1)
    expect(allToastTexts()).toEqual(['已清理较早的工具结果，释放约 23.5K token'])
  })
})

describe('useGlobalAgentListeners — agent:context_compacting', () => {
  const running = () => ({ running: true, content: '', toolActivities: [], teammates: [] })

  it('sets contextCompacting for the event session and clears it on context_compacted', async () => {
    const { store, atoms } = await setup()
    store.set(atoms.agentStreamingStatesAtom, new Map([['sess-1', running()], ['sess-2', running()]]))

    act(() => emit('agent:context_compacting', { sessionId: 'sess-1', trigger: 'auto' }))
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-1')?.contextCompacting).toBe(true)
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-2')?.contextCompacting).toBeFalsy()

    act(() => emit('agent:context_compacted', compactedPayload('sess-1')))
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-1')?.contextCompacting).toBe(false)
  })

  it('overflow compaction also shows progress', async () => {
    const { store, atoms } = await setup()
    store.set(atoms.agentStreamingStatesAtom, new Map([['sess-1', running()]]))

    act(() => emit('agent:context_compacting', { sessionId: 'sess-1', trigger: 'overflow' }))
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-1')?.contextCompacting).toBe(true)
  })

  it('manual /compact keeps its own indicator (no contextCompacting)', async () => {
    const { store, atoms } = await setup()
    store.set(atoms.agentStreamingStatesAtom, new Map([['sess-1', running()]]))

    act(() => emit('agent:context_compacting', { sessionId: 'sess-1', trigger: 'manual' }))
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-1')?.contextCompacting).toBeFalsy()
  })

  it('is cleared when the turn ends', async () => {
    const { store, atoms } = await setup()
    store.set(atoms.agentStreamingStatesAtom, new Map([['sess-1', running()]]))
    act(() => emit('agent:context_compacting', { sessionId: 'sess-1', trigger: 'auto' }))

    act(() => emit('chat:stream-complete', { conversationId: 'sess-1', text: '' }))
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-1')?.contextCompacting).toBe(false)
  })

  it('is cleared when the turn errors or is cancelled', async () => {
    const { store, atoms } = await setup()
    store.set(atoms.agentStreamingStatesAtom, new Map([['sess-1', running()]]))
    act(() => emit('agent:context_compacting', { sessionId: 'sess-1', trigger: 'auto' }))

    act(() => emit('chat:stream-error', { conversationId: 'sess-1', error: 'cancelled' }))
    expect(store.get(atoms.agentStreamingStatesAtom).get('sess-1')?.contextCompacting).toBe(false)
  })
})
