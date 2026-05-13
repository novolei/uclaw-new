import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { RenameInput } from './RenameInput'

describe('RenameInput', () => {
  it('rejects empty name', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts', 'bar.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.keyboard('{Enter}')
    expect(onCommit).not.toHaveBeenCalled()
    expect(screen.getByText('名称不能为空')).toBeTruthy()
  })

  it('rejects separator characters', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.type(input, 'bad/name.ts')
    await user.keyboard('{Enter}')
    expect(onCommit).not.toHaveBeenCalled()
    expect(screen.getByText('名称不能包含 / \\ :')).toBeTruthy()
  })

  it('rejects duplicate sibling', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts', 'bar.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.type(input, 'bar.ts')
    await user.keyboard('{Enter}')
    expect(onCommit).not.toHaveBeenCalled()
    expect(screen.getByText('已存在同名文件')).toBeTruthy()
  })

  it('commits on Enter when valid', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.type(input, 'renamed.ts')
    await user.keyboard('{Enter}')
    expect(onCommit).toHaveBeenCalledWith('renamed.ts')
  })

  it('cancels on Escape', async () => {
    const onCancel = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts'])}
        onCommit={vi.fn()}
        onCancel={onCancel}
      />
    )
    const input = screen.getByRole('textbox')
    await user.click(input)
    await user.keyboard('{Escape}')
    expect(onCancel).toHaveBeenCalled()
  })

  it('does not error when name unchanged (same as initial)', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts', 'bar.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.click(input)
    await user.keyboard('{Enter}')
    expect(onCommit).toHaveBeenCalledWith('foo.ts')
  })
})
