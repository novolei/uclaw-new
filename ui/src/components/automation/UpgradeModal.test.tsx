import { describe, test, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  getMarketplaceDetail: vi.fn(),
  installMarketplaceHuman: vi.fn(),
}))

import { UpgradeModal } from './UpgradeModal'
import { getMarketplaceDetail, installMarketplaceHuman } from '@/lib/tauri-bridge'

const detail = {
  item: { slug: 'xhs', name: '小红书监控', version: '2.0.0', appType: 'automation' },
  specYaml: '',
  parsedSpecJson: {
    requires: {
      skills: [
        { id: 'xhs-search', bundled: true },
        { id: 'xhs-report', bundled: true },
      ],
    },
  },
  requiresMcps: [],
  requiresSkills: [],
  installedVersion: '1.0.0',
}

describe('UpgradeModal', () => {
  // Reset mock call counts between tests so cancel-test assertions are isolated
  beforeEach(() => {
    vi.clearAllMocks()
  })

  test('renders version bump and skill diff', async () => {
    ;(getMarketplaceDetail as ReturnType<typeof vi.fn>).mockResolvedValueOnce(detail)
    const { findByText } = renderWithProviders(
      <UpgradeModal
        slug="xhs"
        name="小红书监控"
        currentVersion="1.0.0"
        installedSkillIds={['xhs-search']}
        onClose={() => {}}
        onUpgraded={() => {}}
      />,
    )
    expect(await findByText(/1\.0\.0/)).toBeInTheDocument()
    expect(await findByText(/2\.0\.0/)).toBeInTheDocument()
    expect(await findByText('xhs-report')).toBeInTheDocument()
  })

  test('confirm calls installMarketplaceHuman with slug', async () => {
    ;(getMarketplaceDetail as ReturnType<typeof vi.fn>).mockResolvedValueOnce(detail)
    ;(installMarketplaceHuman as ReturnType<typeof vi.fn>).mockResolvedValueOnce({})
    const onUpgraded = vi.fn()
    const { findByText } = renderWithProviders(
      <UpgradeModal
        slug="xhs"
        name="小红书监控"
        currentVersion="1.0.0"
        installedSkillIds={['xhs-search']}
        onClose={() => {}}
        onUpgraded={onUpgraded}
      />,
    )
    const confirmBtn = await findByText(/升级到 v2\.0\.0/)
    fireEvent.click(confirmBtn)
    await waitFor(() => expect(installMarketplaceHuman).toHaveBeenCalledWith('xhs'))
  })

  test('cancel closes without calling the bridge', async () => {
    ;(getMarketplaceDetail as ReturnType<typeof vi.fn>).mockResolvedValueOnce(detail)
    const onClose = vi.fn()
    const { findByText } = renderWithProviders(
      <UpgradeModal
        slug="xhs"
        name="小红书监控"
        currentVersion="1.0.0"
        installedSkillIds={['xhs-search']}
        onClose={onClose}
        onUpgraded={() => {}}
      />,
    )
    fireEvent.click(await findByText('取消'))
    expect(onClose).toHaveBeenCalled()
    expect(installMarketplaceHuman).not.toHaveBeenCalled()
  })
})
