import { atom } from 'jotai'

export interface PetMsg { role: 'user' | 'assistant'; content: string }

export const petHistoryAtom = atom<PetMsg[]>([])
export const petBubbleAtom = atom<string | null>(null)
