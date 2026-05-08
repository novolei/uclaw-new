/**
 * AutomationHub — manage TOML-defined automation specs.
 * Lists installed automations, shows activity history, and lets the user
 * trigger a run manually or install a new spec.
 */

import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { Zap, Play, Plus, ChevronDown, ChevronRight, Clock, CheckCircle, XCircle, Loader } from 'lucide-react'
import {
  automationSpecsAtom,
  automationActivitiesAtom,
  selectedAutomationIdAtom,
  type AutomationSpecRow,
  type AutomationActivity,
} from '@/atoms/automation'
import {
  listAutomations,
  triggerAutomationManual,
  getAutomationActivity,
  installAutomation,
} from '@/lib/tauri-bridge'

function statusIcon(status: AutomationActivity['status']): React.ReactElement {
  switch (status) {
    case 'running':    return <Loader size={11} className="animate-spin text-blue-400" />
    case 'completed':  return <CheckCircle size={11} className="text-green-500" />
    case 'failed':     return <XCircle size={11} className="text-red-500" />
    case 'cancelled':  return <XCircle size={11} className="text-muted-foreground" />
  }
}

function ActivityRow({ a }: { a: AutomationActivity }): React.ReactElement {
  const d = a.durationMs > 0 ? `${(a.durationMs / 1000).toFixed(1)}s` : ''
  return (
    <div className="flex items-start gap-2 py-1">
      <div className="mt-0.5">{statusIcon(a.status)}</div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-[11px] font-medium capitalize">{a.trigger}</span>
          {d && <span className="text-[10px] text-muted-foreground">{d}</span>}
        </div>
        {a.result && (
          <p className="text-[10px] text-muted-foreground truncate" title={a.result}>{a.result}</p>
        )}
        {a.error && (
          <p className="text-[10px] text-red-400 truncate" title={a.error}>{a.error}</p>
        )}
      </div>
    </div>
  )
}

function AutomationCard({ spec }: { spec: AutomationSpecRow }): React.ReactElement {
  const [activities, setActivities] = useAtom(automationActivitiesAtom)
  const [selectedId, setSelectedId] = useAtom(selectedAutomationIdAtom)
  const [triggering, setTriggering] = React.useState(false)

  const isExpanded = selectedId === spec.id
  const specActivities = activities[spec.id] ?? []

  const toggle = async () => {
    if (isExpanded) {
      setSelectedId(null)
      return
    }
    setSelectedId(spec.id)
    try {
      const acts = await getAutomationActivity(spec.id, 10)
      setActivities((prev) => ({ ...prev, [spec.id]: acts }))
    } catch {
      // ignore
    }
  }

  const handleTrigger = async (e: React.MouseEvent) => {
    e.stopPropagation()
    setTriggering(true)
    try {
      await triggerAutomationManual(spec.id)
      // Refresh activities after a brief delay
      setTimeout(async () => {
        const acts = await getAutomationActivity(spec.id, 10)
        setActivities((prev) => ({ ...prev, [spec.id]: acts }))
        setTriggering(false)
      }, 1000)
    } catch {
      setTriggering(false)
    }
  }

  return (
    <div className="border border-border/50 rounded-lg overflow-hidden">
      {/* Card header */}
      <button
        onClick={toggle}
        className="w-full flex items-center gap-2 px-3 py-2 hover:bg-accent/30 transition-colors text-left"
      >
        {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        <Zap size={12} className="text-yellow-500 flex-shrink-0" />
        <div className="flex-1 min-w-0">
          <p className="text-[12px] font-medium truncate">{spec.name}</p>
          {spec.description && (
            <p className="text-[10px] text-muted-foreground truncate">{spec.description}</p>
          )}
        </div>
        <button
          onClick={handleTrigger}
          disabled={triggering}
          className="flex items-center gap-1 px-2 py-0.5 rounded text-[10px] bg-primary/10 text-primary hover:bg-primary/20 transition-colors flex-shrink-0"
          title="Trigger manually"
        >
          {triggering ? <Loader size={10} className="animate-spin" /> : <Play size={10} />}
          Run
        </button>
      </button>

      {/* Activity list */}
      {isExpanded && (
        <div className="px-3 pb-2 border-t border-border/30">
          {specActivities.length === 0 ? (
            <p className="text-[11px] text-muted-foreground py-2">No activity yet.</p>
          ) : (
            specActivities.map((a) => <ActivityRow key={a.id} a={a} />)
          )}
        </div>
      )}
    </div>
  )
}

function InstallDialog({ onClose, onInstalled }: { onClose: () => void; onInstalled: () => void }): React.ReactElement {
  const [toml, setToml] = React.useState(
    `name = "Daily Summary"\ndescription = "Summarize today's work each evening"\n\n[trigger]\nkind = "cron"\nexpression = "0 18 * * 1-5"\n\ntask = "Summarize the recent conversation and output a brief daily summary."\n`
  )
  const [error, setError] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  const handleInstall = async () => {
    setLoading(true)
    setError(null)
    try {
      await installAutomation(toml)
      onInstalled()
      onClose()
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background border border-border rounded-xl w-[480px] max-h-[70vh] flex flex-col shadow-2xl">
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <h2 className="text-[14px] font-semibold">Install Automation</h2>
          <button onClick={onClose} className="text-muted-foreground hover:text-foreground text-[18px] leading-none">&times;</button>
        </div>
        <div className="p-4 flex-1 overflow-auto">
          <p className="text-[12px] text-muted-foreground mb-2">Paste your automation TOML below:</p>
          <textarea
            value={toml}
            onChange={(e) => setToml(e.target.value)}
            className="w-full h-[200px] text-[11px] font-mono bg-muted rounded-lg p-2 border border-border resize-none focus:outline-none focus:ring-1 focus:ring-primary"
          />
          {error && <p className="text-[11px] text-red-400 mt-2">{error}</p>}
        </div>
        <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
          <button onClick={onClose} className="px-3 py-1.5 rounded text-[12px] text-muted-foreground hover:text-foreground">Cancel</button>
          <button
            onClick={handleInstall}
            disabled={loading}
            className="px-3 py-1.5 rounded text-[12px] bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
          >
            {loading ? 'Installing…' : 'Install'}
          </button>
        </div>
      </div>
    </div>
  )
}

export function AutomationHub(): React.ReactElement {
  const [specs, setSpecs] = useAtom(automationSpecsAtom)
  const [showInstall, setShowInstall] = React.useState(false)
  const [loading, setLoading] = React.useState(false)

  const refresh = React.useCallback(async () => {
    setLoading(true)
    try {
      const rows = await listAutomations()
      setSpecs(rows)
    } catch {
      // ignore
    } finally {
      setLoading(false)
    }
  }, [setSpecs])

  React.useEffect(() => {
    refresh()
  }, [refresh])

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border/50">
        <div className="flex items-center gap-2">
          <Zap size={14} className="text-yellow-500" />
          <span className="text-[13px] font-semibold">Automations</span>
          {loading && <Loader size={11} className="animate-spin text-muted-foreground" />}
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={refresh}
            className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
            title="Refresh"
          >
            <Clock size={12} />
          </button>
          <button
            onClick={() => setShowInstall(true)}
            className="flex items-center gap-1 px-2 py-1 rounded text-[11px] bg-primary/10 text-primary hover:bg-primary/20 transition-colors"
          >
            <Plus size={11} />
            Install
          </button>
        </div>
      </div>

      {/* Spec list */}
      <div className="flex-1 overflow-auto p-2 flex flex-col gap-2">
        {specs.length === 0 ? (
          <div className="text-center py-8">
            <Zap size={24} className="mx-auto text-muted-foreground/30 mb-2" />
            <p className="text-[12px] text-muted-foreground">No automations installed.</p>
            <button
              onClick={() => setShowInstall(true)}
              className="mt-3 text-[11px] text-primary hover:underline"
            >
              Install your first automation
            </button>
          </div>
        ) : (
          specs.map((spec) => <AutomationCard key={spec.id} spec={spec} />)
        )}
      </div>

      {showInstall && (
        <InstallDialog
          onClose={() => setShowInstall(false)}
          onInstalled={refresh}
        />
      )}
    </div>
  )
}
