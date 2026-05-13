/**
 * GlowIndicator — soft three-layer breathing glow at one screen edge
 * during Focus Mode. The outer opacity is driven by mouse-to-edge
 * distance; the Y trace position is imperatively translated into place
 * via a ref + useEffect so we don't trigger a full React re-render on
 * every mousemove (mousemove fires ~60Hz; React reconcile + diff on
 * every event is wasted work for a pure visual effect).
 *
 * Visibility hides entirely (opacity 0) once the matching island is
 * revealed — once the island is on screen the user doesn't need an
 * additional "you can summon me" hint.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { motion } from 'motion/react'
import { cn } from '@/lib/utils'
import {
  focusRevealSideAtom,
  focusMousePosAtom,
} from '@/atoms/focus-mode-atoms'

interface Props { side: 'left' | 'right' }

/** Distance at which the glow starts to brighten. */
const FADE_START_PX = 80
/** Distance at which the glow reaches full opacity. */
const FADE_PEAK_PX = 16

function proximityOpacity(dist: number): number {
  if (dist > FADE_START_PX) return 0
  if (dist < FADE_PEAK_PX) return 1
  return 1 - (dist - FADE_PEAK_PX) / (FADE_START_PX - FADE_PEAK_PX)
}

export function GlowIndicator({ side }: Props): React.ReactElement {
  const reveal = useAtomValue(focusRevealSideAtom)
  const mouse = useAtomValue(focusMousePosAtom)
  const traceRef = React.useRef<HTMLDivElement>(null)

  // Y trace: imperative DOM update, no React re-render on every mousemove.
  React.useEffect(() => {
    const el = traceRef.current
    if (!el) return
    el.style.transform = `translateY(${mouse.y}px)`
  }, [mouse.y])

  const dist = side === 'left'
    ? mouse.x
    : Math.max(0, window.innerWidth - mouse.x)
  const isRevealed = reveal === side
  const opacity = isRevealed ? 0 : proximityOpacity(dist)

  return (
    <motion.div
      aria-hidden
      data-testid={`focus-glow-${side}`}
      animate={{ opacity }}
      transition={{ duration: 0.15, ease: 'easeOut' }}
      className={cn(
        'fixed top-0 bottom-0 z-[79] pointer-events-none w-1',
        side === 'left' ? 'left-0' : 'right-0',
      )}
    >
      <div className={cn('focus-glow-halo', side === 'right' && 'focus-glow-halo-right')} />
      <div className={cn('focus-glow-soft', side === 'right' && 'focus-glow-soft-right')} />
      <div className="focus-glow-core" />
      <div
        ref={traceRef}
        className={cn('focus-glow-trace', side === 'right' && 'focus-glow-trace-right')}
      />
    </motion.div>
  )
}
