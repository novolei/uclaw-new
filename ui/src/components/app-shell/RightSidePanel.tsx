/**
 * RightSidePanel — 右侧边栏容器（带标签页切换）
 *
 * 在 Agent 模式下显示四个标签页：文件浏览器、Agent Teams、计划文件、轨迹回放。
 * 监听全局 plan:updated 事件，自动切换到 Plan 标签页。
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { FolderOpen, Users, ListChecks, History, Globe } from 'lucide-react'
import { appModeAtom } from '@/atoms/app-mode'
import { currentAgentSessionIdAtom, agentSessionPathMapAtom } from '@/atoms/agent-atoms'
import { activePlanAtom } from '@/atoms/agent-teams'
import { WorkspaceFilesView } from '@/components/agent/SidePanel'
import { AgentTeamsPanel } from '@/components/agent/AgentTeamsPanel'
import { PlanViewer } from '@/components/agent/PlanViewer'
import { TrajectoryReel } from '@/components/agent/TrajectoryReel'
import { BrowserViewer } from '@/components/agent/BrowserViewer'

type ActiveTab = 'files' | 'teams' | 'plan' | 'trajectory' | 'browser'

interface TabButtonProps {
  isActive: boolean
  onClick: () => void
  icon: React.ReactNode
  label: string
}

function TabButton({ isActive, onClick, icon, label }: TabButtonProps): React.ReactElement {
  return (
    <button
      onClick={onClick}
      className={[
        'flex items-center gap-1 px-2 py-1 rounded text-[11px] font-medium transition-colors',
        isActive
          ? 'bg-accent text-foreground'
          : 'text-muted-foreground hover:text-foreground',
      ].join(' ')}
      title={label}
    >
      {icon}
      <span>{label}</span>
    </button>
  )
}

interface PlanUpdatedPayload {
  filename: string
  content: string
}

export function RightSidePanel(): React.ReactElement | null {
  const appMode = useAtomValue(appModeAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const sessionPathMap = useAtomValue(agentSessionPathMapAtom)
  const plan = useAtomValue(activePlanAtom)
  const setActivePlan = useSetAtom(activePlanAtom)

  const [activeTab, setActiveTab] = React.useState<ActiveTab>('files')

  // Subscribe to plan:updated events (global, not session-specific)
  React.useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    listen<PlanUpdatedPayload>('plan:updated', ({ payload }) => {
      setActivePlan({ filename: payload.filename, content: payload.content })
      setActiveTab('plan')
    }).then((fn) => {
      if (cancelled) fn()
      else unlisten = fn
    })

    return () => {
      cancelled = true
      unlisten?.()
    }
    // setActivePlan is a stable Jotai write-atom setter — safe to omit
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Only show in agent mode with an active session
  if (appMode !== 'agent' || !currentSessionId) {
    return null
  }

  const sessionPath = sessionPathMap.get(currentSessionId) ?? null

  return (
    <div className="relative h-full w-[380px] flex-shrink-0 overflow-hidden titlebar-drag-region bg-content-area rounded-2xl shadow-xl">
      {/* Tab bar */}
      <div className="flex items-center gap-0.5 px-2 pt-[38px] pb-1 border-b border-border/50 flex-shrink-0">
        <TabButton
          isActive={activeTab === 'files'}
          onClick={() => setActiveTab('files')}
          icon={<FolderOpen size={13} />}
          label="Files"
        />
        <TabButton
          isActive={activeTab === 'teams'}
          onClick={() => setActiveTab('teams')}
          icon={<Users size={13} />}
          label="Teams"
        />
        <TabButton
          isActive={activeTab === 'plan'}
          onClick={() => setActiveTab('plan')}
          icon={<ListChecks size={13} />}
          label="Plan"
        />
        <TabButton
          isActive={activeTab === 'trajectory'}
          onClick={() => setActiveTab('trajectory')}
          icon={<History size={13} />}
          label="Trajectory"
        />
        <TabButton
          isActive={activeTab === 'browser'}
          onClick={() => setActiveTab('browser')}
          icon={<Globe size={13} />}
          label="Browser"
        />
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-auto h-[calc(100%-72px)]">
        {activeTab === 'files' && (
          <WorkspaceFilesView sessionId={currentSessionId} sessionPath={sessionPath} />
        )}
        {activeTab === 'teams' && (
          <AgentTeamsPanel />
        )}
        {activeTab === 'plan' && (
          plan ? (
            <PlanViewer planContent={plan.content} planFilename={plan.filename} />
          ) : (
            <div className="p-3 text-[12px] text-muted-foreground">
              No active plan. The agent will create one using plan_write.
            </div>
          )
        )}
        {activeTab === 'trajectory' && (
          <TrajectoryReel sessionId={currentSessionId} />
        )}
        {activeTab === 'browser' && (
          <BrowserViewer />
        )}
      </div>
    </div>
  )
}
