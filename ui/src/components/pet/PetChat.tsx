import * as React from 'react'

// Placeholder — real chat panel lands in Slice E Task 3.
export function PetChat({ onClose }: { onClose: () => void }): React.ReactElement {
  return (
    <div data-testid="pet-chat-placeholder">
      <button type="button" onClick={onClose} aria-label="收起">×</button>
    </div>
  )
}
