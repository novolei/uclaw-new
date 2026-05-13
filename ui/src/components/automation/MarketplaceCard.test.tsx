import { describe, test, expect, vi } from 'vitest'
import * as React from 'react'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { MarketplaceCard } from './MarketplaceCard'
import type { MarketplaceItem } from '@/lib/tauri-bridge'

function makeItem(overrides: Partial<MarketplaceItem> = {}): MarketplaceItem {
  return {
    slug: 'test-spec',
    name: '测试规约',
    version: '1.2.3',
    author: 'ryan',
    description: '这是一个测试用规约',
    appType: 'automation',
    category: 'productivity',
    icon: 'package',
    tags: ['test', 'demo'],
    sizeBytes: 1024,
    minAppVersion: '0.5.0',
    locale: 'zh-CN',
    i18nName: null,
    i18nDescription: null,
    ...overrides,
  }
}

describe('MarketplaceCard', () => {
  test('renders core fields', () => {
    const { getByText } = renderWithProviders(
      <MarketplaceCard item={makeItem()} installing={false} onInstall={() => {}} />
    )
    expect(getByText('测试规约')).toBeInTheDocument()
    expect(getByText('by ryan')).toBeInTheDocument()
    expect(getByText('这是一个测试用规约')).toBeInTheDocument()
    expect(getByText('v1.2.3')).toBeInTheDocument()
  })

  test('prefers i18nName/i18nDescription when present', () => {
    const item = makeItem({
      i18nName: 'Test Spec',
      i18nDescription: 'A test spec.',
    })
    const { getByText, queryByText } = renderWithProviders(
      <MarketplaceCard item={item} installing={false} onInstall={() => {}} />
    )
    expect(getByText('Test Spec')).toBeInTheDocument()
    expect(getByText('A test spec.')).toBeInTheDocument()
    expect(queryByText('测试规约')).toBeNull()
  })

  test('truncates tags beyond MAX_VISIBLE_TAGS', () => {
    const item = makeItem({ tags: ['a', 'b', 'c', 'd', 'e'] })
    const { getByText, queryByText } = renderWithProviders(
      <MarketplaceCard item={item} installing={false} onInstall={() => {}} />
    )
    expect(getByText('a')).toBeInTheDocument()
    expect(getByText('b')).toBeInTheDocument()
    expect(getByText('c')).toBeInTheDocument()
    // The 4th and 5th tags should NOT be visible, but a +N indicator should
    expect(queryByText('d')).toBeNull()
    expect(getByText('+2')).toBeInTheDocument()
  })

  test('clicking calls onInstall with slug', () => {
    const onInstall = vi.fn()
    const { getByRole } = renderWithProviders(
      <MarketplaceCard item={makeItem()} installing={false} onInstall={onInstall} />
    )
    fireEvent.click(getByRole('button'))
    expect(onInstall).toHaveBeenCalledWith('test-spec')
  })

  test('disabled state when installing', () => {
    const onInstall = vi.fn()
    const { getByRole, getByText } = renderWithProviders(
      <MarketplaceCard item={makeItem()} installing={true} onInstall={onInstall} />
    )
    const btn = getByRole('button')
    expect(btn).toHaveAttribute('disabled')
    expect(getByText('安装中…')).toBeInTheDocument()
    fireEvent.click(btn)
    expect(onInstall).not.toHaveBeenCalled()
  })
})
