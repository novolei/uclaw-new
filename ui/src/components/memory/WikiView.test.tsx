import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { WikiView } from './WikiView'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}))

// ─── Fixture data ─────────────────────────────────────────────────────────────
//
// Wire shape from memory_entity_page_* commands. All structs are
// #[serde(rename_all = "camelCase")] in Rust; see models.rs.

const ALICE_NODE_ID = 'node-alice-001'
const FALCON_NODE_ID = 'node-falcon-001'

/** MemoryNodeDetail for Alice (used by list + find_by_slug + put + revert). */
const aliceDetail = {
  node: {
    id: ALICE_NODE_ID,
    spaceId: 'default',
    kind: 'entity_page',
    title: 'Alice',
    metadata: { slug: 'person-alice', subkind: 'person' },
    createdAt: '2026-05-01T00:00:00Z',
    updatedAt: '2026-05-10T00:00:00Z',
  },
  activeVersion: {
    id: 'ver-001',
    nodeId: ALICE_NODE_ID,
    content: '---\ntype: person\n---\n\n# Alice\nFounder of Acme.',
    createdAt: '2026-05-10T00:00:00Z',
  },
  routes: [],
  keywords: [],
}

/** MemoryNodeDetail for Concept FTS (used by list). */
const ftsDetail = {
  node: {
    id: 'node-fts-001',
    spaceId: 'default',
    kind: 'entity_page',
    title: 'FTS',
    metadata: { slug: 'concept-fts', subkind: 'concept' },
    createdAt: '2026-05-01T00:00:00Z',
    updatedAt: '2026-05-09T00:00:00Z',
  },
  activeVersion: {
    id: 'ver-002',
    nodeId: 'node-fts-001',
    content: '# FTS\nFull-text search.',
    createdAt: '2026-05-09T00:00:00Z',
  },
  routes: [],
  keywords: [],
}

/** Updated Alice detail returned after a put. */
const aliceDetailEdited = {
  ...aliceDetail,
  activeVersion: {
    id: 'ver-003',
    nodeId: ALICE_NODE_ID,
    content: '---\ntype: person\n---\n\n# Alice\nEdited.',
    createdAt: '2026-05-11T00:00:00Z',
  },
}

/** Alice detail returned after a revert. */
const aliceDetailReverted = {
  ...aliceDetail,
  activeVersion: {
    id: 'ver-004',
    nodeId: ALICE_NODE_ID,
    content: 'old body',
    createdAt: '2026-05-12T00:00:00Z',
  },
}

function routeInvoke(overrides: Record<string, unknown> = {}) {
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    const table: Record<string, unknown> = {
      // list → Vec<MemoryNodeDetail>
      memory_entity_page_list: [aliceDetail, ftsDetail],

      // stats → EntityPageStats (camelCase on wire)
      memory_entity_page_stats: { pageCount: 2, chunkCount: 10, embeddedCount: 8 },

      // orphans → Vec<EntityPageSummary>
      memory_entity_page_orphans: [{ slug: 'orphan-page', title: 'Orphan Page' }],

      // find_by_slug → MemoryNodeDetail | null
      memory_entity_page_find_by_slug: aliceDetail,

      // backlinks → Vec<EntityBacklink> (camelCase on wire: fromSlug, linkType)
      memory_entity_page_backlinks: [{ fromSlug: 'project-falcon', linkType: 'works_at' }],

      // search → Vec<EntitySearchHit>
      memory_entity_page_search: [{ slug: 'concept-fts', title: 'FTS', snippet: 'full text search' }],

      // versions → Vec<EntityPageVersionMeta> (camelCase: versionId, createdAt, content)
      memory_entity_page_versions: [{ versionId: 'ver-old-001', createdAt: '2026-05-05T00:00:00Z', content: 'old body' }],

      // put → MemoryNodeDetail
      memory_entity_page_put: aliceDetailEdited,

      // revert → MemoryNodeDetail
      memory_entity_page_revert: aliceDetailReverted,

      ...overrides,
    }
    // Disambiguate find_by_slug from other slug-based commands using args.
    void args
    const v = table[cmd]
    if (v instanceof Error) return Promise.reject(v)
    return Promise.resolve(v ?? null)
  })
}

describe('WikiView', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    routeInvoke()
  })

  it('renders page list from memory_entity_page_list', async () => {
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

  it('edit flow saves via memory_entity_page_put', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await user.click(await screen.findByText('Alice'))
    await user.click(await screen.findByTestId('wiki-edit-btn'))
    await user.click(screen.getByTestId('wiki-save-btn'))
    await waitFor(() => expect(screen.getByTestId('wiki-detail-body')).toHaveTextContent('Edited'))
    expect(invokeMock).toHaveBeenCalledWith('memory_entity_page_put', expect.objectContaining({ slug: 'person-alice' }))
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

  it('shows not-connected empty state when entity-page returns not-connected error', async () => {
    routeInvoke({ memory_entity_page_list: new Error('gbrain_not_connected') })
    renderWithProviders(<WikiView />)
    expect(await screen.findByText('gbrain 未连接')).toBeInTheDocument()
  })

  it('opens initialSlug on mount via memory_entity_page_find_by_slug', async () => {
    renderWithProviders(<WikiView initialSlug="person-alice" />)
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('memory_entity_page_find_by_slug', expect.objectContaining({ slug: 'person-alice' })),
    )
  })
})
