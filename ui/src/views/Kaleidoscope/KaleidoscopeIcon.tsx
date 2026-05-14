/**
 * KaleidoscopeIcon — 万花筒入口图标（WorkspaceSwitcherBar 最左）。
 *
 * 单色 lucide Aperture（光圈），与 workspace 图标同款处理（size-7 rounded-md、
 * 默认 text-foreground/55、hover 提亮+底色、active 主色 tint）。
 * hover 时用 motion 让光圈缓慢旋转（万花筒转动的隐喻），离开平滑归位。
 * 无常驻动画、无新依赖（motion 已在栈内）。
 */
import * as React from 'react'
import { Aperture } from 'lucide-react'
import { motion } from 'motion/react'
import { cn } from '@/lib/utils'

export interface KaleidoscopeIconProps {
  /** 当前是否身处万花筒 surface（影响 active 视觉态）。 */
  active?: boolean
  onClick?: () => void
}

export function KaleidoscopeIcon({
  active = false,
  onClick,
}: KaleidoscopeIconProps): React.ReactElement {
  const [hovered, setHovered] = React.useState(false)
  return (
    <button
      type="button"
      aria-label="打开万花筒"
      aria-current={active ? 'true' : undefined}
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      className={cn(
        'titlebar-no-drag relative inline-flex items-center justify-center',
        'size-7 rounded-md transition-colors shrink-0',
        active
          ? 'bg-primary/15 text-primary'
          : 'text-foreground/55 hover:text-foreground hover:bg-foreground/[0.05]',
      )}
    >
      <motion.span
        className="inline-flex"
        animate={{ rotate: hovered ? 360 : 0 }}
        transition={
          hovered
            ? { repeat: Infinity, duration: 2.6, ease: 'linear' }
            : { duration: 0.4, ease: 'easeOut' }
        }
      >
        <Aperture className="size-4" aria-hidden />
      </motion.span>
    </button>
  )
}
