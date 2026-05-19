import { describe, it, expect } from 'vitest'
import { renderHook } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import * as React from 'react'
import { useDockLiveness } from './useDockLiveness'
import { memuConsolidatingAtom } from '@/atoms/dock-atoms'
import {
  agentStreamingStatesAtom,
  currentAgentSessionIdAtom,
  type AgentStreamState,
} from '@/atoms/agent-atoms'

function wrapperWith(setup: (s: ReturnType<typeof createStore>) => void) {
  const store = createStore()
  setup(store)
  return ({ children }: { children: React.ReactNode }) => (
    <JotaiProvider store={store}>{children}</JotaiProvider>
  )
}

describe('useDockLiveness', () => {
  it('returns all-off when nothing is active', () => {
    const wrapper = wrapperWith(() => {})
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-agent']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: false,
    })
    expect(result.current['mode-memory']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: false,
    })
  })

  it('sets breathing + streaming on mode-agent when agent is streaming', () => {
    const wrapper = wrapperWith((s) => {
      s.set(currentAgentSessionIdAtom, 'sess-1')
      s.set(agentStreamingStatesAtom, new Map([
        ['sess-1', { running: true, content: '...', toolActivities: [], teammates: [] } as AgentStreamState],
      ]))
    })
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-agent']).toEqual({
      breathing: true,
      streaming: true,
      pulsing: false,
    })
    expect(result.current['mode-memory'].pulsing).toBe(false)
  })

  it('sets pulsing on mode-memory when memuConsolidating is true', () => {
    const wrapper = wrapperWith((s) => {
      s.set(memuConsolidatingAtom, true)
    })
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-memory']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: true,
    })
    expect(result.current['mode-agent'].breathing).toBe(false)
  })

  it('all three signals can be on at once', () => {
    const wrapper = wrapperWith((s) => {
      s.set(currentAgentSessionIdAtom, 'sess-1')
      s.set(agentStreamingStatesAtom, new Map([
        ['sess-1', { running: true, content: '...', toolActivities: [], teammates: [] } as AgentStreamState],
      ]))
      s.set(memuConsolidatingAtom, true)
    })
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-agent']).toEqual({
      breathing: true,
      streaming: true,
      pulsing: false,
    })
    expect(result.current['mode-memory']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: true,
    })
  })
})
