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

vi.mock('./PetChat', () => ({ PetChat: ({ onClose }: { onClose: () => void }) =>
  <div data-testid="petchat-mock"><button onClick={onClose}>x</button></div> }))

import { PetWindow } from './PetWindow'
import { petPersonaGetActive } from '@/lib/tauri-bridge'

const mockPersonaGetActive = petPersonaGetActive as ReturnType<typeof vi.fn>

function renderPet(store?: ReturnType<typeof createStore>) {
  const s = store ?? createStore()
  s.set(petEnabledAtom, true)
  s.set(petCharacterAtom, 'astro')
  return { store: s, ...render(<Provider store={s}><PetWindow /></Provider>) }
}

describe('PetWindow', () => {
  beforeEach(() => {
    // Clear captured listeners between tests
    Object.keys(listeners).forEach((k) => delete listeners[k])
    vi.clearAllMocks()
    // Default: astro persona
    mockPersonaGetActive.mockResolvedValue({
      id: 'astro',
      name: 'Astro',
      system_prompt: '',
      sprite_set: 'astro',
      greeting: 'Hi!',
      source: 'builtin',
    })
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

  it('applies clawby sprite_set from active persona on mount', async () => {
    mockPersonaGetActive.mockResolvedValue({
      id: 'clawby',
      name: 'Clawby',
      system_prompt: '',
      sprite_set: 'clawby',
      greeting: '爪!',
      source: 'builtin',
    })

    const store = createStore()
    store.set(petEnabledAtom, true)
    store.set(petCharacterAtom, 'astro') // starts as astro

    render(<Provider store={store}><PetWindow /></Provider>)

    // After mount, petPersonaGetActive resolves and setCharacter('clawby') is called
    await waitFor(() => {
      expect(store.get(petCharacterAtom)).toBe('clawby')
    })
  })
})
