/**
 * KaleidoscopeRail — 万花筒的 120px 窄轨导航（Arc Library 风格）。
 *
 * 结构（从上至下）：
 *  - 36px 红绿灯让位区
 *  - 资产组：数字人 / 应用商店 / 我的应用 / 产出
 *  - 36px hairline 分组分隔
 *  - 能力组：技能 / 集成 / 记忆
 *  - 1px 分割线
 *  - 44px 行：← 返回主窗口（左）+ ✦ 装饰标识（右，不可交互）
 *  - 1px 分割线
 *  - 48px User / Settings 行（沿用 LeftSidebar 底部规格）
 *
 * 每个模块条目 = lucide 图标 + 中文标签竖排。选中态：资产组用 primary tint，
 * 能力组用 accent tint。全部走 theme token。
 */
import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import {
  ArrowLeft, Sparkles, Settings,
  Bot, Store, LayoutGrid, FileText, Zap, Plug, Brain,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import {
  kaleidoscopeModuleAtom,
  KALEIDOSCOPE_MODULES,
  type KaleidoscopeModuleId,
  type KaleidoscopeGroup,
} from '@/atoms/kaleidoscope'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import { userProfileAtom } from '@/atoms/user-profile'
import { UserAvatar } from '@/components/chat/UserAvatar'

const MODULE_ICON: Record<KaleidoscopeModuleId, LucideIcon> = {
  humans: Bot,
  store: Store,
  apps: LayoutGrid,
  artifacts: FileText,
  skills: Zap,
  integrations: Plug,
  memory: Brain,
}

const ASSET_MODULES = KALEIDOSCOPE_MODULES.filter((m) => m.group === 'asset')
const CAPABILITY_MODULES = KALEIDOSCOPE_MODULES.filter((m) => m.group === 'capability')

interface RailItemProps {
  id: KaleidoscopeModuleId
  label: string
  group: KaleidoscopeGroup
  active: boolean
  onSelect: (id: KaleidoscopeModuleId) => void
}

function RailItem({ id, label, group, active, onSelect }: RailItemProps): React.ReactElement {
  const Icon = MODULE_ICON[id]
  return (
    <button
      type="button"
      onClick={() => onSelect(id)}
      aria-current={active ? 'true' : undefined}
      className={cn(
        'titlebar-no-drag flex flex-col items-center gap-1.5 w-[88%] py-2 rounded-[10px]',
        'transition-colors',
        active
          ? group === 'asset'
            ? 'bg-primary/[0.18] border border-primary/35 text-foreground'
            : 'bg-accent/[0.18] border border-accent/35 text-foreground'
          : 'border border-transparent text-muted-foreground hover:bg-muted/30',
      )}
    >
      <Icon className={cn('size-[22px]', !active && 'opacity-70')} aria-hidden />
      <span className="text-[11px] font-semibold leading-none">{label}</span>
    </button>
  )
}

export function KaleidoscopeRail(): React.ReactElement {
  const [moduleId, setModuleId] = useAtom(kaleidoscopeModuleAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const userProfile = useAtomValue(userProfileAtom)

  return (
    <div className="w-[120px] h-full shrink-0 flex flex-col bg-background rounded-2xl shadow-xl overflow-hidden">
      {/* 红绿灯让位 */}
      <div className="h-9 shrink-0" />

      {/* 主导航 */}
      <div className="flex-1 min-h-0 overflow-y-auto flex flex-col items-center gap-[18px] pt-3">
        {ASSET_MODULES.map((m) => (
          <RailItem
            key={m.id}
            id={m.id}
            label={m.label}
            group={m.group}
            active={moduleId === m.id}
            onSelect={setModuleId}
          />
        ))}

        {/* 分组分隔 */}
        <div className="w-9 h-px bg-border my-0.5" />

        {CAPABILITY_MODULES.map((m) => (
          <RailItem
            key={m.id}
            id={m.id}
            label={m.label}
            group={m.group}
            active={moduleId === m.id}
            onSelect={setModuleId}
          />
        ))}
      </div>

      {/* ── 底部三段（沿用 chat 窗口结构） ── */}

      {/* 分割线 */}
      <div className="h-px bg-border mx-2.5 shrink-0" />

      {/* ① 返回 + 装饰行 (44px) */}
      <div className="h-11 shrink-0 px-3 flex items-center justify-between">
        <button
          type="button"
          onClick={() => setTopLevelView('workspace')}
          aria-label="返回主窗口"
          className="titlebar-no-drag inline-flex items-center justify-center
                     size-7 rounded-[7px] bg-primary/15 border border-primary/35
                     text-primary hover:bg-primary/25 transition-colors"
        >
          <ArrowLeft className="size-3.5" />
        </button>
        {/* 装饰标识 —— 非交互 */}
        <Sparkles
          className="size-[18px] text-primary/35
                     drop-shadow-[0_0_6px_hsl(var(--primary)/0.35)]"
          aria-hidden
        />
      </div>

      {/* 分割线 */}
      <div className="h-px bg-border mx-2.5 shrink-0" />

      {/* ② User / Settings 行 (48px) */}
      <div className="h-12 shrink-0 px-2 flex items-center">
        <button
          type="button"
          aria-label="设置"
          onClick={() => setSettingsOpen(true)}
          className="titlebar-no-drag w-full flex items-center gap-1.5 px-1.5 py-2
                     rounded-[10px] text-foreground/70 hover:bg-foreground/[0.04]
                     hover:text-foreground transition-colors"
        >
          <UserAvatar avatar={userProfile.avatar} size={22} />
          <span className="flex-1 min-w-0 text-[11px] truncate text-left">
            {userProfile.userName}
          </span>
          <Settings className="size-3.5 shrink-0 text-foreground/40" />
        </button>
      </div>
    </div>
  )
}
