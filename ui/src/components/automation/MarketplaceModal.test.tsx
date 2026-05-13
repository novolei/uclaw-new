import { describe, test, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { MarketplaceModal } from './MarketplaceModal'
import type { MarketplaceItem, HumaneSpecRow } from '@/lib/tauri-bridge'

// Mock the tauri-bridge module so we don't hit the network
vi.mock('@/lib/tauri-bridge', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/tauri-bridge')>()
  return {
    ...actual,
    listMarketplaceHumans: vi.fn(),
    installMarketplaceHuman: vi.fn(),
  }
})

import { listMarketplaceHumans, installMarketplaceHuman } from '@/lib/tauri-bridge'

const listMock = vi.mocked(listMarketplaceHumans)
const installMock = vi.mocked(installMarketplaceHuman)

function makeItem(slug: string, overrides: Partial<MarketplaceItem> = {}): MarketplaceItem {
  return {
    slug,
    name: `name-${slug}`,
    version: '1.0.0',
    author: 'author',
    description: 'desc',
    appType: 'automation',
    category: 'productivity',
    icon: null,
    tags: [],
    sizeBytes: null,
    minAppVersion: null,
    locale: null,
    i18nName: null,
    i18nDescription: null,
    ...overrides,
  }
}

function makeRow(): HumaneSpecRow {
  return {
    id: 'spec-1',
    name: 'name-foo',
    version: '1.0.0',
    author: 'a',
    description: 'd',
    systemPrompt: '',
    specFormat: 'yaml',
    specYaml: '',
    specJson: '',
    userConfigValues: '{}',
    permissionsGranted: '[]',
    permissionsDenied: '[]',
    status: 'active',
    enabled: true,
    spaceId: null,
    source: 'marketplace',
    sourceRef: 'marketplace://halo/foo',
    sourceVersion: null,
    createdAt: 1,
    updatedAt: 1,
    lastRunAt: null,
    lastRunOutcome: null,
  }
}

describe('MarketplaceModal', () => {
  beforeEach(() => {
    listMock.mockReset()
    installMock.mockReset()
  })

  test('renders loading state while fetching', async () => {
    let resolveFn: (items: MarketplaceItem[]) => void = () => {}
    listMock.mockReturnValueOnce(new Promise((r) => { resolveFn = r }))
    const { getByText } = renderWithProviders(
      <MarketplaceModal open={true} onClose={() => {}} onInstalled={() => {}} />
    )
    expect(getByText(/加载注册表/)).toBeInTheDocument()
    resolveFn([])
  })

  test('renders empty state', async () => {
    listMock.mockResolvedValueOnce([])
    const { findByText } = renderWithProviders(
      <MarketplaceModal open={true} onClose={() => {}} onInstalled={() => {}} />
    )
    expect(await findByText(/注册表为空/)).toBeInTheDocument()
  })

  test('renders items and installs on click', async () => {
    listMock.mockResolvedValueOnce([makeItem('foo'), makeItem('bar')])
    installMock.mockResolvedValueOnce(makeRow())
    const onClose = vi.fn()
    const onInstalled = vi.fn()
    const { findByText } = renderWithProviders(
      <MarketplaceModal open={true} onClose={onClose} onInstalled={onInstalled} />
    )
    const card = await findByText('name-foo')
    fireEvent.click(card)
    await waitFor(() => expect(installMock).toHaveBeenCalledWith('foo'))
    await waitFor(() => expect(onInstalled).toHaveBeenCalled())
    await waitFor(() => expect(onClose).toHaveBeenCalled())
  })

  test('renders error state when fetch fails', async () => {
    listMock.mockRejectedValueOnce(new Error('boom'))
    const { findByText } = renderWithProviders(
      <MarketplaceModal open={true} onClose={() => {}} onInstalled={() => {}} />
    )
    expect(await findByText(/无法加载注册表/)).toBeInTheDocument()
    expect(await findByText(/boom/)).toBeInTheDocument()
  })

  test('does not fetch when closed', () => {
    listMock.mockResolvedValueOnce([])
    renderWithProviders(
      <MarketplaceModal open={false} onClose={() => {}} onInstalled={() => {}} />
    )
    expect(listMock).not.toHaveBeenCalled()
  })
})
