/**
 * useFileTree — Lazy directory loading for one mount.
 *
 * Owns: root-level children + an expanded-paths cache. Calls
 * `filesRailReadDir` only when a directory is first expanded; subsequent
 * collapse/expand cycles reuse cached children. Watcher events (Task 7) apply
 * via tree-patch without going through this hook.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import {
  expandedPathsAtomFamily,
  fileTreeAtomFamily,
} from '@/atoms/files-rail-atoms'
import { filesRailReadDir } from '@/lib/tauri-bridge'
import {
  applyChanges,
  type FileChange,
  type TreeNode,
} from '@/components/files-rail/utils/tree-patch'

interface UseFileTreeResult {
  nodes: TreeNode[]
  loadState: 'idle' | 'loading' | 'ready' | 'error'
  errorMessage?: string
  isExpanded: (relPath: string) => boolean
  toggleExpand: (relPath: string, isDir: boolean) => Promise<void>
  applyExternalChanges: (changes: FileChange[]) => void
  reload: () => Promise<void>
}

export function useFileTree(mountId: string, sessionId: string | null = null): UseFileTreeResult {
  const [tree, setTree] = useAtom(fileTreeAtomFamily(mountId))
  const [expanded, setExpanded] = useAtom(expandedPathsAtomFamily(mountId))

  const reload = React.useCallback(async () => {
    setTree({ status: 'loading' })
    try {
      const nodes = await filesRailReadDir(mountId, '', sessionId)
      setTree({ status: 'ready', nodes })
    } catch (err) {
      setTree({ status: 'error', message: String(err) })
    }
  }, [mountId, sessionId, setTree])

  React.useEffect(() => {
    if (tree.status === 'idle') void reload()
  }, [tree.status, reload])

  const isExpanded = React.useCallback(
    (relPath: string) => expanded.has(relPath),
    [expanded],
  )

  const toggleExpand = React.useCallback(
    async (relPath: string, isDir: boolean) => {
      if (!isDir) return
      const next = new Set(expanded)
      if (next.has(relPath)) {
        next.delete(relPath)
        setExpanded(next)
        return
      }
      next.add(relPath)
      setExpanded(next)
      if (tree.status !== 'ready') return
      const targetHasChildren = treeHasChildrenAt(tree.nodes, relPath)
      if (targetHasChildren) return
      try {
        const fetched = await filesRailReadDir(mountId, relPath, sessionId)
        setTree((prev) => {
          if (prev.status !== 'ready') return prev
          return { status: 'ready', nodes: setChildrenAt(prev.nodes, relPath, fetched) }
        })
      } catch {
        /* silent — error surfaces at the dir node */
      }
    },
    [expanded, setExpanded, tree, mountId, sessionId, setTree],
  )

  const applyExternalChanges = React.useCallback(
    (changes: FileChange[]) => {
      setTree((prev) => {
        if (prev.status !== 'ready') return prev
        const next = applyChanges(prev.nodes, changes)
        return next === prev.nodes ? prev : { status: 'ready', nodes: next }
      })
    },
    [setTree],
  )

  return {
    nodes: tree.status === 'ready' ? tree.nodes : [],
    loadState: tree.status,
    errorMessage: tree.status === 'error' ? tree.message : undefined,
    isExpanded,
    toggleExpand,
    applyExternalChanges,
    reload,
  }
}

function treeHasChildrenAt(nodes: TreeNode[], relPath: string): boolean {
  for (const n of nodes) {
    if (n.relPath === relPath) return n.children !== undefined
    if (n.kind === 'directory' && n.children && relPath.startsWith(`${n.relPath}/`)) {
      return treeHasChildrenAt(n.children, relPath)
    }
  }
  return false
}

function setChildrenAt(
  nodes: TreeNode[],
  relPath: string,
  children: TreeNode[],
): TreeNode[] {
  return nodes.map((n) => {
    if (n.kind !== 'directory') return n
    if (n.relPath === relPath) return { ...n, children }
    if (n.children && relPath.startsWith(`${n.relPath}/`)) {
      return { ...n, children: setChildrenAt(n.children, relPath, children) }
    }
    return n
  })
}
