/**
 * Tests for `SidebarGitActions` — the relocated `提交` chip + its
 * hairline divider, mounted in `LeftSidebar`'s capability row.
 *
 * Focuses on the boundary behavior most likely to regress silently:
 *   - Renders `null` (no divider) when no workspace cwd is set
 *   - Renders trigger + divider when cwd is set
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { Provider as JotaiProvider, createStore } from 'jotai'
import { SidebarGitActions } from './SidebarGitActions'
import { activeWorkspaceIdAtom, workspacesAtom } from '@/atoms/workspace'

// Mock the IPC layer — both `gitIsRepo` and `gitCurrentBranch` are
// called inside the cwd-change effect. Returning a fixed pair keeps
// the component on the "repo present, branch known" happy path.
vi.mock('@/modules/git/api', () => ({
  gitIsRepo: vi.fn(async () => true),
  gitCurrentBranch: vi.fn(async () => 'main'),
}))

// GitActionsPicker + GitWorkbenchDialog do their own internal IPC; we
// don't care about their internals here — mock so the test stays
// focused on the wrapper.
vi.mock('@/components/chat/git/GitActionsPicker', () => ({
  GitActionsPicker: ({ variant }: { variant?: string }) => (
    <button type="button" data-testid="actions-picker" data-variant={variant}>
      提交
    </button>
  ),
}))
vi.mock('@/components/chat/git/GitWorkbenchDialog', () => ({
  GitWorkbenchDialog: () => null,
}))

function makeStoreWithCwd(cwd: string | null) {
  const store = createStore()
  if (cwd) {
    store.set(workspacesAtom, [{
      id: 'ws-1',
      name: 'test',
      icon: '📁',
      path: cwd,
      attachedDirs: [],
      sortOrder: 0,
      skillTags: [],
      createdAt: '',
      updatedAt: '',
    } as any])
    store.set(activeWorkspaceIdAtom, 'ws-1')
  }
  return store
}

describe('SidebarGitActions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders null when no workspace cwd is set (no divider orphan)', () => {
    const store = makeStoreWithCwd(null)
    const { container } = renderWithProviders(
      <JotaiProvider store={store}>
        <SidebarGitActions />
      </JotaiProvider>,
    )
    // The component returns null on no-cwd — the JotaiProvider wrapper
    // renders just its child, which should be empty markup. No divider,
    // no trigger.
    expect(container.querySelector('[data-testid="actions-picker"]')).toBeNull()
    expect(container.querySelector('[aria-hidden="true"]')).toBeNull()
  })

  it('renders trigger + divider when cwd is set', () => {
    const store = makeStoreWithCwd('/Users/test/workspace')
    renderWithProviders(
      <JotaiProvider store={store}>
        <SidebarGitActions />
      </JotaiProvider>,
    )
    // GitActionsPicker mock → rendered as button. Variant must be the
    // sidebar value so the styling switches correctly.
    const trigger = screen.getByTestId('actions-picker')
    expect(trigger).toBeTruthy()
    expect(trigger.getAttribute('data-variant')).toBe('sidebar')
    // Hairline divider rendered as aria-hidden div.
    const divider = document.querySelector('[aria-hidden="true"]')
    expect(divider).not.toBeNull()
  })
})
