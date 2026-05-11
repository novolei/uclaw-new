/**
 * WorkspaceTabCleaner — effect-only component that drops orphan tabs
 * when a workspace is deleted.
 *
 * Per-workspace tab memory (Task 1) keeps tabs in a flat global pool
 * tagged with workspaceId. When workspacesAtom shrinks (workspace
 * deletion), tabs whose workspaceId is no longer present become
 * orphans — invisible (visibleTabsAtom filters them out) but still
 * occupying memory and capable of resurrecting if a new workspace
 * happens to share an id.
 *
 * The cleaner watches workspacesAtom and on every change drops orphan
 * tabs + clears stale entries in workspaceActiveTabIdMapAtom.
 *
 * Mounted once in AppShell next to TabSessionSyncer. Returns null.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { workspacesAtom } from '@/atoms/workspace'
import {
  tabsAtom, workspaceActiveTabIdMapAtom,
} from '@/atoms/tab-atoms'

export function WorkspaceTabCleaner(): null {
  const workspaces = useAtomValue(workspacesAtom)
  const setTabs = useSetAtom(tabsAtom)
  const setActiveMap = useSetAtom(workspaceActiveTabIdMapAtom)

  React.useEffect(() => {
    const live = new Set(workspaces.map((w) => w.id))
    setTabs((prev) => {
      const next = prev.filter((t) => live.has(t.workspaceId))
      return next.length === prev.length ? prev : next
    })
    setActiveMap((prev) => {
      let mutated = false
      const next = new Map(prev)
      for (const k of next.keys()) {
        if (!live.has(k)) { next.delete(k); mutated = true }
      }
      return mutated ? next : prev
    })
  }, [workspaces, setTabs, setActiveMap])

  return null
}
