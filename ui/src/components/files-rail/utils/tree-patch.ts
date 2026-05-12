/**
 * tree-patch — Apply file-system change events to an in-memory tree.
 *
 * Strategy: walk the change list once, locate the target node by its parent
 * relPath, then mutate the parent's children array. Unexpanded directories
 * (those whose `children` is undefined) ignore events targeting their subtree
 * — they re-fetch lazily on expand. This keeps event handling O(depth) per
 * change without ever walking deep into uncached parts of the tree.
 */

export type NodeKind = 'file' | 'directory'

export interface TreeNode {
  kind: NodeKind
  /** Path relative to the mount root, forward-slash separated. */
  relPath: string
  /** Last segment of relPath. */
  name: string
  size: number
  mtimeMs: number
  /** Undefined → not yet expanded. Empty array → expanded, empty dir. */
  children?: TreeNode[]
}

export type ChangeKind = 'created' | 'modified' | 'removed' | 'renamed'

export interface FileChange {
  kind: ChangeKind
  relPath: string
  newRelPath?: string
  isDir: boolean
}

const parentRel = (rel: string): string => {
  const i = rel.lastIndexOf('/')
  return i === -1 ? '' : rel.slice(0, i)
}

const basename = (rel: string): string => {
  const i = rel.lastIndexOf('/')
  return i === -1 ? rel : rel.slice(i + 1)
}

const sortNodes = (a: TreeNode, b: TreeNode): number => {
  if (a.kind === 'directory' && b.kind === 'file') return -1
  if (a.kind === 'file' && b.kind === 'directory') return 1
  return a.name.toLowerCase().localeCompare(b.name.toLowerCase())
}

/**
 * Locate the node array at parent relPath. Returns `undefined` if the parent
 * is not expanded (children === undefined) — that's an intentional signal to
 * the caller that this event can be ignored (lazy expand will re-fetch).
 */
function findParentChildren(roots: TreeNode[], parent: string): TreeNode[] | undefined {
  if (parent === '') return roots
  const segments = parent.split('/')
  let current: TreeNode[] | undefined = roots
  for (const seg of segments) {
    if (!current) return undefined
    const next: TreeNode | undefined = current.find((n) => n.name === seg && n.kind === 'directory')
    if (!next || next.children === undefined) return undefined
    current = next.children
  }
  return current
}

function insertSorted(siblings: TreeNode[], node: TreeNode): TreeNode[] {
  const out = [...siblings, node]
  out.sort(sortNodes)
  return out
}

function withReplacedChildren(
  roots: TreeNode[],
  parent: string,
  nextChildren: TreeNode[],
): TreeNode[] {
  if (parent === '') return nextChildren
  return roots.map((n) => {
    if (n.kind !== 'directory') return n
    if (n.relPath === parent) {
      return { ...n, children: nextChildren }
    }
    if (parent.startsWith(`${n.relPath}/`) && n.children) {
      return { ...n, children: withReplacedChildren(n.children, parent, nextChildren) }
    }
    return n
  })
}

export function applyChanges(roots: TreeNode[], changes: FileChange[]): TreeNode[] {
  if (changes.length === 0) return roots
  let current = roots
  for (const c of changes) {
    current = applySingle(current, c)
  }
  return current
}

function applySingle(roots: TreeNode[], c: FileChange): TreeNode[] {
  const parent = parentRel(c.relPath)
  const siblings = findParentChildren(roots, parent)
  if (!siblings) return roots // parent not expanded — drop

  if (c.kind === 'created') {
    if (siblings.some((n) => n.relPath === c.relPath)) return roots
    const node: TreeNode = {
      kind: c.isDir ? 'directory' : 'file',
      relPath: c.relPath,
      name: basename(c.relPath),
      size: 0,
      mtimeMs: Date.now(),
    }
    return withReplacedChildren(roots, parent, insertSorted(siblings, node))
  }

  if (c.kind === 'removed') {
    const next = siblings.filter((n) => n.relPath !== c.relPath)
    if (next.length === siblings.length) return roots
    return withReplacedChildren(roots, parent, next)
  }

  if (c.kind === 'modified') {
    const next = siblings.map((n) =>
      n.relPath === c.relPath ? { ...n, mtimeMs: Date.now() } : n,
    )
    return withReplacedChildren(roots, parent, next)
  }

  if (c.kind === 'renamed' && c.newRelPath) {
    const removed = siblings.filter((n) => n.relPath !== c.relPath)
    const newNode: TreeNode = {
      kind: c.isDir ? 'directory' : 'file',
      relPath: c.newRelPath,
      name: basename(c.newRelPath),
      size: 0,
      mtimeMs: Date.now(),
    }
    return withReplacedChildren(roots, parent, insertSorted(removed, newNode))
  }

  return roots
}
