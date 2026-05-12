import * as React from 'react'
import { Loader2, AlertTriangle } from 'lucide-react'

interface XlsxRendererProps {
  bytes: Uint8Array
  name: string
}

export function XlsxRenderer({ bytes, name }: XlsxRendererProps): React.ReactElement {
  const [state, setState] = React.useState<
    { kind: 'loading' } | { kind: 'ready'; html: string } | { kind: 'error'; message: string }
  >({ kind: 'loading' })

  React.useEffect(() => {
    let cancelled = false
    setState({ kind: 'loading' })
    void (async () => {
      try {
        const { convertXlsxToHtml } = await import('@/components/preview/office-parsers/xlsx')
        const result = await convertXlsxToHtml(bytes, name)
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
        <div className="text-[12.5px] text-foreground/70">正在解析 {name}…</div>
      </div>
    )
  }
  if (state.kind === 'error') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertTriangle className="size-7 text-destructive mb-3" aria-hidden />
        <div className="text-[12.5px] font-medium text-destructive">XLSX 解析失败</div>
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
        // Output is HTML-escaped at the parser level (every cell goes through escapeHtml).
        dangerouslySetInnerHTML={{ __html: state.html }}
      />
    </div>
  )
}
