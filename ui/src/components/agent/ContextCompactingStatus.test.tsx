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
  it('shows the status while the current session is compacting', () => {
    const store = storeWith('s1', { s1: state({ contextCompacting: true }) })
    renderWithProviders(<ContextCompactingStatus />, { store })
    expect(screen.getByText(TEXT)).toBeTruthy()
  })

  it('renders nothing when another session is compacting', () => {
    const store = storeWith('s1', { s1: state({}), s2: state({ contextCompacting: true }) })
    renderWithProviders(<ContextCompactingStatus />, { store })
    expect(screen.queryByText(TEXT)).toBeNull()
  })

  it('renders nothing once the turn is no longer running', () => {
    const store = storeWith('s1', { s1: state({ running: false, contextCompacting: true }) })
    renderWithProviders(<ContextCompactingStatus />, { store })
    expect(screen.queryByText(TEXT)).toBeNull()
  })
})
