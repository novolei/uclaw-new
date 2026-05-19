/**
 * TabBarWorkspaceChip — passive label at TabBar's leftmost edge
 * showing the active workspace's icon + truncated name.
 *
 * Phase 4b: downgraded from Phase 4a's interactive dropdown to a pure
 * label. Workspace switching now happens via the bottom
 * WorkspaceSwitcherBar; this chip exists as a supplementary visual
 * anchor in the TabBar chrome.
 *
 * Icon resolves through `getWorkspaceIcon` so both new-style lucide
 * names ('Calendar', 'Folder', ...) and legacy emoji ('📁') render
 * correctly — without this, the workspace.icon string would render
 * as raw text ("Calendar 2222" instead of the calendar glyph).
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'
import { getWorkspaceIcon } from '@/lib/workspace-icons'

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

  const Icon = getWorkspaceIcon(active.icon)

  return (
    // The chip is a purely passive label — no click target, no menu.
    // It must explicitly stay in the drag region because it sits inside the
    // animated strip that covers the TabBar parent.
    <div data-tauri-drag-region className="relative shrink-0 titlebar-drag-region">
      <div
        data-tauri-drag-region
        className="flex items-center gap-1.5 px-2 py-1 rounded-md
                   text-[12px] text-foreground/75 titlebar-drag-region"
        title={`工作区: ${active.name}`}
      >
        <span
          className="inline-flex items-center justify-center size-4 rounded
                     bg-primary/15 text-primary shrink-0"
          aria-hidden
        >
          <Icon className="size-3" />
        </span>
        <span className="font-medium">{truncateName(active.name)}</span>
      </div>
      {/* Right-edge separator — visually mirrors the inter-tab divider
          rendered by TabBarItem, so the chip-to-first-tab gap reads
          with the same rhythm as the gaps between tabs. */}
      <span
        aria-hidden
        className="pointer-events-none absolute right-0 top-2 bottom-2 w-px bg-border/60"
      />
    </div>
  )
}
