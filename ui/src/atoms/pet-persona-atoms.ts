import { atom } from 'jotai'
import type { PetPersona } from '@/lib/tauri-bridge'

/** Frontend cache of the active pet persona (hydrated from petPersonaGetActive). */
export const activePetPersonaAtom = atom<PetPersona | null>(null)
