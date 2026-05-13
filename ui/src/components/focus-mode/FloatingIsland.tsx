/**
 * FloatingIsland — visual wrapper that animates a sidebar into / out of
 * a rounded "island" over the central preview area. The sidebar component
 * (LeftSidebar / RightSidePanel) is passed as `children` and rendered
 * unmodified — this wrapper only handles positioning + animation.
 *
 * Reveal lifecycle (2026-05-13: click-auto-pin removed):
 *   - The island shows while `focusRevealSideAtom === side`. The hotzone
 *     hook owns the show/hide state machine entirely via mouse position.
 *   - Clicking INSIDE the island no longer pins it. Moving the mouse out
 *     starts the 200ms leave timer like any other exit — the user can
 *     click a session row, then drift the mouse back to the preview and
 *     the island auto-hides 200ms later, just like hovering away.
 *   - `focusRevealPinnedAtom` is retained but currently unused — kept
 *     for a possible future "explicit pin button" affordance.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { AnimatePresence, motion, type Variants } from 'motion/react'
import { cn } from '@/lib/utils'
import { focusRevealSideAtom } from '@/atoms/focus-mode-atoms'
import {
  ISLAND_EDGE_GAP,
  ISLAND_LEFT_WIDTH,
  ISLAND_RIGHT_WIDTH,
} from '@/lib/focus-mode-geometry'

interface Props {
  side: 'left' | 'right'
  children: React.ReactNode
}

const islandVariants: Variants = {
  hidden: (side: 'left' | 'right') => ({
    x: side === 'left' ? 'calc(-100% - 12px)' : 'calc(100% + 12px)',
    opacity: 0,
    scale: 0.96,
  }),
  shown: { x: 0, opacity: 1, scale: 1 },
}

export function FloatingIsland({ side, children }: Props): React.ReactElement {
  const reveal = useAtomValue(focusRevealSideAtom)
  const islandRef = React.useRef<HTMLDivElement>(null)
  const visible = reveal === side

  const width = side === 'left' ? ISLAND_LEFT_WIDTH : ISLAND_RIGHT_WIDTH
  const sidePos = side === 'left' ? `left-3` : `right-3`

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          ref={islandRef}
          custom={side}
          variants={islandVariants}
          initial="hidden"
          animate="shown"
          exit="hidden"
          transition={{ duration: 0.26, ease: [0.32, 0.72, 0, 1] }}
          className={cn(
            'fixed z-[80]',
            sidePos,
            'rounded-xl bg-popover/96 backdrop-blur-md overflow-hidden',
            'shadow-[0_1px_3px_rgba(0,0,0,0.10),0_12px_36px_-8px_rgba(0,0,0,0.25),0_0_0_1px_hsl(var(--border)/0.4)]',
          )}
          style={{
            top: ISLAND_EDGE_GAP,
            bottom: ISLAND_EDGE_GAP,
            width,
          }}
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  )
}
