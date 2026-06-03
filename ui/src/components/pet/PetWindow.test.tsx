import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import { petEnabledAtom, petCharacterAtom } from '@/atoms/pet-atoms'

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
})
