/**
 * ModuleHeader — 万花筒 7 个模块主区共用的顶部 header。
 *
 * 结构：分组标签（资产 / 能力，小号大写灰字）+ 模块标题（22px/600）+
 * 可选状态副标题（12px muted）+ 右上角可选操作区（搜索 / CTA）。
 *
 * 全部走 theme token，不写死颜色。
 */
import * as React from 'react'
import type { KaleidoscopeGroup } from '@/atoms/kaleidoscope'

const GROUP_LABEL: Record<KaleidoscopeGroup, string> = {
  asset: '资产',
  capability: '能力',
}

export interface ModuleHeaderProps {
  group: KaleidoscopeGroup
  title: string
  subtitle?: string
  /** 右上角操作区（搜索框 / 主 CTA）。 */
  actions?: React.ReactNode
}

export function ModuleHeader({
  group,
  title,
  subtitle,
  actions,
}: ModuleHeaderProps): React.ReactElement {
  return (
    // titlebar-drag-region directly on the header row: -webkit-app-region
    // does NOT cascade from KaleidoscopeShell's wrapper through the content
    // card, so the drag class must sit on the actual header element. The
    // title block becomes window-drag surface; `actions` opts back out.
    <div className="titlebar-drag-region flex items-start justify-between gap-4 px-8 pt-7 pb-4">
      <div className="min-w-0">
        <div className="text-[11px] uppercase tracking-[0.5px] text-muted-foreground">
          {GROUP_LABEL[group]}
        </div>
        <h1 className="mt-0.5 text-[22px] font-semibold text-foreground truncate">
          {title}
        </h1>
        {subtitle && (
          <div className="mt-0.5 text-[12px] text-muted-foreground truncate">
            {subtitle}
          </div>
        )}
      </div>
      {/* titlebar-no-drag: the action buttons must stay clickable while the
          header's title area (left) stays window-drag surface — see
          KaleidoscopeShell. */}
      {actions && <div className="titlebar-no-drag flex items-center gap-2 shrink-0">{actions}</div>}
    </div>
  )
}
