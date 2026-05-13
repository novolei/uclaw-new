/**
 * Pet widget state atoms. See docs/superpowers/specs/2026-05-13-pet-widget-design.md.
 *
 * Three layers:
 *  - User preferences (persisted): petEnabledAtom, petCharacterAtom
 *  - Primary state (runtime): petPrimaryStateAtom — driven by usePetStateSync
 *  - Hover override (runtime): petHoverActiveAtom — driven by usePetHover
 *  - Display state (derived): petDisplayStateAtom — what PetWidget renders
 *
 * Hover only overrides when primary === 'idle'. Other primary states (thinking /
 * typing / success / error) are agent-critical and must not be interrupted by
 * hover.
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export type PetCharacter = 'astro' | 'clawby'

export type PetPrimaryState = 'idle' | 'thinking' | 'typing' | 'success' | 'error'
export type PetState = PetPrimaryState | 'hover'

export const petEnabledAtom = atomWithStorage<boolean>('pet.enabled', false)
export const petCharacterAtom = atomWithStorage<PetCharacter>('pet.character', 'astro')

export const petPrimaryStateAtom = atom<PetPrimaryState>('idle')
export const petHoverActiveAtom = atom<boolean>(false)

export const petDisplayStateAtom = atom<PetState>((get) => {
  const primary = get(petPrimaryStateAtom)
  const hovering = get(petHoverActiveAtom)
  return hovering && primary === 'idle' ? 'hover' : primary
})
