import * as React from 'react'
import { motion, useSpring, useReducedMotion } from 'motion/react'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'

interface DockItemProps {
  icon: React.ReactNode
  label: string
  isActive: boolean
  index: number
  hoveredIndex: number | null
  onHoverIndexChange: (index: number | null) => void
  onClick: () => void
}

/**
 * 单个 dock 图标。每个 item 占用一个固定宽度的 slot（SLOT_W），icon 在 slot
 * 内部 scale，永远不会越过邻居的 slot 边界 —— 杜绝 hover 时按钮重叠。
 *
 * 放大原点固定在 slot 底部中央，向上长出，模拟 macOS Dock 的视觉行为：
 * 图标随鼠标距离平滑增大并轻微上抬，邻居被牵动得更弱，远处保持原状。
 *
 * 标签不再内嵌于 dock 内（避免推挤兄弟节点），改用 Radix Tooltip 在图标
 * 正上方悬浮，hover 才出现；active 状态用底部小圆点指示。
 */
const SLOT_W = 48 // px, holds 40px icon comfortably even at hover scale (~1.3x)
const ICON_BOX = 38 // px
const HOVER_SCALE = 1.34
const NEIGHBOR_SCALE = 1.12
const HOVER_LIFT = -4 // px
const NEIGHBOR_LIFT = -1 // px

export function DockItem({
  icon,
  label,
  isActive,
  index,
  hoveredIndex,
  onHoverIndexChange,
  onClick,
}: DockItemProps): React.ReactElement {
  const prefersReducedMotion = useReducedMotion()
  const distance =
    hoveredIndex === null ? Infinity : Math.abs(index - hoveredIndex)

  const scaleSpring = useSpring(1, { stiffness: 320, damping: 26, mass: 0.6 })
  const ySpring = useSpring(0, { stiffness: 320, damping: 26, mass: 0.6 })

  React.useEffect(() => {
    if (prefersReducedMotion) {
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
  }, [distance, scaleSpring, ySpring, prefersReducedMotion])

  return (
    <TooltipProvider delayDuration={140} skipDelayDuration={80}>
      <Tooltip>
        <TooltipTrigger asChild>
          <motion.button
            type="button"
            className="relative flex items-end justify-center select-none outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-offset-0 rounded-[14px]"
            style={{
              width: SLOT_W,
              height: SLOT_W,
              scale: scaleSpring,
              y: ySpring,
              transformOrigin: 'bottom center',
            }}
            onMouseEnter={() => onHoverIndexChange(index)}
            onMouseLeave={() => onHoverIndexChange(null)}
            onClick={onClick}
            aria-label={label}
            aria-pressed={isActive}
          >
            {/* Icon renders flush — no slot backplate. The Liquid Glass PNG
                IS the visual; an inner pill would compete with it. */}
            <span
              className="flex items-center justify-center"
              style={{ width: ICON_BOX, height: ICON_BOX }}
            >
              {icon}
            </span>
            {/* Active indicator — solid primary dot 4 px below the button bottom edge, with a soft glow. */}
            {isActive && (
              <span
                data-dock-active-dot
                className="pointer-events-none absolute left-1/2 -translate-x-1/2 -bottom-1 w-1 h-1 rounded-full bg-primary shadow-[0_0_6px_hsl(var(--primary)/0.5)]"
                aria-hidden="true"
              />
            )}
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
  )
}
