import { describe, test, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  listInstalledMarketplaceAutomations: vi.fn(),
  uninstallMarketplaceHuman: vi.fn(),
  listStandaloneInstalls: vi.fn(),
  // AppsTab now calls checkMarketplaceUpdates on mount; resolve to empty so no upgrade buttons appear
  checkMarketplaceUpdates: vi.fn().mockResolvedValue([]),
}))

import { AppsTab } from './AppsTab'
import {
  listInstalledMarketplaceAutomations,
  uninstallMarketplaceHuman,
  listStandaloneInstalls,
} from '@/lib/tauri-bridge'

beforeEach(() => {
  // Default standalone list to [] so existing tests are unaffected
  ;(listStandaloneInstalls as ReturnType<typeof vi.fn>).mockResolvedValue([])
})

const sampleData = [
  {
    slug: 'xhs-monitor',
    name: '小红书关键词监控',
    version: '4.0.0',
    icon: 'social',
    category: 'social',
    bundledSkills: [
      {
        skillId: 'xhs-search',
        description: 'Collects xiaohongshu search data',
        installPath: '/home/x/.uclaw/skills/_marketplace/xhs-monitor/xhs-search',
        fileCount: 2,
      },
    ],
    requiredCapabilities: [
      { mcpId: 'ai-browser', status: 'mapped' as const, mappedTo: 'uClaw 内建浏览器' },
    ],
  },
]

describe('AppsTab', () => {
  test('renders empty state when nothing installed', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>).mockResolvedValueOnce([])
    const { findByText } = renderWithProviders(<AppsTab />)
    expect(await findByText(/暂无已安装的数字人/)).toBeInTheDocument()
  })

  test('lists installed automations with name and version', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>).mockResolvedValueOnce(sampleData)
    const { findByText } = renderWithProviders(<AppsTab />)
    expect(await findByText('小红书关键词监控')).toBeInTheDocument()
    expect(await findByText(/v4\.0\.0/)).toBeInTheDocument()
  })

  test('expand reveals bundled skills and capability checks', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>).mockResolvedValueOnce(sampleData)
    const { findByText, getByText, queryByText } = renderWithProviders(<AppsTab />)
    await findByText('小红书关键词监控')
    expect(queryByText('xhs-search')).not.toBeInTheDocument()
    fireEvent.click(getByText('小红书关键词监控'))
    expect(await findByText('xhs-search')).toBeInTheDocument()
    expect(await findByText('ai-browser')).toBeInTheDocument()
    expect(await findByText(/已映射到 uClaw 内建/)).toBeInTheDocument()
  })

  test('uninstall calls bridge and refreshes', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce(sampleData)
      .mockResolvedValueOnce([])
    ;(uninstallMarketplaceHuman as ReturnType<typeof vi.fn>).mockResolvedValueOnce(undefined)
    vi.spyOn(window, 'confirm').mockReturnValue(true)

    const { findByText, getByText } = renderWithProviders(<AppsTab />)
    await findByText('小红书关键词监控')
    fireEvent.click(getByText('卸载'))
    await waitFor(() => expect(uninstallMarketplaceHuman).toHaveBeenCalledWith('xhs-monitor'))
    expect(await findByText(/暂无已安装的数字人/)).toBeInTheDocument()
  })

  test('renders standalone installs section with uninstall', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>).mockResolvedValueOnce([])
    ;(listStandaloneInstalls as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      { slug: 'summariser', itemType: 'skill', version: '1.0.0', installedAt: 0, mcpServerId: null },
      { slug: 'pg-mcp', itemType: 'mcp', version: '2.0.0', installedAt: 0, mcpServerId: 'srv-1' },
    ])
    ;(uninstallMarketplaceHuman as ReturnType<typeof vi.fn>).mockResolvedValueOnce(undefined)

    const { findByText, getAllByText } = renderWithProviders(<AppsTab />)
    expect(await findByText('summariser')).toBeInTheDocument()
    expect(await findByText('pg-mcp')).toBeInTheDocument()
    fireEvent.click(getAllByText('卸载')[0])
    await waitFor(() => expect(uninstallMarketplaceHuman).toHaveBeenCalled())
  })
})
