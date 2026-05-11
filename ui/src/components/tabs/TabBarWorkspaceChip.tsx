/**
 * TabBarWorkspaceChip — passive label at TabBar's leftmost edge
 * showing the active workspace's emoji + truncated name.
 *
 * Phase 4b: downgraded from Phase 4a's interactive dropdown to a pure
 * label. Workspace switching now happens via the bottom
 * WorkspaceSwitcherBar; this chip exists as a supplementary visual
 * anchor in the TabBar chrome.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'

const MAX_NAME_CHARS = 12

function truncateName(name: string): string {
  if (name.length <= MAX_NAME_CHARS) return name
  return `${name.slice(0, MAX_NAME_CHARS)}…`
}

export function TabBarWorkspaceChip(): React.ReactElement | null {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const active = workspaces.find((w) => w.id === activeId)
  if (!active) return null

  return (
    <div
      className="titlebar-no-drag flex items-center gap-1 px-2 py-1 rounded-md
                 text-[12px] text-foreground/70 shrink-0"
      title={`工作区: ${active.name}`}
    >
      <span className="leading-none text-[13px]">{active.icon}</span>
      <span className="font-medium">{truncateName(active.name)}</span>
    </div>
  )
}
