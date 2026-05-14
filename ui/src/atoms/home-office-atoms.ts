/**
 * HomeOffice state atoms.
 *
 *  - Panel open/close (persisted via Settings later; runtime atom here)
 *  - Agent state machine (mirrors PetWidget's 5 states + tool_activity)
 *  - Character pose (position / direction / walk-vs-pose)
 *  - In-memory sticky notes + diary entries (Phase 4 persists them)
 *  - Currently-open zone modal
 */
import { atom } from 'jotai'

export const homeOfficePanelOpenAtom = atom(false)

export type HomeOfficeState =
  | 'idle'
  | 'thinking'
  | 'typing'
  | 'tool_activity'
  | 'success'
  | 'error'

export const homeOfficeStateAtom = atom<HomeOfficeState>('idle')

export type Vec2 = { x: number; y: number }

// Default position: in front of the central oak desk
export const characterPositionAtom = atom<Vec2>({ x: 0.50, y: 0.55 })

export type Direction = 'N' | 'NE' | 'E' | 'SE' | 'S' | 'SW' | 'W' | 'NW'
export const characterDirectionAtom = atom<Direction>('S')

export const characterMotionAtom = atom<'walk' | 'pose'>('pose')

export type StickyNote = { id: string; text: string; at: number }
export const stickyNotesAtom = atom<StickyNote[]>([])

export type DiaryEntry = { id: string; text: string; at: number; sessionId: string }
export const diaryEntriesAtom = atom<DiaryEntry[]>([])

export type OpenZone = null | 'music' | 'sticky' | 'diary'
export const openZoneAtom = atom<OpenZone>(null)
