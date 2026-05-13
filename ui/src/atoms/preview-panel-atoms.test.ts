import { describe, it, expect, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  autoPreviewDismissedSessionsAtom,
  closePreviewAction,
  openPreviewAction,
  previewPanelOpenAtom,
  selectedPreviewFileAtom,
} from './preview-panel-atoms'

describe('closePreviewAction — dismiss memory', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore()
  })

  it('stamps the current target.sessionId into autoPreviewDismissedSessionsAtom on close', () => {
    store.set(openPreviewAction, {
      mountId: 'workspace:default',
      relPath: 'src/foo.ts',
      name: 'foo.ts',
      sessionId: 'session-abc',
      absolutePath: '/tmp/foo.ts',
    })
    expect(store.get(previewPanelOpenAtom)).toBe(true)

    store.set(closePreviewAction)

    expect(store.get(previewPanelOpenAtom)).toBe(false)
    expect(store.get(autoPreviewDismissedSessionsAtom).has('session-abc')).toBe(true)
  })

  it('does not stamp when target has no sessionId', () => {
    store.set(selectedPreviewFileAtom, {
      mountId: 'workspace:default',
      relPath: 'src/foo.ts',
      name: 'foo.ts',
      absolutePath: '/tmp/foo.ts',
    })
    store.set(previewPanelOpenAtom, true)

    store.set(closePreviewAction)

    expect(store.get(previewPanelOpenAtom)).toBe(false)
    expect(store.get(autoPreviewDismissedSessionsAtom).size).toBe(0)
  })

  it('bypassing the action (direct write to previewPanelOpenAtom) does NOT stamp', () => {
    // Workspace-switch path closes the panel programmatically without
    // signalling user dismissal — verify that escape hatch still works.
    store.set(openPreviewAction, {
      mountId: 'workspace:default',
      relPath: 'src/foo.ts',
      name: 'foo.ts',
      sessionId: 'session-abc',
      absolutePath: '/tmp/foo.ts',
    })

    store.set(previewPanelOpenAtom, false)

    expect(store.get(previewPanelOpenAtom)).toBe(false)
    expect(store.get(autoPreviewDismissedSessionsAtom).has('session-abc')).toBe(false)
  })
})
