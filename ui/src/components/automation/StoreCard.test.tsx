import { describe, test, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { StoreCard } from './StoreCard'
import type { MarketplaceItem } from '@/lib/tauri-bridge'

const makeItem = (overrides: Partial<MarketplaceItem> = {}): MarketplaceItem => ({
  slug: 'ai-news', name: 'AI News', version: '1.0.0', author: 'a',
  description: 'desc', appType: 'automation', category: 'news',
  icon: null, tags: ['ai', 'news'], sizeBytes: null, minAppVersion: null,
  locale: null, i18n: {},
  ...overrides,
})

describe('StoreCard', () => {
  test('renders name, author, version, description', () => {
    const { getByText } = renderWithProviders(<StoreCard item={makeItem()} onClick={() => {}} />)
    expect(getByText('AI News')).toBeInTheDocument()
    expect(getByText('by a')).toBeInTheDocument()
    expect(getByText('v1.0.0')).toBeInTheDocument()
    expect(getByText('desc')).toBeInTheDocument()
  })
  test('shows "已安装" badge when isInstalled', () => {
    const { getByText } = renderWithProviders(
      <StoreCard item={makeItem()} isInstalled={true} onClick={() => {}} />,
    )
    expect(getByText('已安装')).toBeInTheDocument()
  })
  test('shows "有更新" badge when hasUpdate', () => {
    const { getByText } = renderWithProviders(
      <StoreCard item={makeItem()} isInstalled={true} hasUpdate={true} onClick={() => {}} />,
    )
    expect(getByText('有更新')).toBeInTheDocument()
  })
  test('calls onClick with slug on click', () => {
    const onClick = vi.fn()
    const { getByRole } = renderWithProviders(
      <StoreCard item={makeItem()} onClick={onClick} />,
    )
    fireEvent.click(getByRole('button'))
    expect(onClick).toHaveBeenCalledWith('ai-news')
  })
})
