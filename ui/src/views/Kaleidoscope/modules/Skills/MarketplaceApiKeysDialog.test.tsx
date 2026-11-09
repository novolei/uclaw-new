import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'

const bridge = vi.hoisted(() => ({
  getMarketplaceApiKeyStatus: vi.fn(),
  setMarketplaceApiKey: vi.fn(),
}))
vi.mock('@/lib/tauri-bridge', () => bridge)

import { MarketplaceApiKeysDialog } from './MarketplaceApiKeysDialog'

beforeEach(() => {
  bridge.getMarketplaceApiKeyStatus.mockReset().mockResolvedValue({ skillsSh: true, skillsmp: false })
  bridge.setMarketplaceApiKey.mockReset().mockResolvedValue(undefined)
})

describe('MarketplaceApiKeysDialog', () => {
  it('dialog_shows_status_never_the_key', async () => {
    render(<MarketplaceApiKeysDialog open onOpenChange={() => {}} />)
    expect(await screen.findByText('已设置')).toBeInTheDocument()
    expect(screen.getByText('未设置')).toBeInTheDocument()
    const inputs = screen.getAllByPlaceholderText(/粘贴新的 API Key/)
    expect(inputs).toHaveLength(2)
    inputs.forEach((i) => {
      expect(i).toHaveValue('')
      expect(i).toHaveAttribute('type', 'password')
    })
  })

  it('saving_calls_bridge_and_clears_input', async () => {
    render(<MarketplaceApiKeysDialog open onOpenChange={() => {}} />)
    await screen.findByText('已设置')
    const input = screen.getByLabelText('skillsmp.com API Key')
    fireEvent.change(input, { target: { value: 'sk_new' } })
    fireEvent.click(screen.getByRole('button', { name: '保存 skillsmp.com' }))
    await waitFor(() => expect(bridge.setMarketplaceApiKey).toHaveBeenCalledWith('skillsmp', 'sk_new'))
    await waitFor(() => expect(input).toHaveValue(''))
    expect(bridge.getMarketplaceApiKeyStatus).toHaveBeenCalledTimes(2)
  })

  it('clear_sends_empty_key', async () => {
    render(<MarketplaceApiKeysDialog open onOpenChange={() => {}} />)
    await screen.findByText('已设置')
    fireEvent.click(screen.getByRole('button', { name: '清除 skills.sh' }))
    await waitFor(() => expect(bridge.setMarketplaceApiKey).toHaveBeenCalledWith('skills_sh', ''))
  })
})
