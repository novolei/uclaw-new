/**
 * CodeRenderer — Renders source-code text with shiki syntax highlighting.
 *
 * Three rendering paths:
 *   1. Highlighted HTML (default path, shiki + cache)
 *   2. Plain pre/code while shiki is still tokenising (loading)
 *   3. Plain pre/code with truncation banner for files > MAX_HIGHLIGHT_CHARS
 */

import * as React from 'react'
import { cn } from '@/lib/utils'
import { useShikiHighlight } from '@/components/preview/hooks/useShikiHighlight'
import { escapeHtml } from '@/lib/highlight'

interface CodeRendererProps {
  /** Decoded file contents. */
  code: string
  /** Shiki language id (from ext-classifier). */
  language: string
  /** Cache scope key — usually the absolute path. */
  cacheScope: string
  /** Per-file refresh counter (forces re-highlight when bumped). */
  refreshVersion: number
  /** True if the upstream file is larger than MAX_PREVIEW_BYTES and was capped. */
  truncated?: boolean
}

export function CodeRenderer({
  code,
  language,
  cacheScope,
  refreshVersion,
  truncated = false,
}: CodeRendererProps): React.ReactElement {
  const { html, loading, skipped } = useShikiHighlight({
    code,
    language,
    cacheScope,
    refreshVersion,
  })

  const showPlain = skipped || (!html && !loading)

  return (
    <div className="flex flex-col h-full bg-popover">
      {truncated && (
        <div className="flex-shrink-0 px-3 py-1.5 text-[11px] bg-amber-500/12 text-amber-700 dark:text-amber-300 border-b border-border">
          文件超过 50 MB · 仅显示前 50 MB
        </div>
      )}
      {skipped && (
        <div className="flex-shrink-0 px-3 py-1.5 text-[11px] bg-muted/60 text-muted-foreground border-b border-border">
          文件较大 · 跳过语法高亮，仅显示纯文本
        </div>
      )}
      <div className="flex-1 min-h-0 overflow-auto">
        {html && !showPlain ? (
          <div
            className={cn(
              'p-3 text-[12px] font-mono tabular-nums leading-relaxed',
              loading && 'opacity-75',
            )}
            // shiki output is escaped + scoped — safe to dangerouslySetInnerHTML
            dangerouslySetInnerHTML={{ __html: html }}
          />
        ) : (
          <pre className="p-3 text-[12px] font-mono tabular-nums leading-relaxed whitespace-pre-wrap break-all">
            <code dangerouslySetInnerHTML={{ __html: escapeHtml(code) }} />
          </pre>
        )}
      </div>
    </div>
  )
}
