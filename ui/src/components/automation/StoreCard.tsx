import * as React from 'react'
import { cn } from '@/lib/utils'
import { AppTypeBadge } from './AppTypeBadge'
import { CategoryIcon } from './category-icon'
import type { MarketplaceItem } from '@/lib/tauri-bridge'

const MAX_VISIBLE_TAGS = 2

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

  // Pick the icon key: prefer the spec's `icon` keyword (e.g. 'social'),
  // fall back to category. Both flow through CategoryIcon's known/hash map.
  const iconKey = item.icon ?? item.category

  return (
    <button
      type="button"
      onClick={() => onClick(item.slug)}
      className={cn(
        'group w-full text-left p-4',
        'rounded-xl border border-border/50 bg-card',
        'hover:border-primary/40 hover:bg-secondary/30 hover:shadow-sm',
        'transition-all duration-150',
        'flex flex-col min-w-0',
      )}
    >
      {/* Header: icon + title block on left, type badge on right */}
      <div className="flex items-start gap-3 mb-2 min-w-0">
        <div className="w-10 h-10 rounded-lg bg-primary/8 flex items-center justify-center shrink-0 group-hover:bg-primary/12 transition-colors">
          <CategoryIcon name={iconKey} size={16} className="text-primary/80" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 min-w-0">
            <span className="text-[13px] font-medium truncate flex-1 min-w-0">
              {displayName}
            </span>
            <span className="shrink-0">
              <AppTypeBadge type={item.appType} tooltipDirection="up" />
            </span>
          </div>
          <div className="flex items-center gap-1.5 mt-0.5 text-[11px] text-muted-foreground min-w-0">
            <span className="truncate min-w-0">by {item.author}</span>
            <span className="tabular-nums shrink-0">· v{item.version}</span>
          </div>
        </div>
      </div>

      {/* Status badges (only shown when applicable) */}
      {(isInstalled || hasUpdate) && (
        <div className="flex items-center gap-1 mb-2 min-w-0">
          {isInstalled && !hasUpdate && (
            <span className="shrink-0 px-1.5 py-[1px] rounded-md bg-success-bg text-success text-[10px] font-medium">
              已安装
            </span>
          )}
          {hasUpdate && (
            <span className="shrink-0 px-1.5 py-[1px] rounded-md bg-warning-bg text-warning text-[10px] font-medium">
              有更新
            </span>
          )}
        </div>
      )}

      {/* Description (fixed 2-line height to keep cards aligned) */}
      <p className="text-[12px] text-muted-foreground leading-relaxed line-clamp-2 min-h-[2.5em] mb-3">
        {displayDesc}
      </p>

      {/* Tags row — capped at 2 + overflow indicator to prevent wrapping at narrow widths */}
      {visibleTags.length > 0 && (
        <div className="flex items-center gap-1 mt-auto min-w-0 overflow-hidden">
          {visibleTags.map((tag) => (
            <span
              key={tag}
              className="shrink-0 text-[10px] px-1.5 py-0.5 rounded-md bg-muted text-muted-foreground"
            >
              {tag}
            </span>
          ))}
          {hiddenTagCount > 0 && (
            <span className="shrink-0 text-[10px] px-1.5 py-0.5 rounded-md bg-muted text-muted-foreground/70">
              +{hiddenTagCount}
            </span>
          )}
        </div>
      )}
    </button>
  )
}
