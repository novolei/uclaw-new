import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'

// Mock FileTypeIcon so the test isn't coupled to its implementation
vi.mock('@/components/file-browser/FileTypeIcon', () => ({
  FileTypeIcon: ({ className }: { className?: string }) => (
    <span data-testid="file-icon" className={className} />
  ),
}))

import { PreviewTabBar } from './PreviewTabBar'
import {
  previewTabsAtom,
  activePreviewTabKeyAtom,
  previewPanelOpenAtom,
  type PreviewTabItem,
} from '@/atoms/preview-panel-atoms'

const AGENT_FOO: PreviewTabItem = {
  mountId: 'workspace:default',
  relPath: 'foo.md',
  name: 'foo.md',
  absolutePath: '/abs/foo.md',
  sessionId: 's1',
  source: 'agent',
  addedAt: 100,
}
const MANUAL_BAR: PreviewTabItem = {
  mountId: 'workspace:default',
  relPath: 'bar.md',
  name: 'bar.md',
  absolutePath: '/abs/bar.md',
  sessionId: 's1',
  source: 'manual',
  addedAt: 200,
}

function renderWith(
  tabs: PreviewTabItem[],
  activeKey: string | null,
): { store: ReturnType<typeof createStore> } {
  const store = createStore()
  store.set(previewTabsAtom, tabs)
  store.set(activePreviewTabKeyAtom, activeKey)
  store.set(previewPanelOpenAtom, true)
  render(
    <Provider store={store}>
      <PreviewTabBar />
    </Provider>,
  )
  return { store }
}

describe('PreviewTabBar', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders nothing when there are 0 tabs', () => {
    const { container } = render(
      <Provider store={createStore()}>
        <PreviewTabBar />
      </Provider>,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders one tab per item with correct active state', () => {
    renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:bar.md')
    const tabs = screen.getAllByRole('tab')
    expect(tabs).toHaveLength(2)
    expect(tabs[0]).toHaveAttribute('aria-selected', 'false')
    expect(tabs[1]).toHaveAttribute('aria-selected', 'true')
  })

  it('shows the agent ✨ marker only on agent-source tabs', () => {
    renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    expect(screen.getAllByTitle('opened by agent')).toHaveLength(1)
  })

  it('clicking a tab activates it', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    fireEvent.click(screen.getByLabelText('bar.md'))
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:bar.md')
  })

  it('close X click removes the tab from the pool', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    fireEvent.click(screen.getByLabelText('close bar.md'))
    expect(store.get(previewTabsAtom).map((t) => t.name)).toEqual(['foo.md'])
  })

  it('middle-click on tab closes it', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    // fireEvent.auxClick isn't available in @testing-library/dom v10; dispatch manually
    const tab = screen.getByLabelText('bar.md')
    fireEvent(tab, new MouseEvent('auxclick', { button: 1, bubbles: true }))
    expect(store.get(previewTabsAtom).map((t) => t.name)).toEqual(['foo.md'])
  })

  it('close click does NOT also activate that tab (stopPropagation works)', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    fireEvent.click(screen.getByLabelText('close bar.md'))
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })
})
