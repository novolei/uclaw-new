/**
 * DiffRenderer — side-by-side diff with hunk-collapse and density bar.
 *
 * 2-column layout (left = old, right = new). Each side renders one
 * DiffLineRow per logical line, including blank placeholders for the
 * other side's add/del. Unchanged regions between hunks become
 * expandable gap markers; "show full" toggle re-builds with context=∞.
 *
 * Truncation: cap at 5000 rendered lines; banner explains the cap.
 */

import * as React from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useDiffHunks, gapLineCount, type DiffLine } from './useDiffHunks'
import { DiffLineRow } from './DiffLineRow'
import { DiffDensityCells } from './DiffDensityCells'

interface Props {
  left: { content: string; label: string }
  right: { content: string; label: string }
  /** shiki language id, currently unused but reserved for syntax tint. */
  language?: string
}

const MAX_RENDER_LINES = 5000

type RenderItem =
  | { kind: 'line'; line: DiffLine; key: string }
  | { kind: 'gap'; count: number; key: string }

export function DiffRenderer({ left, right, language: _language }: Props): React.ReactElement {
  const [showFull, setShowFull] = React.useState(false)
  const [expandedGaps, setExpandedGaps] = React.useState<Set<string>>(new Set())

  const { hunks, totals, isFreshFile, fullLines } = useDiffHunks({
    oldContent: left.content,
    newContent: right.content,
    showFull,
  })

  // Flatten to render-ready items, inserting gap markers between hunks.
  const renderItems = React.useMemo<RenderItem[]>(() => {
    if (isFreshFile && fullLines) {
      return fullLines.slice(0, MAX_RENDER_LINES).map((line, i) => ({
        kind: 'line' as const,
        line,
        key: `fresh-${i}`,
      }))
    }
    const items: RenderItem[] = []
    let lineCount = 0
    for (let hi = 0; hi < hunks.length; hi++) {
      const hunk = hunks[hi]!
      if (hi > 0) {
        const gap = gapLineCount(hunks[hi - 1]!.hunk, hunk.hunk)
        if (gap > 0) {
          items.push({ kind: 'gap', count: gap, key: `gap-${hi}` })
        }
      }
      for (let li = 0; li < hunk.lines.length; li++) {
        if (lineCount >= MAX_RENDER_LINES) break
        items.push({ kind: 'line', line: hunk.lines[li]!, key: `h${hi}-l${li}` })
        lineCount += 1
      }
      if (lineCount >= MAX_RENDER_LINES) break
    }
    return items
  }, [hunks, fullLines, isFreshFile])

  const truncated = renderItems.length === MAX_RENDER_LINES

  return (
    <div className="flex flex-col h-full bg-popover">
      {/* Header */}
      <div className="flex-shrink-0 border-b border-border">
        <div className="flex items-center justify-between px-3 py-1.5 text-[11.5px]">
          <div className="flex items-center gap-3 truncate">
            <span className="text-muted-foreground">{left.label}</span>
            <span className="text-muted-foreground">→</span>
            <span className="font-medium">{right.label}</span>
          </div>
          <button
            type="button"
            onClick={() => setShowFull((f) => !f)}
            className="rounded px-2 py-0.5 text-muted-foreground hover:text-foreground hover:bg-foreground/[0.06]"
          >
            {showFull ? '折叠未变更' : '展开全部'}
          </button>
        </div>
        <DiffDensityCells totals={totals} />
      </div>

      {truncated && (
        <div className="flex-shrink-0 px-3 py-1.5 text-[11px] bg-amber-500/12 text-amber-700 dark:text-amber-300 border-b border-border">
          差异超过 {MAX_RENDER_LINES} 行 · 仅显示前 {MAX_RENDER_LINES} 行
        </div>
      )}

      {/* 2-column body */}
      <div className="flex-1 min-h-0 overflow-auto">
        <div className="grid grid-cols-2 gap-x-2">
          <div className="border-r border-border">
            {renderItems.map((item) =>
              item.kind === 'gap' ? (
                <GapMarker
                  key={item.key + '-l'}
                  count={item.count}
                  expanded={expandedGaps.has(item.key)}
                  onToggle={() =>
                    setExpandedGaps((s) => {
                      const n = new Set(s)
                      n.has(item.key) ? n.delete(item.key) : n.add(item.key)
                      return n
                    })
                  }
                />
              ) : (
                <DiffLineRow key={item.key + '-l'} line={item.line} column="left" />
              ),
            )}
          </div>
          <div>
            {renderItems.map((item) =>
              item.kind === 'gap' ? (
                <GapMarker
                  key={item.key + '-r'}
                  count={item.count}
                  expanded={expandedGaps.has(item.key)}
                  onToggle={() =>
                    setExpandedGaps((s) => {
                      const n = new Set(s)
                      n.has(item.key) ? n.delete(item.key) : n.add(item.key)
                      return n
                    })
                  }
                />
              ) : (
                <DiffLineRow key={item.key + '-r'} line={item.line} column="right" />
              ),
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

function GapMarker({ count, expanded, onToggle }: { count: number; expanded: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className={cn(
        'flex items-center gap-1 w-full h-[18px] px-2 text-[11px] text-muted-foreground',
        'bg-muted/30 hover:bg-muted/50 transition-colors',
      )}
    >
      {expanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
      <span>未变更 {count} 行</span>
    </button>
  )
}
