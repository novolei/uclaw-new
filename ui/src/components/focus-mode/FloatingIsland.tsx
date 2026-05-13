/**
 * FloatingIsland — visual wrapper that animates a sidebar into / out of
 * a rounded "island" over the central preview area. The sidebar component
 * (LeftSidebar / RightSidePanel) is passed as `children` and rendered
 * unmodified — this wrapper only handles positioning + animation + the
 * click-outside-to-unpin contract.
 *
 * Click-outside detection uses a capture-phase document listener and
 * explicitly EXCLUDES Radix portal nodes ([data-radix-portal] /
 * data-radix-popper-content-wrapper / [role="dialog"]) so that
 * dropdowns, tooltips, and the global ApprovalModal can be interacted
 * with without un-pinning the island.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { AnimatePresence, motion, type Variants } from 'motion/react'
import { cn } from '@/lib/utils'
import {
  focusRevealSideAtom,
  focusRevealPinnedAtom,
} from '@/atoms/focus-mode-atoms'
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

/** Returns true if `target` is inside a Radix-managed floating overlay
 *  (portal / popper / dialog). These nodes are visually OUTSIDE the
 *  island in the DOM but are logically "inside the same interaction" —
 *  clicking them must not un-pin. */
function isInsideRadixPortal(target: Element | null): boolean {
  if (!target) return false
  return Boolean(
    target.closest('[data-radix-portal]') ||
    target.closest('[data-radix-popper-content-wrapper]') ||
    target.closest('[role="dialog"]') ||
    target.closest('[role="menu"]') ||
    target.closest('[role="tooltip"]'),
  )
}

export function FloatingIsland({ side, children }: Props): React.ReactElement {
  const reveal = useAtomValue(focusRevealSideAtom)
  const setPinned = useSetAtom(focusRevealPinnedAtom)
  const islandRef = React.useRef<HTMLDivElement>(null)
  const visible = reveal === side

  React.useEffect(() => {
    if (!visible) return
    const onDocClick = (e: MouseEvent) => {
      const target = e.target as Element | null
      if (isInsideRadixPortal(target)) return
      if (islandRef.current?.contains(target)) {
        setPinned(true)
      } else {
        setPinned(false)
      }
    }
    document.addEventListener('click', onDocClick, true)
    return () => document.removeEventListener('click', onDocClick, true)
  }, [visible, setPinned])

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
