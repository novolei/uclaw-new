import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { TooltipProvider } from '@/components/ui/tooltip'
import { modelStatusAtom, sttModalStateAtom, activeComposerAtom } from '@/atoms/stt-atoms'
import { SpeechButton } from './speech-button'

vi.mock('@/hooks/useShortcut', () => ({
  useShortcut: vi.fn(),
}))

function renderWith(store: ReturnType<typeof createStore>, props = {}) {
  return render(
    <Provider store={store}>
      <TooltipProvider>
        <SpeechButton composer="chat" {...props} />
      </TooltipProvider>
    </Provider>,
  )
}

describe('SpeechButton', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('dispatches uclaw:stt-start when ready and idle', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
    const spy = vi.fn()
    window.addEventListener('uclaw:stt-start', spy)
    renderWith(store)
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(spy).toHaveBeenCalledTimes(1)
    window.removeEventListener('uclaw:stt-start', spy)
  })

  it('dispatches uclaw:stt-end when the modal is open for this composer', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
    store.set(activeComposerAtom, 'chat')
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0, bands: [], interimText: '',
    })
    const spy = vi.fn()
    window.addEventListener('uclaw:stt-end', spy)
    renderWith(store)
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(spy).toHaveBeenCalledTimes(1)
    window.removeEventListener('uclaw:stt-end', spy)
  })

  it('calls onShowDownloadDialog when model is not ready', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'not-downloaded', expectedDir: '/m' })
    const onShow = vi.fn()
    renderWith(store, { onShowDownloadDialog: onShow })
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(onShow).toHaveBeenCalledTimes(1)
  })

  it('does nothing when another composer holds the session', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
    store.set(activeComposerAtom, 'agent')
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0, bands: [], interimText: '',
    })
    const startSpy = vi.fn()
    const endSpy = vi.fn()
    window.addEventListener('uclaw:stt-start', startSpy)
    window.addEventListener('uclaw:stt-end', endSpy)
    renderWith(store)
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(startSpy).not.toHaveBeenCalled()
    expect(endSpy).not.toHaveBeenCalled()
    window.removeEventListener('uclaw:stt-start', startSpy)
    window.removeEventListener('uclaw:stt-end', endSpy)
  })
})
