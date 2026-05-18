import * as React from 'react'
import { cn } from '@/lib/utils'

interface ScreenshotResultRendererProps {
  result: string
  isError: boolean
}

/**
 * Renders browser_screenshot tool results as an actual preview image.
 * The result JSON is {"ok":true,"data":"<base64-png>","width":N,"height":N}.
 * Instead of showing the raw base64 string, display the image directly.
 */
export function ScreenshotResultRenderer({
  result,
  isError,
}: ScreenshotResultRendererProps): React.ReactElement {
  const parsed = React.useMemo(() => {
    try {
      const obj = JSON.parse(result) as Record<string, unknown>
      if (
        obj.ok === true &&
        typeof obj.data === 'string' &&
        typeof obj.width === 'number' &&
        typeof obj.height === 'number'
      ) {
        return { data: obj.data as string, width: obj.width as number, height: obj.height as number }
      }
    } catch {
      // not parseable — fall through
    }
    return null
  }, [result])

  if (isError || !parsed) {
    return (
      <pre className="whitespace-pre-wrap break-all text-xs px-3 py-2 rounded-md text-destructive bg-destructive/5">
        {result}
      </pre>
    )
  }

  return (
    <div className="space-y-1.5">
      <img
        src={`data:image/png;base64,${parsed.data}`}
        alt={`Screenshot ${parsed.width}×${parsed.height}`}
        className={cn(
          'w-full rounded-md border border-border/40',
          'object-contain max-h-[480px]',
        )}
        style={{ aspectRatio: `${parsed.width} / ${parsed.height}` }}
      />
      <p className="text-[11px] text-muted-foreground/60 text-right tabular-nums">
        {parsed.width} × {parsed.height}
      </p>
    </div>
  )
}
