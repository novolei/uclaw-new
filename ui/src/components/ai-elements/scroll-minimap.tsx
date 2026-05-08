/**
 * ScrollMinimap — 消息导航迷你地图 + 滚动进度条
 *
 * 在消息区域右侧显示：
 * 1. 短横杠代表每条消息的位置（迷你地图），悬浮时弹出消息预览列表
 * 2. 可拖拽的滚动进度条，提供丝滑的滚动体验
 * 必须放在 StickToBottom（Conversation）内部使用。
 *
 * 1:1 移植自 Proma：apps/electron/src/renderer/components/ai-elements/scroll-minimap.tsx
 */

import * as React from 'react'
import { createPortal } from 'react-dom'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { AlertTriangle, Search } from 'lucide-react'
import { useConversationContext } from '@/components/ai-elements/conversation'
import { Input } from '@/components/ui/input'
import { UserAvatar } from '@/components/chat/UserAvatar'
import { getModelLogo } from '@/lib/model-logo'
import { cn } from '@/lib/utils'

export interface MinimapItem {
  id: string
  role: 'user' | 'assistant' | 'status'
  preview: string
  avatar?: string | null
  model?: string
}

interface ScrollMinimapProps {
  items: MinimapItem[]
  /** 兼容旧调用：保留以避免类型破坏（实际由内部 scroll 容器自动计算） */
  visibleIds?: Set<string>
  /** 兼容旧调用：保留以避免类型破坏（内部直接 scroll 到目标） */
  onItemClick?: (id: string) => void
  className?: string
}

/** 最少消息数才显示迷你地图 */
const MIN_ITEMS = 1
/** 迷你地图最多渲染的横杠数 */
const MAX_BARS = 20

// ── Markdown 预览配置（轻量级，禁用重量级渲染） ──

const PREVIEW_REMARK_PLUGINS = [remarkGfm]

const PREVIEW_MD_COMPONENTS = {
  pre: ({ children }: { children?: React.ReactNode }) => <pre className="text-[11px] opacity-70 truncate">{children}</pre>,
  code: ({ children }: { children?: React.ReactNode }) => <code className="text-[11px] bg-muted/50 px-0.5 rounded">{children}</code>,
  img: () => null as unknown as React.ReactElement,
  a: ({ children }: { children?: React.ReactNode }) => <span>{children}</span>,
} as const

// ── 辅助函数 ──

/** 计算 node 相对于 container 的实际顶部偏移（递归累积 offsetTop） */
function getOffsetTopRelativeTo(node: HTMLElement, container: HTMLElement): number {
  let top = 0
  let el: HTMLElement | null = node
  while (el && el !== container) {
    top += el.offsetTop
    el = el.offsetParent as HTMLElement | null
  }
  return top
}

/** 转义正则特殊字符 */
function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

// ── 主组件 ──

export function ScrollMinimap({ items }: ScrollMinimapProps): React.ReactElement | null {
  const ctx = useConversationContext()
  if (!ctx || !ctx.viewportEl) return null
  return createPortal(
    <ScrollMinimapInner items={items} scrollRef={ctx.scrollRef} />,
    ctx.viewportEl,
  )
}

interface InnerProps {
  items: MinimapItem[]
  scrollRef: React.RefObject<HTMLDivElement | null>
}

function ScrollMinimapInner({ items, scrollRef }: InnerProps): React.ReactElement | null {
  const [hovered, setHovered] = React.useState(false)
  const [isLeaving, setIsLeaving] = React.useState(false)
  const [visibleIds, setVisibleIds] = React.useState<Set<string>>(new Set())
  const [canScroll, setCanScroll] = React.useState(false)
  const [searchQuery, setSearchQuery] = React.useState('')
  const [isDragging, setIsDragging] = React.useState(false)
  const [scrollMetrics, setScrollMetrics] = React.useState({ scrollTop: 0, scrollHeight: 1, clientHeight: 1 })
  const closeTimerRef = React.useRef<ReturnType<typeof setTimeout>>()
  const fadeTimerRef = React.useRef<ReturnType<typeof setTimeout>>()
  const searchInputRef = React.useRef<HTMLInputElement>(null)
  const trackRef = React.useRef<HTMLDivElement>(null)
  const listRef = React.useRef<HTMLDivElement>(null)

  React.useEffect(() => {
    return () => {
      if (closeTimerRef.current) clearTimeout(closeTimerRef.current)
      if (fadeTimerRef.current) clearTimeout(fadeTimerRef.current)
    }
  }, [])

  // 可见消息 + 滚动指标追踪
  React.useEffect(() => {
    const el = scrollRef.current
    if (!el) return

    const update = (): void => {
      const { scrollTop, scrollHeight, clientHeight } = el
      setCanScroll(scrollHeight > clientHeight + 10)
      setScrollMetrics({ scrollTop, scrollHeight, clientHeight })
      if (scrollHeight <= 0) return

      const nodes = el.querySelectorAll<HTMLElement>('[data-message-id]')
      const ids = new Set<string>()
      for (const node of nodes) {
        const top = getOffsetTopRelativeTo(node, el)
        const bottom = top + node.offsetHeight
        if (bottom > scrollTop && top < scrollTop + clientHeight) {
          const id = node.getAttribute('data-message-id')
          if (id) ids.add(id)
        }
      }
      setVisibleIds(ids)
    }

    update()
    el.addEventListener('scroll', update, { passive: true })
    const observer = new ResizeObserver(update)
    observer.observe(el)

    return () => {
      el.removeEventListener('scroll', update)
      observer.disconnect()
    }
  }, [scrollRef, items])

  // 面板打开时自动聚焦搜索框
  React.useEffect(() => {
    if (hovered && searchInputRef.current) {
      const timer = setTimeout(() => searchInputRef.current?.focus(), 80)
      return () => clearTimeout(timer)
    }
  }, [hovered])

  // 面板关闭时清空搜索
  React.useEffect(() => {
    if (!hovered) setSearchQuery('')
  }, [hovered])

  // 面板打开后（且无搜索时），滚动消息列表到当前主区可见消息处
  React.useEffect(() => {
    if (!hovered) return
    if (searchQuery.trim()) return
    if (visibleIds.size === 0) return

    // 等 DOM 完成 mount 动画再 scroll，避免初始 transform 干扰
    const raf = requestAnimationFrame(() => {
      const list = listRef.current
      if (!list) return
      const firstVisible = list.querySelector<HTMLElement>('[data-visible="true"]')
      if (!firstVisible) return
      const listRect = list.getBoundingClientRect()
      const itemRect = firstVisible.getBoundingClientRect()
      const target = list.scrollTop + (itemRect.top - listRect.top) - listRect.height * 0.3
      list.scrollTo({ top: Math.max(0, target), behavior: 'auto' })
    })
    return () => cancelAnimationFrame(raf)
  }, [hovered, searchQuery, visibleIds])

  const handleMouseEnter = (): void => {
    if (closeTimerRef.current) clearTimeout(closeTimerRef.current)
    if (fadeTimerRef.current) clearTimeout(fadeTimerRef.current)
    setIsLeaving(false)
    setHovered(true)
  }

  const handleMouseLeave = (): void => {
    closeTimerRef.current = setTimeout(() => {
      setIsLeaving(true)
      fadeTimerRef.current = setTimeout(() => {
        setHovered(false)
        setIsLeaving(false)
      }, 80)
    }, 40)
  }

  const scrollToMessage = React.useCallback((id: string) => {
    const el = scrollRef.current
    if (!el) return
    const target = el.querySelector<HTMLElement>(`[data-message-id="${id}"]`)
    if (!target) return

    const offsetTop = getOffsetTopRelativeTo(target, el)
    const targetHeight = target.offsetHeight
    const viewportHeight = el.clientHeight
    const scrollTarget = targetHeight < viewportHeight
      ? offsetTop - (viewportHeight - targetHeight) / 2
      : offsetTop - 32
    el.scrollTo({ top: Math.max(0, scrollTarget), behavior: 'smooth' })

    setHovered(false)
  }, [scrollRef])

  const filteredItems = React.useMemo(() => {
    if (!searchQuery.trim()) return items
    const q = searchQuery.toLowerCase()
    return items.filter((item) => item.preview.toLowerCase().includes(q))
  }, [items, searchQuery])

  const handleThumbMouseDown = React.useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()

    const el = scrollRef.current
    const track = trackRef.current
    if (!el || !track) return

    setIsDragging(true)
    const startY = e.clientY
    const startScrollTop = el.scrollTop
    const trackHeight = track.clientHeight
    const { scrollHeight, clientHeight } = el
    const scrollRange = scrollHeight - clientHeight
    const thumbHeight = Math.max(trackHeight * 0.1, (clientHeight / scrollHeight) * trackHeight)
    const scrollableTrack = trackHeight - thumbHeight

    const onMouseMove = (ev: MouseEvent): void => {
      ev.preventDefault()
      const delta = ev.clientY - startY
      const scrollDelta = scrollableTrack > 0 ? (delta / scrollableTrack) * scrollRange : 0
      el.scrollTop = Math.max(0, Math.min(scrollRange, startScrollTop + scrollDelta))
    }

    const onMouseUp = (): void => {
      setIsDragging(false)
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
      document.body.style.userSelect = ''
      document.body.style.cursor = ''
    }

    document.body.style.userSelect = 'none'
    document.body.style.cursor = 'grabbing'
    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
  }, [scrollRef])

  const handleTrackMouseDown = React.useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return

    const track = trackRef.current
    const el = scrollRef.current
    if (!track || !el) return

    const rect = track.getBoundingClientRect()
    const clickRatio = (e.clientY - rect.top) / rect.height
    const { scrollHeight, clientHeight } = el
    const targetTop = clickRatio * (scrollHeight - clientHeight)
    el.scrollTo({ top: Math.max(0, targetTop), behavior: 'smooth' })
  }, [scrollRef])

  // 仅当无消息时隐藏；不再要求容器可滚动 — 即便消息很少也保留导航入口
  if (items.length < MIN_ITEMS) return null

  const barCount = Math.min(items.length, MAX_BARS)

  const { scrollTop, scrollHeight, clientHeight } = scrollMetrics
  const scrollRange = scrollHeight - clientHeight
  const thumbRatio = scrollHeight > 0 ? Math.min(clientHeight / scrollHeight, 1) : 1
  const thumbHeightPct = Math.max(10, thumbRatio * 100)
  const thumbTopPct = scrollRange > 0 ? (scrollTop / scrollRange) * (100 - thumbHeightPct) : 0

  return (
    <div
      data-scroll-minimap
      className="absolute right-5 top-0 bottom-0 z-50 flex pointer-events-none"
    >
      {/* 迷你地图悬停区域（面板 + 横杠） */}
      <div className="flex items-start h-full">
        {/* 展开面板 */}
        {hovered && (
          <div
            className={cn(
              'mr-2 w-[300px] rounded-xl border border-border/40 bg-popover/95 backdrop-blur-xl',
              'shadow-[0_8px_32px_-8px_rgba(0,0,0,0.18)] dark:shadow-[0_8px_32px_-4px_rgba(0,0,0,0.5)]',
              'origin-top-right flex flex-col overflow-hidden pointer-events-auto',
              isLeaving
                ? 'animate-out fade-out-0 zoom-out-95 duration-100'
                : 'animate-in fade-in-0 zoom-in-95 slide-in-from-right-1 duration-150',
            )}
            style={{ maxHeight: 'min(440px, 60vh)', marginTop: 8 }}
            onMouseEnter={handleMouseEnter}
            onMouseLeave={handleMouseLeave}
          >
            {/* 搜索框（同时承担标题作用） */}
            <div className="px-2.5 pt-2.5 pb-2 shrink-0">
              <div className="relative">
                <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground/60" />
                <Input
                  ref={searchInputRef}
                  placeholder="搜索消息"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  onFocus={() => {
                    if (closeTimerRef.current) clearTimeout(closeTimerRef.current)
                    if (fadeTimerRef.current) clearTimeout(fadeTimerRef.current)
                    setIsLeaving(false)
                  }}
                  className="h-8 text-xs pl-8 bg-muted/40 border-0 focus-visible:ring-1 focus-visible:ring-primary/30"
                />
                <span className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] tabular-nums text-muted-foreground/60 select-none pointer-events-none">
                  {visibleIds.size}/{items.length}
                </span>
              </div>
            </div>

            {/* 消息列表 */}
            <div ref={listRef} className="overflow-y-auto flex-1 px-1.5 pb-1.5 space-y-px scrollbar-thin">
              {filteredItems.length === 0 ? (
                <div className="py-8 text-center text-xs text-muted-foreground/70">
                  未找到匹配消息
                </div>
              ) : (
                filteredItems.map((item) => {
                  const isVisible = visibleIds.has(item.id)
                  return (
                    <button
                      key={item.id}
                      type="button"
                      data-visible={isVisible || undefined}
                      className={cn(
                        'group relative flex items-start gap-2.5 w-full rounded-lg px-2 py-1.5 text-left',
                        'transition-[background-color,transform,box-shadow] duration-150 ease-out',
                        'hover:bg-accent hover:translate-x-[2px] hover:shadow-sm',
                        'active:translate-x-0 active:scale-[0.99]',
                        isVisible && 'bg-accent/40',
                      )}
                      onClick={() => scrollToMessage(item.id)}
                    >
                      {/* 当前可见消息的左侧高亮条 */}
                      {isVisible && (
                        <span className="absolute left-0 top-1.5 bottom-1.5 w-[2px] rounded-full bg-primary" />
                      )}
                      <ItemIcon item={item} />
                      <div className="flex-1 min-w-0">
                        <HighlightedPreview text={item.preview} query={searchQuery} />
                      </div>
                    </button>
                  )
                })
              )}
            </div>
          </div>
        )}

        {/* 迷你地图条（极简悬浮 dock） */}
        <div
          className={cn(
            'group relative mt-2 flex-shrink-0 pointer-events-auto flex flex-col items-center',
            'rounded-full px-0.5 py-1 transition-colors duration-200',
            hovered
              ? 'bg-foreground/[0.06] dark:bg-foreground/[0.08]'
              : 'hover:bg-foreground/[0.04]',
          )}
          onMouseEnter={handleMouseEnter}
          onMouseLeave={handleMouseLeave}
          title="消息导航"
        >
          {/* 横杠容器 */}
          <div className="relative" style={{ width: 14, height: Math.max(barCount * 5, 18) }}>
            {Array.from({ length: barCount }, (_, i) => {
              const start = Math.floor((i * items.length) / barCount)
              const end = Math.floor(((i + 1) * items.length) / barCount)
              const group = items.slice(start, end)
              const isVisible = group.some((it) => visibleIds.has(it.id))
              const hasUser = group.some((it) => it.role === 'user')
              const top = ((i + 0.5) / barCount) * 100
              return (
                <div
                  key={i}
                  className={cn(
                    'absolute left-1/2 rounded-full transition-all duration-200 ease-out',
                    isVisible
                      ? 'bg-primary h-[2px] w-[14px]'
                      : hasUser
                        ? 'bg-foreground/45 h-[1.5px] w-[9px] group-hover:w-[12px] group-hover:bg-foreground/70'
                        : 'bg-foreground/20 h-[1.5px] w-[3px] group-hover:w-[6px] group-hover:bg-foreground/55',
                  )}
                  style={{ top: `${top}%`, transform: 'translate(-50%, -50%)' }}
                />
              )
            })}
          </div>
        </div>
      </div>

      {/* 自绘进度条已移除 — 改用原生 scrollbar；minimap 仅保留消息条入口 */}
    </div>
  )
}

// ── 子组件 ──

function ItemIcon({ item }: { item: MinimapItem }): React.ReactElement {
  if (item.role === 'user' && item.avatar) {
    return <UserAvatar avatar={item.avatar} size={16} className="mt-0.5" />
  }
  if (item.role === 'assistant' && item.model) {
    return (
      <img
        src={getModelLogo(item.model)}
        alt=""
        className="size-4 shrink-0 mt-0.5 rounded-[20%] object-cover"
      />
    )
  }
  if (item.role === 'status') {
    return <AlertTriangle className="size-4 shrink-0 mt-0.5 text-destructive" />
  }
  return <div className="size-4 shrink-0 mt-0.5 rounded-[20%] bg-muted" />
}

/** Markdown 预览（无搜索时）或 纯文本+高亮（搜索时） */
function HighlightedPreview({ text, query }: { text: string; query: string }): React.ReactElement {
  if (!text) {
    return <span className="text-xs opacity-40">(空消息)</span>
  }

  if (query.trim()) {
    const escaped = escapeRegExp(query)
    const parts = text.split(new RegExp(`(${escaped})`, 'gi'))
    return (
      <span className="text-xs text-popover-foreground/80 line-clamp-3">
        {parts.map((part, i) =>
          part.toLowerCase() === query.toLowerCase()
            ? <mark key={i} className="bg-primary/20 text-primary rounded-sm px-0.5">{part}</mark>
            : part,
        )}
      </span>
    )
  }

  return (
    <div className="prose prose-sm dark:prose-invert max-w-none text-xs text-popover-foreground/80 prose-p:my-0 prose-headings:my-0.5 prose-headings:text-xs prose-li:my-0 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 line-clamp-3 overflow-hidden">
      <Markdown remarkPlugins={PREVIEW_REMARK_PLUGINS} components={PREVIEW_MD_COMPONENTS}>
        {text}
      </Markdown>
    </div>
  )
}
