import React from 'react'
import { cn } from '@/lib/utils'
import { FragmentCard, SUBTYPE_COLORS } from './FragmentCard'
import { memoryGraphListFragments } from '@/lib/tauri-bridge'
import type { FragmentItem } from '@/lib/tauri-bridge'
import { getShortcutForPlatform } from '@/lib/shortcut-defaults'

const FILTER_TAGS = [
  { key: null, label: '全部' },
  { key: 'daily', label: '日常' },
  { key: 'credential', label: '凭证' },
  { key: 'location', label: '位置' },
  { key: 'reminder', label: '提醒' },
  { key: 'inspiration', label: '灵感' },
  { key: 'bookmark', label: '书签' },
] as const

interface FragmentCardViewProps {
  spaceId?: string
  onSelectNode?: (nodeId: string) => void
}

export function FragmentCardView({ spaceId, onSelectNode }: FragmentCardViewProps) {
  const [activeTag, setActiveTag] = React.useState<string | null>(null)
  const [fragments, setFragments] = React.useState<FragmentItem[]>([])
  const [loading, setLoading] = React.useState(true)
  const [hasMore, setHasMore] = React.useState(true)
  
  const PAGE_SIZE = 50

  const loadFragments = React.useCallback(async (tag: string | null, offset = 0) => {
    try {
      setLoading(true)
      const result = await memoryGraphListFragments({
        spaceId: spaceId || undefined,
        tag: tag || undefined,
        limit: PAGE_SIZE,
        offset,
      })
      if (offset === 0) {
        setFragments(result)
      } else {
        setFragments(prev => [...prev, ...result])
      }
      setHasMore(result.length >= PAGE_SIZE)
    } catch (e) {
      console.error('Failed to load fragments:', e)
    } finally {
      setLoading(false)
    }
  }, [spaceId])

  React.useEffect(() => {
    loadFragments(activeTag)
  }, [activeTag, loadFragments])

  const loadMore = () => {
    if (!loading && hasMore) {
      loadFragments(activeTag, fragments.length)
    }
  }

  return (
    <div className="flex flex-col h-full gap-3 p-3">
      {/* 标签过滤栏 */}
      <div className="flex gap-1.5 flex-wrap shrink-0">
        {FILTER_TAGS.map(({ key, label }) => (
          <button
            key={label}
            type="button"
            onClick={() => setActiveTag(key)}
            className={cn(
              'px-2.5 py-1 rounded-full text-[12px] font-medium transition-colors',
              'border min-h-[28px]',
              activeTag === key
                ? 'bg-foreground text-background border-foreground'
                : 'bg-muted/50 text-muted-foreground border-border/60 hover:bg-muted',
            )}
          >
            {label}
          </button>
        ))}
      </div>

      {/* 碎片列表 */}
      <div className="flex-1 overflow-y-auto space-y-2">
        {fragments.map(fragment => (
          <FragmentCard
            key={fragment.id}
            fragment={fragment}
            onClick={() => onSelectNode?.(fragment.id)}
          />
        ))}
        
        {/* 加载更多 */}
        {hasMore && !loading && (
          <button
            type="button"
            onClick={loadMore}
            className="w-full py-2 text-[12px] text-muted-foreground hover:text-foreground"
          >
            加载更多...
          </button>
        )}
        
        {loading && (
          <div className="py-4 text-center text-[12px] text-muted-foreground">加载中...</div>
        )}
        
        {/* 空状态 */}
        {!loading && fragments.length === 0 && (
          <div className="py-8 text-center space-y-2">
            <p className="text-[14px] text-muted-foreground">暂无记忆碎片</p>
            <p className="text-[12px] text-muted-foreground/60">
              使用 {getShortcutForPlatform('quick-capture') || 'Cmd+Shift+.'} 快速记录，或 {getShortcutForPlatform('quick-memory-voice') || 'Cmd+Shift+M'} 语音记录
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
