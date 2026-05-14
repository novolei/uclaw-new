/**
 * KaleidoscopeIcon — 万花筒入口图标（WorkspaceSwitcherBar 最左）。
 *
 * lucide Aperture（光圈），比 workspace 图标略大（size-8）且常驻主色 tint
 * （text-primary + bg-primary/10）—— 它是"传送门"入口，要在一排灰色 workspace
 * 图标里跳出来。与 KaleidoscopeRail 底部的返回按钮成对、同尺寸同处理。
 * hover 时用 motion 让光圈缓慢旋转（万花筒转动的隐喻），离开平滑归位。
 * 点击时从图标中心迸发一小簇五彩纸屑（confetti burst）—— 节庆性瞬时装饰，
 * 用固定喜庆色板（非主题 UI chrome），650ms 后自动清理。
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

/**
 * 点击迸发的五彩纸屑 —— 每片固定方向 / 颜色 / 旋转，避免每次渲染重算。
 * 节庆性瞬时装饰，刻意用固定喜庆色板而非主题 token（confetti 本就该五彩）。
 * dx/dy 飞散半径控制在 ~26px 内，留在图标附近、不被父容器裁掉。
 */
const CONFETTI = [
  { dx: -22, dy: -16, rot: -120, color: '#f59e0b' },
  { dx: 20, dy: -20, rot: 150, color: '#ec4899' },
  { dx: -6, dy: -26, rot: 70, color: '#06b6d4' },
  { dx: 14, dy: -8, rot: -190, color: '#8b5cf6' },
  { dx: -18, dy: 8, rot: 95, color: '#22c55e' },
  { dx: 24, dy: 4, rot: -85, color: '#f59e0b' },
  { dx: 2, dy: 20, rot: 165, color: '#ec4899' },
  { dx: -14, dy: 22, rot: -135, color: '#06b6d4' },
  { dx: 12, dy: 26, rot: 120, color: '#8b5cf6' },
] as const

export function KaleidoscopeIcon({
  active = false,
  onClick,
}: KaleidoscopeIconProps): React.ReactElement {
  const [hovered, setHovered] = React.useState(false)
  const [burst, setBurst] = React.useState<number | null>(null)
  const burstSeq = React.useRef(0)

  const handleClick = React.useCallback(() => {
    const id = ++burstSeq.current
    setBurst(id)
    // 650ms 后清理纸屑 DOM（晚于 550ms 的飞散动画）。守卫 cur === id 防止
    // 快速连点时旧 timeout 误清掉新一轮 burst。
    window.setTimeout(() => setBurst((cur) => (cur === id ? null : cur)), 650)
    onClick?.()
  }, [onClick])

  return (
    <button
      type="button"
      aria-label="打开万花筒"
      aria-current={active ? 'true' : undefined}
      onClick={handleClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      className={cn(
        'titlebar-no-drag relative inline-flex items-center justify-center',
        'size-8 rounded-md transition-colors shrink-0 text-primary',
        active ? 'bg-primary/20' : 'bg-primary/10 hover:bg-primary/20',
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
        <Aperture className="size-[18px]" aria-hidden />
      </motion.span>

      {/* 点击迸发的五彩纸屑 —— burst 变化（key）触发整簇重新挂载、重放动画 */}
      {burst !== null && (
        <span
          key={burst}
          aria-hidden
          data-testid="kaleidoscope-confetti"
          className="pointer-events-none absolute inset-0"
        >
          {CONFETTI.map((c, i) => (
            <motion.span
              key={i}
              className="absolute left-1/2 top-1/2 size-1 -ml-0.5 -mt-0.5 rounded-[1px]"
              style={{ backgroundColor: c.color }}
              initial={{ x: 0, y: 0, scale: 0, opacity: 1, rotate: 0 }}
              animate={{ x: c.dx, y: c.dy, scale: 1, opacity: 0, rotate: c.rot }}
              transition={{ duration: 0.55, ease: [0.22, 0.61, 0.36, 1] }}
            />
          ))}
        </span>
      )}
    </button>
  )
}
