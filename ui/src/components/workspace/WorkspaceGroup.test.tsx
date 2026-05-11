import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { Provider } from 'jotai'
import { WorkspaceGroup } from './WorkspaceGroup'

// Mock the bridge so tests don't hit a real backend.
vi.mock('@/lib/tauri-bridge', () => ({
  updateWorkspace: vi.fn().mockResolvedValue({}),
  deleteWorkspace: vi.fn().mockResolvedValue(undefined),
  listSpaces: vi.fn().mockResolvedValue([]),
}))

describe('WorkspaceGroup', () => {
  it('shows hover Pencil + Trash buttons on non-default workspace', () => {
    const onSelectSession = vi.fn()
    const onSelectWorkspace = vi.fn()
    render(
      <Provider>
        <WorkspaceGroup
          id="ws-x"
          name="Test"
          icon="📁"
          sessions={[]}
          isActive={false}
          activeSessionId={null}
          onSelectSession={onSelectSession}
          onSelectWorkspace={onSelectWorkspace}
        />
      </Provider>
    )
    expect(screen.getByTitle('重命名')).toBeTruthy()
    expect(screen.getByTitle('删除')).toBeTruthy()
  })

  it('hides Trash and Pencil on default workspace', () => {
    render(
      <Provider>
        <WorkspaceGroup
          id="default"
          name="默认工作区"
          icon="📁"
          sessions={[]}
          isActive={false}
          activeSessionId={null}
          onSelectSession={() => {}}
          onSelectWorkspace={() => {}}
        />
      </Provider>
    )
    expect(screen.queryByTitle('删除')).toBeNull()
    expect(screen.queryByTitle('重命名')).toBeNull()
  })

  it('clicking Pencil enters inline rename mode', () => {
    render(
      <Provider>
        <WorkspaceGroup
          id="ws-x"
          name="Original"
          icon="📁"
          sessions={[]}
          isActive={false}
          activeSessionId={null}
          onSelectSession={() => {}}
          onSelectWorkspace={() => {}}
        />
      </Provider>
    )
    fireEvent.click(screen.getByTitle('重命名'))
    const input = screen.getByDisplayValue('Original') as HTMLInputElement
    expect(input).toBeTruthy()
  })
})
