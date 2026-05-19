import * as React from 'react'

/**
 * Visual-only affordance hinting that the dock supports reorder / pinning.
 * Renders four small dots centered above the dock body. Default opacity 0;
 * the dock's hover state (applied via the `group` modifier in BottomDock)
 * fades it to 0.55 over 150 ms.
 *
 * Phase 2 wires the actual drag behavior via dnd-kit; this component stays
 * presentational and decoupled from drag state.
 */
export function DockDragHandle(): React.ReactElement {
  return (
    <div
      aria-hidden="true"
      data-state="idle"
      data-dock-drag-handle
      className="pointer-events-none absolute left-1/2 -translate-x-1/2 top-1.5 flex items-center justify-center gap-[3px] opacity-0 group-hover:opacity-[0.55] transition-opacity duration-150"
    >
      <span aria-hidden="true" className="block w-1 h-1 rounded-full bg-foreground/45" />
      <span aria-hidden="true" className="block w-1 h-1 rounded-full bg-foreground/45" />
      <span aria-hidden="true" className="block w-1 h-1 rounded-full bg-foreground/45" />
      <span aria-hidden="true" className="block w-1 h-1 rounded-full bg-foreground/45" />
    </div>
  )
}
