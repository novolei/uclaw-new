import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen } from '@/test-utils/render'
import { agentStreamingStatesAtom, currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import type { AgentStreamState } from '@/atoms/agent-atoms'
import { ContextCompactingStatus } from './ContextCompactingStatus'

const TEXT = '正在压缩上下文…'

function state(over: Partial<AgentStreamState>): AgentStreamState {
  return { running: true, content: '', toolActivities: [], teammates: [], ...over }
}

function storeWith(current: string, states: Record<string, AgentStreamState>) {
  const store = createStore()
  store.set(currentAgentSessionIdAtom, current)
  store.set(agentStreamingStatesAtom, new Map(Object.entries(states)))
  return store
}

describe('ContextCompactingStatus', () => {
  it('shows the status while its session is compacting', () => {
    const store = storeWith('s1', { s1: state({ contextCompacting: true }) })
    renderWithProviders(<ContextCompactingStatus sessionId="s1" />, { store })
    expect(screen.getByText(TEXT)).toBeTruthy()
  })

  it('renders nothing for another session that is compacting', () => {
    const store = storeWith('s1', { s1: state({}), s2: state({ contextCompacting: true }) })
    renderWithProviders(<ContextCompactingStatus sessionId="s1" />, { store })
    expect(screen.queryByText(TEXT)).toBeNull()
  })

  it("a view for session B stays quiet while the current session A compacts", () => {
    // AgentMessages is also mounted for non-current sessions (automation
    // HomeThreadView / RunSessionSubView): the status follows the view's own
    // session, never the global current one.
    const store = storeWith('A', { A: state({ contextCompacting: true }), B: state({}) })
    renderWithProviders(<ContextCompactingStatus sessionId="B" />, { store })
    expect(screen.queryByText(TEXT)).toBeNull()
  })

  it('shows for a non-current session view whose own session compacts', () => {
    const store = storeWith('A', { A: state({}), B: state({ contextCompacting: true }) })
    renderWithProviders(<ContextCompactingStatus sessionId="B" />, { store })
    expect(screen.getByText(TEXT)).toBeTruthy()
  })

  it('renders nothing once the turn is no longer running', () => {
    const store = storeWith('s1', { s1: state({ running: false, contextCompacting: true }) })
    renderWithProviders(<ContextCompactingStatus sessionId="s1" />, { store })
    expect(screen.queryByText(TEXT)).toBeNull()
  })
})
