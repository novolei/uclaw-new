import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { SpeechButton } from './speech-button'
import { createStore } from 'jotai'
import { modelStatusAtom } from '@/atoms/stt-atoms'
import { installAudioStubs, type InstalledStubs } from '@/test-utils/stt-mocks'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => null),
}))

let stubs: InstalledStubs
beforeEach(() => {
  stubs = installAudioStubs()
})

describe('SpeechButton', () => {
  it('renders enabled mic button regardless of model state (download trigger lives in click handler)', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'not-downloaded', expectedDir: '/tmp' })
    renderWithProviders(<SpeechButton composer="chat" onTranscript={() => {}} />, { store })
    const btn = screen.getByRole('button', { name: /语音输入/ })
    expect(btn.hasAttribute('disabled')).toBe(false)
    stubs.cleanup()
  })

  it('click invokes onShowDownloadDialog when model not ready', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'not-downloaded', expectedDir: '/tmp' })
    const onShowDownload = vi.fn()
    renderWithProviders(
      <SpeechButton
        composer="chat"
        onTranscript={() => {}}
        onShowDownloadDialog={onShowDownload}
      />,
      { store },
    )
    fireEvent.click(screen.getByRole('button', { name: /语音输入/ }))
    expect(onShowDownload).toHaveBeenCalledOnce()
    stubs.cleanup()
  })
})
