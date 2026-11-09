import * as React from 'react'
import { useAtomValue } from 'jotai'
import { ChevronRight, FileText } from 'lucide-react'
import { cn } from '@/lib/utils'
import { agentMessageRefreshAtom, currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { pathBaseName, useOpenSessionFile } from '@/hooks/useOpenSessionFile'
import { getSessionFileWorkingSet, onFileWorkingSetChanged } from '@/lib/tauri-bridge'
import type { FileWorkingSet, ModifiedFile, ReadFile } from '@/lib/types'
import { ChangeRow, type FileChangeBadge } from './ChangeRow'

/**
 * FileChangesPanel — the session's persisted file working set
 * (context continuity P2): files the agent modified, files it only read,
 * and the active plan. Backed by `get_session_file_working_set`; refreshed
 * on mount, on `agent:file_working_set_changed` for this session, and when
 * the session's `agentMessageRefreshAtom` version bumps.
 */

interface FileChangesPanelProps {
  /** Session to show. Falls back to the current agent session when omitted. */
  sessionId?: string | null
}

function badgeFor(f: ModifiedFile): FileChangeBadge {
  if (f.deleted) return 'removed'
  if (f.created) return 'created'
  return 'modified'
}

function isEmpty(ws: FileWorkingSet | null): boolean {
  return !ws || (ws.modified.length === 0 && ws.read.length === 0 && !ws.activePlan)
}

export function FileChangesPanel({ sessionId: sessionIdProp }: FileChangesPanelProps): React.ReactElement {
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const sessionId = sessionIdProp === undefined ? currentSessionId : sessionIdProp
  const refreshVersion = useAtomValue(agentMessageRefreshAtom).get(sessionId ?? '') ?? 0

  const [workingSet, setWorkingSet] = React.useState<FileWorkingSet | null>(null)
  const [loadFailed, setLoadFailed] = React.useState(false)
  const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set())
  const [readOpen, setReadOpen] = React.useState(false)

  // Monotonic request id — drops stale responses after a session switch or
  // when several refreshes race.
  const requestSeq = React.useRef(0)
  const fetchWorkingSet = React.useCallback(() => {
    const seq = ++requestSeq.current
    if (!sessionId) {
      setWorkingSet(null)
      return
    }
    getSessionFileWorkingSet(sessionId)
      .then((ws) => {
        if (requestSeq.current !== seq) return
        setWorkingSet(ws)
        setLoadFailed(false)
      })
      .catch((e) => {
        if (requestSeq.current !== seq) return
        console.warn('[FileChangesPanel] failed to load file working set', e)
        setWorkingSet(null)
        setLoadFailed(true)
      })
  }, [sessionId])

  // Mount / session switch / refresh-version bump.
  React.useEffect(() => {
    fetchWorkingSet()
  }, [fetchWorkingSet, refreshVersion])

  // Reset per-session state on session switch — drop the previous session's
  // working set at once so its paths aren't shown (or clickable) while the
  // new one loads.
  React.useEffect(() => {
    setWorkingSet(null)
    setLoadFailed(false)
    setExpanded(new Set())
    setReadOpen(false)
  }, [sessionId])

  React.useEffect(() => {
    if (!sessionId) return
    let disposed = false
    let unlisten: (() => void) | null = null
    void onFileWorkingSetChanged((payload) => {
      if (payload.sessionId === sessionId) fetchWorkingSet()
    }).then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [sessionId, fetchWorkingSet])

  const openPath = useOpenSessionFile(sessionId)

  const toggleExpanded = React.useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }, [])

  const modified = React.useMemo(
    () => [...(workingSet?.modified ?? [])].sort((a, b) => b.lastModifiedAt - a.lastModifiedAt),
    [workingSet],
  )

  if (loadFailed) {
    return (
      <div className="flex flex-col items-center justify-center flex-1 min-h-[120px] p-6 text-center">
        <div className="text-[12px] text-muted-foreground">加载文件改动失败</div>
        <button
          type="button"
          onClick={fetchWorkingSet}
          className={cn(
            'mt-2 px-2 h-[22px] rounded text-[11px] text-foreground/80 bg-foreground/[0.05]',
            'hover:bg-foreground/[0.08] transition-colors',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          )}
        >
          重试
        </button>
      </div>
    )
  }

  if (isEmpty(workingSet)) {
    return (
      <div className="flex flex-col items-center justify-center flex-1 min-h-[120px] p-6 text-center">
        <div className="text-[12px] text-muted-foreground">
          这个会话还没有文件改动
        </div>
        <div className="mt-1 text-[11px] text-muted-foreground/60">
          Agent 写入或修改文件后会出现在这里
        </div>
      </div>
    )
  }

  const read = workingSet?.read ?? []
  const activePlan = workingSet?.activePlan ?? null

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-y-auto py-1">
      {activePlan && (
        <button
          type="button"
          onClick={() => openPath(activePlan)}
          title={activePlan}
          className={cn(
            'flex items-center w-full h-[26px] px-3 gap-2 text-[12px] text-left',
            'text-foreground/85 hover:bg-foreground/[0.04] transition-colors',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          )}
        >
          <FileText size={12} className="shrink-0 text-foreground/60" />
          <span className="truncate">当前计划：{pathBaseName(activePlan)}</span>
        </button>
      )}

      {modified.length > 0 && (
        <section className="mt-1">
          <SectionTitle>已修改 · {modified.length}</SectionTitle>
          {modified.map((f) => (
            <ModifiedRow
              key={f.path}
              file={f}
              expanded={expanded.has(f.path)}
              onToggle={() => toggleExpanded(f.path)}
              onOpen={() => openPath(f.path)}
            />
          ))}
        </section>
      )}

      {read.length > 0 && (
        <section className="mt-1">
          <button
            type="button"
            aria-expanded={readOpen}
            onClick={() => setReadOpen((v) => !v)}
            className={cn(
              'flex items-center w-full h-[24px] px-3 gap-1 text-[11px] font-medium text-muted-foreground',
              'hover:text-foreground/80 transition-colors',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
            )}
          >
            <ChevronRight size={11} className={cn('shrink-0 transition-transform', readOpen && 'rotate-90')} />
            <span>读过 · {read.length}</span>
          </button>
          {readOpen && read.map((f) => (
            <ReadRow key={f.path} file={f} onOpen={() => openPath(f.path)} />
          ))}
        </section>
      )}
    </div>
  )
}

function SectionTitle({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex items-center h-[24px] px-3 text-[11px] font-medium text-muted-foreground">
      {children}
    </div>
  )
}

interface ModifiedRowProps {
  file: ModifiedFile
  expanded: boolean
  onToggle: () => void
  onOpen: () => void
}

function ModifiedRow({ file, expanded, onToggle, onOpen }: ModifiedRowProps): React.ReactElement {
  return (
    <div data-testid="modified-file-row" data-path={file.path}>
      {/* Clicking anywhere on the row toggles the summary. The path button
          stops propagation (it opens the preview instead); the count button
          has no handler of its own — its click (incl. keyboard Enter/Space)
          bubbles here, so it toggles exactly once. */}
      <div className="flex items-center min-w-0 cursor-pointer" onClick={onToggle}>
        <div className="flex-1 min-w-0">
          <ChangeRow
            badge={badgeFor(file)}
            path={file.path}
            disabled={file.deleted}
            onClick={(e) => {
              e.stopPropagation()
              onOpen()
            }}
          />
        </div>
        {file.created && !file.deleted && <Tag className="text-[hsl(var(--success))]">新建</Tag>}
        {file.deleted && <Tag className="text-destructive">已删除</Tag>}
        <button
          type="button"
          aria-expanded={expanded}
          className={cn(
            'shrink-0 flex items-center gap-0.5 pr-3 pl-1 h-[26px] text-[11px] text-muted-foreground tabular-nums',
            'hover:text-foreground/80 transition-colors',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          )}
        >
          <span>修改 {file.editCount} 次</span>
          <ChevronRight size={11} className={cn('transition-transform', expanded && 'rotate-90')} />
        </button>
      </div>
      {file.rewound && (
        <div className="pl-8 pr-3 pb-0.5">
          <span className="inline-block rounded px-1.5 py-px text-[10.5px] bg-muted text-muted-foreground">
            已回退的轮次 · 磁盘仍保留
          </span>
        </div>
      )}
      {expanded && file.changes.length > 0 && (
        <ul className="pl-8 pr-3 pb-1 space-y-0.5">
          {file.changes.map((c, idx) => (
            <li key={idx} className="text-[11px] text-foreground/70 break-words">
              {c}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function Tag({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <span className={cn('shrink-0 px-1 text-[10.5px]', className)}>{children}</span>
}

function ReadRow({ file, onOpen }: { file: ReadFile; onOpen: () => void }): React.ReactElement {
  return (
    <button
      type="button"
      onClick={onOpen}
      title={file.path}
      className={cn(
        'flex items-center w-full h-[24px] pl-7 pr-3 gap-2 text-[12px] text-left',
        'text-foreground/75 hover:bg-foreground/[0.04] transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
      )}
    >
      <span className="truncate font-mono flex-1 min-w-0" dir="rtl">{file.path}</span>
      {file.ranges.length > 0 && (
        <span className="shrink-0 text-[11px] text-muted-foreground tabular-nums">
          {file.ranges.join(', ')}
        </span>
      )}
    </button>
  )
}
