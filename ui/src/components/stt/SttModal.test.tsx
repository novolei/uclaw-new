import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { sttModalStateAtom } from '@/atoms/stt-atoms'
import { SttModal } from './SttModal'

// mock the session hook — SttModal only renders from the atom + calls handle methods
const endMock = vi.fn().mockResolvedValue(undefined)
const cancelMock = vi.fn()
vi.mock('@/hooks/useSttStreamingSession', () => ({
  useSttStreamingSession: () => ({
    state: { kind: 'idle' },
    start: vi.fn(),
    end: endMock,
    cancel: cancelMock,
  }),
}))

function renderWith(store: ReturnType<typeof createStore>) {
  return render(
    <Provider store={store}>
      <SttModal composer="chat" onSegmentFinalized={vi.fn()} />
    </Provider>,
  )
}

describe('SttModal', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders nothing when state is idle', () => {
    const store = createStore()
    const { container } = renderWith(store)
    expect(container.querySelector('.stt-modal-overlay')).toBeNull()
  })

  it('renders the panel + glow when listening', () => {
    const store = createStore()
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0.3, bands: [0.2,0.6,0.4,0.8,0.3,0.5,0.1], interimText: '你好世界',
    })
    const { container } = renderWith(store)
    expect(container.querySelector('.stt-modal-overlay')).not.toBeNull()
    expect(container.querySelector('.stt-modal-glow')).not.toBeNull()
    expect(screen.getByText('你好世界')).toBeInTheDocument()
  })

  it('shows the placeholder when listening with empty interim text', () => {
    const store = createStore()
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0, bands: [], interimText: '',
    })
    renderWith(store)
    expect(screen.getByText('请开始说话')).toBeInTheDocument()
  })

  it('renders permission-denied state', () => {
    const store = createStore()
    store.set(sttModalStateAtom, { kind: 'permission-denied' })
    renderWith(store)
    expect(screen.getByText(/麦克风权限/)).toBeInTheDocument()
  })
})
