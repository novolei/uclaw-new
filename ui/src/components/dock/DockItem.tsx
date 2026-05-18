import * as React from 'react'
import { motion, useSpring } from 'motion/react'
import { cn } from '@/lib/utils'

interface DockItemProps {
  icon: React.ReactNode
  label: string
  isActive: boolean
  index: number
  hoveredIndex: number | null
  onHoverIndexChange: (index: number | null) => void
  onClick: () => void
}

export function DockItem({
  icon,
  label,
  isActive,
  index,
  hoveredIndex,
  onHoverIndexChange,
  onClick,
}: DockItemProps) {
  const distance = hoveredIndex === null ? Infinity : Math.abs(index - hoveredIndex)

  const scaleSpring = useSpring(1, { stiffness: 320, damping: 22 })
  const ySpring = useSpring(0, { stiffness: 320, damping: 22 })

  React.useEffect(() => {
    if (distance === 0) {
      scaleSpring.set(1.38)
      ySpring.set(-5)
    } else if (distance === 1) {
      scaleSpring.set(1.15)
      ySpring.set(-2)
    } else {
      scaleSpring.set(1)
      ySpring.set(0)
    }
  }, [distance, scaleSpring, ySpring])

  // L1: label expands on hover or when active
  const showLabel = distance === 0 || isActive

  return (
    <motion.button
      className="relative flex flex-col items-center gap-0.5 select-none outline-none focus-visible:ring-2 focus-visible:ring-indigo-500/50 rounded-[11px]"
      style={{ scale: scaleSpring, y: ySpring }}
      onMouseEnter={() => onHoverIndexChange(index)}
      onMouseLeave={() => onHoverIndexChange(null)}
      onClick={onClick}
      aria-label={label}
      aria-pressed={isActive}
    >
      <div
        className={cn(
          'w-10 h-10 rounded-[11px] flex items-center justify-center transition-colors duration-150',
          isActive
            ? 'bg-gradient-to-b from-indigo-500/40 to-indigo-600/30 ring-1 ring-indigo-500/50 shadow-[0_0_12px_rgba(99,102,241,0.4)]'
            : 'bg-white/[0.08] hover:bg-white/[0.12]'
        )}
      >
        {icon}
      </div>
      {/* Active dot */}
      {isActive && (
        <span className="absolute -bottom-1.5 w-1 h-1 rounded-full bg-indigo-400" />
      )}
      {/* L1 label — max-width transition */}
      <span
        className="text-[10px] text-white/60 font-medium overflow-hidden whitespace-nowrap"
        style={{
          maxWidth: showLabel ? '60px' : '0px',
          opacity: showLabel ? 1 : 0,
          transition: 'max-width 500ms cubic-bezier(0.22, 1, 0.36, 1), opacity 500ms cubic-bezier(0.22, 1, 0.36, 1)',
        }}
      >
        {label}
      </span>
    </motion.button>
  )
}
