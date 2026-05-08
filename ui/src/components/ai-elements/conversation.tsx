/**
 * Conversation — 对话容器组件
 *
 * 提供：
 * 1. 滚动容器 + 自动滚到底部按钮
 * 2. **自动跟随新内容到底部** — 当用户处于底部时，新消息（流式输出）/
 *    展开 ThinkingBlock 等任何会撑高内容的操作都会让视图自动跟到最底
 * 3. 通过 `useConversationContext()` 暴露 `scrollToBottom`，
 *    供 `ScrollPositionManager` 在 sessionId 变化时跳到底部
 */

import * as React from 'react'
import { ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'

// ===== ConversationContext — 向子组件（minimap、ScrollTopLoader、ScrollPositionManager）暴露 =====

interface ConversationContextValue {
  scrollRef: React.RefObject<HTMLDivElement | null>
  /** 外壳容器（不参与滚动），供 minimap 等浮层 portal 进入 */
  viewportEl: HTMLDivElement | null
  /** 主动滚动到底部 — 同时把 isAtBottom 标记重置为 true，保证之后的新内容继续跟随 */
  scrollToBottom: (behavior?: ScrollBehavior) => void
}

const ConversationContext = React.createContext<ConversationContextValue | null>(null)

export function useConversationContext(): ConversationContextValue | null {
  return React.useContext(ConversationContext)
}

// ===== Conversation 容器 =====

interface ConversationProps {
  resize?: 'smooth' | 'instant'
  className?: string
  children: React.ReactNode
}

/** 距离底部多少像素以内仍视为"在底部"（决定是否自动跟随） */
const STICK_THRESHOLD = 50
/** 显示"回到底部"按钮的距离阈值 */
const SHOW_BUTTON_THRESHOLD = 100

export function Conversation({ resize, className, children }: ConversationProps): React.ReactElement {
  const scrollRef = React.useRef<HTMLDivElement>(null)
  const contentRef = React.useRef<HTMLDivElement>(null)
  const [viewportEl, setViewportEl] = React.useState<HTMLDivElement | null>(null)
  const [showScrollButton, setShowScrollButton] = React.useState(false)

  /** 当前是否处于底部（用 ref，避免 ResizeObserver 闭包读到旧值） */
  const isAtBottomRef = React.useRef(true)
  /** 用户是否主动滚动过（区分"程序自动 scroll"和"用户拖动 scrollbar"） */
  const userScrollingRef = React.useRef(false)
  /** 主动 scrollToBottom 调用期间，临时屏蔽 onScroll 把 isAtBottom 改成 false */
  const suppressScrollListenerRef = React.useRef(false)

  // —— 内部工具：当前是否在底部 ——
  const computeIsAtBottom = React.useCallback((): boolean => {
    const el = scrollRef.current
    if (!el) return true
    const { scrollTop, scrollHeight, clientHeight } = el
    return scrollHeight - scrollTop - clientHeight <= STICK_THRESHOLD
  }, [])

  // —— 主动滚到底部 ——
  const scrollToBottom = React.useCallback((behavior: ScrollBehavior = 'auto') => {
    const el = scrollRef.current
    if (!el) return
    suppressScrollListenerRef.current = true
    el.scrollTo({ top: el.scrollHeight, behavior })
    isAtBottomRef.current = true
    setShowScrollButton(false)
    // 给 smooth 滚动留一点恢复时间
    const releaseAfter = behavior === 'smooth' ? 500 : 50
    window.setTimeout(() => { suppressScrollListenerRef.current = false }, releaseAfter)
  }, [])

  // —— scroll 事件：仅在用户主动滚动时更新 isAtBottom ——
  const handleScroll = React.useCallback(() => {
    if (suppressScrollListenerRef.current) return
    const el = scrollRef.current
    if (!el) return
    const { scrollTop, scrollHeight, clientHeight } = el
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight
    isAtBottomRef.current = distanceFromBottom <= STICK_THRESHOLD
    setShowScrollButton(distanceFromBottom > SHOW_BUTTON_THRESHOLD)
    userScrollingRef.current = true
  }, [])

  // —— 自动跟随：当内容尺寸变化（流式新增 / 展开 thinking block 等）时，
  //         若当前在底部，把视图保持在底部 ——
  React.useEffect(() => {
    const scrollEl = scrollRef.current
    const contentEl = contentRef.current
    if (!scrollEl || !contentEl) return

    const observer = new ResizeObserver(() => {
      if (!isAtBottomRef.current) return
      // 用 instant 跟随，避免被 smooth 动画抢走 scrollTop
      suppressScrollListenerRef.current = true
      scrollEl.scrollTop = scrollEl.scrollHeight
      window.setTimeout(() => { suppressScrollListenerRef.current = false }, 0)
    })
    observer.observe(contentEl)

    return () => observer.disconnect()
  }, [])

  // —— 初始化：首次 mount 完成后，把视图放到底部（默认进入会话即看最新消息） ——
  React.useEffect(() => {
    const id = window.requestAnimationFrame(() => {
      scrollToBottom('auto')
    })
    return () => window.cancelAnimationFrame(id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const ctxValue = React.useMemo(
    () => ({ scrollRef, viewportEl, scrollToBottom }),
    [viewportEl, scrollToBottom],
  )

  return (
    <ConversationContext.Provider value={ctxValue}>
      <div ref={setViewportEl} className="relative flex-1 flex flex-col min-h-0">
        <div
          className={cn('flex-1 overflow-y-auto relative', className)}
          ref={scrollRef}
          onScroll={handleScroll}
        >
          {/* 用 contentRef 包裹真实内容，让 ResizeObserver 能监听内容高度变化 */}
          <div ref={contentRef}>
            {children}
          </div>
          {showScrollButton && (
            <div className="sticky bottom-3 flex justify-center pointer-events-none z-10">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                className="pointer-events-auto rounded-full shadow-lg gap-1 h-7 px-3 text-xs"
                onClick={() => scrollToBottom(resize === 'smooth' ? 'smooth' : 'auto')}
              >
                <ChevronDown className="size-3.5" />
                回到底部
              </Button>
            </div>
          )}
        </div>
      </div>
    </ConversationContext.Provider>
  )
}

// ===== ConversationContent 消息列表容器 =====

export function ConversationContent({ children }: { children: React.ReactNode }): React.ReactElement {
  return <div className="flex flex-col gap-1 pb-4">{children}</div>
}

// ===== ConversationScrollButton =====

export function ConversationScrollButton(): React.ReactElement | null {
  // Scroll button is now integrated into Conversation container
  return null
}
