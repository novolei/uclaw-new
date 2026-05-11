/**
 * TabBarWorkspaceChip — leftmost element of TabBar showing the active
 * workspace + a dropdown to switch between workspaces.
 *
 * Mounted by TabBar. Hidden when no active workspace. Dropdown lists
 * all workspaces (sort_order ASC) with their Cmd+N hints for the first
 * 9 entries; footer item opens WorkspaceCreateDialog.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { ChevronDown, Check, Plus } from 'lucide-react'
import {
  DropdownMenu, DropdownMenuTrigger, DropdownMenuContent,
  DropdownMenuItem, DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  selectWorkspaceAtom,
  refreshWorkspacesAtom,
} from '@/atoms/workspace'
import { WorkspaceCreateDialog } from '@/components/workspace/WorkspaceCreateDialog'
import { cn } from '@/lib/utils'

const isMac = typeof navigator !== 'undefined'
  && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)
const modPrefix = isMac ? '⌘' : 'Ctrl+'

const MAX_NAME_CHARS = 12

function truncateName(name: string): string {
  if (name.length <= MAX_NAME_CHARS) return name
  return `${name.slice(0, MAX_NAME_CHARS)}…`
}

export function TabBarWorkspaceChip(): React.ReactElement | null {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const refresh = useSetAtom(refreshWorkspacesAtom)
  const [createOpen, setCreateOpen] = React.useState(false)

  const active = workspaces.find((w) => w.id === activeId)
  if (!active) return null

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="titlebar-no-drag flex items-center gap-1 px-2 py-1 rounded-md
                       text-[12px] text-foreground/80 hover:text-foreground
                       hover:bg-foreground/[0.04] transition-colors shrink-0"
            aria-label={`工作区: ${active.name}`}
            title={`工作区: ${active.name}`}
          >
            <span className="leading-none text-[13px]">{active.icon}</span>
            <span className="font-medium">{truncateName(active.name)}</span>
            <ChevronDown className="size-3 text-muted-foreground/60" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" sideOffset={4} className="w-56 z-[100]">
          {workspaces.map((w, i) => (
            <DropdownMenuItem
              key={w.id}
              onSelect={() => { void selectWorkspace(w.id) }}
              className="flex items-center gap-2 text-xs"
            >
              <Check
                className={cn(
                  'size-3.5 shrink-0',
                  w.id === activeId ? 'opacity-100' : 'opacity-0'
                )}
              />
              <span className="text-[13px] leading-none">{w.icon}</span>
              <span className="flex-1 truncate">{w.name}</span>
              {i < 9 && (
                <span className="text-[10px] text-muted-foreground/60 font-mono shrink-0">
                  {modPrefix}{i + 1}
                </span>
              )}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onSelect={() => setCreateOpen(true)}
            className="flex items-center gap-2 text-xs text-primary"
          >
            <Plus className="size-3.5" />
            新建工作区
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <WorkspaceCreateDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={async (ws) => {
          await refresh()
          void selectWorkspace(ws.id)
        }}
      />
    </>
  )
}
