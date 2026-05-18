/**
 * <PreviewPanel /> — W4a preview container.
 *
 * Rendered inside `MainArea`'s horizontal split (chat on the left, preview
 * on the right) when `previewPanelOpenAtom === true`. The parent owns the
 * width via `previewPanelSplitRatioAtom` + a drag handle between the two
 * panes; this component is a passive flex child that just fills whatever
 * space its parent gives it.
 *
 * Layout: header + surface. Surface picks the renderer.
 *
 * The component returns `null` when closed, but the conventional way to
 * use it is to gate the mount at the parent (so the layout collapses back
 * to chat-only without an empty flex slot).
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { usePreviewState } from '@/components/preview/hooks/usePreviewState'
import { closePreviewAction } from '@/atoms/preview-panel-atoms'
import { PreviewHeader } from './PreviewHeader'
import { PreviewSurface } from './PreviewSurface'
import { PreviewTabBar } from './PreviewTabBar'

export function PreviewPanel(): React.ReactElement | null {
  const { open, target } = usePreviewState()
  const closePreview = useSetAtom(closePreviewAction)

  // ESC closes the panel
  React.useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        closePreview()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [open, closePreview])

  if (!open) return null

  return (
    <aside
      className="flex flex-col h-full w-full min-w-0 bg-popover"
      aria-label="文件预览"
    >
      <PreviewTabBar />
      <PreviewHeader target={target} />
      <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
        <PreviewSurface target={target} />
      </div>
    </aside>
  )
}
