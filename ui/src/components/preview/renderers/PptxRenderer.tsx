import * as React from 'react'
import { Loader2, AlertTriangle } from 'lucide-react'

interface PptxRendererProps {
  bytes: Uint8Array
  name: string
}

export function PptxRenderer({ bytes, name }: PptxRendererProps): React.ReactElement {
  const [state, setState] = React.useState<
    { kind: 'loading' } | { kind: 'ready'; html: string } | { kind: 'error'; message: string }
  >({ kind: 'loading' })

  React.useEffect(() => {
    let cancelled = false
    setState({ kind: 'loading' })
    void (async () => {
      try {
        const { convertPptxToHtml } = await import('@/components/preview/office-parsers/pptx')
        const result = await convertPptxToHtml(bytes, name)
        if (cancelled) return
        setState({ kind: 'ready', html: result.html })
      } catch (err) {
        if (cancelled) return
        setState({ kind: 'error', message: err instanceof Error ? err.message : String(err) })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [bytes, name])

  if (state.kind === 'loading') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Loader2 className="size-7 text-foreground/40 animate-spin motion-reduce:animate-none mb-3" aria-hidden />
        <div className="text-[12.5px] text-foreground/70">正在提取 {name} 文本…</div>
      </div>
    )
  }
  if (state.kind === 'error') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertTriangle className="size-7 text-destructive mb-3" aria-hidden />
        <div className="text-[12.5px] font-medium text-destructive">PPTX 解析失败</div>
        <div className="mt-1 text-[11px] text-muted-foreground max-w-[320px] break-words">
          {state.message}
        </div>
      </div>
    )
  }
  return (
    <div className="flex-1 min-h-0 overflow-auto bg-popover">
      <div
        className="office-preview-host"
        dangerouslySetInnerHTML={{ __html: state.html }}
      />
    </div>
  )
}
