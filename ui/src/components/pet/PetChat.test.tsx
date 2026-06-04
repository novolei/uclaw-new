import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { createStore, Provider } from 'jotai'

// Mock @tauri-apps/api/event before importing the component
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}))

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

// Mock @/lib/tauri-bridge — provide petPersonaGetActive
vi.mock('@/lib/tauri-bridge', () => ({
  petPersonaGetActive: vi.fn().mockResolvedValue({
    id: 'astro',
    name: 'Astro',
    system_prompt: '',
    sprite_set: 'astro',
    greeting: 'Hi!',
    source: 'builtin',
  }),
}))

import { PetChat } from './PetChat'
import { emit } from '@tauri-apps/api/event'
import { streamPetChat, PetModelNotReady } from '@/lib/pet-chat'
import { petPersonaGetActive } from '@/lib/tauri-bridge'

const mockStreamPetChat = streamPetChat as ReturnType<typeof vi.fn>
const mockEmit = emit as ReturnType<typeof vi.fn>
const mockPersonaGetActive = petPersonaGetActive as ReturnType<typeof vi.fn>

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
  // Default: persona with empty system_prompt (so existing tests behave as before)
  mockPersonaGetActive.mockResolvedValue({
    id: 'astro',
    name: 'Astro',
    system_prompt: '',
    sprite_set: 'astro',
    greeting: 'Hi!',
    source: 'builtin',
  })
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

  it('injects active persona system_prompt as first message when present', async () => {
    // Override: persona with a non-empty system_prompt
    mockPersonaGetActive.mockResolvedValue({
      id: 'astro',
      name: 'Astro',
      system_prompt: '你是小宇',
      sprite_set: 'astro',
      greeting: '嗨!',
      source: 'builtin',
    })

    mockStreamPetChat.mockImplementation(
      (_msgs: unknown, onDelta: (t: string) => void) => {
        onDelta('你好')
        return Promise.resolve()
      },
    )

    renderChat()

    // Wait for persona to be loaded (petPersonaGetActive is async)
    await waitFor(() => expect(mockPersonaGetActive).toHaveBeenCalled())

    const textarea = screen.getByTestId('pet-chat-input')
    fireEvent.change(textarea, { target: { value: '嗨' } })
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false })

    await waitFor(() => {
      expect(mockStreamPetChat).toHaveBeenCalled()
    })

    // First element of the messages array passed to streamPetChat must be the system message
    expect(vi.mocked(streamPetChat).mock.calls[0][0][0]).toEqual({
      role: 'system',
      content: '你是小宇',
    })
  })
})
