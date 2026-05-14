/**
 * KaleidoscopeIconFallback — 万花筒入口图标的静态 SVG 兜底。
 *
 * 当 Lottie 动画数据缺失或运行时加载失败时渲染这个。彩色小篮子 + sparkle，
 * 渐变用 theme token（primary → accent），SVG 内描用 primary-foreground，
 * 保证每个主题下都有对比度。
 *
 * 纯展示组件 —— 不绑定点击；交互由父组件（KaleidoscopeIcon）处理。
 */
import * as React from 'react'

export interface KaleidoscopeIconFallbackProps {
  /** 外框边长（含渐变背板），单位 px。默认 30。 */
  size?: number
}

export function KaleidoscopeIconFallback({
  size = 30,
}: KaleidoscopeIconFallbackProps): React.ReactElement {
  const svgSize = Math.round(size * 0.6)
  return (
    <div
      aria-label="万花筒"
      role="img"
      style={{ width: size, height: size }}
      className="inline-flex items-center justify-center rounded-[8px]
                 bg-gradient-to-br from-primary to-accent
                 shadow-[0_1px_3px_hsl(var(--primary)/0.35)]"
    >
      <svg
        viewBox="0 0 24 24"
        width={svgSize}
        height={svgSize}
        fill="none"
        className="text-primary-foreground"
        aria-hidden
      >
        {/* basket body */}
        <path
          d="M5 10 Q5 9 6 9 H18 Q19 9 19 10 L18 19 Q17.8 20 17 20 H7 Q6.2 20 6 19 Z"
          fill="currentColor"
          opacity="0.95"
        />
        <path d="M5 10 H19" stroke="currentColor" strokeWidth="1.4" opacity="0.6" />
        <ellipse cx="12" cy="9" rx="5.5" ry="0.9" fill="currentColor" opacity="0.4" />
        {/* sparkle */}
        <path
          d="M16.5 4 L17.2 5.5 L18.8 6.2 L17.2 6.9 L16.5 8.4 L15.8 6.9 L14.2 6.2 L15.8 5.5 Z"
          fill="currentColor"
        />
        <circle cx="19.5" cy="3.5" r="0.7" fill="currentColor" opacity="0.85" />
      </svg>
    </div>
  )
}
