import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  previewTabsAtom,
  activePreviewTabKeyAtom,
  previewTabKey,
  openBrowserTabAction,
  type PreviewTabItem,
} from './preview-panel-atoms'

describe('openBrowserTabAction', () => {
  it('creates a browser-type tab and activates it', () => {
    const store = createStore()
    store.set(openBrowserTabAction, { agentSessionId: 'sess-1', initialUrl: 'https://example.com' })
    const tabs = store.get(previewTabsAtom)
    expect(tabs).toHaveLength(1)
    const tab = tabs[0] as PreviewTabItem
    expect(tab.type).toBe('browser')
    expect(tab.browser?.agentSessionId).toBe('sess-1')
    expect(store.get(activePreviewTabKeyAtom)).toBe(previewTabKey(tab))
  })

  it('re-activates an existing browser tab without duplication', () => {
    const store = createStore()
    store.set(openBrowserTabAction, { agentSessionId: 'sess-1', initialUrl: 'https://a.com' })
    store.set(openBrowserTabAction, { agentSessionId: 'sess-1', initialUrl: 'https://b.com' })
    expect(store.get(previewTabsAtom)).toHaveLength(1)
  })
})
