/**
 * CodeRenderer — Renders source-code text with shiki syntax highlighting.
 *
 * Layout: line-number gutter on the left, content on the right.
 * Wrapping: long lines wrap by default (matches the user's expected reading
 * experience inside a narrow split-panel). The `wrap` state can be toggled
 * via the header in a later polish wave (W4c+); for now, always-wrap is the
 * sensible default for a previewer.
 *
 * Three rendering paths:
 *   1. Highlighted HTML (default — shiki output + W1 cache)
 *   2. Plain text fallback (shiki still tokenizing, or `skipped` for 200k+ files)
 *   3. Plain text + amber "truncated" banner for files > MAX_PREVIEW_BYTES
 */

import * as React from 'react'
import { cn } from '@/lib/utils'
import { AlertCircle, Sparkles } from 'lucide-react'
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

function NoticeBanner({
  kind,
  icon: Icon,
  children,
}: {
  kind: 'warn' | 'info'
  icon: typeof AlertCircle
  children: React.ReactNode
}): React.ReactElement {
  const palette =
    kind === 'warn'
      ? 'bg-amber-500/10 text-amber-800 dark:text-amber-200 border-amber-500/20'
      : 'bg-muted text-muted-foreground border-border'
  return (
    <div
      className={cn(
        'flex-shrink-0 flex items-center gap-1.5 px-3 py-1.5',
        'text-[11px] border-b',
        palette,
      )}
      role="status"
    >
      <Icon size={11} aria-hidden className="shrink-0" />
      <span className="truncate">{children}</span>
    </div>
  )
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

  // Stable line count — used for the gutter. Done once per code change.
  const lineCount = React.useMemo(() => {
    if (!code) return 0
    let n = 1
    for (let i = 0; i < code.length; i++) {
      if (code.charCodeAt(i) === 10 /* \n */) n++
    }
    // Trailing newline shouldn't produce an empty extra line in the gutter.
    return code.endsWith('\n') ? n - 1 : n
  }, [code])

  const gutter = React.useMemo(() => {
    if (lineCount === 0) return null
    const lines: number[] = []
    for (let i = 1; i <= lineCount; i++) lines.push(i)
    return lines
  }, [lineCount])

  return (
    <div className="flex flex-col h-full bg-popover">
      {truncated && (
        <NoticeBanner kind="warn" icon={AlertCircle}>
          文件超过 50 MB · 仅显示前 50 MB
        </NoticeBanner>
      )}
      {skipped && (
        <NoticeBanner kind="info" icon={Sparkles}>
          文件较大 · 跳过语法高亮，仅显示纯文本
        </NoticeBanner>
      )}

      <div className="flex-1 min-h-0 overflow-auto preview-code-scroll">
        <div className="flex min-w-0">
          {gutter && (
            <div
              aria-hidden
              className={cn(
                'flex-shrink-0 select-none text-right',
                'pl-3 pr-3 py-3',
                'text-[11px] font-mono tabular-nums leading-[1.65]',
                'text-muted-foreground/40',
                'bg-muted/30 border-r border-border/40',
                'sticky left-0',
              )}
            >
              {gutter.map((n) => (
                <div key={n} className="whitespace-pre">
                  {n}
                </div>
              ))}
            </div>
          )}

          <div
            className={cn(
              'flex-1 min-w-0 py-3 px-4',
              'text-[12px] font-mono leading-[1.65]',
              // Force shiki's <pre> + <code> to wrap and not blow out the panel.
              '[&_pre]:!whitespace-pre-wrap [&_pre]:!break-words',
              '[&_pre]:!bg-transparent [&_pre]:!m-0 [&_pre]:!p-0',
              '[&_code]:!whitespace-pre-wrap [&_code]:!break-words',
              '[&_code]:!bg-transparent [&_code]:!p-0',
              '[&_.line]:!break-words',
              loading && 'opacity-75',
            )}
          >
            {html && !showPlain ? (
              // shiki output is HTML-escaped at the token level — safe.
              <div dangerouslySetInnerHTML={{ __html: html }} />
            ) : (
              <pre className="!whitespace-pre-wrap !break-words">
                <code dangerouslySetInnerHTML={{ __html: escapeHtml(code) }} />
              </pre>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
