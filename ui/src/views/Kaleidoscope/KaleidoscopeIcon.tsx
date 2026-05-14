/**
 * KaleidoscopeIcon — 万花筒入口图标（放在 WorkspaceSwitcherBar 最左）。
 *
 * 行为：
 *  - 无 animationData（Phase 1 默认，Lottie 文件尚未到位）→ 渲染静态
 *    KaleidoscopeIconFallback。
 *  - 有 animationData → 渲染 Lottie；hover 播放、leave 倒回 frame 0、
 *    active 定格结尾帧。Lottie 渲染被 ErrorBoundary 包裹，运行时报错
 *    回退到 KaleidoscopeIconFallback。
 *
 * Lottie JSON 到位后，调用方传入 `animationData={import('...json')}` 即可。
 * active 态的结尾帧号待 Lottie 文件到位后由 props 补充（见 spec §6.1）。
 */
import * as React from 'react'
import Lottie, { type LottieRefCurrentProps } from 'lottie-react'
import { cn } from '@/lib/utils'
import { KaleidoscopeIconFallback } from './KaleidoscopeIconFallback'

export interface KaleidoscopeIconProps {
  /** Lottie 动画 JSON。缺省时走静态 SVG 兜底。 */
  animationData?: object
  /** 当前是否身处万花筒 surface（影响 active 视觉态）。 */
  active?: boolean
  onClick?: () => void
  /** 外框边长 px，默认 30。 */
  size?: number
}

/** 包裹 Lottie 渲染，运行时异常时回退到静态 SVG。 */
class LottieErrorBoundary extends React.Component<
  { fallback: React.ReactNode; children: React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false }
  static getDerivedStateFromError(): { hasError: boolean } {
    return { hasError: true }
  }
  componentDidCatch(err: unknown): void {
    console.warn('[KaleidoscopeIcon] Lottie render failed, using static fallback:', err)
  }
  render(): React.ReactNode {
    return this.state.hasError ? this.props.fallback : this.props.children
  }
}

export function KaleidoscopeIcon({
  animationData,
  active = false,
  onClick,
  size = 30,
}: KaleidoscopeIconProps): React.ReactElement {
  const lottieRef = React.useRef<LottieRefCurrentProps>(null)

  const handleEnter = React.useCallback(() => {
    lottieRef.current?.setDirection(1)
    lottieRef.current?.play()
  }, [])
  const handleLeave = React.useCallback(() => {
    lottieRef.current?.setDirection(-1)
    lottieRef.current?.play()
  }, [])

  const fallback = <KaleidoscopeIconFallback size={size} />

  const inner = animationData ? (
    <LottieErrorBoundary fallback={fallback}>
      <Lottie
        lottieRef={lottieRef}
        animationData={animationData}
        autoplay={false}
        loop
        style={{ width: size, height: size }}
      />
    </LottieErrorBoundary>
  ) : (
    fallback
  )

  return (
    <button
      type="button"
      aria-label="打开万花筒"
      aria-current={active ? 'true' : undefined}
      onClick={onClick}
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
      className={cn(
        'titlebar-no-drag inline-flex items-center justify-center rounded-[8px]',
        'transition-transform duration-200 ease-out shrink-0',
        'hover:scale-[1.06] active:scale-[0.92]',
        active && 'ring-2 ring-primary/40',
      )}
    >
      {inner}
    </button>
  )
}
