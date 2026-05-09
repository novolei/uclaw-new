/**
 * SearchPalette — global ⌘K command palette for finding conversations,
 * messages, and agent turns by full-text content.
 *
 * Mounts once at the app root. Toggle via Cmd/Ctrl+K. Wraps `cmdk` for
 * keyboard navigation + accessibility.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Command } from 'cmdk'
import { Search, MessageSquare, Bot, FileText } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { cn } from '@/lib/utils'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'

interface SearchResult {
  id: string
  title: string
  snippet: string
  source: 'conversation' | 'chat_message' | 'agent_turn' | 'file'
  sourceId: string
  messageId?: string
  createdAt: string
}

const DEBOUNCE_MS = 150

export interface SearchPaletteProps {
  /**
   * Called when the user picks a result. Caller is responsible for navigating
   * to the right tab/session. Passes the full result so the caller can decide
   * how to use messageId / source.
   */
  onSelect?: (result: SearchResult) => void
}

export function SearchPalette({ onSelect }: SearchPaletteProps): React.ReactElement | null {
  const [open, setOpen] = useAtom(searchPaletteOpenAtom)
  const [query, setQuery] = React.useState('')
  const [results, setResults] = React.useState<SearchResult[]>([])
  const [loading, setLoading] = React.useState(false)
  const debounceRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // Global ⌘K / Ctrl+K toggle
  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setOpen((v) => !v)
      }
      if (e.key === 'Escape' && open) {
        setOpen(false)
      }
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [open, setOpen])

  // Debounced search
  React.useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    if (!open || query.trim().length < 2) {
      setResults([])
      setLoading(false)
      return
    }
    setLoading(true)
    debounceRef.current = setTimeout(async () => {
      try {
        const raw = await invoke<SearchResult[]>('search_conversations', {
          input: { query: query.trim() },
        })
        setResults(raw ?? [])
      } catch (err) {
        console.error('[SearchPalette] search failed:', err)
        setResults([])
      } finally {
        setLoading(false)
      }
    }, DEBOUNCE_MS)
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [open, query])

  // Reset query when palette closes
  React.useEffect(() => {
    if (!open) setQuery('')
  }, [open])

  if (!open) return null

  const handleSelect = (r: SearchResult) => {
    setOpen(false)
    onSelect?.(r)
  }

  return (
    <div
      // Backdrop — clicking dismisses
      className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh] bg-black/30 backdrop-blur-sm"
      onClick={() => setOpen(false)}
    >
      <div
        // Stop click-through to backdrop
        onClick={(e) => e.stopPropagation()}
        className={cn(
          'w-full max-w-[640px] mx-4 rounded-xl border border-border/40 bg-popover',
          'shadow-[0_24px_48px_rgba(0,0,0,0.18)] dark:shadow-[0_24px_48px_rgba(0,0,0,0.5)]',
          'overflow-hidden',
        )}
      >
        {/* shouldFilter={false}: cmdk's built-in fuzzy filter operates on the
            item `value` prop (we use ids), which doesn't match the user's
            query. The backend already does the FTS filtering, so disable
            cmdk's client-side filter so all server-returned hits stay visible. */}
        <Command label="Global search" loop shouldFilter={false}>
          <div className="flex items-center gap-2 px-3.5 py-3 border-b border-border/40">
            <Search className="size-4 shrink-0 text-muted-foreground/60" />
            <Command.Input
              autoFocus
              placeholder="Search conversations, messages, tools..."
              value={query}
              onValueChange={setQuery}
              className="flex-1 bg-transparent outline-none text-[14px] text-foreground placeholder:text-muted-foreground/50"
            />
            {loading && (
              <span className="text-[11px] text-muted-foreground/60 tabular-nums">…</span>
            )}
          </div>
          <Command.List className="max-h-[420px] overflow-y-auto p-1.5 scrollbar-thin">
            {query.trim().length < 2 ? (
              <div className="py-8 text-center text-xs text-muted-foreground/70">
                Type to search across all conversations
              </div>
            ) : results.length === 0 && !loading ? (
              <Command.Empty className="py-8 text-center text-xs text-muted-foreground/70">
                No results
              </Command.Empty>
            ) : (
              results.map((r) => (
                <Command.Item
                  key={r.id}
                  value={r.id}
                  onSelect={() => handleSelect(r)}
                  className={cn(
                    'flex items-start gap-2.5 rounded-md px-2.5 py-2 cursor-pointer',
                    'text-[13px] text-foreground/80',
                    'data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground',
                    'transition-colors',
                  )}
                >
                  <ResultIcon source={r.source} />
                  <div className="flex-1 min-w-0">
                    <div className="truncate font-medium text-foreground/90">
                      {r.title || '(untitled)'}
                    </div>
                    <div
                      className="truncate text-[12px] text-muted-foreground/80"
                      // Snippet contains <b>...</b> markup from FTS5 snippet().
                      dangerouslySetInnerHTML={{ __html: r.snippet }}
                    />
                  </div>
                </Command.Item>
              ))
            )}
          </Command.List>
        </Command>
      </div>
    </div>
  )
}

function ResultIcon({ source }: { source: SearchResult['source'] }): React.ReactElement {
  const cls = 'size-4 shrink-0 mt-0.5 text-muted-foreground/65'
  if (source === 'conversation') return <MessageSquare className={cls} />
  if (source === 'agent_turn') return <Bot className={cls} />
  if (source === 'file') return <FileText className={cls} />
  return <MessageSquare className={cls} />
}
