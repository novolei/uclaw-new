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
  togglePinAgentSessionAtom,
} from '@/atoms/agent-atoms'
import { tabsAtom } from '@/atoms/tab-atoms'
import type { AgentWorkspace } from '@/lib/agent-types'
import { toast } from 'sonner'

interface WorkspaceRailProps {
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
}

/** Section label inside WorkspaceRail. Matches the OVERVIEW labels
 *  used in the approval modal (text-[10px], uppercase, tracking-wider). */
function SegmentHeader({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <p className="text-[10px] font-semibold uppercase tracking-wider
                  text-muted-foreground/70 mt-2 mb-1 px-2">
      {children}
    </p>
  )
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

  const togglePin = useSetAtom(togglePinAgentSessionAtom)

  // Open-tab indicator — derive the set of session ids that currently have
  // a tab in the global pool. Read tabsAtom (not visibleTabsAtom) so the
  // indicator works even before workspace switch settles the visible slice.
  const tabs = useAtomValue(tabsAtom)
  const openSessionIds = React.useMemo(
    () => new Set(tabs.map((t) => t.sessionId)),
    [tabs],
  )

  const sessions = activeWorkspaceId
    ? (workspaceSessions[activeWorkspaceId] ?? [])
    : []

  // Two-segment split: pinned (sorted by pinnedAt DESC — most recently
  // pinned at the top) and unpinned (preserves the source atom's
  // updatedAt DESC order).
  const pinned = sessions
    .filter((s) => s.pinnedAt !== null)
    .sort((a, b) => (b.pinnedAt ?? 0) - (a.pinnedAt ?? 0))
  const unpinned = sessions.filter((s) => s.pinnedAt === null)

  const handleTogglePin = async (id: string): Promise<void> => {
    try {
      await togglePin(id)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`固定失败：${msg}`)
    }
  }

  return (
    <>
      {/* `key={activeWorkspaceId}` forces a remount when the workspace
          switches, retriggering the one-shot `animate-in fade-in` so the
          new workspace's session list slides into view rather than
          snapping in. Subtle (180ms slide-from-bottom-1) — enough to
          read as "new context" without feeling theatrical. */}
      <div
        key={activeWorkspaceId ?? 'no-workspace'}
        className="flex-1 overflow-y-auto px-3 pt-1 pb-1 scrollbar-none animate-in fade-in-0 slide-in-from-bottom-1 duration-180 ease-out"
      >
        {sessions.length === 0 && (
          <p className="text-[11px] text-muted-foreground px-2 py-3 italic">
            尚无会话。点击上方"新会话"开始。
          </p>
        )}

        {pinned.length > 0 && (
          <>
            <SegmentHeader>📌 固定</SegmentHeader>
            {pinned.map((s) => (
              <SessionItem
                key={s.id}
                id={s.id}
                title={s.title}
                titleEmoji={s.titleEmoji}
                titlePending={s.titlePending}
                isActive={activeSessionId === s.id}
                running={indicatorMap.get(s.id) === 'running'}
                isPinned
                isOpen={openSessionIds.has(s.id)}
                onClick={() => onSelectSession(s.id)}
                onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
                onMove={() => setMoveTargetSessionId(s.id)}
                onTogglePin={() => void handleTogglePin(s.id)}
              />
            ))}
          </>
        )}

        {pinned.length > 0 && unpinned.length > 0 && (
          <SegmentHeader>会话</SegmentHeader>
        )}

        {unpinned.map((s) => (
          <SessionItem
            key={s.id}
            id={s.id}
            title={s.title}
            titleEmoji={s.titleEmoji}
            titlePending={s.titlePending}
            isActive={activeSessionId === s.id}
            running={indicatorMap.get(s.id) === 'running'}
            isPinned={false}
            isOpen={openSessionIds.has(s.id)}
            onClick={() => onSelectSession(s.id)}
            onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
            onMove={() => setMoveTargetSessionId(s.id)}
            onTogglePin={() => void handleTogglePin(s.id)}
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
