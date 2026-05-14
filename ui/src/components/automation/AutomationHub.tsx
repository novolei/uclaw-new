/**
 * AutomationHub — manage Humane YAML automation specs.
 * Lists installed Humane automations, shows activity history, escalation badges,
 * and lets the user trigger a run manually or install a new spec (paste YAML or file picker).
 *
 * TODO: AutomationHub test coverage
 */

import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { toast } from 'sonner'
import {
  Zap, Play, Plus, ChevronDown, ChevronRight, Clock,
  CheckCircle, XCircle, Loader, Filter, FileQuestion, AlertCircle,
  ToggleLeft, ToggleRight, Trash2, RefreshCw, Store,
} from 'lucide-react'
import {
  humaneSpecsAtom,
  automationActivitiesAtom,
  selectedAutomationIdAtom,
  type HumaneSpecRow,
  type AutomationActivity,
} from '@/atoms/automation'
import { automationsSubviewAtom } from '@/atoms/marketplace'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import { useOpenSession } from '@/hooks/useOpenSession'
import {
  listAutomationsHumane,
  triggerAutomationManualHumane,
  getAutomationActivity,
  installHumaneSpec,
  importHumaneSpecFile,
  setAutomationEnabled,
  uninstallAutomation,
} from '@/lib/tauri-bridge'

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

// ─── AutomationCard ───────────────────────────────────────────────────────────

function AutomationCard({
  spec,
  onSpecsChange,
}: {
  spec: HumaneSpecRow
  onSpecsChange: (updater: (prev: HumaneSpecRow[]) => HumaneSpecRow[]) => void
}): React.ReactElement {
  const [activities, setActivities] = useAtom(automationActivitiesAtom)
  const [selectedId, setSelectedId] = useAtom(selectedAutomationIdAtom)
  const [triggering, setTriggering] = React.useState(false)
  const [togglingEnabled, setTogglingEnabled] = React.useState(false)
  const [deleting, setDeleting] = React.useState(false)
  const openSession = useOpenSession()

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
      // ignore — activity list is best-effort
    }
  }

  const handleTrigger = async (e: React.MouseEvent) => {
    e.stopPropagation()
    setTriggering(true)
    try {
      await triggerAutomationManualHumane(spec.id)
      toast.success(`已触发：${spec.name}`)
      setTimeout(async () => {
        try {
          const acts = await getAutomationActivity(spec.id, 10)
          setActivities((prev) => ({ ...prev, [spec.id]: acts }))
        } catch { /* ignore */ }
        setTriggering(false)
      }, 1000)
    } catch (e) {
      toast.error(`触发失败：${String(e)}`)
      setTriggering(false)
    }
  }

  const handleToggleEnabled = async (e: React.MouseEvent) => {
    e.stopPropagation()
    setTogglingEnabled(true)
    try {
      await setAutomationEnabled(spec.id, !spec.enabled)
      onSpecsChange((prev) =>
        prev.map((s) => (s.id === spec.id ? { ...s, enabled: !spec.enabled } : s))
      )
    } catch (e) {
      toast.error(`切换失败：${String(e)}`)
    } finally {
      setTogglingEnabled(false)
    }
  }

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation()
    if (!window.confirm(`确定删除「${spec.name}」吗？`)) return
    setDeleting(true)
    try {
      await uninstallAutomation(spec.id)
      onSpecsChange((prev) => prev.filter((s) => s.id !== spec.id))
      toast.success(`已删除：${spec.name}`)
    } catch (e) {
      toast.error(`删除失败：${String(e)}`)
      setDeleting(false)
    }
  }

  const statusBadgeClass =
    spec.status === 'active'
      ? 'text-green-500 bg-green-500/10'
      : spec.status === 'error'
        ? 'text-red-500 bg-red-500/10'
        : spec.status === 'paused'
          ? 'text-amber-500 bg-amber-500/10'
          : 'text-muted-foreground bg-accent'

  return (
    <div className="border border-border/50 rounded-lg overflow-hidden">
      {/* Card header — a div (not <button>) because it nests action <button>s;
          button-in-button is invalid DOM. role/tabIndex/onKeyDown keep it
          keyboard-accessible. */}
      <div
        role="button"
        tabIndex={0}
        onClick={toggle}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            toggle()
          }
        }}
        className="w-full flex items-start gap-2 px-3 py-2 hover:bg-accent/30 transition-colors text-left cursor-pointer"
      >
        <div className="mt-0.5 flex-shrink-0">
          {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </div>
        <Zap size={12} className="text-yellow-500 flex-shrink-0 mt-0.5" />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <p className="text-[12px] font-medium truncate">{spec.name}</p>
            {spec.version && (
              <span className="text-[10px] text-muted-foreground">v{spec.version}</span>
            )}
            {spec.author && (
              <span className="text-[10px] text-muted-foreground">by {spec.author}</span>
            )}
            <span className={`text-[10px] px-1.5 py-0.5 rounded ${statusBadgeClass}`}>
              {spec.status}
            </span>
            {spec.status === 'needs_review' && (
              <span className="text-[10px] px-1.5 py-0.5 rounded text-amber-500 bg-amber-500/10">
                needs review
              </span>
            )}
            {spec.lastRunOutcome && (
              <span className="text-[10px] text-muted-foreground">[{spec.lastRunOutcome}]</span>
            )}
          </div>
          {spec.description && (
            <p className="text-[10px] text-muted-foreground truncate mt-0.5">{spec.description}</p>
          )}
        </div>
        <div
          className="flex items-center gap-1 flex-shrink-0"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Enable/disable toggle */}
          <button
            onClick={handleToggleEnabled}
            disabled={togglingEnabled}
            className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
            title={spec.enabled ? '暂停' : '启用'}
          >
            {spec.enabled
              ? <ToggleRight size={14} className="text-green-500" />
              : <ToggleLeft size={14} className="text-muted-foreground" />}
          </button>
          {/* Manual trigger */}
          <button
            onClick={handleTrigger}
            disabled={triggering}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-[10px] bg-primary/10 text-primary hover:bg-primary/20 transition-colors"
            title="手动触发"
          >
            {triggering ? <Loader size={10} className="animate-spin" /> : <Play size={10} />}
            Run
          </button>
          {/* Delete */}
          <button
            onClick={handleDelete}
            disabled={deleting}
            className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-red-500 transition-colors"
            title="删除"
          >
            {deleting ? <Loader size={10} className="animate-spin" /> : <Trash2 size={10} />}
          </button>
        </div>
      </div>

      {/* Activity list */}
      {isExpanded && (
        <div className="px-3 pb-2 border-t border-border/30">
          {specActivities.length === 0 ? (
            <p className="text-[11px] text-muted-foreground py-2">暂无运行记录。</p>
          ) : (
            specActivities.map((a) => (
              <ActivityRow
                key={a.id}
                a={a}
                onOpen={(sessionId) =>
                  openSession('agent', sessionId, `${spec.name} · 运行`)
                }
              />
            ))
          )}
        </div>
      )}
    </div>
  )
}

// ─── AutomationHub ────────────────────────────────────────────────────────────

export function AutomationHub(): React.ReactElement {
  const [specs, setSpecs] = useAtom(humaneSpecsAtom)
  const setSubview = useSetAtom(automationsSubviewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const [yamlInput, setYamlInput] = React.useState('')
  const [installing, setInstalling] = React.useState(false)
  const [loading, setLoading] = React.useState(false)
  const [showInstall, setShowInstall] = React.useState(false)

  const refresh = React.useCallback(async () => {
    setLoading(true)
    try {
      const rows = await listAutomationsHumane()
      setSpecs(rows)
    } catch (err) {
      toast.error(`加载失败：${String(err)}`)
    } finally {
      setLoading(false)
    }
  }, [setSpecs])

  React.useEffect(() => {
    refresh()
  }, [refresh])

  const handlePasteInstall = async () => {
    if (!yamlInput.trim()) return
    setInstalling(true)
    try {
      const row = await installHumaneSpec(yamlInput)
      setSpecs((prev) => [row, ...prev])
      setYamlInput('')
      setShowInstall(false)
      toast.success(`已安装：${row.name}`)
    } catch (e) {
      toast.error(`安装失败：${String(e)}`)
    } finally {
      setInstalling(false)
    }
  }

  const handleFileImport = async () => {
    try {
      const path = await openDialog({
        filters: [{ name: 'Humane YAML', extensions: ['yaml', 'yml'] }],
      })
      if (!path || typeof path !== 'string') return
      const row = await importHumaneSpecFile(path)
      setSpecs((prev) => [row, ...prev])
      setShowInstall(false)
      toast.success(`已导入：${row.name}`)
    } catch (e) {
      toast.error(`导入失败：${String(e)}`)
    }
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border/50">
        <div className="flex items-center gap-2">
          <Zap size={14} className="text-yellow-500" />
          <span className="text-[13px] font-semibold">Automations</span>
          {loading && <Loader size={11} className="animate-spin text-muted-foreground" />}
        </div>
        {/* titlebar-no-drag: header action buttons stay clickable; the
            header's empty middle stays window-drag surface. */}
        <div className="titlebar-no-drag flex items-center gap-1">
          <button
            onClick={refresh}
            className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
            title="刷新"
          >
            <RefreshCw size={12} />
          </button>
          <button
            onClick={() => setShowInstall((v) => !v)}
            className="flex items-center gap-1 px-2 py-1 rounded text-[11px] bg-primary/10 text-primary hover:bg-primary/20 transition-colors"
          >
            <Plus size={11} />
            安装
          </button>
        </div>
      </div>

      {/* Install panel */}
      {showInstall && (
        <div className="titlebar-no-drag p-3 border-b border-border/50 flex flex-col gap-2">
          <p className="text-[11px] text-muted-foreground">粘贴 Humane YAML 规约：</p>
          <textarea
            value={yamlInput}
            onChange={(e) => setYamlInput(e.target.value)}
            className="w-full h-[140px] text-[11px] font-mono bg-muted rounded-lg p-2 border border-border resize-none focus:outline-none focus:ring-1 focus:ring-primary text-foreground placeholder:text-muted-foreground"
            placeholder="name: My Automation&#10;version: 1.0.0&#10;..."
          />
          <div className="flex items-center gap-2 justify-end">
            <button
              onClick={() => { setSubview('store'); setKaleidoscopeModule('store') }}
              className="flex items-center gap-1 px-3 py-1.5 rounded text-[11px] border border-border text-foreground hover:bg-accent transition-colors"
            >
              <Store size={11} />
              浏览数字人市场
            </button>
            <button
              onClick={handleFileImport}
              className="px-3 py-1.5 rounded text-[11px] border border-border text-foreground hover:bg-accent transition-colors"
            >
              从文件导入
            </button>
            <button
              onClick={handlePasteInstall}
              disabled={installing || !yamlInput.trim()}
              className="px-3 py-1.5 rounded text-[11px] bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
            >
              {installing ? '安装中…' : '安装'}
            </button>
          </div>
        </div>
      )}

      {/* Spec list — titlebar-no-drag: the scrollable body opts out of the
          window-drag region so scrolling + the cards/buttons inside stay
          interactive (a drag region swallows scroll on macOS WKWebView). */}
      <div className="titlebar-no-drag flex-1 overflow-auto p-2 flex flex-col gap-2">
        {specs.length === 0 ? (
          <div className="text-center py-8">
            <Zap size={24} className="mx-auto text-muted-foreground/30 mb-2" />
            <p className="text-[12px] text-muted-foreground">暂未安装任何自动化规约。</p>
            <button
              onClick={() => setShowInstall(true)}
              className="mt-3 text-[11px] text-primary hover:underline"
            >
              安装第一个自动化规约
            </button>
          </div>
        ) : (
          specs.map((spec) => (
            <AutomationCard key={spec.id} spec={spec} onSpecsChange={setSpecs} />
          ))
        )}
      </div>
    </div>
  )
}

export default AutomationHub
