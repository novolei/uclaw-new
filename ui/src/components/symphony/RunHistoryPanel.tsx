/**
 * RunHistoryPanel — per-workflow run rollup, rendered as the right sidebar
 * of the Run sub-view. Click a row to load that run into the canvas.
 */

import * as React from 'react'
import { cn } from '@/lib/utils'
import type { SymphonyRunRow, SymphonyRunStatus } from '@/lib/tauri-bridge'
import { CheckCircle, Clock, Loader, Square, XCircle } from 'lucide-react'

export interface RunHistoryPanelProps {
  workflowId: string
  runs: SymphonyRunRow[]
  currentRunId: string | null
  onSelect: (runId: string) => void
}

// Icon colors use theme tokens only (no raw `text-green-500` / `text-amber-500`
// per CLAUDE.md Part 1). `bg-primary` carries the success semantic in our
// themes; `bg-destructive` carries failure.
const STATUS_ICON: Record<SymphonyRunStatus, React.ReactNode> = {
  queued: <Clock size={12} className="text-muted-foreground" />,
  running: <Loader size={12} className="animate-spin text-primary" />,
  completed: <CheckCircle size={12} className="text-primary" />,
  failed: <XCircle size={12} className="text-destructive" />,
  cancelled: <Square size={12} className="text-muted-foreground" />,
  quota_exceeded: <XCircle size={12} className="text-destructive" />,
}

function relativeAge(ms: number): string {
  const diff = Date.now() - ms
  if (diff < 60_000) return `${Math.round(diff / 1000)}s ago`
  if (diff < 3_600_000) return `${Math.round(diff / 60_000)}m ago`
  if (diff < 86_400_000) return `${Math.round(diff / 3_600_000)}h ago`
  return `${Math.round(diff / 86_400_000)}d ago`
}

export function RunHistoryPanel({
  workflowId: _workflowId,
  runs,
  currentRunId,
  onSelect,
}: RunHistoryPanelProps): React.ReactElement {
  // `relativeAge` and the running-duration display read from Date.now(), so
  // without a re-render tick the strings freeze at first render. Tick every
  // 5s only while any run is in flight — for fully terminal lists we don't
  // bother (the strings are stable past ~1 minute boundaries anyway).
  const [, forceTick] = React.useReducer((n: number) => n + 1, 0)
  React.useEffect(() => {
    const hasInFlight = runs.some(
      (r) => r.status === 'queued' || r.status === 'running',
    )
    if (!hasInFlight) return
    const id = window.setInterval(() => forceTick(), 5_000)
    return () => window.clearInterval(id)
  }, [runs])

  return (
    <div className="flex h-full flex-col bg-card">
      <div className="border-b border-border px-3 py-2">
        <h3 className="text-xs font-semibold uppercase text-muted-foreground">
          Recent runs
        </h3>
      </div>
      <div className="flex-1 overflow-y-auto">
        {runs.length === 0 ? (
          <div className="px-3 py-4 text-xs text-muted-foreground">
            No runs yet. Press Run to start one.
          </div>
        ) : (
          runs.map((r) => {
            const duration =
              r.completedAt && r.startedAt
                ? `${((r.completedAt - r.startedAt) / 1000).toFixed(1)}s`
                : r.startedAt
                  ? `${((Date.now() - r.startedAt) / 1000).toFixed(1)}s…`
                  : ''
            return (
              <button
                key={r.id}
                onClick={() => onSelect(r.id)}
                className={cn(
                  'flex w-full items-start gap-2 border-b border-border/40 px-3 py-2 text-left transition-colors hover:bg-accent/40',
                  currentRunId === r.id && 'bg-accent/60',
                )}
              >
                <div className="mt-0.5">{STATUS_ICON[r.status]}</div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-[11px] font-medium">
                      {r.id.slice(0, 8)}
                    </span>
                    <span className="text-[10px] text-muted-foreground">
                      {relativeAge(r.queuedAt)}
                    </span>
                  </div>
                  <div className="mt-0.5 flex items-center gap-2 text-[10px] text-muted-foreground">
                    {duration && <span>{duration}</span>}
                    {r.totalCostUsd > 0 && (
                      <span>${r.totalCostUsd.toFixed(4)}</span>
                    )}
                    {r.outcome && (
                      <span className="rounded bg-muted px-1">{r.outcome}</span>
                    )}
                  </div>
                </div>
              </button>
            )
          })
        )}
      </div>
    </div>
  )
}
