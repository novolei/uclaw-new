import { describe, it, expect, vi, beforeEach } from 'vitest'
import { Provider as JotaiProvider, createStore } from 'jotai'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import type { MarketplaceSkillSummary } from '@/lib/types'

const bridge = vi.hoisted(() => ({
  searchSkillsMarketplace: vi.fn(),
  listSkillsMarketplace: vi.fn(),
  getSkillMarketplaceDetail: vi.fn(),
  getSkillMarketplaceAudit: vi.fn(),
  installSkillFromMarketplace: vi.fn(),
  uninstallSkillFromMarketplace: vi.fn(),
  listStandaloneInstalls: vi.fn(),
  getMarketplaceApiKeyStatus: vi.fn(),
  setMarketplaceApiKey: vi.fn(),
}))
vi.mock('@/lib/tauri-bridge', () => bridge)

import { SkillsMarketplaceTab } from './SkillsMarketplaceTab'

const row: MarketplaceSkillSummary = {
  id: 'o/r/excel',
  slug: 'excel',
  name: 'excel-automation',
  source: 'o/r',
  installs: 42,
  sourceType: 'github',
  installUrl: 'o/r/excel',
  url: '',
  description: '',
}

function renderTab(query = '', workspaceId: string | null = 'ws-1') {
  const store = createStore()
  store.set(activeWorkspaceIdAtom, workspaceId)
  return render(
    <JotaiProvider store={store}>
      <SkillsMarketplaceTab query={query} />
    </JotaiProvider>,
  )
}

beforeEach(() => {
  Object.values(bridge).forEach((f) => f.mockReset())
  bridge.listSkillsMarketplace.mockResolvedValue([row])
  bridge.searchSkillsMarketplace.mockResolvedValue([row])
  bridge.listStandaloneInstalls.mockResolvedValue([])
  bridge.installSkillFromMarketplace.mockResolvedValue('o__r__excel')
  bridge.uninstallSkillFromMarketplace.mockResolvedValue(undefined)
  bridge.getMarketplaceApiKeyStatus.mockResolvedValue({ skillsSh: false, skillsmp: false })
})

describe('SkillsMarketplaceTab', () => {
  it('defaults_to_skills_sh_and_shows_trending', async () => {
    renderTab()
    expect(await screen.findByText('excel-automation')).toBeInTheDocument()
    expect(bridge.listSkillsMarketplace).toHaveBeenCalledWith('trending', 0, 'skills_sh')
  })

  it('install_global_calls_bridge_with_scope_global', async () => {
    renderTab()
    fireEvent.click(await screen.findByRole('button', { name: '全局' }))
    await waitFor(() =>
      expect(bridge.installSkillFromMarketplace).toHaveBeenCalledWith('o/r/excel', 'global', undefined, 'skills_sh', 'o/r/excel'),
    )
    expect(await screen.findByText('已安装（全局）')).toBeInTheDocument()
  })

  it('workspace_button_disabled_without_active_workspace', async () => {
    renderTab('', null)
    expect(await screen.findByRole('button', { name: '本工作区' })).toBeDisabled()
  })

  it('uninstall_requires_confirmation_then_calls_bridge', async () => {
    bridge.listStandaloneInstalls.mockResolvedValue([
      { slug: 'o__r__excel', itemType: 'skill', version: 'h1', installedAt: 0, mcpServerId: null },
      { slug: 'some-mcp', itemType: 'mcp', version: '1', installedAt: 0, mcpServerId: 'm' },
    ])
    renderTab()
    expect(await screen.findByText('o__r__excel')).toBeInTheDocument()
    expect(screen.queryByText('some-mcp')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '卸载' }))
    expect(bridge.uninstallSkillFromMarketplace).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '确认' }))
    await waitFor(() => expect(bridge.uninstallSkillFromMarketplace).toHaveBeenCalledWith('o__r__excel'))
  })

  it('rate_limited_error_shows_chinese_hint', async () => {
    bridge.searchSkillsMarketplace.mockRejectedValue('请求太频繁，请稍后再试或在设置中填写 API Key')
    renderTab('excel')
    expect(await screen.findByText('请求太频繁，请稍后再试或在设置中填写 API Key')).toBeInTheDocument()
  })

  it('missing_key_on_trending_shows_search_hint', async () => {
    bridge.listSkillsMarketplace.mockRejectedValue('需要先在设置中填写该市场的 API Key')
    renderTab()
    expect(await screen.findByText('输入关键词搜索 skills.sh；填写 API Key 后可浏览热门技能')).toBeInTheDocument()
  })

  it('api_key_button_opens_dialog', async () => {
    renderTab()
    await screen.findByText('excel-automation')
    fireEvent.click(screen.getByRole('button', { name: 'API Key' }))
    expect(await screen.findByText('技能市场 API Key')).toBeInTheDocument()
  })

  it('installed_list_hides_non_marketplace_skill_rows', async () => {
    bridge.listStandaloneInstalls.mockResolvedValue([
      { slug: 'o__r__excel', itemType: 'skill', version: 'h1', installedAt: 0, mcpServerId: null },
      { slug: 'halo-automation-skill', itemType: 'skill', version: '1', installedAt: 0, mcpServerId: null },
    ])
    renderTab()
    expect(await screen.findByText('o__r__excel')).toBeInTheDocument()
    expect(screen.queryByText('halo-automation-skill')).toBeNull()
  })
})
