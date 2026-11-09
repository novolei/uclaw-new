import * as React from 'react'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { CompactionFoldCard } from './CompactionFoldCard'

const core = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => core)

vi.mock('@/components/ai-elements/message', () => ({
  MessageResponse: ({ children }: { children?: React.ReactNode }) => (
    <div data-testid="fold-body">{children}</div>
  ),
}))

const FOLD = '## Earlier conversation (compacted)\n\n### 目标\n修 bug\n\n### 进度\n完成一半'
const BLOCK = [
  FOLD,
  '',
  '<file_working_set>\n已修改的文件：\n- src/a.rs（修改 3 次）\n- src/b.ts（新建）\n读过的文件：\n- README.md\n</file_working_set>',
  '',
  '<current_plan path=".uclaw/plans/p.md">\n- [ ] step\n</current_plan>',
  '',
  '<recent_files>\n<file path="src/a.rs">\n### not a section\n</file>\n</recent_files>',
].join('\n')

beforeEach(() => {
  core.invoke.mockReset()
  core.invoke.mockResolvedValue([])
})

describe('CompactionFoldCard', () => {
  it('collapsed copy names the kept modified files', () => {
    renderWithProviders(<CompactionFoldCard markdown={BLOCK} createdAt={Date.now()} sessionId="s1" />)
    expect(screen.getByText('上下文已压缩 · 保留了 2 个改动文件的记录')).toBeTruthy()
    expect(screen.queryByText('上下文已压缩为结构化摘要')).toBeNull()
  })

  it('collapsed copy keeps the section count for an old block without files', () => {
    renderWithProviders(<CompactionFoldCard markdown={FOLD} createdAt={Date.now()} sessionId="s1" />)
    expect(screen.getByText('上下文已压缩为结构化摘要')).toBeTruthy()
    expect(screen.getByText('· 2 个分项')).toBeTruthy()
  })

  it('expanded view shows summary, file chips, plan and reloaded files without raw tags', async () => {
    const { user } = renderWithProviders(
      <CompactionFoldCard markdown={BLOCK} createdAt={Date.now()} sessionId="s1" />,
    )
    await user.click(screen.getByRole('button', { name: /上下文已压缩/ }))

    expect(screen.getByText('上下文摘要')).toBeTruthy()
    expect(screen.queryByText(/StructuredFold/)).toBeNull()
    // Section count ignores `###` lines inside reloaded file bodies.
    expect(screen.getByText(/^2 个分项/)).toBeTruthy()

    const body = screen.getByTestId('fold-body').textContent ?? ''
    expect(body).toContain('### 目标')
    expect(body).not.toContain('Earlier conversation')
    expect(body).not.toContain('<file_working_set>')
    expect(body).not.toContain('<recent_files>')

    expect(screen.getByText('改动过的文件')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'src/a.rs' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'src/b.ts' })).toBeTruthy()
    expect(screen.getByText('当前计划：.uclaw/plans/p.md')).toBeTruthy()
    expect(screen.getByText('已重新载入：src/a.rs')).toBeTruthy()
    expect(document.body.textContent).not.toContain('<current_plan')
  })

  it('clicking a file chip opens a preview for that path', async () => {
    const { user } = renderWithProviders(
      <CompactionFoldCard markdown={BLOCK} createdAt={Date.now()} sessionId="s1" />,
    )
    await user.click(screen.getByRole('button', { name: /上下文已压缩/ }))
    await user.click(screen.getByRole('button', { name: 'src/b.ts' }))
    await waitFor(() =>
      expect(core.invoke).toHaveBeenCalledWith('preview_resolve_chips', {
        paths: ['src/b.ts'],
        sessionId: 's1',
      }),
    )
  })
})
