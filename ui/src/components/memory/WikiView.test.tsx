import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { WikiView } from './WikiView'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}))

function routeInvoke(overrides: Record<string, unknown> = {}) {
  invokeMock.mockImplementation((cmd: string) => {
    const table: Record<string, unknown> = {
      gbrain_list_pages: [
        { slug: 'person-alice', title: 'Alice', type: 'person', updated_at: '2026-05-10T00:00:00Z' },
        { slug: 'concept-fts', title: 'FTS', type: 'concept', updated_at: '2026-05-09T00:00:00Z' },
      ],
      gbrain_get_stats: { page_count: 2, chunk_count: 10, embedded_count: 8, link_count: 3, tag_count: 1 },
      gbrain_find_orphans: { total_orphans: 1, total_pages: 2 },
      gbrain_get_page: {
        slug: 'person-alice', title: 'Alice', type: 'person',
        compiled_truth: '# Alice\nFounder of Acme.', frontmatter: { type: 'person' },
        created_at: null, updated_at: null, tags: [], raw_markdown: '---\ntype: person\n---\n\n# Alice\nFounder of Acme.',
      },
      gbrain_get_backlinks: [{ from_slug: 'project-falcon', link_type: 'works_at' }],
      gbrain_search: [{ slug: 'concept-fts', title: 'FTS', snippet: 'full text search', similarity: 0.9 }],
      gbrain_get_versions: [{ id: 3, snapshot_at: '2026-05-05T00:00:00Z', compiled_truth: 'old body' }],
      gbrain_put_page: { slug: 'person-alice', title: 'Alice', type: 'person', compiled_truth: '# Alice\nEdited.', frontmatter: { type: 'person' }, created_at: null, updated_at: null, tags: [], raw_markdown: '---\ntype: person\n---\n\n# Alice\nEdited.' },
      gbrain_revert_version: { slug: 'person-alice', title: 'Alice', type: 'person', compiled_truth: 'old body', frontmatter: {}, created_at: null, updated_at: null, tags: [], raw_markdown: 'old body' },
      ...overrides,
    }
    const v = table[cmd]
    if (v instanceof Error) return Promise.reject(v)
    return Promise.resolve(v)
  })
}

describe('WikiView', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    routeInvoke()
  })

  it('renders page list from gbrain_list_pages', async () => {
    renderWithProviders(<WikiView />)
    expect(await screen.findByText('Alice')).toBeInTheDocument()
    expect(screen.getByText('FTS')).toBeInTheDocument()
  })

  it('shows overview stats + orphan badge', async () => {
    renderWithProviders(<WikiView />)
    await screen.findByText('Alice')
    expect(screen.getByText(/2 页/)).toBeInTheDocument()
    expect(screen.getByText(/1 孤儿页/)).toBeInTheDocument()
  })

  it('opens a page and renders markdown + backlinks', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await user.click(await screen.findByText('Alice'))
    await waitFor(() => expect(screen.getByTestId('wiki-detail-body')).toHaveTextContent('Founder of Acme'))
    expect(screen.getByTestId('wiki-backlinks')).toHaveTextContent('project-falcon')
  })

  it('search switches list to result mode', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await screen.findByText('Alice')
    const input = screen.getByTestId('wiki-search-input')
    await user.type(input, 'full text{Enter}')
    await waitFor(() => expect(screen.getByText('full text search')).toBeInTheDocument())
  })

  it('edit flow saves via gbrain_put_page', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await user.click(await screen.findByText('Alice'))
    await user.click(await screen.findByTestId('wiki-edit-btn'))
    await user.click(screen.getByTestId('wiki-save-btn'))
    await waitFor(() => expect(screen.getByTestId('wiki-detail-body')).toHaveTextContent('Edited'))
    expect(invokeMock).toHaveBeenCalledWith('gbrain_put_page', expect.objectContaining({ slug: 'person-alice' }))
  })

  it('version drawer lists versions and reverts', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await user.click(await screen.findByText('Alice'))
    await user.click(await screen.findByTestId('wiki-versions-btn'))
    const drawer = await screen.findByTestId('wiki-version-drawer')
    expect(drawer).toHaveTextContent('回滚到此版本')
    await user.click(screen.getByText('回滚到此版本'))
    await waitFor(() => expect(screen.getByTestId('wiki-detail-body')).toHaveTextContent('old body'))
  })

  it('shows not-connected empty state', async () => {
    routeInvoke({ gbrain_list_pages: new Error('gbrain_not_connected') })
    renderWithProviders(<WikiView />)
    expect(await screen.findByText('gbrain 未连接')).toBeInTheDocument()
  })

  it('opens initialSlug on mount', async () => {
    renderWithProviders(<WikiView initialSlug="person-alice" />)
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('gbrain_get_page', { slug: 'person-alice' }))
  })
})
