/**
 * ScrollMinimap — 对话滚动缩略图
 *
 * 在对话右侧显示消息缩略图，支持点击快速定位。
 * 从 Proma 迁移。
 */

import * as React from 'react'
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
  /** 当前可视区域内的消息 ID 集合 */
  visibleIds?: Set<string>
  /** 点击消息定位回调 */
  onItemClick?: (id: string) => void
  className?: string
}

/** 角色对应的颜色指示 */
const ROLE_COLORS: Record<string, string> = {
  user: 'bg-primary/60',
  assistant: 'bg-emerald-500/60',
  status: 'bg-yellow-500/60',
}

export function ScrollMinimap({
  items,
  visibleIds,
  onItemClick,
  className,
}: ScrollMinimapProps): React.ReactElement | null {
  if (items.length < 3) return null

  return (
    <div
      className={cn(
        'absolute right-1 top-0 bottom-0 w-[3px] z-10 opacity-0 hover:opacity-100 transition-opacity group',
        className,
      )}
    >
      <div className="relative h-full py-2">
        {items.map((item, index) => {
          const isVisible = visibleIds?.has(item.id) ?? false
          const topPercent = (index / items.length) * 100

          return (
            <button
              key={item.id}
              type="button"
              className={cn(
                'absolute left-0 w-full rounded-full transition-all cursor-pointer',
                ROLE_COLORS[item.role] ?? 'bg-muted-foreground/30',
                isVisible ? 'opacity-100 h-[6px]' : 'opacity-60 h-[3px]',
              )}
              style={{ top: `${topPercent}%` }}
              onClick={() => onItemClick?.(item.id)}
              title={item.preview.slice(0, 60)}
            />
          )
        })}
      </div>
    </div>
  )
}
