import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  type PreviewFileTarget,
  type PreviewTabItem,
  previewTabsAtom,
  activePreviewTabKeyAtom,
  previewPanelOpenAtom,
  selectedPreviewFileAtom,
  openPreviewTabAction,
  closePreviewTabAction,
  clearAllPreviewTabsAction,
  previewTabKey,
  openPreviewAction,
} from './preview-panel-atoms'

const FOO: PreviewFileTarget = {
  mountId: 'workspace:default',
  relPath: 'foo.md',
  name: 'foo.md',
  absolutePath: '/abs/foo.md',
  sessionId: 's1',
}
const BAR: PreviewFileTarget = {
  mountId: 'workspace:default',
  relPath: 'bar.md',
  name: 'bar.md',
  absolutePath: '/abs/bar.md',
  sessionId: 's1',
}
const BAZ: PreviewFileTarget = {
  mountId: 'workspace:default',
  relPath: 'baz.md',
  name: 'baz.md',
  absolutePath: '/abs/baz.md',
  sessionId: 's1',
}

describe('previewTabKey', () => {
  it('composes mountId and relPath', () => {
    expect(previewTabKey({ mountId: 'm', relPath: 'a/b.md' })).toBe('m:a/b.md')
  })
})

describe('openPreviewTabAction', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-05-18T00:00:00Z'))
  })

  it('inserts a new tab, activates it, opens the panel', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    expect(store.get(previewTabsAtom)).toHaveLength(1)
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
    expect(store.get(previewPanelOpenAtom)).toBe(true)
  })

  it('focuses existing tab when same key is opened again (no duplicate)', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:bar.md')
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    expect(store.get(previewTabsAtom)).toHaveLength(2)
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })

  it('agent-source tabs cluster left of manual tabs', () => {
    const store = createStore()
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: BAR, source: 'agent' })
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: BAZ, source: 'manual' })
    const order = store.get(previewTabsAtom).map((t) => t.name)
    expect(order).toEqual(['bar.md', 'foo.md', 'baz.md'])
  })

  it('promotes a manual tab to agent on agent re-open and re-sorts', () => {
    const store = createStore()
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'agent' })
    const order = store.get(previewTabsAtom).map((t) => t.name)
    expect(order).toEqual(['bar.md', 'foo.md'])
    const barTab = store.get(previewTabsAtom).find((t) => t.name === 'bar.md')
    expect(barTab?.source).toBe('agent')
  })
})

describe('closePreviewTabAction', () => {
  it('removes the tab and activates the neighbor on the right', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAZ, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:bar.md')
    store.set(closePreviewTabAction, 'workspace:default:bar.md')
    expect(store.get(previewTabsAtom).map((t) => t.name)).toEqual(['foo.md', 'baz.md'])
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:baz.md')
  })

  it('falls back to left neighbor when right neighbor missing', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:bar.md')
    store.set(closePreviewTabAction, 'workspace:default:bar.md')
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })

  it('closing the last tab nulls active and closes the panel', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(closePreviewTabAction, 'workspace:default:foo.md')
    expect(store.get(previewTabsAtom)).toHaveLength(0)
    expect(store.get(activePreviewTabKeyAtom)).toBeNull()
    expect(store.get(previewPanelOpenAtom)).toBe(false)
  })

  it('closing an inactive tab leaves the active tab unchanged', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:foo.md')
    store.set(closePreviewTabAction, 'workspace:default:bar.md')
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })

  it('no-op when key not found', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(closePreviewTabAction, 'nonexistent:key')
    expect(store.get(previewTabsAtom)).toHaveLength(1)
  })
})

describe('selectedPreviewFileAtom (derived)', () => {
  it('returns null when no tab is active', () => {
    const store = createStore()
    expect(store.get(selectedPreviewFileAtom)).toBeNull()
  })

  it('returns the active tab projected as PreviewFileTarget', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    const sel = store.get(selectedPreviewFileAtom)
    expect(sel).toEqual({
      mountId: FOO.mountId,
      relPath: FOO.relPath,
      name: FOO.name,
      absolutePath: FOO.absolutePath,
      sessionId: FOO.sessionId,
    })
  })

  it('updates when the active tab changes', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    expect(store.get(selectedPreviewFileAtom)?.name).toBe('bar.md')
    store.set(activePreviewTabKeyAtom, 'workspace:default:foo.md')
    expect(store.get(selectedPreviewFileAtom)?.name).toBe('foo.md')
  })
})

describe('openPreviewAction (legacy compat wrapper)', () => {
  it('delegates to openPreviewTabAction with source manual', () => {
    const store = createStore()
    store.set(openPreviewAction, FOO)
    expect(store.get(previewTabsAtom)).toHaveLength(1)
    const tab = store.get(previewTabsAtom)[0] as PreviewTabItem
    expect(tab.source).toBe('manual')
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })
})

describe('clearAllPreviewTabsAction', () => {
  it('removes all tabs, nulls active, closes panel', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'agent' })
    store.set(clearAllPreviewTabsAction)
    expect(store.get(previewTabsAtom)).toHaveLength(0)
    expect(store.get(activePreviewTabKeyAtom)).toBeNull()
    expect(store.get(previewPanelOpenAtom)).toBe(false)
  })
})
