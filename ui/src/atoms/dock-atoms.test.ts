import { createStore } from 'jotai'
import { describe, it, expect } from 'vitest'
import {
  bottomDockEnabledAtom,
  internetOnlineAtom,
  backendOnlineAtom,
  memuOnlineAtom,
  dockOrderAtom,
  applyDockReorder,
  type DockItemSpec,
} from './dock-atoms'

describe('dock-atoms', () => {
  it('bottomDockEnabledAtom defaults to false', () => {
    const store = createStore()
    expect(store.get(bottomDockEnabledAtom)).toBe(false)
  })

  it('internetOnlineAtom defaults to true', () => {
    const store = createStore()
    expect(store.get(internetOnlineAtom)).toBe(true)
  })

  it('backendOnlineAtom defaults to true', () => {
    const store = createStore()
    expect(store.get(backendOnlineAtom)).toBe(true)
  })

  it('memuOnlineAtom defaults to null', () => {
    const store = createStore()
    expect(store.get(memuOnlineAtom)).toBeNull()
  })

  it('bottomDockEnabledAtom can be toggled', () => {
    const store = createStore()
    store.set(bottomDockEnabledAtom, true)
    expect(store.get(bottomDockEnabledAtom)).toBe(true)
  })
})

describe('dockOrderAtom', () => {
  it('seeds with the 4 mode entries in chat/agent/memory/kaleidoscope order', () => {
    const store = createStore()
    const order = store.get(dockOrderAtom)
    expect(order).toEqual([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
      { kind: 'mode', mode: 'memory' },
      { kind: 'mode', mode: 'kaleidoscope' },
    ])
  })

  it('persists writes (round-trip)', () => {
    const store = createStore()
    const next: DockItemSpec[] = [
      { kind: 'mode', mode: 'agent' },
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'memory' },
      { kind: 'mode', mode: 'kaleidoscope' },
    ]
    store.set(dockOrderAtom, next)
    expect(store.get(dockOrderAtom)).toEqual(next)
  })

  it('accepts pinned-* variants in the type (compiles + roundtrips)', () => {
    // Phase 2A doesn't render these — but the atom must accept them so Phase 2B
    // doesn't have to widen the union.
    const store = createStore()
    const next: DockItemSpec[] = [
      { kind: 'mode', mode: 'agent' },
      { kind: 'pinned-conversation', sessionId: 'abc', type: 'chat' },
      { kind: 'pinned-workspace', spaceId: 'workspace-1' },
      { kind: 'pinned-automation', specId: 'spec-1' },
    ]
    store.set(dockOrderAtom, next)
    expect(store.get(dockOrderAtom)).toEqual(next)
  })
})

describe('applyDockReorder', () => {
  const defaultOrder: DockItemSpec[] = [
    { kind: 'mode', mode: 'chat' },
    { kind: 'mode', mode: 'agent' },
    { kind: 'mode', mode: 'memory' },
    { kind: 'mode', mode: 'kaleidoscope' },
  ]
  const ids = ['mode-chat', 'mode-agent', 'mode-memory', 'mode-kaleidoscope']

  it('moves agent (idx 1) to position 0 when dropped on chat', () => {
    const next = applyDockReorder(defaultOrder, ids, 'mode-agent', 'mode-chat')
    expect(next[0]).toEqual({ kind: 'mode', mode: 'agent' })
    expect(next[1]).toEqual({ kind: 'mode', mode: 'chat' })
  })

  it('returns the same array when activeId === overId (no-op)', () => {
    const next = applyDockReorder(defaultOrder, ids, 'mode-chat', 'mode-chat')
    expect(next).toBe(defaultOrder) // referential equality — no allocation
  })

  it('returns the same array when over-id is unknown', () => {
    const next = applyDockReorder(defaultOrder, ids, 'mode-chat', 'bogus')
    expect(next).toBe(defaultOrder)
  })

  it('returns the same array when active-id is unknown', () => {
    const next = applyDockReorder(defaultOrder, ids, 'bogus', 'mode-chat')
    expect(next).toBe(defaultOrder)
  })
})
