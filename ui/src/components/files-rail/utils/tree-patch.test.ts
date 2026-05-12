import { describe, it, expect } from 'vitest'
import { applyChanges, type TreeNode } from './tree-patch'

const dir = (rel: string, name: string, children?: TreeNode[]): TreeNode => ({
  kind: 'directory', relPath: rel, name, size: 0, mtimeMs: 0, children,
})
const file = (rel: string, name: string, mtime = 0): TreeNode => ({
  kind: 'file', relPath: rel, name, size: 1, mtimeMs: mtime,
})

describe('tree-patch', () => {
  it('returns the original tree when no changes', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt')]
    const next = applyChanges(t, [])
    expect(next).toBe(t)
  })

  it('inserts a created file alphabetically into root', () => {
    const t: TreeNode[] = [file('b.txt', 'b.txt'), file('d.txt', 'd.txt')]
    const next = applyChanges(t, [{ kind: 'created', relPath: 'c.txt', isDir: false }])
    expect(next.map((n) => n.relPath)).toEqual(['b.txt', 'c.txt', 'd.txt'])
  })

  it('inserts dirs before files at the same level', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt')]
    const next = applyChanges(t, [{ kind: 'created', relPath: 'sub', isDir: true }])
    expect(next.map((n) => n.name)).toEqual(['sub', 'a.txt'])
  })

  it('removes a deleted file', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt'), file('b.txt', 'b.txt')]
    const next = applyChanges(t, [{ kind: 'removed', relPath: 'a.txt', isDir: false }])
    expect(next.map((n) => n.name)).toEqual(['b.txt'])
  })

  it('updates mtime on modify', () => {
    const t: TreeNode[] = [file('a.txt', 'a.txt', 1000)]
    const next = applyChanges(t, [{ kind: 'modified', relPath: 'a.txt', isDir: false }])
    expect(next[0].mtimeMs).toBeGreaterThan(1000)
  })

  it('ignores events targeting unexpanded subtrees (no children loaded)', () => {
    const t: TreeNode[] = [dir('sub', 'sub')] // sub has no children loaded
    const next = applyChanges(t, [{ kind: 'created', relPath: 'sub/inner.txt', isDir: false }])
    // sub still has no children — lazy expand will fetch fresh
    expect(next[0].children).toBeUndefined()
  })

  it('applies a change inside an expanded subtree', () => {
    const t: TreeNode[] = [dir('sub', 'sub', [file('sub/a.txt', 'a.txt')])]
    const next = applyChanges(t, [{ kind: 'created', relPath: 'sub/b.txt', isDir: false }])
    expect(next[0].children?.map((n) => n.name)).toEqual(['a.txt', 'b.txt'])
  })

  it('handles rename as remove-then-insert', () => {
    const t: TreeNode[] = [file('old.txt', 'old.txt')]
    const next = applyChanges(t, [
      { kind: 'renamed', relPath: 'old.txt', newRelPath: 'new.txt', isDir: false },
    ])
    expect(next.map((n) => n.name)).toEqual(['new.txt'])
  })
})
