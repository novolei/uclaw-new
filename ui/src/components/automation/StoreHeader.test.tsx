import { describe, test, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { StoreHeader } from './StoreHeader'

describe('StoreHeader', () => {
  test('renders search input and type tabs', () => {
    const { getByPlaceholderText, getByText } = renderWithProviders(
      <StoreHeader onRefresh={() => {}} />,
    )
    expect(getByPlaceholderText(/搜索数字人/)).toBeInTheDocument()
    expect(getByText('全部')).toBeInTheDocument()
    expect(getByText('数字人')).toBeInTheDocument()
    expect(getByText('技能')).toBeInTheDocument()
    expect(getByText('MCP')).toBeInTheDocument()
  })

  test('refresh button triggers callback', () => {
    const onRefresh = vi.fn()
    const { getByTitle } = renderWithProviders(<StoreHeader onRefresh={onRefresh} />)
    fireEvent.click(getByTitle('刷新注册表'))
    expect(onRefresh).toHaveBeenCalledOnce()
  })
})
