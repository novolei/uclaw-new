import { useState, useMemo } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { toggleArchiveAgentSession, openFile, openExternal } from '@/lib/tauri-bridge'
import { ActivityMarkdown } from './ActivityMarkdown'

// ─── Shared types and config (exported for RunSessionSubView) ─────────────────

export interface ReportArtifact {
  kind: string
  path?: string
  title: string
}

export const OUTCOME_CONFIG: Record<string, { label: string; className: string }> = {
  useful:  { label: '有效',   className: 'bg-green-500/15 text-green-600 dark:text-green-400' },
  noop:    { label: '无操作', className: 'bg-muted text-muted-foreground' },
  skipped: { label: '跳过',   className: 'bg-muted text-muted-foreground' },
  error:   { label: '错误',   className: 'bg-danger/10 text-danger' },
}

// ─── ArtifactChip (exported for RunSessionSubView) ────────────────────────────

interface ChipProps {
  artifact: ReportArtifact
  workingDir: string
}

export function ArtifactChip({ artifact, workingDir }: ChipProps) {
  const icon = artifact.kind === 'file' ? '📄' : artifact.kind === 'url' ? '🔗' : '📝'
  const clickable = artifact.kind === 'file' || artifact.kind === 'url'

  function handleClick() {
    if (artifact.kind === 'file' && artifact.path) {
      void openFile(`${workingDir}/${artifact.path}`)
    } else if (artifact.kind === 'url') {
      void openExternal(artifact.path ?? artifact.title)
    }
  }

  if (!clickable) {
    return (
      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] bg-muted text-muted-foreground">
        {icon} {artifact.title}
      </span>
    )
  }

  return (
    <button
      onClick={handleClick}
      className="titlebar-no-drag inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] bg-primary/10 text-primary hover:bg-primary/20 transition-colors"
    >
      {icon} {artifact.title}
    </button>
  )
}

// ─── Status config ────────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<string, { label: string; className: string }> = {
  completed:    { label: '已完成', className: 'text-success' },
  failed:       { label: '失败',   className: 'text-danger' },
  cancelled:    { label: '已取消', className: 'text-muted-foreground' },
  filtered_out: { label: '已跳过', className: 'text-muted-foreground' },
  waiting_user: { label: '待确认', className: 'text-warning' },
  running:      { label: '运行中', className: 'text-primary' },
  queued:       { label: '排队中', className: 'text-muted-foreground' },
}

function formatTs(ms: number | null): string {
  if (!ms) return '—'
  return new Date(ms).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  })
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

// ─── ActivityListItem ─────────────────────────────────────────────────────────

interface Props {
  activity: AutomationActivity
  onOpenRunSession?: (sessionId: string) => void
  onArchived?: (sessionId: string) => void
}

export function ActivityListItem({ activity, onOpenRunSession, onArchived }: Props) {
  const [archiving, setArchiving] = useState(false)

  const cfg = STATUS_CONFIG[activity.status] ?? {
    label: activity.status,
    className: 'text-muted-foreground',
  }
  const outcomeCfg = activity.reportOutcome
    ? (OUTCOME_CONFIG[activity.reportOutcome] ?? null)
    : null
  const isEscalation = activity.status === 'waiting_user'
  const isActive = activity.status === 'running' || activity.status === 'queued'

  const artifacts = useMemo<ReportArtifact[]>(() => {
    try { return JSON.parse(activity.reportArtifactsJson) as ReportArtifact[] }
    catch { return [] }
  }, [activity.reportArtifactsJson])

  async function handleArchive() {
    if (!activity.sessionId || archiving) return
    setArchiving(true)
    try {
      await toggleArchiveAgentSession(activity.sessionId)
      onArchived?.(activity.sessionId)
    } finally {
      setArchiving(false)
    }
  }

  return (
    <div
      data-testid={`activity-row-${activity.id}`}
      className={[
        'group rounded-lg border bg-background/60',
        isEscalation
          ? 'border-warning ring-1 ring-warning/20'
          : 'border-border/40',
      ].join(' ')}
    >
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 text-xs">
        <span className="text-muted-foreground shrink-0">
          {formatTs(activity.startedAt ?? activity.queuedAt)}
        </span>
        <span className={cfg.className}>{cfg.label}</span>
        {outcomeCfg && (
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${outcomeCfg.className}`}>
            {outcomeCfg.label}
          </span>
        )}
        {activity.durationMs > 0 && (
          <span className="text-muted-foreground">
            {formatDuration(activity.durationMs)}
          </span>
        )}
        <div className="ml-auto flex items-center gap-2 shrink-0">
          {activity.sessionId && (
            <button
              onClick={handleArchive}
              disabled={archiving}
              className="titlebar-no-drag text-xs text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
              aria-label="归档"
            >
              归档
            </button>
          )}
          {activity.sessionId && (
            <button
              onClick={() => onOpenRunSession?.(activity.sessionId!)}
              className="titlebar-no-drag text-xs text-primary hover:underline"
            >
              查看进程 &gt;
            </button>
          )}
        </div>
      </div>

      {/* Body: Markdown or running placeholder */}
      {(isActive || activity.reportText) && (
        <div className="px-3 pb-2">
          {isActive && !activity.reportText ? (
            <p className="text-xs text-muted-foreground italic">运行中，暂无报告…</p>
          ) : activity.reportText ? (
            <ActivityMarkdown content={activity.reportText} />
          ) : null}
        </div>
      )}

      {/* Artifact chips */}
      {artifacts.length > 0 && (
        <div className="flex flex-wrap gap-1.5 px-3 pb-2">
          {artifacts.map((a, i) => (
            <ArtifactChip key={i} artifact={a} workingDir={activity.workingDir} />
          ))}
        </div>
      )}
    </div>
  )
}
