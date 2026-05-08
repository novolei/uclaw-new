/**
 * Conversation — 对话容器组件
 *
 * 提供对话消息列表的滚动容器、自动滚动到底部按钮。
 * 从 Proma 迁移。
 */

import * as React from 'react'
import { ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'

// ===== ConversationContext — 向子组件（minimap、ScrollTopLoader）暴露 scrollRef =====

interface ConversationContextValue {
  scrollRef: React.RefObject<HTMLDivElement | null>
  /** 外壳容器（不参与滚动），供 minimap 等浮层 portal 进入 */
  viewportEl: HTMLDivElement | null
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

export function Conversation({ resize, className, children }: ConversationProps): React.ReactElement {
  const scrollRef = React.useRef<HTMLDivElement>(null)
  const [viewportEl, setViewportEl] = React.useState<HTMLDivElement | null>(null)
  const [showScrollButton, setShowScrollButton] = React.useState(false)

  const handleScroll = React.useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    const { scrollTop, scrollHeight, clientHeight } = el
    setShowScrollButton(scrollHeight - scrollTop - clientHeight > 100)
  }, [])

  const scrollToBottom = React.useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    el.scrollTo({
      top: el.scrollHeight,
      behavior: resize === 'smooth' ? 'smooth' : 'auto',
    })
  }, [resize])

  const ctxValue = React.useMemo(() => ({ scrollRef, viewportEl }), [viewportEl])

  return (
    <ConversationContext.Provider value={ctxValue}>
      <div ref={setViewportEl} className="relative flex-1 flex flex-col min-h-0">
        <div
          className={cn('flex-1 overflow-y-auto relative', className)}
          ref={scrollRef}
          onScroll={handleScroll}
        >
          {children}
          {showScrollButton && (
            <div className="sticky bottom-3 flex justify-center pointer-events-none z-10">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                className="pointer-events-auto rounded-full shadow-lg gap-1 h-7 px-3 text-xs"
                onClick={scrollToBottom}
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
