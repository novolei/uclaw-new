import { describe, it, expect, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import * as React from 'react'
import { useDirtyBuffer } from './useDirtyBuffer'
import { dirtyBuffersAtom } from '@/atoms/preview-editor-atoms'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

describe('useDirtyBuffer', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore()
  })

  it('registers a dirty buffer when content diverges from baseline', () => {
    const { rerender } = renderHook(
      ({ content }: { content: string }) =>
        useDirtyBuffer({
          filePath: '/foo.ts',
          baselineContent: 'init',
          baselineMtimeMs: 1000,
          currentContent: content,
        }),
      { initialProps: { content: 'init' }, wrapper: wrapper(store) },
    )

    expect(store.get(dirtyBuffersAtom).has('/foo.ts')).toBe(false)

    rerender({ content: 'changed' })

    expect(store.get(dirtyBuffersAtom).has('/foo.ts')).toBe(true)
    expect(store.get(dirtyBuffersAtom).get('/foo.ts')?.content).toBe('changed')
  })

  it('clears the buffer when content returns to baseline', () => {
    const { rerender } = renderHook(
      ({ content }: { content: string }) =>
        useDirtyBuffer({
          filePath: '/foo.ts',
          baselineContent: 'init',
          baselineMtimeMs: 1000,
          currentContent: content,
        }),
      { initialProps: { content: 'changed' }, wrapper: wrapper(store) },
    )
    expect(store.get(dirtyBuffersAtom).has('/foo.ts')).toBe(true)

    rerender({ content: 'init' })

    expect(store.get(dirtyBuffersAtom).has('/foo.ts')).toBe(false)
  })

  it('registers in markdown / auto-save mode too (single dirty source of truth)', () => {
    // Mtime-OCC removal (2026-05-13): both editors now register here so
    // usePreviewRefresh's dirty-guard can block external refresh bumps
    // while a markdown file has an unsaved draft.
    renderHook(
      () =>
        useDirtyBuffer({
          filePath: '/foo.md',
          baselineContent: 'init',
          baselineMtimeMs: 1000,
          currentContent: 'changed',
        }),
      { wrapper: wrapper(store) },
    )

    expect(store.get(dirtyBuffersAtom).has('/foo.md')).toBe(true)
  })
})
