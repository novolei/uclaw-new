import { describe, it, expect, vi, beforeEach } from 'vitest'
import { Provider as JotaiProvider, createStore } from 'jotai'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { skillsRefreshAtom } from '@/atoms/agent-atoms'

const bridge = vi.hoisted(() => ({ installSkillFromMarketplace: vi.fn() }))
vi.mock('@/lib/tauri-bridge', () => bridge)

import { SkillMarketplaceSearchResult } from './SkillMarketplaceSearchResult'

const result = JSON.stringify({
  ok: true,
  provider: 'skills_sh',
  results: [
    { id: 'a/b/low', name: 'low-skill', source: 'a/b', installs: 5, installUrl: 'a/b/low' },
    { id: 'c/d/top', name: 'top-skill', source: 'c/d', installs: 1200, installUrl: 'c/d/top' },
  ],
})

function renderCard(r = result, isError = false) {
  const store = createStore()
  store.set(activeWorkspaceIdAtom, 'ws-1')
  const view = render(
    <JotaiProvider store={store}>
      <SkillMarketplaceSearchResult result={r} isError={isError} />
    </JotaiProvider>,
  )
  return { store, ...view }
}

beforeEach(() => {
  bridge.installSkillFromMarketplace.mockReset().mockResolvedValue('c__d__top')
})

describe('SkillMarketplaceSearchResult', () => {
  it('renders_rows_sorted_by_installs', () => {
    renderCard()
    const names = screen.getAllByTestId('marketplace-row-name').map((n) => n.textContent)
    expect(names).toEqual(['top-skill', 'low-skill'])
  })

  it('install_global_shows_installed_label', async () => {
    const user = userEvent.setup()
    const { store } = renderCard()
    await user.click(screen.getByRole('button', { name: '安装 top-skill' }))
    await user.click(await screen.findByRole('menuitem', { name: /安装到全局/ }))
    await waitFor(() =>
      expect(bridge.installSkillFromMarketplace).toHaveBeenCalledWith('c/d/top', 'global', undefined, 'skills_sh', 'c/d/top'),
    )
    expect(await screen.findByText('已安装（全局）')).toBeInTheDocument()
    expect(store.get(skillsRefreshAtom)).toBe(1)
  })

  it('invalid_json_falls_back_to_default_renderer_text', () => {
    renderCard('not json at all')
    expect(screen.getByText(/not json at all/)).toBeInTheDocument()
  })

  it('empty_results_show_hint', () => {
    renderCard(JSON.stringify({ ok: true, results: [] }))
    expect(screen.getByText(/没有找到匹配的技能/)).toBeInTheDocument()
  })
})
