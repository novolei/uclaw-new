import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  automationSelectedSpecIdAtom,
  automationActiveTabAtom,
  automationActivityRunSessionIdAtom,
} from './automation-ui'

describe('automation-ui atoms', () => {
  it('automationSelectedSpecIdAtom defaults to null', () => {
    const store = createStore()
    expect(store.get(automationSelectedSpecIdAtom)).toBeNull()
  })

  it('automationActiveTabAtom defaults to activity', () => {
    const store = createStore()
    expect(store.get(automationActiveTabAtom)).toBe('activity')
  })

  it('automationActivityRunSessionIdAtom defaults to null', () => {
    const store = createStore()
    expect(store.get(automationActivityRunSessionIdAtom)).toBeNull()
  })

  it('atoms are writable and independent', () => {
    const store = createStore()
    store.set(automationSelectedSpecIdAtom, 'spec-123')
    store.set(automationActiveTabAtom, 'chat')
    store.set(automationActivityRunSessionIdAtom, 'session-abc')

    expect(store.get(automationSelectedSpecIdAtom)).toBe('spec-123')
    expect(store.get(automationActiveTabAtom)).toBe('chat')
    expect(store.get(automationActivityRunSessionIdAtom)).toBe('session-abc')
  })
})
