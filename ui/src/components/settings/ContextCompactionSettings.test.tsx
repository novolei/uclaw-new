import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor, fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  getAgentCompactionThreshold: vi.fn(),
  setAgentCompactionThreshold: vi.fn(),
}))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

import { ContextCompactionSettings } from './ContextCompactionSettings'
import { getAgentCompactionThreshold, setAgentCompactionThreshold } from '@/lib/tauri-bridge'
import { toast } from 'sonner'

const getMock = vi.mocked(getAgentCompactionThreshold)
const setMock = vi.mocked(setAgentCompactionThreshold)

async function renderLoaded(initial = 128_000) {
  getMock.mockResolvedValue(initial)
  const r = renderWithProviders(<ContextCompactionSettings />)
  const input = (await screen.findByLabelText('压缩阈值 (K)')) as HTMLInputElement
  await waitFor(() => expect(input.value).toBe(String(initial / 1000)))
  return { ...r, input }
}

describe('ContextCompactionSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setMock.mockImplementation(async (tokens: number) => tokens)
  })

  it('loads current threshold and shows 128K', async () => {
    const { input } = await renderLoaded(128_000)
    expect(input.value).toBe('128')
    expect(screen.getByText('上下文与压缩')).toBeInTheDocument()
    expect(
      screen.getByText(
        '对话超过该长度时，会把较早的内容压缩成摘要，并保留最近几轮原文。数值越大记得越多，费用也越高。',
      ),
    ).toBeInTheDocument()
    expect(input.step).toBe('8')
    expect(input.min).toBe('32')
    expect(input.max).toBe('1000')
  })

  it('saves new value on change', async () => {
    const { input } = await renderLoaded()
    fireEvent.change(input, { target: { value: '200' } })
    fireEvent.blur(input)
    await waitFor(() => expect(setMock).toHaveBeenCalledWith(200000))
    expect(setMock).toHaveBeenCalledTimes(1)
  })

  it('rejects below 32K', async () => {
    const { input } = await renderLoaded()
    fireEvent.change(input, { target: { value: '16' } })
    fireEvent.blur(input)
    expect(await screen.findByRole('alert')).toHaveTextContent('32K')
    expect(setMock).not.toHaveBeenCalled()
  })

  it('rejects above 1000K', async () => {
    const { input } = await renderLoaded()
    fireEvent.change(input, { target: { value: '1001' } })
    fireEvent.blur(input)
    expect(await screen.findByRole('alert')).toBeInTheDocument()
    expect(setMock).not.toHaveBeenCalled()
  })

  it('reset restores 128K', async () => {
    const { input } = await renderLoaded(256_000)
    fireEvent.click(screen.getByRole('button', { name: '恢复默认' }))
    await waitFor(() => expect(setMock).toHaveBeenCalledWith(128000))
    await waitFor(() => expect(input.value).toBe('128'))
  })

  it('shows a toast when saving fails', async () => {
    setMock.mockRejectedValueOnce('boom')
    const { input } = await renderLoaded()
    fireEvent.change(input, { target: { value: '200' } })
    fireEvent.blur(input)
    await waitFor(() => expect(toast.error).toHaveBeenCalled())
  })

  it('does not show 128 as the value before the threshold has loaded', async () => {
    let resolve!: (v: number) => void
    getMock.mockReturnValue(new Promise<number>((r) => (resolve = r)))
    renderWithProviders(<ContextCompactionSettings />)
    const input = (await screen.findByLabelText('压缩阈值 (K)')) as HTMLInputElement
    expect(input.value).not.toBe('128')
    expect(input).toBeDisabled()
    expect(screen.getByRole('button', { name: '恢复默认' })).toBeDisabled()

    // Edits before the load completes are ignored, so the late load can't
    // clobber them and nothing is saved from a placeholder value.
    fireEvent.change(input, { target: { value: '200' } })
    fireEvent.blur(input)
    resolve(256_000)
    await waitFor(() => expect(input.value).toBe('256'))
    expect(input).not.toBeDisabled()
    expect(setMock).not.toHaveBeenCalled()
  })

  it('shows an error state (not the default) when loading fails, and can retry', async () => {
    getMock.mockRejectedValueOnce('db locked')
    renderWithProviders(<ContextCompactionSettings />)
    expect(await screen.findByRole('alert')).toHaveTextContent('读取压缩阈值失败')
    const input = screen.getByLabelText('压缩阈值 (K)') as HTMLInputElement
    expect(input.value).not.toBe('128')
    expect(input).toBeDisabled()

    getMock.mockResolvedValueOnce(64_000)
    fireEvent.click(screen.getByRole('button', { name: '重试' }))
    await waitFor(() => expect(input.value).toBe('64'))
    expect(input).not.toBeDisabled()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})
