import { describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { Provider } from 'jotai'

vi.mock('@/lib/tauri-bridge', () => ({
  localModelList: vi.fn().mockResolvedValue([
    { model_id: 'minicpm5-1b', installed: false, files: [], total_bytes: 0 },
  ]),
  localModelDelete: vi.fn().mockResolvedValue(undefined),
}))

import { MiniCPMSettings } from './MiniCPMSettings'

describe('MiniCPMSettings', () => {
  it('renders the not-installed state and a start button', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByText(/未安装/)).toBeInTheDocument())
    expect(screen.getByText(/开始设置/)).toBeInTheDocument()
  })
})
