import { describe, it, expect, vi } from 'vitest'
import { screen } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { BranchPicker } from './BranchPicker'

// Mock the IPC layer used by useBranchPicker.
vi.mock('@/modules/git/api', () => ({
  gitBranches: vi.fn(async () => '* main         abcdef1 init\n  feat/foo     1234567 wip'),
  gitStatus: vi.fn(async () => '## main\n M src/foo.ts'),
  gitCheckoutBranch: vi.fn(async () => undefined),
  gitCreateBranch: vi.fn(async () => undefined),
  gitInitRepo: vi.fn(async () => undefined),
  parseBranchList: vi.fn((raw: string) =>
    raw.split('\n').filter(Boolean).map((line) => {
      const isCurrent = line.trim().startsWith('*')
      const name = line.replace(/^[*+]\s*/, '').split(/\s+/)[0]
      return { name, isCurrent }
    }),
  ),
  uncommittedFromStatus: vi.fn((raw: string | null) =>
    raw ? raw.split('\n').slice(1).filter((l) => l.trim().length > 0).length : 0,
  ),
}))

// sonner toast is called by BranchPicker's no-repo init flow; mock it.
vi.mock('sonner', () => ({
  toast: vi.fn(),
}))

describe('BranchPicker', () => {
  it('renders the current branch label on the trigger', () => {
    renderWithProviders(
      <BranchPicker
        cwd="/tmp/test-repo"
        currentBranch="main"
        isGitRepo={true}
      />,
    )
    // Trigger displays the current branch name
    expect(screen.getByText('main')).toBeInTheDocument()
    // ARIA label confirms it's the active picker, not no-repo state
    expect(screen.getByRole('button', { name: '切换 git 分支' })).toBeInTheDocument()
  })

  it('shows the no-repo amber affordance when isGitRepo is false', () => {
    renderWithProviders(
      <BranchPicker
        cwd="/tmp/empty-dir"
        currentBranch=""
        isGitRepo={false}
      />,
    )
    // Amber prompt text
    expect(screen.getByText(/无 Git 仓库/)).toBeInTheDocument()
    // Init-state aria-label
    expect(screen.getByRole('button', { name: '初始化 Git 仓库' })).toBeInTheDocument()
  })
})
