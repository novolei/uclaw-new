/**
 * PdfRenderer — Renders a PDF file to a stack of canvas elements via pdfjs.
 *
 * Lazy-imports pdfjs-dist so the ~6 MB worker chunk only loads when a PDF is
 * opened. Worker URL uses Vite's `?url` import to get a hashed path under
 * `assets/`.
 *
 * Zoom: discrete steps 50% → 300%. Re-renders on every step change.
 * Big PDFs (50+ pages) are still rendered eagerly — if this becomes a
 * perf issue we can virtualise per-page later.
 */

import * as React from 'react'
import { Loader2, AlertTriangle, ZoomIn, ZoomOut } from 'lucide-react'
import { cn } from '@/lib/utils'

interface PdfRendererProps {
  bytes: Uint8Array
  name: string
}

const ZOOM_STEPS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3] as const
const DEFAULT_STEP_IDX = 2 // 1.0x

export function PdfRenderer({ bytes, name }: PdfRendererProps): React.ReactElement {
  const containerRef = React.useRef<HTMLDivElement>(null)
  const [stepIdx, setStepIdx] = React.useState<number>(DEFAULT_STEP_IDX)
  const [state, setState] = React.useState<
    | { kind: 'loading' }
    | { kind: 'ready'; numPages: number }
    | { kind: 'error'; message: string }
  >({ kind: 'loading' })
  // Hold the loaded document across zoom changes.
  const pdfDocRef = React.useRef<PDFDocumentProxy | null>(null)

  // Load the document once on `bytes` change.
  React.useEffect(() => {
    let cancelled = false
    setState({ kind: 'loading' })
    pdfDocRef.current = null
    void (async () => {
      try {
        // Lazy import + worker URL.
        const pdfjs = await import('pdfjs-dist')
        // Use ?url so Vite emits a hashed worker file we can pass to pdfjs.
        const workerUrl = (await import('pdfjs-dist/build/pdf.worker.min.mjs?url')).default
        pdfjs.GlobalWorkerOptions.workerSrc = workerUrl

        const doc = await pdfjs.getDocument({ data: bytes }).promise
        if (cancelled) return
        pdfDocRef.current = doc as PDFDocumentProxy
        setState({ kind: 'ready', numPages: doc.numPages })
      } catch (err) {
        if (cancelled) return
        setState({
          kind: 'error',
          message: err instanceof Error ? err.message : String(err),
        })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [bytes])

  // Re-render all pages whenever the document loads OR zoom changes.
  React.useEffect(() => {
    if (state.kind !== 'ready' || !pdfDocRef.current || !containerRef.current) return
    let cancelled = false
    const doc = pdfDocRef.current
    const container = containerRef.current
    container.innerHTML = ''

    void (async () => {
      const scale = ZOOM_STEPS[stepIdx]!
      const dpr = window.devicePixelRatio || 1
      for (let pageNum = 1; pageNum <= doc.numPages; pageNum++) {
        if (cancelled) return
        const page = await doc.getPage(pageNum)
        const viewport = page.getViewport({ scale: scale * dpr })
        const canvas = document.createElement('canvas')
        canvas.width = viewport.width
        canvas.height = viewport.height
        canvas.style.width = `${viewport.width / dpr}px`
        canvas.style.height = `${viewport.height / dpr}px`
        canvas.style.display = 'block'
        canvas.style.margin = '0 auto 12px'
        canvas.style.borderRadius = '4px'
        canvas.style.boxShadow = '0 2px 8px hsl(var(--foreground) / 0.08)'
        const ctx = canvas.getContext('2d')
        if (!ctx) continue
        await page.render({ canvasContext: ctx, viewport }).promise
        if (!cancelled) container.appendChild(canvas)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [state, stepIdx])

  const handleZoomIn = React.useCallback(() => {
    setStepIdx((idx) => Math.min(ZOOM_STEPS.length - 1, idx + 1))
  }, [])
  const handleZoomOut = React.useCallback(() => {
    setStepIdx((idx) => Math.max(0, idx - 1))
  }, [])

  return (
    <div className="flex flex-col h-full bg-popover">
      {state.kind === 'ready' && (
        <div className="flex-shrink-0 flex items-center gap-2 h-[32px] px-3 border-b border-border text-[11px] text-muted-foreground">
          <span>共 {state.numPages} 页</span>
          <span className="ml-auto" />
          <ToolbarButton aria-label="缩小" onClick={handleZoomOut} disabled={stepIdx === 0}>
            <ZoomOut size={13} />
          </ToolbarButton>
          <span className="font-mono tabular-nums text-foreground/70 min-w-[42px] text-center">
            {Math.round(ZOOM_STEPS[stepIdx]! * 100)}%
          </span>
          <ToolbarButton
            aria-label="放大"
            onClick={handleZoomIn}
            disabled={stepIdx === ZOOM_STEPS.length - 1}
          >
            <ZoomIn size={13} />
          </ToolbarButton>
        </div>
      )}
      <div className="flex-1 min-h-0 overflow-auto p-4">
        {state.kind === 'loading' && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <Loader2 className="size-7 text-foreground/40 animate-spin motion-reduce:animate-none mb-3" aria-hidden />
            <div className="text-[12.5px] text-foreground/70">正在加载 PDF…</div>
          </div>
        )}
        {state.kind === 'error' && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <AlertTriangle className="size-7 text-destructive mb-3" aria-hidden />
            <div className="text-[12.5px] font-medium text-destructive">PDF 加载失败</div>
            <div className="mt-1 text-[11px] text-muted-foreground max-w-[320px] break-words">
              {name}：{state.message}
            </div>
          </div>
        )}
        {state.kind === 'ready' && <div ref={containerRef} />}
      </div>
    </div>
  )
}

interface ToolbarButtonProps {
  children: React.ReactNode
  onClick: () => void
  disabled?: boolean
  'aria-label': string
}

function ToolbarButton({
  children,
  onClick,
  disabled,
  'aria-label': ariaLabel,
}: ToolbarButtonProps): React.ReactElement {
  return (
    <button
      type="button"
      aria-label={ariaLabel}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'size-6 inline-flex items-center justify-center rounded',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        disabled
          ? 'text-foreground/25 cursor-not-allowed'
          : 'text-foreground/60 hover:text-foreground hover:bg-foreground/[0.06]',
      )}
    >
      {children}
    </button>
  )
}

// Minimal pdfjs typing — avoid pulling in `@types/pdfjs-dist` to keep
// the dep surface small. We only use `numPages` + `getPage().getViewport()`
// + `page.render()`.
interface PDFPageViewport {
  width: number
  height: number
}
interface PDFPageProxy {
  getViewport(opts: { scale: number }): PDFPageViewport
  render(opts: { canvasContext: CanvasRenderingContext2D; viewport: PDFPageViewport }): {
    promise: Promise<void>
  }
}
interface PDFDocumentProxy {
  numPages: number
  getPage(num: number): Promise<PDFPageProxy>
}
