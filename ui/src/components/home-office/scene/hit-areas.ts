/**
 * Zone hit-area coordinates, normalized to the 1920x1080 scene background.
 * `center` is the click target + character walk-to point; `w`/`h` are the
 * box used for pointer hit detection and hover highlight.
 *
 * `kind`:
 *   - 'modal': opens openZoneAtom = target
 *   - 'navigate': triggers a side effect (settings panel, history)
 *   - 'state': pure visual anchor; no click
 */
export type ZoneKind = 'modal' | 'navigate' | 'state'

export type Zone = {
  id: string
  center: { x: number; y: number }
  w: number
  h: number
  kind: ZoneKind
  target: 'music' | 'sticky' | 'diary' | 'skills' | 'history' | null
  label: string
}

export const ZONES: Record<string, Zone> = {
  garden:  { id: 'garden',  center: { x: 0.18, y: 0.78 }, w: 0.14, h: 0.18, kind: 'navigate', target: 'skills',  label: '🌿 Garden' },
  music:   { id: 'music',   center: { x: 0.13, y: 0.45 }, w: 0.18, h: 0.32, kind: 'modal',    target: 'music',   label: '🎵 Music Gazebo' },
  sticky:  { id: 'sticky',  center: { x: 0.36, y: 0.18 }, w: 0.16, h: 0.24, kind: 'modal',    target: 'sticky',  label: '📌 Sticky Wall' },
  diary:   { id: 'diary',   center: { x: 0.50, y: 0.45 }, w: 0.20, h: 0.40, kind: 'modal',    target: 'diary',   label: '✍️ Oak Desk' },
  library: { id: 'library', center: { x: 0.68, y: 0.22 }, w: 0.13, h: 0.34, kind: 'navigate', target: 'history', label: '📚 Library Tower' },
  fire:    { id: 'fire',    center: { x: 0.42, y: 0.75 }, w: 0.14, h: 0.18, kind: 'state',    target: null,      label: '🔥 Fire Pit' },
  hammock: { id: 'hammock', center: { x: 0.82, y: 0.62 }, w: 0.14, h: 0.18, kind: 'state',    target: null,      label: '🛋️ Hammock' },
  sakura:  { id: 'sakura',  center: { x: 0.70, y: 0.55 }, w: 0.14, h: 0.24, kind: 'state',    target: null,      label: '🌸 Sakura' },
} as const

// State → which zone the character should walk to
export const STATE_TO_ZONE: Record<string, keyof typeof ZONES | null> = {
  idle:          'hammock',
  thinking:      'library',
  typing:        'diary',
  tool_activity: 'fire',   // workshop/forge anchor — colocated with fire pit visually
  success:       null,     // stays in place 4s then walks to hammock
  error:         'fire',
}
