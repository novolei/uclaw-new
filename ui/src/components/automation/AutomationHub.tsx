/**
 * AutomationHub — three-column shell: spec list sidebar + run surface.
 * Delegates spec display to SpecList and activity/run detail to SpecRunSurface.
 * Install functionality is handled separately (out of scope for this view).
 *
 * NOTE: ActivityRow is kept as a named export for AutomationHub.test.tsx compatibility.
 */

import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import {
  Clock, CheckCircle, XCircle, Loader, Filter, FileQuestion, AlertCircle,
} from 'lucide-react'
import {
  humaneSpecsAtom,
  automationActivitiesAtom,
  type AutomationActivity,
} from '@/atoms/automation'
import { automationSelectedSpecIdAtom, automationActiveTabAtom } from '@/atoms/automation-ui'
import { listAutomationsHumane, getAutomationActivity } from '@/lib/tauri-bridge'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'
import { SpecList } from './SpecList'
import { SpecRunSurface } from './SpecRunSurface'

// ─── Status icon ──────────────────────────────────────────────────────────────

function statusIcon(status: AutomationActivity['status']): React.ReactElement {
  switch (status) {
    case 'queued':           return <Clock size={11} className="text-muted-foreground" />
    case 'running':          return <Loader size={11} className="animate-spin text-blue-400" />
    case 'completed':        return <CheckCircle size={11} className="text-green-500" />
    case 'failed':           return <XCircle size={11} className="text-red-500" />
    case 'cancelled':        return <XCircle size={11} className="text-muted-foreground" />
    case 'waiting_user':     return <AlertCircle size={11} className="text-amber-500" />
    case 'filtered_out':     return <Filter size={11} className="text-muted-foreground" />
    case 'deferred_phase_2': return <FileQuestion size={11} className="text-muted-foreground" />
    default:                 return <Clock size={11} className="text-muted-foreground" />
  }
}

// ─── ActivityRow ──────────────────────────────────────────────────────────────
// Kept as a named export for test compatibility (AutomationHub.test.tsx).

export function ActivityRow({
  a,
  onOpen,
}: {
  a: AutomationActivity
  onOpen: (sessionId: string) => void
}): React.ReactElement {
  const d = a.durationMs > 0 ? `${(a.durationMs / 1000).toFixed(1)}s` : ''
  const subtitle = a.reportText ?? a.errorText ?? ''
  const clickable = a.sessionId !== null
  return (
    <div
      className={`flex items-start gap-2 py-1 ${clickable ? 'cursor-pointer hover:bg-accent/40 rounded -mx-1 px-1' : ''}`}
      onClick={() => { if (a.sessionId) onOpen(a.sessionId) }}
      onKeyDown={clickable ? (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          if (a.sessionId) onOpen(a.sessionId)
        }
      } : undefined}
      role={clickable ? 'button' : undefined}
      tabIndex={clickable ? 0 : undefined}
      title={clickable ? '在 Agent 视图中查看此次运行' : undefined}
    >
      <div className="mt-0.5">{statusIcon(a.status)}</div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-[11px] font-medium">{a.triggerSourceType}</span>
          {d && <span className="text-[10px] text-muted-foreground">{d}</span>}
          {a.reportOutcome && (
            <span className="text-[10px] text-muted-foreground">[{a.reportOutcome}]</span>
          )}
        </div>
        {subtitle && (
          <p
            className={`text-[10px] truncate ${a.errorText ? 'text-red-400' : 'text-muted-foreground'}`}
            title={subtitle}
          >
            {subtitle}
          </p>
        )}
      </div>
    </div>
  )
}

// ─── AutomationHub ────────────────────────────────────────────────────────────

export function AutomationHub({ initialSpecs }: { initialSpecs?: HumaneSpecRow[] } = {}) {
  const [specs, setSpecs] = useAtom(humaneSpecsAtom)
  const setActivities = useSetAtom(automationActivitiesAtom)
  const [selectedSpecId, setSelectedSpecId] = useAtom(automationSelectedSpecIdAtom)
  const setActiveTab = useSetAtom(automationActiveTabAtom)

  // Load specs on mount
  React.useEffect(() => {
    if (initialSpecs) {
      setSpecs(initialSpecs)
      return
    }
    listAutomationsHumane().then(setSpecs).catch(() => {})
  }, [initialSpecs, setSpecs])

  // Load activities for selected spec
  React.useEffect(() => {
    if (!selectedSpecId) return
    getAutomationActivity(selectedSpecId, 50).then((acts) =>
      setActivities((prev) => ({ ...prev, [selectedSpecId]: acts }))
    ).catch(() => {})
  }, [selectedSpecId, setActivities])

  // Auto-select first spec if none selected
  React.useEffect(() => {
    if (!selectedSpecId && specs.length > 0) {
      setSelectedSpecId(specs[0].id)
    }
  }, [specs, selectedSpecId, setSelectedSpecId])

  return (
    <div className="flex h-full overflow-hidden">
      {/* spec list sidebar */}
      <div className="w-[240px] shrink-0 flex flex-col border-r border-border/50 overflow-hidden">
        <div className="titlebar-drag-region flex items-center px-3 py-2 border-b border-border/50 text-sm font-semibold shrink-0">
          数字人
        </div>
        <SpecList
          selectedSpecId={selectedSpecId}
          onSelect={(id) => { setSelectedSpecId(id); setActiveTab('activity') }}
          onRun={(id) => { setSelectedSpecId(id); setActiveTab('activity') }}
        />
      </div>

      {/* run surface */}
      <div className="flex-1 flex overflow-hidden">
        {selectedSpecId ? (
          <SpecRunSurface specId={selectedSpecId} />
        ) : (
          <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
            选择一个数字人
          </div>
        )}
      </div>
    </div>
  )
}

export default AutomationHub
