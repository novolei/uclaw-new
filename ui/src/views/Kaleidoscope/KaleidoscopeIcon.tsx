/**
 * KaleidoscopeIcon — 万花筒入口图标（WorkspaceSwitcherBar 最左）。
 *
 * 彩色小篮子 + sparkle，纯 SVG + CSS 动画（无 Lottie 依赖）：
 *  - idle：渐变背板 3.5s 一呼一吸的 glow
 *  - hover：整体 scale 1.06，idle glow 停止；篮子 600ms 摆头一次；sparkle 800ms 闪烁循环
 *  - active（身处万花筒 surface）：ring-2 外圈
 *  - 全部 transform/opacity/filter —— GPU 加速；颜色走 theme token，11 主题自适应
 */
import * as React from 'react'
import { cn } from '@/lib/utils'

export interface KaleidoscopeIconProps {
  /** 当前是否身处万花筒 surface（影响 active 视觉态）。 */
  active?: boolean
  onClick?: () => void
  /** 外框边长 px，默认 30。 */
  size?: number
}

/** SVG <g> 的 transform 必须绕自身中心 —— SVG transform-origin 默认是坐标原点。 */
const G_TRANSFORM: React.CSSProperties = {
  transformBox: 'fill-box',
  transformOrigin: 'center',
}

export function KaleidoscopeIcon({
  active = false,
  onClick,
  size = 30,
}: KaleidoscopeIconProps): React.ReactElement {
  const svgSize = Math.round(size * 0.6)
  return (
    <button
      type="button"
      aria-label="打开万花筒"
      aria-current={active ? 'true' : undefined}
      onClick={onClick}
      style={{ width: size, height: size }}
      className={cn(
        'group titlebar-no-drag inline-flex items-center justify-center rounded-[8px] shrink-0',
        'bg-gradient-to-br from-primary to-accent',
        'transition-transform duration-200 ease-out',
        'hover:scale-[1.06] active:scale-[0.92]',
        'animate-kaleido-idle-breath hover:animate-none',
        active && 'ring-2 ring-primary/40',
      )}
    >
      <svg
        viewBox="0 0 24 24"
        width={svgSize}
        height={svgSize}
        fill="none"
        className="text-primary-foreground"
        aria-hidden
      >
        {/* basket body —— hover 时 600ms 摆头一次 */}
        <g
          style={G_TRANSFORM}
          className="group-hover:animate-kaleido-basket-wobble"
        >
          <path
            d="M5 10 Q5 9 6 9 H18 Q19 9 19 10 L18 19 Q17.8 20 17 20 H7 Q6.2 20 6 19 Z"
            fill="currentColor"
            opacity="0.95"
          />
          <path d="M5 10 H19" stroke="currentColor" strokeWidth="1.4" opacity="0.6" />
          <ellipse cx="12" cy="9" rx="5.5" ry="0.9" fill="currentColor" opacity="0.4" />
        </g>
        {/* sparkle —— hover 时 800ms 闪烁循环 */}
        <g
          style={G_TRANSFORM}
          className="group-hover:animate-kaleido-sparkle-twinkle"
        >
          <path
            d="M16.5 4 L17.2 5.5 L18.8 6.2 L17.2 6.9 L16.5 8.4 L15.8 6.9 L14.2 6.2 L15.8 5.5 Z"
            fill="currentColor"
          />
          <circle cx="19.5" cy="3.5" r="0.7" fill="currentColor" opacity="0.85" />
        </g>
      </svg>
    </button>
  )
}
