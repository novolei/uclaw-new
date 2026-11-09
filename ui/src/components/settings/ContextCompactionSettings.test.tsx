import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor, fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'

vi.mock('@/lib/tauri-bridge', () => ({
  getAgentCompactionThreshold: vi.fn(),
  setAgentCompactionThreshold: vi.fn(),
  getAgentContextSettings: vi.fn(),
  setAgentContextSettings: vi.fn(),
}))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

import { ContextCompactionSettings } from './ContextCompactionSettings'
import {
  getAgentCompactionThreshold,
  setAgentCompactionThreshold,
  getAgentContextSettings,
  setAgentContextSettings,
} from '@/lib/tauri-bridge'
import type { AgentContextSettings } from '@/lib/types'
import { toast } from 'sonner'

const getMock = vi.mocked(getAgentCompactionThreshold)
const setMock = vi.mocked(setAgentCompactionThreshold)
const getCtxMock = vi.mocked(getAgentContextSettings)
const setCtxMock = vi.mocked(setAgentContextSettings)

const CTX_DEFAULTS: AgentContextSettings = {
  keepToolResultGroups: 3,
  toolClearFloorTokens: 20_000,
  summaryTargetTokens: 3_000,
  reloadRecentFiles: 5,
}

const CTX_LABELS = {
  groups: '保留最近工具结果组数',
  floor: '批量清理下限 (K)',
  summary: '摘要目标长度 (K)',
  reload: '压缩后重载文件数',
} as const

async function renderCtxLoaded(initial: AgentContextSettings = CTX_DEFAULTS) {
  getMock.mockResolvedValue(128_000)
  getCtxMock.mockResolvedValue(initial)
  renderWithProviders(<ContextCompactionSettings />)
  const groups = (await screen.findByLabelText(CTX_LABELS.groups)) as HTMLInputElement
  await waitFor(() => expect(groups.value).toBe(String(initial.keepToolResultGroups)))
  return {
    groups,
    floor: screen.getByLabelText(CTX_LABELS.floor) as HTMLInputElement,
    summary: screen.getByLabelText(CTX_LABELS.summary) as HTMLInputElement,
    reload: screen.getByLabelText(CTX_LABELS.reload) as HTMLInputElement,
  }
}

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
    getCtxMock.mockResolvedValue(CTX_DEFAULTS)
    setCtxMock.mockImplementation(async (s: AgentContextSettings) => s)
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

  describe('agent context settings rows', () => {
    it('loads and shows the four context settings (K fields in thousands)', async () => {
      const { groups, floor, summary, reload } = await renderCtxLoaded()
      expect(groups.value).toBe('3')
      expect(floor.value).toBe('20')
      expect(summary.value).toBe('3')
      expect(reload.value).toBe('5')
      expect(
        screen.getByText('更早的工具结果在上下文接近阈值时会被替换为一行摘要'),
      ).toBeInTheDocument()
      expect(screen.getByText('可清理内容少于此值时不清理，避免频繁打断缓存')).toBeInTheDocument()
      expect(screen.getByText('压缩时生成的摘要长度')).toBeInTheDocument()
      expect(
        screen.getByText('压缩后重新放入最近改过的文件内容（单个超过 5K 只给路径）'),
      ).toBeInTheDocument()
    })

    it('saves one changed field and keeps the others', async () => {
      const { groups } = await renderCtxLoaded()
      fireEvent.change(groups, { target: { value: '5' } })
      fireEvent.blur(groups)
      await waitFor(() =>
        expect(setCtxMock).toHaveBeenCalledWith({ ...CTX_DEFAULTS, keepToolResultGroups: 5 }),
      )
      expect(setCtxMock).toHaveBeenCalledTimes(1)
    })

    it('multiplies K fields by 1000 when saving', async () => {
      const { floor } = await renderCtxLoaded()
      fireEvent.change(floor, { target: { value: '40' } })
      fireEvent.keyDown(floor, { key: 'Enter' })
      await waitFor(() =>
        expect(setCtxMock).toHaveBeenCalledWith({ ...CTX_DEFAULTS, toolClearFloorTokens: 40_000 }),
      )
    })

    it('shows the clamped value the backend returns', async () => {
      setCtxMock.mockImplementation(async (s: AgentContextSettings) => ({
        ...s,
        reloadRecentFiles: Math.min(s.reloadRecentFiles, 10),
      }))
      const { reload } = await renderCtxLoaded()
      fireEvent.change(reload, { target: { value: '50' } })
      fireEvent.blur(reload)
      await waitFor(() => expect(setCtxMock).toHaveBeenCalled())
      await waitFor(() => expect(reload.value).toBe('10'))
    })

    it('shows a toast and rolls back when saving fails', async () => {
      setCtxMock.mockRejectedValueOnce('disk full')
      const { summary } = await renderCtxLoaded()
      fireEvent.change(summary, { target: { value: '6' } })
      fireEvent.blur(summary)
      await waitFor(() =>
        expect(toast.error).toHaveBeenCalledWith('保存上下文设置失败', expect.anything()),
      )
      await waitFor(() => expect(summary.value).toBe('3'))
    })

    it('keeps the rows disabled until loaded, and offers retry when loading fails', async () => {
      getMock.mockResolvedValue(128_000)
      getCtxMock.mockRejectedValueOnce('db locked')
      renderWithProviders(<ContextCompactionSettings />)
      expect(await screen.findByRole('alert')).toHaveTextContent('读取上下文设置失败')
      const groups = screen.getByLabelText(CTX_LABELS.groups) as HTMLInputElement
      expect(groups).toBeDisabled()
      expect(groups.value).not.toBe('3')

      getCtxMock.mockResolvedValueOnce({ ...CTX_DEFAULTS, keepToolResultGroups: 7 })
      fireEvent.click(screen.getByRole('button', { name: '重试' }))
      await waitFor(() => expect(groups.value).toBe('7'))
      expect(groups).not.toBeDisabled()
      expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    })
  })
})
