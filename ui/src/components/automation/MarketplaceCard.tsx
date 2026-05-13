import * as React from 'react'
import { Loader2, Download } from 'lucide-react'
import type { MarketplaceItem } from '@/lib/tauri-bridge'

const MAX_VISIBLE_TAGS = 3

interface Props {
  item: MarketplaceItem
  installing: boolean
  onInstall: (slug: string) => void
}

export function MarketplaceCard({ item, installing, onInstall }: Props): React.ReactElement {
  const displayName = item.i18nName ?? item.name
  const displayDesc = item.i18nDescription ?? item.description
  const visibleTags = item.tags.slice(0, MAX_VISIBLE_TAGS)
  const hiddenTagCount = item.tags.length - visibleTags.length

  return (
    <button
      onClick={() => onInstall(item.slug)}
      disabled={installing}
      className="w-full text-left p-4 rounded-xl border border-border hover:border-primary/40 hover:bg-secondary/50 transition-colors cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 min-w-0">
          {item.icon && (
            <span className="text-base flex-shrink-0" aria-hidden>
              {/* icon is a category string (e.g. "news", "social", "productivity"); Phase 3 maps to icons */}
              {item.icon}
            </span>
          )}
          <span className="text-sm font-medium text-foreground truncate">
            {displayName}
          </span>
        </div>
        <span className="text-xs text-muted-foreground flex-shrink-0">
          v{item.version}
        </span>
      </div>

      <p className="text-xs text-muted-foreground mt-1">
        by {item.author}
      </p>

      <p className="text-xs text-muted-foreground mt-2 line-clamp-2">
        {displayDesc}
      </p>

      {visibleTags.length > 0 && (
        <div className="flex flex-wrap gap-1 mt-3">
          {visibleTags.map((tag) => (
            <span
              key={tag}
              className="text-xs px-2 py-0.5 rounded-full bg-secondary text-muted-foreground"
            >
              {tag}
            </span>
          ))}
          {hiddenTagCount > 0 && (
            <span className="text-xs px-2 py-0.5 rounded-full bg-secondary text-muted-foreground">
              +{hiddenTagCount}
            </span>
          )}
        </div>
      )}

      <div className="flex items-center gap-2 mt-3 text-xs text-muted-foreground">
        {installing ? (
          <><Loader2 size={11} className="animate-spin" /><span>安装中…</span></>
        ) : (
          <><Download size={11} /><span>点击安装</span></>
        )}
      </div>
    </button>
  )
}
