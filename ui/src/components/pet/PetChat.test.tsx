import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { createStore, Provider } from 'jotai'

// Mock @tauri-apps/api/event before importing the component
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn().mockResolvedValue(undefined) }))

// Mock @/lib/pet-chat — export a real class so instanceof works in component
vi.mock('@/lib/pet-chat', () => {
  class PetModelNotReady extends Error {
    constructor(msg?: string) { super(msg ?? 'model not ready') }
  }
  return {
    PetModelNotReady,
    streamPetChat: vi.fn(),
  }
})

import { PetChat } from './PetChat'
import { emit } from '@tauri-apps/api/event'
import { streamPetChat, PetModelNotReady } from '@/lib/pet-chat'

const mockStreamPetChat = streamPetChat as ReturnType<typeof vi.fn>
const mockEmit = emit as ReturnType<typeof vi.fn>

function renderChat() {
  const store = createStore()
  const onClose = vi.fn()
  const utils = render(
    <Provider store={store}>
      <PetChat onClose={onClose} />
    </Provider>,
  )
  return { ...utils, onClose }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('PetChat', () => {
  it('happy path: sends message via Enter and shows assistant reply in pet-reply', async () => {
    mockStreamPetChat.mockImplementation(
      (_msgs: unknown, onDelta: (t: string) => void) => {
        onDelta('你好')
        return Promise.resolve()
      },
    )

    renderChat()
    const textarea = screen.getByTestId('pet-chat-input')
    fireEvent.change(textarea, { target: { value: '你好吗' } })
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false })

    await waitFor(() => {
      expect(screen.getByTestId('pet-reply')).toHaveTextContent('你好')
    })
  })

  it('503 not-ready: emits pet://open-wizard', async () => {
    // The rejected value must be an instance of the SAME PetModelNotReady the component imports
    mockStreamPetChat.mockImplementation(() =>
      Promise.reject(new PetModelNotReady('model not ready')),
    )

    renderChat()
    const textarea = screen.getByTestId('pet-chat-input')
    fireEvent.change(textarea, { target: { value: '你好' } })
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false })

    await waitFor(() => {
      expect(mockEmit).toHaveBeenCalledWith('pet://open-wizard')
    })
  })

  it('Escape key calls onClose', () => {
    mockStreamPetChat.mockResolvedValue(undefined)
    const { onClose } = renderChat()
    const textarea = screen.getByTestId('pet-chat-input')
    fireEvent.keyDown(textarea, { key: 'Escape' })
    expect(onClose).toHaveBeenCalled()
  })

  it('Shift+Enter does not send', async () => {
    mockStreamPetChat.mockResolvedValue(undefined)
    renderChat()
    const textarea = screen.getByTestId('pet-chat-input')
    fireEvent.change(textarea, { target: { value: 'hello' } })
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: true })
    // streamPetChat should NOT have been called
    expect(mockStreamPetChat).not.toHaveBeenCalled()
  })
})
