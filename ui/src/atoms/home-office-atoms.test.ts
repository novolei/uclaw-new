import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  homeOfficePanelOpenAtom,
  homeOfficeStateAtom,
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
  stickyNotesAtom,
  diaryEntriesAtom,
  openZoneAtom,
} from './home-office-atoms'

describe('home-office-atoms defaults', () => {
  it('panel starts closed', () => {
    const store = createStore()
    expect(store.get(homeOfficePanelOpenAtom)).toBe(false)
  })

  it('agent state defaults to idle', () => {
    const store = createStore()
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
  })

  it('character defaults near center (oak desk area)', () => {
    const store = createStore()
    const pos = store.get(characterPositionAtom)
    expect(pos.x).toBeGreaterThan(0.4)
    expect(pos.x).toBeLessThan(0.6)
    expect(pos.y).toBeGreaterThan(0.4)
    expect(pos.y).toBeLessThan(0.7)
  })

  it('character defaults facing south', () => {
    const store = createStore()
    expect(store.get(characterDirectionAtom)).toBe('S')
  })

  it('motion defaults to pose (not walking)', () => {
    const store = createStore()
    expect(store.get(characterMotionAtom)).toBe('pose')
  })

  it('sticky notes and diary entries start empty', () => {
    const store = createStore()
    expect(store.get(stickyNotesAtom)).toEqual([])
    expect(store.get(diaryEntriesAtom)).toEqual([])
  })

  it('no zone modal open by default', () => {
    const store = createStore()
    expect(store.get(openZoneAtom)).toBeNull()
  })
})

describe('home-office-atoms writes', () => {
  it('can set agent state to thinking', () => {
    const store = createStore()
    store.set(homeOfficeStateAtom, 'thinking')
    expect(store.get(homeOfficeStateAtom)).toBe('thinking')
  })

  it('can add sticky note', () => {
    const store = createStore()
    const note = { id: 'a', text: 'remember milk', at: 123 }
    store.set(stickyNotesAtom, [note])
    expect(store.get(stickyNotesAtom)).toEqual([note])
  })
})
