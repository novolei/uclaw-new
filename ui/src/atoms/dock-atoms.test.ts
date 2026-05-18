import { createStore } from 'jotai'
import { describe, it, expect } from 'vitest'
import {
  bottomDockEnabledAtom,
  internetOnlineAtom,
  backendOnlineAtom,
  memuOnlineAtom,
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
