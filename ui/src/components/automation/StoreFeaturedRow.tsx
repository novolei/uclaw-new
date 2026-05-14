import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Sparkles } from 'lucide-react'
import { cn } from '@/lib/utils'
import { AppTypeBadge } from './AppTypeBadge'
import {
  marketplaceItemsAtom,
  marketplaceSelectedSlugAtom,
  automationsSubviewAtom,
  marketplaceUpdatesAtom,
} from '@/atoms/marketplace'
import { humaneSpecsAtom } from '@/atoms/automation'

// Phase 3a hardcoded featured list — Phase 4 makes this remote-driven.
const FEATURED_SLUGS = [
  'ai-daily-news',
  'github-pr-reviewer',
  'weibo-hot-tracker',
  'wechat-article-monitor',
]

export function StoreFeaturedRow(): React.ReactElement | null {
  const items = useAtomValue(marketplaceItemsAtom)
  const updates = useAtomValue(marketplaceUpdatesAtom)
  const installedSpecs = useAtomValue(humaneSpecsAtom)
  const setSelectedSlug = useSetAtom(marketplaceSelectedSlugAtom)
  const setSubview = useSetAtom(automationsSubviewAtom)

  const featured = React.useMemo(
    () => FEATURED_SLUGS.map((slug) => items.find((i) => i.slug === slug)).filter((x): x is NonNullable<typeof x> => x !== undefined),
    [items],
  )

  const updateSlugs = React.useMemo(() => new Set(updates.map((u) => u.slug)), [updates])
  const installedSlugs = React.useMemo(() => {
    return new Set(
      installedSpecs
        .filter((s) => s.source === 'marketplace' && s.sourceRef)
        .map((s) => /^marketplace:\/\/[^/]+\/(.+)$/.exec(s.sourceRef ?? '')?.[1] ?? null)
        .filter((x): x is string => x !== null),
    )
  }, [installedSpecs])

  if (featured.length === 0) return null

  return (
    <div className="px-6 pt-4 pb-2">
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
        <Sparkles size={11} className="text-primary" />
        <span>今日推荐</span>
      </div>
      <div className="flex gap-3 overflow-x-auto pb-1 -mx-6 px-6">
        {featured.map((item) => (
          <button
            key={item.slug}
            type="button"
            onClick={() => {
              setSelectedSlug(item.slug)
              setSubview('store-detail')
            }}
            className={cn(
              'shrink-0 w-[320px] p-4',
              'rounded-xl border border-border/50 bg-card',
              'hover:border-primary/40 hover:bg-secondary/50',
              'transition-colors text-left',
            )}
          >
            <div className="flex items-center gap-2 mb-2">
              <div className="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center text-[18px]">
                {item.icon ?? '🤖'}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5">
                  <span className="text-[14px] font-semibold truncate">{item.i18nName ?? item.name}</span>
                  <AppTypeBadge type={item.appType} />
                </div>
                <div className="flex items-center gap-1.5 flex-wrap">
                  <span className="text-[11px] text-muted-foreground">by {item.author}</span>
                  {installedSlugs.has(item.slug) && !updateSlugs.has(item.slug) && (
                    <span className="px-1.5 py-[1px] rounded-md bg-success-bg text-success text-[10px] font-medium">
                      已安装
                    </span>
                  )}
                  {updateSlugs.has(item.slug) && (
                    <span className="px-1.5 py-[1px] rounded-md bg-warning-bg text-warning text-[10px] font-medium">
                      有更新
                    </span>
                  )}
                </div>
              </div>
            </div>
            <p className="text-[12px] text-muted-foreground line-clamp-2">
              {item.i18nDescription ?? item.description}
            </p>
          </button>
        ))}
      </div>
    </div>
  )
}
