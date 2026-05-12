import { describe, it, expect, vi } from 'vitest'
import { screen } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { GitActionsPicker } from './GitActionsPicker'

vi.mock('@/modules/git/api', () => ({
  ghAvailable: vi.fn(async () => true),
  gitCommit: vi.fn(async () => ({ status: 'created', message: 'feat: test' })),
  gitCommitPushPr: vi.fn(async () => 'Committed → branch `feat/x` → PR https://...'),
  gitCreateBranch: vi.fn(async () => undefined),
  gitInitRepo: vi.fn(async () => undefined),
  ghCreatePr: vi.fn(async () => ({ url: '...', wasExisting: false, base: 'main' })),
}))

vi.mock('sonner', () => ({
  toast: vi.fn(),
}))

describe('GitActionsPicker', () => {
  it('renders the trigger button', () => {
    renderWithProviders(
      <GitActionsPicker cwd="/tmp/test-repo" isGitRepo={true} />,
    )
    // The trigger button exists (exact label text comes from the source)
    const buttons = screen.getAllByRole('button')
    expect(buttons.length).toBeGreaterThan(0)
  })

  it('renders with isGitRepo=null (probing state) without crashing', () => {
    renderWithProviders(
      <GitActionsPicker cwd="/tmp/test-repo" isGitRepo={null} />,
    )
    // Just verify it mounts; probing state shouldn't throw
    const buttons = screen.getAllByRole('button')
    expect(buttons.length).toBeGreaterThan(0)
  })
})
