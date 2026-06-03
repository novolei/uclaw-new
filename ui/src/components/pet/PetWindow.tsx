import * as React from 'react'
import { PetWidget } from '@/components/agent/PetWidget'
import { PetChat } from './PetChat'
import './PetWindow.css'

export function PetWindow(): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  return (
    <div className="pet-window-root">
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
