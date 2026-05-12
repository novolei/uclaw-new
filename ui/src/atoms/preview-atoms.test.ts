import { describe, it, expect, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  previewRefreshVersionAtomFamily,
  bumpPreviewRefreshAtom,
  resetAllPreviewRefreshAtom,
} from './preview-atoms'

describe('preview-atoms', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore()
    store.set(resetAllPreviewRefreshAtom)
  })

  it('defaults to 0 for any new file path', () => {
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(0)
  })

  it('bumps the version for one file', () => {
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(1)
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(2)
  })

  it('does not bump siblings', () => {
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    expect(store.get(previewRefreshVersionAtomFamily('/x/b.ts'))).toBe(0)
  })

  it('reset returns all known paths to 0', () => {
    store.set(bumpPreviewRefreshAtom, '/x/a.ts')
    store.set(bumpPreviewRefreshAtom, '/x/b.ts')
    store.set(resetAllPreviewRefreshAtom)
    expect(store.get(previewRefreshVersionAtomFamily('/x/a.ts'))).toBe(0)
    expect(store.get(previewRefreshVersionAtomFamily('/x/b.ts'))).toBe(0)
  })
})
