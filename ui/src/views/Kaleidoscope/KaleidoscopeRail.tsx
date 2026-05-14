/**
 * KaleidoscopeRail — 万花筒的 120px 窄轨导航（Arc Library 风格）。
 *
 * 结构（从上至下）：
 *  - 36px 红绿灯让位区
 *  - 资产组：数字人 / 应用商店 / 我的应用 / 产出
 *  - 36px hairline 分组分隔
 *  - 能力组：技能 / 集成 / 记忆
 *  - 底部两段（结构/高度对齐 chat 窗口 LeftSidebar 底部）：
 *    · 返回 + ✦ 装饰行 —— border-t 自带分割线，px-3 py-2（= chat switcher 行规格）
 *    · User / Settings 行 —— px-3 pb-3 pt-2（= chat User 行规格），无独立分割线
 *  - 返回按钮 size-8 主色处理，与 chat 窗口 KaleidoscopeIcon 入口成对、同尺寸
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

      {/* ── 底部两段（结构/高度对齐 chat 窗口 LeftSidebar 底部）── */}

      {/* ① 返回 + ✦ 装饰行 —— border-t 自带单条分割线，px-3 py-2（= chat switcher 行规格）。
          返回按钮 size-8 主色处理，与 chat 窗口的 KaleidoscopeIcon 入口成对、同尺寸。 */}
      <div className="shrink-0 px-3 py-2 border-t border-border/40 flex items-center justify-between">
        <button
          type="button"
          onClick={() => setTopLevelView('workspace')}
          aria-label="返回主窗口"
          className="titlebar-no-drag inline-flex items-center justify-center
                     size-8 rounded-md bg-primary/10 text-primary
                     hover:bg-primary/20 transition-colors shrink-0"
        >
          <ArrowLeft className="size-[18px]" />
        </button>
        {/* 装饰标识 —— 非交互 */}
        <Sparkles
          className="size-[18px] text-primary/35
                     drop-shadow-[0_0_6px_hsl(var(--primary)/0.35)]"
          aria-hidden
        />
      </div>

      {/* ② User / Settings 行 —— px-3 pb-3 pt-2（= chat User 行规格），无独立分割线。
          内容尺寸按 120px rail 宽度适配（avatar 24 / text-xs），高度节奏与 chat 一致。 */}
      <div className="shrink-0 px-3 pb-3 pt-2">
        <button
          type="button"
          aria-label="设置"
          onClick={() => setSettingsOpen(true)}
          className="titlebar-no-drag w-full flex items-center gap-2 px-2.5 py-2
                     rounded-[10px] text-foreground/70 hover:bg-foreground/[0.04]
                     hover:text-foreground transition-colors"
        >
          <UserAvatar avatar={userProfile.avatar} size={24} />
          <span className="flex-1 min-w-0 text-xs truncate text-left">
            {userProfile.userName}
          </span>
          <Settings className="size-4 shrink-0 text-foreground/40" />
        </button>
      </div>
    </div>
  )
}
