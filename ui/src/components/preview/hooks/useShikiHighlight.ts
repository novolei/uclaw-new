/**
 * useShikiHighlight — Highlight a code string with shiki, caching the HTML
 * via the W1 codeHighlightCache so repeat previews skip both shiki and React
 * re-render churn.
 *
 * Skips highlighting entirely for files larger than MAX_HIGHLIGHT_CHARS (200k).
 */

import * as React from 'react'
import { highlightCode, getShikiThemeForCurrentApp } from '@/lib/highlight'
import {
  cacheGet,
  cacheKey,
  cacheSet,
  shouldSkipHighlight,
} from '@/components/preview/codeHighlightCache'

export interface UseShikiHighlightArgs {
  code: string
  language: string
  /** Used as part of the cache key — usually a filePath or mountId:relPath. */
  cacheScope: string
  /** Per-file refresh counter from usePreviewRefresh. */
  refreshVersion: number
}

export interface ShikiHighlightState {
  /** True when highlight is in flight. Code can still be shown as plaintext. */
  loading: boolean
  /** Sanitized HTML, or null if not yet highlighted (or skipped for size). */
  html: string | null
  /** True if the file exceeded the size cap and we are NOT highlighting it. */
  skipped: boolean
}

export function useShikiHighlight({
  code,
  language,
  cacheScope,
  refreshVersion,
}: UseShikiHighlightArgs): ShikiHighlightState {
  const [html, setHtml] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  const skipped = React.useMemo(() => shouldSkipHighlight(code), [code])

  React.useEffect(() => {
    if (skipped) {
      setHtml(null)
      setLoading(false)
      return
    }
    if (!code) {
      setHtml(null)
      setLoading(false)
      return
    }
    const theme = getShikiThemeForCurrentApp()
    const key = cacheKey({
      gitRoot: null,
      filePath: cacheScope,
      refreshVersion,
    })
    const cached = cacheGet(key)
    if (
      cached?.highlightedHtml &&
      cached.highlightedLanguage === language &&
      cached.highlightedTheme === theme
    ) {
      setHtml(cached.highlightedHtml)
      setLoading(false)
      return
    }
    let cancelled = false
    setLoading(true)
    void (async () => {
      try {
        const result = await highlightCode(code, language)
        if (cancelled) return
        setHtml(result)
        setLoading(false)
        cacheSet(key, {
          oldContent: code,
          newContent: code,
          highlightedHtml: result,
          highlightedLanguage: language,
          highlightedTheme: theme,
        })
      } catch {
        if (!cancelled) {
          setHtml(null)
          setLoading(false)
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [code, language, cacheScope, refreshVersion, skipped])

  return { loading, html, skipped }
}
