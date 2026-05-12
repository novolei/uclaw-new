/**
 * <PreviewPanel /> — W4a slide-in preview container.
 *
 * Mounted as a sibling to the agent SidePanel inside the agent right rail.
 * Visible when `previewPanelOpenAtom === true`. Width is user-resizable via
 * the left edge drag handle (atomWithStorage-persisted).
 *
 * Layout: header + surface. Surface picks the renderer.
 */

import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { cn } from '@/lib/utils'
import { usePreviewState } from '@/components/preview/hooks/usePreviewState'
import { closePreviewAction, previewPanelWidthAtom } from '@/atoms/preview-panel-atoms'
import { PreviewHeader } from './PreviewHeader'
import { PreviewSurface } from './PreviewSurface'

const MIN_WIDTH = 380
const MAX_WIDTH = 1100

export function PreviewPanel(): React.ReactElement | null {
  const { open, target } = usePreviewState()
  const [width, setWidth] = useAtom(previewPanelWidthAtom)
  const closePreview = useSetAtom(closePreviewAction)
  const draggingRef = React.useRef(false)

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

  const onResizeStart = React.useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault()
      draggingRef.current = true
      const startX = e.clientX
      const startWidth = width
      const onMove = (ev: MouseEvent) => {
        if (!draggingRef.current) return
        const delta = startX - ev.clientX // dragging left increases width
        const next = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startWidth + delta))
        setWidth(next)
      }
      const onUp = () => {
        draggingRef.current = false
        window.removeEventListener('mousemove', onMove)
        window.removeEventListener('mouseup', onUp)
      }
      window.addEventListener('mousemove', onMove)
      window.addEventListener('mouseup', onUp)
    },
    [width, setWidth],
  )

  if (!open) return null

  return (
    <aside
      className={cn(
        'relative flex flex-col h-full flex-shrink-0',
        'border-l border-border bg-popover shadow-xl',
        'transition-[width] duration-200 ease-out motion-reduce:transition-none',
      )}
      style={{ width }}
      aria-label="文件预览"
    >
      <button
        type="button"
        onMouseDown={onResizeStart}
        aria-label="拖动调整预览面板宽度"
        title="拖动调整宽度"
        className={cn(
          'absolute -left-1 top-0 bottom-0 w-2 cursor-col-resize',
          'hover:bg-foreground/[0.04] active:bg-foreground/[0.08]',
          'transition-colors',
        )}
      />
      <PreviewHeader target={target} />
      <div className="flex-1 min-h-0 flex flex-col">
        <PreviewSurface target={target} />
      </div>
    </aside>
  )
}
