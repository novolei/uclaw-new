import { describe, it, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { InlineRecorder } from './InlineRecorder'

describe('InlineRecorder', () => {
  it('renders 5 waveform bars when recording', () => {
    renderWithProviders(
      <InlineRecorder
        state={{ kind: 'recording', startedAtMs: Date.now(), volume: 0.5 }}
        onStop={() => {}}
        onCancel={() => {}}
      />,
    )
    const bars = document.querySelectorAll('[data-testid="stt-waveform-bar"]')
    expect(bars.length).toBe(5)
  })

  it('shows mm:ss timer', () => {
    renderWithProviders(
      <InlineRecorder
        state={{ kind: 'recording', startedAtMs: Date.now() - 4500, volume: 0.3 }}
        onStop={() => {}}
        onCancel={() => {}}
      />,
    )
    expect(screen.getByText(/00:0[45]/)).not.toBeNull()
  })

  it('cancel button triggers onCancel', () => {
    const onCancel = vi.fn()
    renderWithProviders(
      <InlineRecorder
        state={{ kind: 'recording', startedAtMs: Date.now(), volume: 0.2 }}
        onStop={() => {}}
        onCancel={onCancel}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /取消录音/ }))
    expect(onCancel).toHaveBeenCalledOnce()
  })

  it('stop button triggers onStop', () => {
    const onStop = vi.fn()
    renderWithProviders(
      <InlineRecorder
        state={{ kind: 'recording', startedAtMs: Date.now(), volume: 0.2 }}
        onStop={onStop}
        onCancel={() => {}}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /完成并转写/ }))
    expect(onStop).toHaveBeenCalledOnce()
  })

  it('warns visually (text-amber) when elapsed > 50s', () => {
    renderWithProviders(
      <InlineRecorder
        state={{ kind: 'recording', startedAtMs: Date.now() - 51_000, volume: 0.2 }}
        onStop={() => {}}
        onCancel={() => {}}
      />,
    )
    const timer = screen.getByTestId('stt-timer')
    expect(timer.className).toContain('text-amber')
  })

  it('renders nothing for idle state', () => {
    const { container } = renderWithProviders(
      <InlineRecorder
        state={{ kind: 'idle' }}
        onStop={() => {}}
        onCancel={() => {}}
      />,
    )
    expect(container.innerHTML).toBe('')
  })
})
