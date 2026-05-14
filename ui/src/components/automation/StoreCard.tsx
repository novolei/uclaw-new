import * as React from 'react'
import { Download } from 'lucide-react'
import { cn } from '@/lib/utils'
import { AppTypeBadge } from './AppTypeBadge'
import type { MarketplaceItem } from '@/lib/tauri-bridge'

const MAX_VISIBLE_TAGS = 3

interface Props {
  item: MarketplaceItem
  hasUpdate?: boolean
  isInstalled?: boolean
  onClick: (slug: string) => void
}

export function StoreCard({ item, hasUpdate, isInstalled, onClick }: Props): React.ReactElement {
  const displayName = item.i18nName ?? item.name
  const displayDesc = item.i18nDescription ?? item.description
  const visibleTags = item.tags.slice(0, MAX_VISIBLE_TAGS)
  const hiddenTagCount = item.tags.length - visibleTags.length

  return (
    <button
      type="button"
      onClick={() => onClick(item.slug)}
      className={cn(
        'w-full text-left p-4',
        'rounded-xl border border-border/50 bg-card',
        'hover:border-primary/40 hover:bg-secondary/50',
        'transition-colors',
      )}
    >
      {/* Row 1: icon + name + type + version */}
      <div className="flex items-start justify-between gap-2 mb-1">
        <div className="flex items-center gap-2 min-w-0 flex-1">
          <div className="w-7 h-7 rounded-md bg-primary/10 flex items-center justify-center text-[12px] shrink-0">
            {item.icon ?? '🤖'}
          </div>
          <span className="text-[13px] font-medium truncate">{displayName}</span>
          <AppTypeBadge type={item.appType} tooltipDirection="up" />
        </div>
        <span className="text-[10px] text-muted-foreground tabular-nums shrink-0 mt-0.5">
          v{item.version}
        </span>
      </div>

      {/* Row 2: author + status indicators */}
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span>by {item.author}</span>
        {isInstalled && !hasUpdate && (
          <span className="px-1.5 py-[1px] rounded-md bg-success-bg text-success text-[10px] font-medium">
            已安装
          </span>
        )}
        {hasUpdate && (
          <span className="px-1.5 py-[1px] rounded-md bg-warning-bg text-warning text-[10px] font-medium">
            有更新
          </span>
        )}
      </div>

      {/* Description */}
      <p className="text-[12px] text-muted-foreground mt-2 line-clamp-2 min-h-[2.5em]">
        {displayDesc}
      </p>

      {/* Tags */}
      {visibleTags.length > 0 && (
        <div className="flex flex-wrap gap-1 mt-3">
          {visibleTags.map((tag) => (
            <span
              key={tag}
              className="text-[10px] px-2 py-0.5 rounded-full bg-secondary text-muted-foreground"
            >
              {tag}
            </span>
          ))}
          {hiddenTagCount > 0 && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-secondary text-muted-foreground">
              +{hiddenTagCount}
            </span>
          )}
        </div>
      )}

      {/* CTA hint */}
      <div className="flex items-center gap-1 mt-3 text-[10px] text-muted-foreground">
        <Download size={10} />
        <span>查看详情</span>
      </div>
    </button>
  )
}
