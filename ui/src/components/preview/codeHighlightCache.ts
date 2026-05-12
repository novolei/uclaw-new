/**
 * Code-highlight cache — Wave 1 of the Proma preview port.
 *
 * Pure LRU keyed by gitRoot + filePath + refreshVersion. Stores both raw
 * content and (optionally) the rendered Shiki HTML so W4's CodeRenderer can
 * skip both IPC and tokenization when the same file is re-previewed under
 * the same theme/language.
 */

export const CACHE_MAX = 50
export const MAX_HIGHLIGHT_CHARS = 200_000

export interface CacheEntry {
  oldContent: string
  newContent: string
  highlightedHtml?: string
  highlightedLanguage?: string
  highlightedTheme?: string
}

export interface CacheKeyParts {
  gitRoot: string | null
  filePath: string
  refreshVersion: number
}

const SEP = '\0'

const cache = new Map<string, CacheEntry>()

export function cacheKey(parts: CacheKeyParts): string {
  return `${parts.gitRoot ?? ''}${SEP}${parts.filePath}${SEP}v${parts.refreshVersion}`
}

export function cacheGet(key: string): CacheEntry | undefined {
  const entry = cache.get(key)
  if (entry === undefined) return undefined
  // promote to MRU
  cache.delete(key)
  cache.set(key, entry)
  return entry
}

export function cacheSet(key: string, entry: CacheEntry): void {
  if (cache.has(key)) {
    cache.delete(key)
  } else if (cache.size >= CACHE_MAX) {
    const oldest = cache.keys().next().value
    if (oldest !== undefined) cache.delete(oldest)
  }
  cache.set(key, entry)
}

export function shouldSkipHighlight(content: string): boolean {
  return content.length > MAX_HIGHLIGHT_CHARS
}

export function __resetCacheForTests(): void {
  cache.clear()
}
