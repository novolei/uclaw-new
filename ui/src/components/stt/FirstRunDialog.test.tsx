import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FirstRunDialog } from './FirstRunDialog'
import { createStore } from 'jotai'
import { modelStatusAtom } from '@/atoms/stt-atoms'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

beforeEach(() => {
  invokeMock.mockReset()
  invokeMock.mockResolvedValue('/tmp/sensevoice')
})

describe('FirstRunDialog', () => {
  it('invite state: shows model size + start button when not downloaded', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'not-downloaded', expectedDir: '/tmp/x' })
    renderWithProviders(
      <FirstRunDialog open onOpenChange={() => {}} onReady={() => {}} />,
      { store },
    )
    expect(screen.getByText(/230MB|230 MB/)).not.toBeNull()
    expect(screen.getByRole('button', { name: /开始下载/ })).not.toBeNull()
  })

  it('clicking download invokes stt_download_model', async () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'not-downloaded', expectedDir: '/tmp/x' })
    renderWithProviders(
      <FirstRunDialog open onOpenChange={() => {}} onReady={() => {}} />,
      { store },
    )
    fireEvent.click(screen.getByRole('button', { name: /开始下载/ }))
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('stt_download_model', expect.any(Object)),
    )
  })

  it('ready state: shows countdown + immediate start button', async () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/tmp/x' })
    const onReady = vi.fn()
    renderWithProviders(
      <FirstRunDialog open onOpenChange={() => {}} onReady={onReady} />,
      { store },
    )
    expect(screen.getByText(/模型已就绪|已就绪/)).not.toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /立即开始/ }))
    expect(onReady).toHaveBeenCalledOnce()
  })

  it('downloading state: shows progress bar with percent', () => {
    const store = createStore()
    store.set(modelStatusAtom, {
      kind: 'downloading',
      file: 'model_quant.onnx',
      downloaded: 50 * 1024 * 1024,
      total: 230 * 1024 * 1024,
      percent: 21,
    })
    renderWithProviders(
      <FirstRunDialog open onOpenChange={() => {}} onReady={() => {}} />,
      { store },
    )
    expect(screen.getByText(/21%/)).not.toBeNull()
    expect(screen.getByText(/model_quant/)).not.toBeNull()
  })
})
