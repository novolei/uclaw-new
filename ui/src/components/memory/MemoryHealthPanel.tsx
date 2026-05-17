/**
 * MemoryHealthPanel — surfaces structural-integrity findings produced
 * by the zero-LLM `memory_health` scenario (Phase 4).
 *
 * Layout:
 *   - Header — workspace label, "scan now" button, last-run stats
 *     ("inserted X / active Y / scanned in Zms").
 *   - Severity-grouped list (error → warn → info), each finding shows
 *     subject (node_id / edge_id / route_id), kind, payload snippet,
 *     dismiss button + optional jump-to-node callback.
 *   - Empty state explaining the panel updates every ~30 min.
 *
 * Theme: all colours via tokens (`bg-popover`, `text-destructive`,
 * `text-amber-500` for warn, `text-muted-foreground` for info) so the
 * 11 uClaw themes render consistently.
 */

import * as React from 'react'
import { Loader2, RefreshCw, ShieldCheck, X, ExternalLink } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn, formatDateTime } from '@/lib/utils'
import {
  memoryHealthListFindings,
  memoryHealthDismissFinding,
  memoryHealthRunNow,
} from '@/lib/tauri-bridge'
import type { HealthFindingDto, HealthRunOutcome } from '@/lib/types'

interface MemoryHealthPanelProps {
  spaceId?: string
  /** Optional callback when the user clicks "Go to node" / a subject row.
   * Receives the finding's `subject` — typically a node_id, but could be
   * an edge_id or route_id depending on `check_kind`. */
  onSelectSubject?: (subject: string, checkKind: string) => void
  className?: string
}

// Display labels for the seven Phase 4 check kinds. Keys are intentionally
// the wire strings so falling through to "Other" is automatic for any
// future check_kind the backend introduces (Phase 5 lint kinds, etc.).
const CHECK_KIND_LABEL: Record<string, string> = {
  orphan: 'Orphan node',
  stub: 'Stub EntityPage',
  dangling_fts: 'Dangling FTS row',
  index_drift: 'Index drift',
  phantom_slug: 'Phantom slug',
  empty_versions: 'Empty version chain',
  missing_route: 'Missing primary route',
}

const SEVERITY_ORDER: ReadonlyArray<'error' | 'warn' | 'info'> = [
  'error',
  'warn',
  'info',
]

function severityClass(sev: string): string {
  switch (sev) {
    case 'error':
      return 'text-destructive'
    case 'warn':
      return 'text-amber-500'
    case 'info':
      return 'text-muted-foreground'
    default:
      return 'text-muted-foreground'
  }
}

function severityIcon(sev: string): string {
  switch (sev) {
    case 'error':
      return '⚠'
    case 'warn':
      return '⚠'
    case 'info':
      return 'ⓘ'
    default:
      return '·'
  }
}

export function MemoryHealthPanel({
  spaceId,
  onSelectSubject,
  className,
}: MemoryHealthPanelProps): React.ReactElement {
  const space = spaceId ?? 'default'
  const [findings, setFindings] = React.useState<HealthFindingDto[]>([])
  const [loading, setLoading] = React.useState<boolean>(true)
  const [scanning, setScanning] = React.useState<boolean>(false)
  const [lastRun, setLastRun] = React.useState<HealthRunOutcome | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [dismissingIds, setDismissingIds] = React.useState<Set<string>>(new Set())

  const fetchFindings = React.useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const list = await memoryHealthListFindings({
        spaceId: space,
        limit: 200,
      })
      setFindings(Array.isArray(list) ? list : [])
    } catch (e) {
      setError(`Failed to load findings: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [space])

  React.useEffect(() => {
    void fetchFindings()
  }, [fetchFindings])

  const handleScan = async (): Promise<void> => {
    setScanning(true)
    setError(null)
    try {
      const outcome = await memoryHealthRunNow({ spaceId: space })
      setLastRun(outcome)
      await fetchFindings()
      if (outcome.total_inserted > 0) {
        toast.success(
          `Scan complete — ${outcome.total_inserted} new finding${outcome.total_inserted === 1 ? '' : 's'}`,
        )
      } else {
        toast.success('Scan complete — no new findings ✓')
      }
    } catch (e) {
      const msg = String(e)
      setError(msg)
      toast.error(`Scan failed: ${msg}`)
    } finally {
      setScanning(false)
    }
  }

  const handleDismiss = async (id: string): Promise<void> => {
    setDismissingIds((prev) => new Set(prev).add(id))
    try {
      await memoryHealthDismissFinding({ findingId: id })
      // Optimistic local update so the row disappears immediately.
      setFindings((prev) => prev.filter((f) => f.id !== id))
    } catch (e) {
      toast.error(`Dismiss failed: ${String(e)}`)
    } finally {
      setDismissingIds((prev) => {
        const next = new Set(prev)
        next.delete(id)
        return next
      })
    }
  }

  // ─── Grouped view ──────────────────────────────────────────────────

  const grouped = React.useMemo(() => {
    const buckets = new Map<string, HealthFindingDto[]>()
    for (const f of findings) {
      const arr = buckets.get(f.severity) ?? []
      arr.push(f)
      buckets.set(f.severity, arr)
    }
    return buckets
  }, [findings])

  const totalActive = findings.length

  return (
    <div
      className={cn('flex flex-col h-full bg-popover text-foreground', className)}
      data-testid="memory-health-panel"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border/50">
        <div className="flex items-center gap-2">
          <ShieldCheck className="size-4 text-muted-foreground" />
          <span className="text-xs font-medium">
            Memory Health · workspace `{space}`
          </span>
          {totalActive > 0 && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0">
              {totalActive} active
            </Badge>
          )}
          {lastRun && (
            <span className="text-[10px] text-muted-foreground/70 ml-1">
              last scan: {lastRun.total_inserted} new · {lastRun.active_total} active · {lastRun.duration_ms}ms
            </span>
          )}
        </div>
        <Button
          size="sm"
          variant="ghost"
          className="text-xs h-7 gap-1"
          onClick={handleScan}
          disabled={scanning}
        >
          <RefreshCw className={cn('size-3', scanning && 'animate-spin')} />
          Scan now
        </Button>
      </div>

      {/* Error banner */}
      {error && (
        <div className="px-3 py-1.5 bg-destructive/10 text-destructive text-xs">
          {error}
        </div>
      )}

      {/* Findings list */}
      <ScrollArea className="flex-1">
        <div className="p-3 space-y-3">
          {loading && findings.length === 0 ? (
            <div className="flex items-center justify-center py-10">
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            </div>
          ) : findings.length === 0 ? (
            <EmptyState />
          ) : (
            SEVERITY_ORDER.map((sev) => {
              const items = grouped.get(sev)
              if (!items || items.length === 0) return null
              return (
                <SeverityGroup
                  key={sev}
                  severity={sev}
                  findings={items}
                  dismissingIds={dismissingIds}
                  onDismiss={handleDismiss}
                  onSelectSubject={onSelectSubject}
                />
              )
            })
          )}
          {/* Any severities outside the canonical 3 (forward-compat) */}
          {Array.from(grouped.entries())
            .filter(([sev]) => !SEVERITY_ORDER.includes(sev as 'error' | 'warn' | 'info'))
            .map(([sev, items]) => (
              <SeverityGroup
                key={sev}
                severity={sev}
                findings={items}
                dismissingIds={dismissingIds}
                onDismiss={handleDismiss}
                onSelectSubject={onSelectSubject}
              />
            ))}
        </div>
      </ScrollArea>
    </div>
  )
}

// ─── EmptyState ─────────────────────────────────────────────────────

function EmptyState(): React.ReactElement {
  return (
    <div className="flex flex-col items-center gap-2 py-10 text-center px-4">
      <ShieldCheck className="size-8 text-muted-foreground/40" />
      <p className="text-xs text-muted-foreground">
        Memory graph looks healthy.
      </p>
      <p className="text-[10px] text-muted-foreground/60">
        Auto-scans run every ~30 min. Hit "Scan now" to check immediately.
      </p>
    </div>
  )
}

// ─── SeverityGroup ──────────────────────────────────────────────────

function SeverityGroup({
  severity,
  findings,
  dismissingIds,
  onDismiss,
  onSelectSubject,
}: {
  severity: string
  findings: HealthFindingDto[]
  dismissingIds: Set<string>
  onDismiss: (id: string) => void
  onSelectSubject?: (subject: string, checkKind: string) => void
}): React.ReactElement {
  return (
    <div>
      <div
        className={cn(
          'text-[10px] uppercase tracking-wide font-medium mb-1.5 flex items-center gap-1',
          severityClass(severity),
        )}
      >
        <span>{severityIcon(severity)}</span>
        <span>{severity}</span>
        <span className="text-muted-foreground/60">({findings.length})</span>
      </div>
      <div className="space-y-1">
        {findings.map((f) => (
          <FindingRow
            key={f.id}
            finding={f}
            dismissing={dismissingIds.has(f.id)}
            onDismiss={() => onDismiss(f.id)}
            onSelectSubject={onSelectSubject}
          />
        ))}
      </div>
    </div>
  )
}

// ─── FindingRow ─────────────────────────────────────────────────────

function FindingRow({
  finding,
  dismissing,
  onDismiss,
  onSelectSubject,
}: {
  finding: HealthFindingDto
  dismissing: boolean
  onDismiss: () => void
  onSelectSubject?: (subject: string, checkKind: string) => void
}): React.ReactElement {
  const label = CHECK_KIND_LABEL[finding.checkKind] ?? 'Other'
  const payloadSnippet = React.useMemo(() => {
    if (!finding.payloadJson) return null
    try {
      const obj = JSON.parse(finding.payloadJson) as Record<string, unknown>
      const title =
        typeof obj.title === 'string'
          ? obj.title
          : typeof obj.missing_node_id === 'string'
            ? obj.missing_node_id
            : typeof obj.missing_child_id === 'string'
              ? obj.missing_child_id
              : null
      const kind = typeof obj.kind === 'string' ? obj.kind : null
      return [title, kind].filter(Boolean).join(' · ')
    } catch {
      return null
    }
  }, [finding.payloadJson])

  return (
    <div
      className={cn(
        'flex items-start gap-2 px-2 py-1.5 rounded-sm text-xs',
        'hover:bg-muted/60 transition-colors',
        'border border-transparent hover:border-border/40',
      )}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="font-medium">{label}</span>
          <Badge variant="outline" className="text-[9px] px-1 py-0">
            {finding.checkKind}
          </Badge>
          {finding.isLint && (
            <Badge
              variant="outline"
              className="text-[9px] px-1 py-0 border-amber-500/50 text-amber-500"
              title="This finding was produced by the LLM lint scenario (Phase 5)"
            >
              lint
            </Badge>
          )}
        </div>
        <button
          type="button"
          onClick={() => onSelectSubject?.(finding.subject, finding.checkKind)}
          className="font-mono text-[10px] text-muted-foreground hover:text-foreground transition-colors"
          title={finding.subject}
        >
          {finding.subject.length > 36
            ? `${finding.subject.slice(0, 36)}…`
            : finding.subject}
          {onSelectSubject && <ExternalLink className="inline size-2.5 ml-0.5 align-baseline" />}
        </button>
        {payloadSnippet && (
          <div className="text-[10px] text-muted-foreground/80 mt-0.5">
            {payloadSnippet}
          </div>
        )}
        <span className="text-[10px] text-muted-foreground/50">
          {formatDateTime(new Date(finding.discoveredAt).toISOString())}
        </span>
      </div>
      <button
        type="button"
        onClick={onDismiss}
        disabled={dismissing}
        className={cn(
          'p-1 rounded-sm text-muted-foreground/60 hover:text-foreground hover:bg-muted',
          'transition-colors disabled:opacity-50',
        )}
        title="Dismiss"
      >
        {dismissing ? (
          <Loader2 className="size-3 animate-spin" />
        ) : (
          <X className="size-3" />
        )}
      </button>
    </div>
  )
}
