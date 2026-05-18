import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserScreencastActiveAtom,
  browserDOMOverlayVisibleAtom,
} from './browser-atoms'

describe('browserScreencastFrameAtom', () => {
  it('starts empty', () => {
    const store = createStore()
    expect(store.get(browserScreencastFrameAtom).size).toBe(0)
  })
})

describe('browserDOMStateAtom', () => {
  it('starts empty', () => {
    const store = createStore()
    expect(store.get(browserDOMStateAtom).size).toBe(0)
  })
})

describe('browserScreencastActiveAtom', () => {
  it('starts empty', () => {
    const store = createStore()
    expect(store.get(browserScreencastActiveAtom).size).toBe(0)
  })
})

describe('browserDOMOverlayVisibleAtom', () => {
  it('starts false', () => {
    const store = createStore()
    expect(store.get(browserDOMOverlayVisibleAtom)).toBe(false)
  })
})
