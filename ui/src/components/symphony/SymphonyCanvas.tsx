/**
 * SymphonyCanvas — top-level view for one workflow.
 *
 * Three sub-tabs (Design / Run / Raw) controlled by `symphonySubViewAtom`,
 * shared workflow data via the `symphony*Atom` family. IPC subscriptions
 * convert backend events into atom writes (`applyNodeUpdateAtom`,
 * `upsertRunAtom`, `finalizeRunAtom`).
 *
 * Theme tokens only — never hardcoded colors per CLAUDE.md Part 1.
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import {
  symphonyGetWorkflow,
  symphonyListRuns,
  symphonyListWorkflows,
  symphonyTriggerRun,
  symphonyCancelRun,
  type SymphonyNodeLogEvent,
  type SymphonyNodeUpdateEvent,
  type SymphonyRunCompletedEvent,
  type SymphonyRunRow,
  type SymphonyRunStartedEvent,
} from '@/lib/tauri-bridge'
import {
  applyNodeUpdateAtom,
  currentSymphonyRunIdAtom,
  currentSymphonyWorkflowIdAtom,
  finalizeRunAtom,
  symphonyNodeRunsByRunAtom,
  symphonyRunsByWorkflowAtom,
  symphonyWorkflowDetailsAtom,
  symphonyWorkflowsAtom,
  upsertRunAtom,
} from '@/atoms/symphony'
import { symphonySubViewAtom } from '@/atoms/symphony-canvas'
import { cn } from '@/lib/utils'
import { Play, Square, FileCode, GitBranch, Activity } from 'lucide-react'

import { WorkflowCanvas } from './canvas/WorkflowCanvas'
import { WorkflowMarkdownEditor } from './WorkflowMarkdownEditor'
import { RunHistoryPanel } from './RunHistoryPanel'

export interface SymphonyCanvasProps {
  workflowId: string
}

const SUBVIEW_LABELS: Record<'design' | 'run' | 'raw', string> = {
  design: 'Design',
  run: 'Run',
  raw: 'Raw',
}

const SUBVIEW_ICONS: Record<'design' | 'run' | 'raw', React.ReactNode> = {
  design: <GitBranch size={13} />,
  run: <Activity size={13} />,
  raw: <FileCode size={13} />,
}

export function SymphonyCanvas({
  workflowId,
}: SymphonyCanvasProps): React.ReactElement {
  const [subView, setSubView] = useAtom(symphonySubViewAtom)
  const setCurrentWorkflow = useSetAtom(currentSymphonyWorkflowIdAtom)
  const [details, setDetails] = useAtom(symphonyWorkflowDetailsAtom)
  const [workflows, setWorkflows] = useAtom(symphonyWorkflowsAtom)
  const [runs, setRuns] = useAtom(symphonyRunsByWorkflowAtom)
  const nodeRunsByRun = useAtomValue(symphonyNodeRunsByRunAtom)
  const [currentRunId, setCurrentRunId] = useAtom(currentSymphonyRunIdAtom)
  const applyNodeUpdate = useSetAtom(applyNodeUpdateAtom)
  const upsertRun = useSetAtom(upsertRunAtom)
  const finalizeRun = useSetAtom(finalizeRunAtom)

  const detail = details[workflowId]
  const workflowRuns = runs[workflowId] ?? []

  // Mark current workflow on mount + when the prop changes.
  React.useEffect(() => {
    setCurrentWorkflow(workflowId)
  }, [workflowId, setCurrentWorkflow])

  // Initial data load.
  React.useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const [list, d, r] = await Promise.all([
          symphonyListWorkflows(),
          symphonyGetWorkflow(workflowId),
          symphonyListRuns(workflowId),
        ])
        if (cancelled) return
        setWorkflows(list)
        setDetails((prev) => ({ ...prev, [workflowId]: d }))
        setRuns((prev) => ({ ...prev, [workflowId]: r }))
      } catch (e) {
        // SymphonyCanvas tolerates a missing workflow during initial load —
        // the empty state below renders an "import or create" prompt.
        console.warn('[symphony] initial fetch failed:', e)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [workflowId, setWorkflows, setDetails, setRuns])

  // IPC subscriptions. Symmetric with AgentView's listen pattern.
  React.useEffect(() => {
    const handles: Array<Promise<UnlistenFn>> = []
    handles.push(
      listen<SymphonyNodeUpdateEvent>('symphony:node_update', (ev) =>
        applyNodeUpdate(ev.payload),
      ),
    )
    handles.push(
      listen<SymphonyNodeLogEvent>('symphony:node_log', (_ev) => {
        // Per-node log streaming is rendered by NodeCard's inline drawer (T+).
        // For now the canvas just keeps the heartbeat alive; the atom write
        // would land here if we tracked partial-text per-node.
      }),
    )
    handles.push(
      listen<SymphonyRunStartedEvent>('symphony:run_started', (ev) => {
        upsertRun({
          id: ev.payload.runId,
          workflowId: ev.payload.workflowId,
          workflowVersion: detail?.summary.currentVersion ?? 1,
          status: 'running',
          outcome: null,
          totalCostUsd: 0,
          queuedAt: ev.payload.startedAt,
          startedAt: ev.payload.startedAt,
          completedAt: null,
        } satisfies SymphonyRunRow)
        setCurrentRunId(ev.payload.runId)
      }),
    )
    handles.push(
      listen<SymphonyRunCompletedEvent>('symphony:run_completed', (ev) => {
        finalizeRun({
          runId: ev.payload.runId,
          status: ev.payload.status,
          totalCostUsd: ev.payload.totalCostUsd,
        })
      }),
    )
    return () => {
      handles.forEach((p) => {
        p.then((fn) => fn()).catch(() => {})
      })
    }
  }, [applyNodeUpdate, upsertRun, finalizeRun, detail, setCurrentRunId])

  const handleRun = React.useCallback(async () => {
    try {
      const runId = await symphonyTriggerRun(workflowId)
      setCurrentRunId(runId)
      setSubView('run')
    } catch (e) {
      console.error('[symphony] trigger failed:', e)
    }
  }, [workflowId, setCurrentRunId, setSubView])

  const handleCancel = React.useCallback(async () => {
    if (!currentRunId) return
    try {
      await symphonyCancelRun(currentRunId)
    } catch (e) {
      console.error('[symphony] cancel failed:', e)
    }
  }, [currentRunId])

  if (!detail) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground text-sm">
        Loading workflow {workflowId}…
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col bg-background">
      {/* Header — title + sub-view tabs + actions */}
      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <div className="flex items-center gap-3">
          <h2 className="text-sm font-semibold text-foreground">
            {detail.summary.name}
          </h2>
          <span className="text-xs text-muted-foreground">
            v{detail.summary.currentVersion} · {detail.definition.nodes.length}{' '}
            node{detail.definition.nodes.length === 1 ? '' : 's'}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex rounded-md bg-muted p-0.5">
            {(['design', 'run', 'raw'] as const).map((v) => (
              <button
                key={v}
                onClick={() => setSubView(v)}
                className={cn(
                  'flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-medium transition-colors',
                  subView === v
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground',
                )}
              >
                {SUBVIEW_ICONS[v]}
                {SUBVIEW_LABELS[v]}
              </button>
            ))}
          </div>
          {currentRunId && workflowRuns.find((r) => r.id === currentRunId)?.status === 'running' ? (
            <button
              onClick={handleCancel}
              className="flex items-center gap-1.5 rounded-md bg-destructive/10 px-3 py-1 text-xs font-medium text-destructive hover:bg-destructive/20"
            >
              <Square size={13} />
              Cancel
            </button>
          ) : (
            <button
              onClick={handleRun}
              className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:opacity-90"
            >
              <Play size={13} />
              Run
            </button>
          )}
        </div>
      </div>

      {/* Body — sub-view router */}
      <div className="flex-1 overflow-hidden">
        {subView === 'design' && (
          <WorkflowCanvas
            detail={detail}
            mode="design"
            runId={null}
            nodeRuns={[]}
          />
        )}
        {subView === 'run' && (
          <div className="flex h-full">
            <div className="flex-1">
              <WorkflowCanvas
                detail={detail}
                mode="run"
                runId={currentRunId}
                nodeRuns={
                  currentRunId ? (nodeRunsByRun[currentRunId] ?? []) : []
                }
              />
            </div>
            <div className="w-80 border-l border-border">
              <RunHistoryPanel
                workflowId={workflowId}
                runs={workflowRuns}
                currentRunId={currentRunId}
                onSelect={setCurrentRunId}
              />
            </div>
          </div>
        )}
        {subView === 'raw' && (
          <WorkflowMarkdownEditor workflowId={workflowId} detail={detail} />
        )}
      </div>
    </div>
  )
}
