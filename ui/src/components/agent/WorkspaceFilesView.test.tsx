import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { TooltipProvider } from '@/components/ui/tooltip'
import { WorkspaceFilesView } from './SidePanel'
import { workspaceAttachedDirsMapAtom, agentSessionAttachedDirsMapAtom, currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'

vi.mock('@/lib/tauri-bridge', () => ({
  attachWorkspaceDirectory: vi.fn(),
  detachWorkspaceDirectory: vi.fn(),
  attachSessionDirectory: vi.fn(),
  detachSessionDirectory: vi.fn(),
  openFolderDialog: vi.fn().mockResolvedValue(null),
  showInFinder: vi.fn(),
  openFile: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: vi.fn((path: string) => `asset://${path}`),
  invoke: vi.fn(),
}))

vi.mock('@/components/file-browser', () => ({
  FileBrowser: () => <div data-testid="file-browser" />,
  FileDropZone: () => <div data-testid="file-drop-zone" />,
}))

describe('WorkspaceFilesView', () => {
  it('renders attached-dirs section header when workspace has attached dirs', () => {
    const store = createStore()
    store.set(workspaceAttachedDirsMapAtom, new Map([['ws-x', ['/tmp/extra']]]))
    store.set(agentSessionAttachedDirsMapAtom, new Map())
    store.set(currentAgentWorkspaceIdAtom, 'ws-x')

    render(
      <Provider store={store}>
        <TooltipProvider>
          <WorkspaceFilesView sessionId="s-1" sessionPath="/some/path" />
        </TooltipProvider>
      </Provider>
    )
    expect(screen.getByText(/附加目录/)).toBeTruthy()
  })
})
