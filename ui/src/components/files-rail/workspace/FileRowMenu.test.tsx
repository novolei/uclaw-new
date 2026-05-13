import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FileRowMenu } from './FileRowMenu'
import type { MountRoot } from '@/atoms/files-rail-atoms'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  convertFileSrc: (s: string) => s,
}))

const wsMount: MountRoot = {
  id: 'workspace:abc',
  label: 'Workspace',
  path: '/ws/root',
  kind: 'workspace',
  editable: true,
}
const attachedMount: MountRoot = {
  id: 'workspace-attached:abc:hash123',
  label: 'External',
  path: '/external/dir',
  kind: 'attached_dir',
  editable: false,
}

describe('FileRowMenu', () => {
  it('renders all 5 items enabled on a workspace mount file', async () => {
    const { user } = renderWithProviders(
      <FileRowMenu
        mount={wsMount}
        sessionId="sess-1"
        relPath="sub/foo.ts"
        name="foo.ts"
        isDirectory={false}
        absolutePath="/ws/root/sub/foo.ts"
      />
    )
    await user.click(screen.getByRole('button', { name: '更多操作' }))
    expect(screen.getByText('添加到聊天')).toBeTruthy()
    expect(screen.getByText('在文件夹中显示')).toBeTruthy()
    expect(screen.getByText('移动到…')).toBeTruthy()
    expect(screen.getByText('重命名')).toBeTruthy()
    expect(screen.getByText('删除')).toBeTruthy()
    // None should have data-disabled
    const move = screen.getByText('移动到…').closest('[role="menuitem"]')!
    expect(move.getAttribute('data-disabled')).toBeNull()
  })

  it('disables 移动到… / 重命名 / 删除 on a read-only attached mount', async () => {
    const { user } = renderWithProviders(
      <FileRowMenu
        mount={attachedMount}
        sessionId="sess-1"
        relPath="img.png"
        name="img.png"
        isDirectory={false}
        absolutePath="/external/dir/img.png"
      />
    )
    await user.click(screen.getByRole('button', { name: '更多操作' }))
    const move = screen.getByText('移动到…').closest('[role="menuitem"]')!
    const rename = screen.getByText('重命名').closest('[role="menuitem"]')!
    const del = screen.getByText('删除').closest('[role="menuitem"]')!
    expect(move.getAttribute('data-disabled')).not.toBeNull()
    expect(rename.getAttribute('data-disabled')).not.toBeNull()
    expect(del.getAttribute('data-disabled')).not.toBeNull()
    // Items 1 + 2 stay enabled
    const addToChat = screen.getByText('添加到聊天').closest('[role="menuitem"]')!
    const reveal = screen.getByText('在文件夹中显示').closest('[role="menuitem"]')!
    expect(addToChat.getAttribute('data-disabled')).toBeNull()
    expect(reveal.getAttribute('data-disabled')).toBeNull()
  })

  it('hides 添加到聊天 for directories', async () => {
    const { user } = renderWithProviders(
      <FileRowMenu
        mount={wsMount}
        sessionId="sess-1"
        relPath="sub"
        name="sub"
        isDirectory={true}
        absolutePath="/ws/root/sub"
      />
    )
    await user.click(screen.getByRole('button', { name: '更多操作' }))
    expect(screen.queryByText('添加到聊天')).toBeNull()
  })
})
