/**
 * WorkspaceRail — flat session list for the currently active workspace.
 *
 * Phase 4b (ARC-style switcher): replaces the previous tree-of-all-
 * workspaces render. WorkspaceRail used to map over all workspaces and
 * render a <WorkspaceGroup> per workspace; now it renders only the
 * active workspace's sessions as a flat list.
 *
 * Workspace-level affordances (rename / delete / create) moved to
 * WorkspaceHeader (top) and WorkspaceSwitcherBar (bottom).
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  workspaceSessionsAtom,
  refreshWorkspacesAtom,
} from '@/atoms/workspace'
import { SessionItem } from './SessionItem'
import { MoveSessionDialog } from '@/components/agent/MoveSessionDialog'
import {
  agentSessionsAtom,
  agentSessionIndicatorMapAtom,
} from '@/atoms/agent-atoms'
import type { AgentWorkspace } from '@/lib/agent-types'

interface WorkspaceRailProps {
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
}

export function WorkspaceRail({
  activeSessionId,
  onSelectSession,
  onDeleteSession,
}: WorkspaceRailProps): React.ReactElement {
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const workspaceSessions = useAtomValue(workspaceSessionsAtom)
  const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
  const refreshWorkspaces = useSetAtom(refreshWorkspacesAtom)

  const [moveTargetSessionId, setMoveTargetSessionId] = React.useState<string | null>(null)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const moveTargetSession = moveTargetSessionId
    ? agentSessions.find((s) => s.id === moveTargetSessionId)
    : null

  const agentWorkspaces: AgentWorkspace[] = React.useMemo(
    () => workspaces.map((w) => ({
      id: w.id,
      name: w.name,
      icon: w.icon,
      path: w.path,
      createdAt: Date.parse(w.createdAt) || Date.now(),
      updatedAt: Date.parse(w.updatedAt) || Date.now(),
    })),
    [workspaces],
  )

  React.useEffect(() => {
    refreshWorkspaces()
  }, [refreshWorkspaces])

  const sessions = activeWorkspaceId
    ? (workspaceSessions[activeWorkspaceId] ?? [])
    : []

  return (
    <>
      <div className="flex-1 overflow-y-auto px-3 pt-1 pb-1 scrollbar-none">
        {sessions.length === 0 && (
          <p className="text-[11px] text-muted-foreground px-2 py-3 italic">
            尚无会话。点击上方"新会话"开始。
          </p>
        )}
        {sessions.map((s) => (
          <SessionItem
            key={s.id}
            id={s.id}
            title={s.title}
            titleEmoji={s.titleEmoji}
            titlePending={s.titlePending}
            isActive={activeSessionId === s.id}
            running={indicatorMap.get(s.id) === 'running'}
            onClick={() => onSelectSession(s.id)}
            onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
            onMove={() => setMoveTargetSessionId(s.id)}
          />
        ))}
      </div>
      {moveTargetSession && (
        <MoveSessionDialog
          open={moveTargetSessionId !== null}
          onOpenChange={(open) => { if (!open) setMoveTargetSessionId(null) }}
          sessionId={moveTargetSession.id}
          currentWorkspaceId={moveTargetSession.workspaceId}
          workspaces={agentWorkspaces}
          onMoved={() => {
            setMoveTargetSessionId(null)
            void refreshWorkspaces()
          }}
        />
      )}
    </>
  )
}
