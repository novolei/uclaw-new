/**
 * CompactionFoldCard — renders the compaction fold row ("## Earlier
 * conversation (compacted)") as ONE collapsible card instead of a regular
 * user bubble (the earlier divider + Markdown bubble read as "two
 * compactions").
 *
 * Since context continuity P2 the row is the whole session block: fold
 * summary + `<file_working_set>` + `<current_plan>` + `<recent_files>`. The
 * tags are parsed by `parseSessionBlock` and shown as friendly UI (file chips,
 * plan, reloaded files) — raw XML never reaches the user.
 */

import * as React from 'react'
import { Brain, ChevronDown, ChevronRight, FileText } from 'lucide-react'
import { cn } from '@/lib/utils'
import { MessageResponse } from '@/components/ai-elements/message'
import { parseSessionBlock } from '@/lib/session-block'
import { useOpenSessionFile } from '@/hooks/useOpenSessionFile'

/** 相对时间戳 — 简化显示，如 "2m ago" / "刚刚" */
export function formatRelativeShort(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000)
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return new Date(ts).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' })
}

export function CompactionFoldCard({
  markdown,
  createdAt,
  sessionId,
}: {
  markdown: string
  createdAt: number
  sessionId?: string | null
}): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  const openFile = useOpenSessionFile(sessionId)

  const parsed = React.useMemo(() => parseSessionBlock(markdown), [markdown])
  const keptModified = parsed.modifiedFiles.length + parsed.modifiedOmittedCount

  // Section count — `### ` headings of the fold summary only (reloaded file
  // bodies may contain their own Markdown headings).
  const sectionCount = React.useMemo(() => {
    const matches = parsed.summary.match(/^###\s+/gm)
    return matches?.length ?? 0
  }, [parsed.summary])

  // Strip the leading `## Earlier conversation (compacted)\n` header since we
  // render our own header in the card chrome.
  const body = React.useMemo(() => {
    return parsed.summary.replace(/^##\s+Earlier conversation \(compacted\)\s*\n+/, '')
  }, [parsed.summary])

  return (
    <div
      className={cn(
        'flex items-center gap-3 my-4 px-1',
        expanded && 'flex-col items-stretch',
      )}
    >
      {!expanded && (
        <>
          <div className="flex-1 h-px bg-border/40" />
          <button
            type="button"
            onClick={() => setExpanded(true)}
            className={cn(
              'shrink-0 inline-flex items-center gap-1.5',
              'rounded-full border border-border/40 bg-muted/30',
              'px-2.5 py-0.5 text-[11px] text-muted-foreground/75',
              'hover:bg-muted/50 hover:text-foreground transition-colors',
            )}
          >
            <Brain className="size-3" />
            {keptModified > 0 ? (
              <span>{`上下文已压缩 · 保留了 ${keptModified} 个改动文件的记录`}</span>
            ) : (
              <>
                <span>上下文已压缩为结构化摘要</span>
                {sectionCount > 0 && (
                  <span className="text-[10px] text-muted-foreground/45">
                    · {sectionCount} 个分项
                  </span>
                )}
              </>
            )}
            <ChevronRight className="size-3 opacity-60" />
          </button>
          <div className="flex-1 h-px bg-border/40" />
        </>
      )}
      {expanded && (
        <div
          className={cn(
            'rounded-xl border border-border/50 bg-muted/20',
            'overflow-hidden',
          )}
        >
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className={cn(
              'flex w-full items-center gap-2 px-3 py-2',
              'border-b border-border/40 bg-muted/30',
              'text-left text-[11px] text-muted-foreground/85',
              'hover:bg-muted/50 transition-colors',
            )}
          >
            <Brain className="size-3.5 text-emerald-500/85" />
            <span className="flex-1 font-medium">上下文摘要</span>
            <span className="text-[10px] text-muted-foreground/55">
              {sectionCount} 个分项 · {formatRelativeShort(createdAt)}
            </span>
            <ChevronDown className="size-3.5 opacity-60" />
          </button>
          <div className="px-4 py-3 text-[12.5px] text-foreground/85">
            <MessageResponse sessionId={null}>{body}</MessageResponse>
          </div>
          {(keptModified > 0 || parsed.plan || parsed.reloadedFiles.length > 0) && (
            <div className="space-y-2 border-t border-border/40 px-4 py-3 text-[11.5px] text-muted-foreground/85">
              {parsed.modifiedFiles.length > 0 && (
                <div>
                  <div className="mb-1.5 font-medium text-foreground/70">改动过的文件</div>
                  <div className="flex flex-wrap gap-1.5">
                    {parsed.modifiedFiles.map((path) => (
                      <button
                        key={path}
                        type="button"
                        title={path}
                        onClick={() => openFile(path)}
                        className={cn(
                          'inline-flex max-w-[260px] items-center gap-1 rounded-md',
                          'bg-muted/60 px-2 py-0.5 text-[11.5px] text-muted-foreground',
                          'hover:bg-muted hover:text-foreground transition-colors',
                        )}
                      >
                        <FileText className="size-3 shrink-0" aria-hidden />
                        <span className="truncate">{path}</span>
                      </button>
                    ))}
                    {parsed.modifiedOmittedCount > 0 && (
                      <span className="px-1 py-0.5 text-muted-foreground/60">
                        {`另有 ${parsed.modifiedOmittedCount} 个文件`}
                      </span>
                    )}
                  </div>
                </div>
              )}
              {parsed.plan && <div className="truncate">{`当前计划：${parsed.plan}`}</div>}
              {parsed.reloadedFiles.length > 0 && (
                <div className="truncate">{`已重新载入：${parsed.reloadedFiles.join('、')}`}</div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
