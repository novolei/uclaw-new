import * as React from 'react'
import { motion, useSpring, useReducedMotion } from 'motion/react'
import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { useSetAtom, useAtomValue } from 'jotai'
import { pinIdColorSeed, dockOrderAtom, removeDockPin } from '@/atoms/dock-atoms'
import { toast } from 'sonner'
import { PinOff } from 'lucide-react'

/**
 * Renders a pinned dock entry (conversation / workspace / automation) as
 * a CSS squircle with a deterministic 2-color gradient backplate and a
 * single character glyph (emoji if provided, else first letter of label
 * uppercased). Visually adjacent to DockItem (same SLOT_W / ICON_BOX /
 * magnification spring), but the icon visual is generated rather than
 * a PNG — pinned entries don't have a Liquid Glass asset.
 *
 * The active dot indicator from DockItem is intentionally absent: pinned
 * items don't have a current/active state in Phase 2B (only modes do).
 */
const SLOT_W = 56
const ICON_BOX = 44
const HOVER_SCALE = 1.34
const NEIGHBOR_SCALE = 1.12
const HOVER_LIFT = -4
const NEIGHBOR_LIFT = -1

interface DockPinnedItemProps {
  sortableId: string
  label: string
  /** Optional emoji takes precedence over the label initial. */
  emoji?: string
  index: number
  hoveredIndex: number | null
  onHoverIndexChange: (index: number | null) => void
  onClick: () => void
}

export function DockPinnedItem({
  sortableId,
  label,
  emoji,
  index,
  hoveredIndex,
  onHoverIndexChange,
  onClick,
}: DockPinnedItemProps): React.ReactElement {
  const prefersReducedMotion = useReducedMotion()
  const distance =
    hoveredIndex === null ? Infinity : Math.abs(index - hoveredIndex)

  const scaleSpring = useSpring(1, { stiffness: 320, damping: 26, mass: 0.6 })
  const ySpring = useSpring(0, { stiffness: 320, damping: 26, mass: 0.6 })

  // DockPinnedItem always has a sortableId (required prop), so we pass it
  // directly — no dummy-fallback pattern needed (unlike DockItem).
  const sortable = useSortable({ id: sortableId })

  const dockOrder = useAtomValue(dockOrderAtom)
  const setDockOrder = useSetAtom(dockOrderAtom)

  const handleUnpin = React.useCallback(() => {
    const next = removeDockPin(dockOrder, sortableId)
    if (next === dockOrder) return // safety: already gone
    setDockOrder(next)
    toast.success('已从 Dock 取消固定')
  }, [dockOrder, setDockOrder, sortableId])

  React.useEffect(() => {
    // While dragging, suppress magnification — the lifted item carries its
    // own constant 1.05 scale; neighbors shouldn't bobble in/out.
    if (sortable.isDragging || prefersReducedMotion) {
      scaleSpring.set(1)
      ySpring.set(0)
      return
    }
    if (distance === 0) {
      scaleSpring.set(HOVER_SCALE)
      ySpring.set(HOVER_LIFT)
    } else if (distance === 1) {
      scaleSpring.set(NEIGHBOR_SCALE)
      ySpring.set(NEIGHBOR_LIFT)
    } else {
      scaleSpring.set(1)
      ySpring.set(0)
    }
  }, [distance, scaleSpring, ySpring, prefersReducedMotion, sortable.isDragging])

  const dragTransform = sortable.transform
    ? CSS.Transform.toString(sortable.transform)
    : undefined

  // Branch the style object: dnd-kit owns transform during drag; motion
  // springs own it otherwise. Avoids motion-vs-CSS transform collision.
  const motionStyle = sortable.isDragging
    ? {
        width: SLOT_W,
        height: SLOT_W,
        transformOrigin: 'bottom center' as const,
        transform: dragTransform
          ? `${dragTransform} scale(1.05)`
          : 'scale(1.05)',
        transition: sortable.transition,
        zIndex: 50,
      }
    : {
        width: SLOT_W,
        height: SLOT_W,
        scale: scaleSpring,
        y: ySpring,
        transformOrigin: 'bottom center' as const,
      }

  const seed = pinIdColorSeed(sortableId)
  const tileBackground = `linear-gradient(135deg, ${seed.from} 0%, ${seed.to} 100%)`
  const glyph = emoji ?? label.charAt(0).toUpperCase()

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <span className="contents">
          <TooltipProvider delayDuration={140} skipDelayDuration={80}>
            <Tooltip>
              <TooltipTrigger asChild>
                <motion.button
                  ref={sortable.setNodeRef}
                  type="button"
                  data-sortable-id={sortableId}
                  data-dragging={sortable.isDragging ? 'true' : undefined}
                  data-dock-pin
                  className="relative flex items-end justify-center select-none outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-offset-0 rounded-[14px]"
                  style={motionStyle}
                  {...sortable.attributes}
                  {...sortable.listeners}
                  onMouseEnter={() => onHoverIndexChange(index)}
                  onMouseLeave={() => onHoverIndexChange(null)}
                  onClick={onClick}
                  aria-label={label}
                >
                  <span
                    data-dock-pin-tile
                    aria-hidden="true"
                    className="flex items-center justify-center rounded-[11px] text-white font-semibold text-[18px] shadow-[inset_0_-1px_2px_rgba(0,0,0,0.15),inset_0_1px_1px_rgba(255,255,255,0.18)]"
                    style={{
                      width: ICON_BOX,
                      height: ICON_BOX,
                      background: tileBackground,
                    }}
                  >
                    <span data-dock-pin-glyph>{glyph}</span>
                  </span>
                </motion.button>
              </TooltipTrigger>
              <TooltipContent
                side="top"
                sideOffset={10}
                className="text-[11px] font-medium px-2 py-1 rounded-md bg-popover/95 text-popover-foreground border border-border/60 shadow-md"
              >
                {label}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </span>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={handleUnpin}>
          <PinOff size={14} className="mr-2" />
          从 Dock 取消固定
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}
