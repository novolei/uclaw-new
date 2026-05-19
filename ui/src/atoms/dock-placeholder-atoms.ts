/**
 * Dock placeholder panel atoms — Connections + Alert.
 *
 * These two dock surfaces ship as "under construction" placeholders ahead
 * of the real views (which are scheduled in a later phase). The atoms
 * control whether each placeholder Dialog is open. Not persisted —
 * placeholders always start closed on a fresh app launch.
 */
import { atom } from 'jotai'

export const connectionsPanelOpenAtom = atom(false)
export const alertPanelOpenAtom = atom(false)
