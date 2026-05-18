/**
 * <PreviewPanel /> — W4a preview container.
 *
 * Dispatches on active tab type:
 *  - type === 'browser' → <BrowserPanel> (live CDP screencast)
 *  - type === 'file' (default) → <PreviewHeader> + <PreviewSurface>
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { usePreviewState } from '@/components/preview/hooks/usePreviewState'
import { closePreviewAction, previewTabsAtom, activePreviewTabKeyAtom, previewTabKey } from '@/atoms/preview-panel-atoms'
import { PreviewHeader } from './PreviewHeader'
import { PreviewSurface } from './PreviewSurface'
import { PreviewTabBar } from './PreviewTabBar'
import { BrowserPanel } from '@/components/browser/BrowserPanel'

export function PreviewPanel(): React.ReactElement | null {
  const { open, target } = usePreviewState()
  const closePreview = useSetAtom(closePreviewAction)
  const tabs = useAtomValue(previewTabsAtom)
  const activeKey = useAtomValue(activePreviewTabKeyAtom)

  const activeTab = tabs.find((t) => previewTabKey(t) === activeKey) ?? null

  React.useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closePreview()
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [open, closePreview])

  if (!open) return null

  return (
    <aside
      className="flex flex-col h-full w-full min-w-0 bg-popover"
      aria-label="预览面板"
    >
      <PreviewTabBar />

      {activeTab?.type === 'browser' && activeTab.browser ? (
        <BrowserPanel agentSessionId={activeTab.browser.agentSessionId} />
      ) : (
        <>
          <PreviewHeader target={target} />
          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            <PreviewSurface target={target} />
          </div>
        </>
      )}
    </aside>
  )
}
