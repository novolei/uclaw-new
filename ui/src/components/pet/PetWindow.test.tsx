import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import { petEnabledAtom, petCharacterAtom } from '@/atoms/pet-atoms'

// Capture listen callbacks so tests can trigger them manually
const listeners: Record<string, (e: any) => void> = {}
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: (e: any) => void) => {
    listeners[name] = cb
    return Promise.resolve(() => {})
  }),
  emit: vi.fn(),
}))

vi.mock('./PetChat', () => ({ PetChat: ({ onClose }: { onClose: () => void }) =>
  <div data-testid="petchat-mock"><button onClick={onClose}>x</button></div> }))

import { PetWindow } from './PetWindow'

function renderPet() {
  const store = createStore()
  store.set(petEnabledAtom, true)
  store.set(petCharacterAtom, 'astro')
  return render(<Provider store={store}><PetWindow /></Provider>)
}

describe('PetWindow', () => {
  beforeEach(() => {
    // Clear captured listeners between tests
    Object.keys(listeners).forEach((k) => delete listeners[k])
  })

  it('shows the sprite, panel hidden initially', () => {
    renderPet()
    expect(screen.getByTestId('pet-sprite')).toBeInTheDocument()
    expect(screen.queryByTestId('pet-panel')).toBeNull()
  })

  it('click sprite toggles the chat panel', () => {
    renderPet()
    fireEvent.click(screen.getByTestId('pet-sprite'))
    expect(screen.getByTestId('pet-panel')).toBeInTheDocument()
    fireEvent.click(screen.getByTestId('pet-sprite'))
    expect(screen.queryByTestId('pet-panel')).toBeNull()
  })

  it('shows bubble when pet://nudge fires', async () => {
    renderPet()
    // Wait for listen() promise to resolve and register the callback
    await waitFor(() => expect(listeners['pet://nudge']).toBeDefined())

    act(() => {
      listeners['pet://nudge']({ payload: { text: '你好呀' } })
    })

    expect(screen.getByTestId('pet-bubble')).toHaveTextContent('你好呀')
  })

  it('bubble is absent before any nudge', async () => {
    renderPet()
    await waitFor(() => expect(listeners['pet://nudge']).toBeDefined())
    expect(screen.queryByTestId('pet-bubble')).toBeNull()
  })
})
