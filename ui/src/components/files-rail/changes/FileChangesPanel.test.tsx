import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor, within } from '@/test-utils/render'
import { FileChangesPanel } from './FileChangesPanel'
import { agentMessageRefreshAtom } from '@/atoms/agent-atoms'
import type { FileWorkingSet } from '@/lib/types'

const bridge = vi.hoisted(() => ({
  getSessionFileWorkingSet: vi.fn(),
  onFileWorkingSetChanged: vi.fn(),
  listeners: [] as Array<(p: { sessionId: string }) => void>,
}))

vi.mock('@/lib/tauri-bridge', () => ({
  getSessionFileWorkingSet: bridge.getSessionFileWorkingSet,
  onFileWorkingSetChanged: bridge.onFileWorkingSetChanged,
}))

const core = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: core.invoke,
  convertFileSrc: (s: string) => s,
}))

const EMPTY: FileWorkingSet = { modified: [], read: [], activePlan: null }

const SET: FileWorkingSet = {
  // Deliberately out of order — the panel must sort by lastModifiedAt desc.
  modified: [
    {
      path: 'src/old.ts', created: false, deleted: false, editCount: 2,
      changes: ['edit a', 'edit b'], lastModifiedAt: 100, rewound: false,
    },
    {
      path: 'src/new.ts', created: true, deleted: false, editCount: 1,
      changes: ['created file'], lastModifiedAt: 300, rewound: false,
    },
    {
      path: 'src/gone.ts', created: false, deleted: true, editCount: 3,
      changes: ['deleted'], lastModifiedAt: 200, rewound: true,
    },
  ],
  read: [
    { path: 'README.md', ranges: ['1-40', '80-120'], lastReadAt: 50 },
  ],
  activePlan: '.uclaw/plans/refactor.md',
}

beforeEach(() => {
  bridge.listeners = []
  bridge.getSessionFileWorkingSet.mockReset()
  bridge.onFileWorkingSetChanged.mockReset()
  bridge.onFileWorkingSetChanged.mockImplementation(
    async (cb: (p: { sessionId: string }) => void) => {
      bridge.listeners.push(cb)
      return () => {
        bridge.listeners = bridge.listeners.filter((l) => l !== cb)
      }
    },
  )
  core.invoke.mockReset()
  core.invoke.mockResolvedValue([
    { input: 'x', exists: true, mountId: 'workspace:w1', relPath: 'src/old.ts', absolutePath: '/ws/src/old.ts' },
  ])
})

describe('FileChangesPanel', () => {
  it('shows the empty state when there is no session', () => {
    renderWithProviders(<FileChangesPanel sessionId={null} />)
    expect(screen.getByText('这个会话还没有文件改动')).toBeTruthy()
    expect(bridge.getSessionFileWorkingSet).not.toHaveBeenCalled()
  })

  it('shows the empty state when the working set is empty', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(EMPTY)
    renderWithProviders(<FileChangesPanel sessionId="s1" />)
    await waitFor(() => expect(bridge.getSessionFileWorkingSet).toHaveBeenCalledWith('s1'))
    expect(screen.getByText('这个会话还没有文件改动')).toBeTruthy()
  })

  it('renders modified files sorted newest first with tags, counts and rewound label', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(SET)
    renderWithProviders(<FileChangesPanel sessionId="s1" />)
    expect(await screen.findByText('已修改 · 3')).toBeTruthy()

    const rows = screen.getAllByTestId('modified-file-row')
    expect(rows.map((r) => r.getAttribute('data-path'))).toEqual([
      'src/new.ts', 'src/gone.ts', 'src/old.ts',
    ])
    expect(within(rows[0]).getByText('新建')).toBeTruthy()
    expect(within(rows[0]).getByText('修改 1 次')).toBeTruthy()
    expect(within(rows[1]).getByText('已删除')).toBeTruthy()
    expect(within(rows[1]).getByText('修改 3 次')).toBeTruthy()
    expect(within(rows[1]).getByText('已回退的轮次 · 磁盘仍保留')).toBeTruthy()
    expect(within(rows[2]).getByText('修改 2 次')).toBeTruthy()
    expect(within(rows[2]).queryByText('已回退的轮次 · 磁盘仍保留')).toBeNull()
    expect(within(rows[0]).queryByText('已回退的轮次 · 磁盘仍保留')).toBeNull()
  })

  it('expands the change summary when the row is clicked', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(SET)
    const { user } = renderWithProviders(<FileChangesPanel sessionId="s1" />)
    await screen.findByText('已修改 · 3')
    expect(screen.queryByText('edit b')).toBeNull()
    await user.click(screen.getByText('修改 2 次'))
    expect(screen.getByText('edit a')).toBeTruthy()
    expect(screen.getByText('edit b')).toBeTruthy()
  })

  it('shows the active plan line', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(SET)
    renderWithProviders(<FileChangesPanel sessionId="s1" />)
    expect(await screen.findByText('当前计划：refactor.md')).toBeTruthy()
  })

  it('keeps the read section collapsed by default and expands it on click', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(SET)
    const { user } = renderWithProviders(<FileChangesPanel sessionId="s1" />)
    const header = await screen.findByText('读过 · 1')
    expect(screen.queryByText('README.md')).toBeNull()
    await user.click(header)
    expect(screen.getByText('README.md')).toBeTruthy()
    expect(screen.getByText('1-40, 80-120')).toBeTruthy()
  })

  it('refetches on a matching file_working_set_changed event only', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(SET)
    renderWithProviders(<FileChangesPanel sessionId="s1" />)
    await screen.findByText('已修改 · 3')
    await waitFor(() => expect(bridge.listeners.length).toBe(1))
    expect(bridge.getSessionFileWorkingSet).toHaveBeenCalledTimes(1)

    bridge.listeners[0]({ sessionId: 'other' })
    await new Promise((r) => setTimeout(r, 10))
    expect(bridge.getSessionFileWorkingSet).toHaveBeenCalledTimes(1)

    bridge.listeners[0]({ sessionId: 's1' })
    await waitFor(() => expect(bridge.getSessionFileWorkingSet).toHaveBeenCalledTimes(2))
  })

  it('refetches when the session refresh version bumps', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(SET)
    const { store } = renderWithProviders(<FileChangesPanel sessionId="s1" />)
    await screen.findByText('已修改 · 3')
    expect(bridge.getSessionFileWorkingSet).toHaveBeenCalledTimes(1)
    store.set(agentMessageRefreshAtom, new Map([['other', 1]]))
    await new Promise((r) => setTimeout(r, 10))
    expect(bridge.getSessionFileWorkingSet).toHaveBeenCalledTimes(1)
    store.set(agentMessageRefreshAtom, new Map([['other', 1], ['s1', 1]]))
    await waitFor(() => expect(bridge.getSessionFileWorkingSet).toHaveBeenCalledTimes(2))
  })

  it('opens a preview when a live path is clicked but not for a deleted file', async () => {
    bridge.getSessionFileWorkingSet.mockResolvedValue(SET)
    const { user } = renderWithProviders(<FileChangesPanel sessionId="s1" />)
    await screen.findByText('已修改 · 3')

    const gone = screen.getByRole('button', { name: /src\/gone\.ts/ })
    expect((gone as HTMLButtonElement).disabled).toBe(true)
    await user.click(gone)
    expect(core.invoke).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: /src\/old\.ts/ }))
    await waitFor(() =>
      expect(core.invoke).toHaveBeenCalledWith('preview_resolve_chips', {
        paths: ['src/old.ts'],
        sessionId: 's1',
      }),
    )
    // Clicking the path opens the preview — it does not toggle the summary.
    expect(screen.queryByText('edit a')).toBeNull()
  })
})
