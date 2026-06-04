import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { PetWidget } from '@/components/agent/PetWidget'
import { PetChat } from './PetChat'
import { petBubbleAtom } from '@/atoms/pet-chat-atoms'
import { petCharacterAtom } from '@/atoms/pet-atoms'
import { petPersonaGetActive } from '@/lib/tauri-bridge'
import './PetWindow.css'

export function PetWindow(): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  const [bubble, setBubble] = useAtom(petBubbleAtom)
  const setCharacter = useSetAtom(petCharacterAtom)

  // Listen for pet://nudge events (broadcast from any window)
  React.useEffect(() => {
    let un: (() => void) | undefined
    let cancelled = false
    listen<{ text: string }>('pet://nudge', (e) => setBubble(e.payload?.text ?? null))
      .then((fn) => { if (cancelled) fn(); else un = fn })
    return () => { cancelled = true; un?.() }
  }, [setBubble])

  // Apply active persona's sprite_set on mount + live-update on persona-changed
  React.useEffect(() => {
    let cancelled = false
    const apply = () => petPersonaGetActive().then((p) => {
      if (!cancelled && (p.sprite_set === 'astro' || p.sprite_set === 'clawby')) setCharacter(p.sprite_set)
    }).catch(() => {})
    apply()
    let un: (() => void) | undefined
    listen('pet://persona-changed', apply).then((fn) => { if (cancelled) fn(); else un = fn })
    return () => { cancelled = true; un?.() }
  }, [setCharacter])

  // Auto-dismiss the bubble after 6 seconds
  React.useEffect(() => {
    if (!bubble) return
    const t = window.setTimeout(() => setBubble(null), 6000)
    return () => window.clearTimeout(t)
  }, [bubble, setBubble])

  return (
    <div className="pet-window-root">
      {bubble && <div className="pet-bubble" data-testid="pet-bubble">{bubble}</div>}
      <div
        className="pet-sprite"
        data-tauri-drag-region
        onClick={() => setExpanded((e) => !e)}
        data-testid="pet-sprite"
      >
        <PetWidget />
      </div>
      {expanded && (
        <div className="pet-panel" data-testid="pet-panel">
          <PetChat onClose={() => setExpanded(false)} />
        </div>
      )}
    </div>
  )
}
