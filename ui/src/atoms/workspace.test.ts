import { describe, it, expect, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  activeWorkspaceCwdAtom,
  type WorkspaceInfo,
} from './workspace'

function makeWorkspace(id: string, path: string | null): WorkspaceInfo {
  return {
    id,
    name: `Workspace ${id}`,
    icon: 'Folder',
    path,
    attachedDirs: [],
    sortOrder: 0,
    createdAt: '2026-05-13T00:00:00Z',
    updatedAt: '2026-05-13T00:00:00Z',
  }
}

describe('activeWorkspaceCwdAtom', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore()
  })

  it('returns the active workspace path', () => {
    store.set(workspacesAtom, [
      makeWorkspace('w1', '/Users/me/projects/foo'),
      makeWorkspace('w2', '/Users/me/projects/bar'),
    ])
    store.set(activeWorkspaceIdAtom, 'w2')

    expect(store.get(activeWorkspaceCwdAtom)).toBe('/Users/me/projects/bar')
  })

  it('returns null when no workspace is active', () => {
    store.set(workspacesAtom, [makeWorkspace('w1', '/Users/me/projects/foo')])
    store.set(activeWorkspaceIdAtom, null)

    expect(store.get(activeWorkspaceCwdAtom)).toBeNull()
  })

  it('returns null when active workspace has null path', () => {
    store.set(workspacesAtom, [makeWorkspace('w1', null)])
    store.set(activeWorkspaceIdAtom, 'w1')

    expect(store.get(activeWorkspaceCwdAtom)).toBeNull()
  })
})
