import { useState, useEffect, useCallback } from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import {
  automationActiveTabAtom,
  automationActivityRunSessionIdAtom,
  type AutomationTab,
} from '@/atoms/automation-ui'
import { automationActivitiesAtom, humaneSpecsAtom } from '@/atoms/automation'
import { triggerAutomationManualHumane, getAutomationActivity } from '@/lib/tauri-bridge'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'
import { SpecRunHeader } from './SpecRunHeader'
import { HomeThreadView } from './HomeThreadView'
import { ActivityHistoryView } from './ActivityHistoryView'
import { ChatThreadsTab } from './ChatThreadsTab'
import { SpecSettingsView } from './SpecSettingsView'
import { AutomationRightPanel } from './AutomationRightPanel'

const TAB_LABELS: Record<AutomationTab, string> = {
  chat: '聊天',
  activity: '动态',
  threads: '会话',
  settings: '设置',
}

function liveSpecMeta(spec: HumaneSpecRow): { platform?: string; roomId?: string; roomTitle?: string } | null {
  try {
    const raw = JSON.parse(spec.specJson)
    if (raw?.x_uclaw_runtime?.kind !== 'live_room_moderator') return null
    const config = raw.config ?? {}
    return {
      platform: config.platform ?? 'douyin',
      roomId: config.room_id ?? config.roomId,
      roomTitle: config.room_title ?? config.roomTitle,
    }
  } catch {
    return null
  }
}

interface Props {
  specId: string
}

export function SpecRunSurface({ specId }: Props) {
  const [activeTab, setActiveTab] = useAtom(automationActiveTabAtom)
  const [runSessionId, setRunSessionId] = useAtom(automationActivityRunSessionIdAtom)
  const [specs, setSpecs] = useAtom(humaneSpecsAtom)
  const activitiesMap = useAtomValue(automationActivitiesAtom)
  const setActivitiesMap = useSetAtom(automationActivitiesAtom)
  const [showRightPanel, setShowRightPanel] = useState(false)
  const [isRunning, setIsRunning] = useState(false)

  const spec = specs.find((s) => s.id === specId)
  const activities = activitiesMap[specId] ?? []

  const refreshActivities = useCallback(async () => {
    try {
      const acts = await getAutomationActivity(specId, 50)
      setActivitiesMap((prev) => ({ ...prev, [specId]: acts }))
    } catch { /* ignore */ }
  }, [specId, setActivitiesMap])

  // Poll every 3 s while any activity is running or queued.
  const hasActiveRun = activities.some(
    (a) => a.status === 'running' || a.status === 'queued'
  )
  useEffect(() => {
    if (!hasActiveRun) return
    const id = setInterval(() => { void refreshActivities() }, 3000)
    return () => clearInterval(id)
  }, [hasActiveRun, refreshActivities])

  if (!spec) return null
  const liveMeta = liveSpecMeta(spec)
  const activeLiveRuns = liveMeta
    ? activities.filter((a) => a.status === 'running' || a.status === 'queued').length
    : 0
  const latestLiveReport = activities.find((a) => a.reportText?.includes('Live Room Run Report'))

  async function handleRun() {
    setIsRunning(true)
    try {
      await triggerAutomationManualHumane(specId)
      setActiveTab('activity')
      // The queued activity row is already in the DB when the command returns —
      // fetch immediately so the new row appears without waiting for the poller.
      await refreshActivities()
    } finally {
      setIsRunning(false)
    }
  }

  const showPanel =
    showRightPanel &&
    (activeTab === 'chat' || (activeTab === 'activity' && runSessionId != null))

  return (
    <div className="flex flex-col flex-1 h-full overflow-hidden">
      <SpecRunHeader specName={spec.name} onRun={handleRun} isRunning={isRunning} />

      {liveMeta && (
        <div className="flex items-center gap-3 px-3 py-2 border-b border-border/50 text-xs text-muted-foreground shrink-0">
          <span className="font-medium text-foreground">{liveMeta.platform}</span>
          <span>{liveMeta.roomTitle || liveMeta.roomId || '未设置房间'}</span>
          <span>{activeLiveRuns} active</span>
          {latestLiveReport?.reportText && <span>report ready</span>}
        </div>
      )}

      {/* tab bar */}
      <div className="flex gap-0 border-b border-border/50 px-3 shrink-0">
        {(Object.keys(TAB_LABELS) as AutomationTab[]).map((t) => (
          <button
            key={t}
            onClick={() => {
              setActiveTab(t)
              if (t !== 'activity') setRunSessionId(null)
            }}
            className={[
              'titlebar-no-drag px-3 py-2 text-sm border-b-2 transition-colors',
              activeTab === t
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground',
            ].join(' ')}
          >
            {TAB_LABELS[t]}
          </button>
        ))}

        {/* right-panel toggle */}
        <button
          onClick={() => setShowRightPanel((v) => !v)}
          className={[
            'titlebar-no-drag ml-auto px-2 py-2 text-sm transition-colors',
            showRightPanel
              ? 'text-primary'
              : 'text-muted-foreground hover:text-foreground',
          ].join(' ')}
          title="切换右侧面板"
        >
          ⊞
        </button>
      </div>

      {/* content + right panel */}
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-col flex-1 overflow-hidden">
          {activeTab === 'chat' && <HomeThreadView specId={specId} />}
          {activeTab === 'activity' && (
            <ActivityHistoryView
              specId={specId}
              activities={activities}
              onOpenRunSession={(sid) => setRunSessionId(sid)}
              activeRunSessionId={runSessionId}
              onCloseRunSession={() => setRunSessionId(null)}
            />
          )}
          {activeTab === 'threads' && <ChatThreadsTab specId={specId} />}
          {activeTab === 'settings' && (
            <SpecSettingsView
              spec={spec}
              onSpecChange={(updated: HumaneSpecRow) =>
                setSpecs((prev) => prev.map((s) => (s.id === updated.id ? updated : s)))
              }
            />
          )}
        </div>

        {showPanel && (
          <AutomationRightPanel
            sessionId={activeTab === 'activity' && runSessionId ? runSessionId : ''}
            sessionPath={null}
          />
        )}
      </div>
    </div>
  )
}
