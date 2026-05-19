import { createStore } from 'jotai'
import { describe, it, expect } from 'vitest'
import {
  bottomDockEnabledAtom,
  internetOnlineAtom,
  backendOnlineAtom,
  memuOnlineAtom,
  dockOrderAtom,
  applyDockReorder,
  ensureCanonicalModes,
  CANONICAL_DOCK_MODES,
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
  it('seeds with all 9 canonical mode entries in canonical order', () => {
    const store = createStore()
    const order = store.get(dockOrderAtom)
    expect(order).toEqual([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
      { kind: 'mode', mode: 'symphony' },
      { kind: 'mode', mode: 'memory' },
      { kind: 'mode', mode: 'kaleidoscope' },
      { kind: 'mode', mode: 'home' },
      { kind: 'mode', mode: 'connections' },
      { kind: 'mode', mode: 'alert' },
      { kind: 'mode', mode: 'settings' },
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

describe('ensureCanonicalModes', () => {
  it('returns the same reference when every canonical mode is already present', () => {
    const current: DockItemSpec[] = CANONICAL_DOCK_MODES.map((mode) => ({
      kind: 'mode' as const,
      mode,
    }))
    expect(ensureCanonicalModes(current)).toBe(current)
  })

  it('appends missing canonical modes while preserving existing order + pins', () => {
    // Simulates a localStorage from before symphony/home/connections/alert/settings landed.
    const legacy: DockItemSpec[] = [
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
      { kind: 'pinned-conversation', sessionId: 's1', type: 'agent' },
      { kind: 'mode', mode: 'memory' },
      { kind: 'mode', mode: 'kaleidoscope' },
    ]
    const next = ensureCanonicalModes(legacy)
    expect(next).not.toBe(legacy)
    // Original entries preserved in original positions
    expect(next.slice(0, 5)).toEqual(legacy)
    // Missing modes appended in canonical order
    expect(next.slice(5)).toEqual([
      { kind: 'mode', mode: 'symphony' },
      { kind: 'mode', mode: 'home' },
      { kind: 'mode', mode: 'connections' },
      { kind: 'mode', mode: 'alert' },
      { kind: 'mode', mode: 'settings' },
    ])
  })

  it('appends just the missing one when only a single mode is absent', () => {
    const current: DockItemSpec[] = CANONICAL_DOCK_MODES
      .filter((m) => m !== 'alert')
      .map((mode) => ({ kind: 'mode' as const, mode }))
    const next = ensureCanonicalModes(current)
    expect(next).toHaveLength(CANONICAL_DOCK_MODES.length)
    expect(next[next.length - 1]).toEqual({ kind: 'mode', mode: 'alert' })
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

import { addDockPin, removeDockPin, pinIdColorSeed } from './dock-atoms'

describe('addDockPin', () => {
  const base: DockItemSpec[] = [
    { kind: 'mode', mode: 'chat' },
    { kind: 'mode', mode: 'agent' },
    { kind: 'mode', mode: 'memory' },
    { kind: 'mode', mode: 'kaleidoscope' },
  ]

  it('appends a new pinned-conversation', () => {
    const next = addDockPin(base, {
      kind: 'pinned-conversation',
      sessionId: 'sess-1',
      type: 'agent',
    })
    expect(next).toHaveLength(5)
    expect(next[4]).toEqual({
      kind: 'pinned-conversation',
      sessionId: 'sess-1',
      type: 'agent',
    })
  })

  it('appends a pinned-workspace', () => {
    const next = addDockPin(base, { kind: 'pinned-workspace', spaceId: 'space-1' })
    expect(next).toHaveLength(5)
    expect(next[4]).toEqual({ kind: 'pinned-workspace', spaceId: 'space-1' })
  })

  it('appends a pinned-automation', () => {
    const next = addDockPin(base, { kind: 'pinned-automation', specId: 'spec-1' })
    expect(next).toHaveLength(5)
    expect(next[4]).toEqual({ kind: 'pinned-automation', specId: 'spec-1' })
  })

  it('is idempotent — adding the same pin twice does not duplicate', () => {
    const once = addDockPin(base, { kind: 'pinned-workspace', spaceId: 'space-1' })
    const twice = addDockPin(once, { kind: 'pinned-workspace', spaceId: 'space-1' })
    expect(twice).toBe(once) // referential equality — no allocation
    expect(twice).toHaveLength(5)
  })
})

describe('removeDockPin', () => {
  const base: DockItemSpec[] = [
    { kind: 'mode', mode: 'chat' },
    { kind: 'pinned-workspace', spaceId: 'space-1' },
    { kind: 'mode', mode: 'agent' },
  ]

  it('removes the matching entry by sortable id', () => {
    const next = removeDockPin(base, 'space-space-1')
    expect(next).toHaveLength(2)
    expect(next).toEqual([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
    ])
  })

  it('returns the same array reference when id not found', () => {
    const next = removeDockPin(base, 'space-nonexistent')
    expect(next).toBe(base)
  })

  it('cannot remove a mode entry — modes are not pins', () => {
    const next = removeDockPin(base, 'mode-chat')
    expect(next).toBe(base)
  })
})

describe('pinIdColorSeed', () => {
  it('returns a 2-stop gradient (from, to) as CSS hsl strings', () => {
    const seed = pinIdColorSeed('space-workspace-1')
    expect(seed.from).toMatch(/^hsl\(\d+(?:\.\d+)?,\s*\d+%,\s*\d+%\)$/)
    expect(seed.to).toMatch(/^hsl\(\d+(?:\.\d+)?,\s*\d+%,\s*\d+%\)$/)
  })

  it('is deterministic — same id maps to same gradient', () => {
    const a = pinIdColorSeed('space-workspace-1')
    const b = pinIdColorSeed('space-workspace-1')
    expect(a).toEqual(b)
  })

  it('different ids produce different gradients (hash spread)', () => {
    const a = pinIdColorSeed('space-workspace-1')
    const b = pinIdColorSeed('space-workspace-2')
    expect(a.from).not.toBe(b.from)
  })
})

import { dockBounceKeysAtom } from './dock-atoms'

describe('dockBounceKeysAtom', () => {
  it('starts empty', () => {
    const store = createStore()
    expect(store.get(dockBounceKeysAtom)).toEqual({})
  })

  it('can write a per-target bounce counter', () => {
    const store = createStore()
    store.set(dockBounceKeysAtom, (prev) => ({
      ...prev,
      'mode-agent': (prev['mode-agent'] ?? 0) + 1,
    }))
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 1 })

    store.set(dockBounceKeysAtom, (prev) => ({
      ...prev,
      'mode-agent': (prev['mode-agent'] ?? 0) + 1,
    }))
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 2 })
  })
})

import { memuConsolidatingAtom } from './dock-atoms'

describe('memuConsolidatingAtom', () => {
  it('starts false', () => {
    const store = createStore()
    expect(store.get(memuConsolidatingAtom)).toBe(false)
  })

  it('can be toggled', () => {
    const store = createStore()
    store.set(memuConsolidatingAtom, true)
    expect(store.get(memuConsolidatingAtom)).toBe(true)
    store.set(memuConsolidatingAtom, false)
    expect(store.get(memuConsolidatingAtom)).toBe(false)
  })
})
