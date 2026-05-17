/**
 * SafetyModeSelector - 安全模式选择器
 *
 * 提供三种安全模式切换：
 * - Ask（蓝色）— 每次工具调用都询问
 * - Supervised（金色）— 高风险时询问
 * - YOLO（绿色）— 全部自动执行
 *
 * 使用 DropdownMenu 切换并调用 tauri-bridge setSafetyMode。
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { Shield, ShieldAlert, ShieldCheck, ShieldOff, ChevronDown } from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { getSafetyPolicy, setSafetyMode } from '@/lib/tauri-bridge'
import { silencedPlanModeSessionsAtom } from '@/atoms/plan-mode-suggest-atoms'
import type { SafetyMode } from '@/lib/types'

interface ModeConfig {
  label: string
  description: string
  icon: React.ElementType
  color: string
  bgClass: string
}

const MODES: Record<SafetyMode, ModeConfig> = {
  ask: {
    label: 'Ask',
    description: '每次工具调用都需要确认',
    icon: ShieldAlert,
    color: 'var(--safety-ask, #3b82f6)',
    bgClass: 'hover:bg-blue-500/10',
  },
  supervised: {
    label: 'Supervised',
    description: '高风险操作时询问',
    icon: ShieldCheck,
    color: 'var(--safety-supervised, #c9963a)',
    bgClass: 'hover:bg-amber-500/10',
  },
  yolo: {
    label: 'YOLO',
    description: '全部自动执行，无需确认',
    icon: ShieldOff,
    color: 'var(--safety-yolo, #22c55e)',
    bgClass: 'hover:bg-green-500/10',
  },
}

interface SafetyModeSelectorProps {
  className?: string
}

export function SafetyModeSelector({ className }: SafetyModeSelectorProps): React.ReactElement {
  const [mode, setMode] = React.useState<SafetyMode>('supervised')
  const [loading, setLoading] = React.useState(false)
  const setSilenced = useSetAtom(silencedPlanModeSessionsAtom)

  // 初始化：获取当前安全策略
  React.useEffect(() => {
    getSafetyPolicy()
      .then((policy) => {
        const m = policy.globalMode as SafetyMode
        if (m && MODES[m]) setMode(m)
      })
      .catch((err) => console.error('[SafetyModeSelector] 获取安全策略失败:', err))
  }, [])

  const handleSelect = async (newMode: SafetyMode): Promise<void> => {
    if (newMode === mode || loading) return
    setLoading(true)
    try {
      await setSafetyMode({ mode: newMode })
      setMode(newMode)
      // User explicitly changed mode → clear silenced sessions so the
      // banner can re-fire if the next message matches again.
      setSilenced(new Set())
    } catch (err) {
      console.error('[SafetyModeSelector] 设置安全模式失败:', err)
    } finally {
      setLoading(false)
    }
  }

  const current = MODES[mode]
  const Icon = current.icon

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className={cn('h-7 gap-1.5 px-2 text-xs font-medium', className)}
              disabled={loading}
            >
              <Icon className="size-3.5" style={{ color: current.color }} />
              <span style={{ color: current.color }}>{current.label}</span>
              <ChevronDown className="size-3 text-muted-foreground" />
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          <p className="text-xs">安全模式：{current.description}</p>
        </TooltipContent>
      </Tooltip>

      <DropdownMenuContent align="end" className="w-56">
        {(Object.entries(MODES) as [SafetyMode, ModeConfig][]).map(([key, cfg]) => {
          const ItemIcon = cfg.icon
          return (
            <DropdownMenuItem
              key={key}
              onClick={() => handleSelect(key)}
              className={cn('gap-2 cursor-pointer', cfg.bgClass)}
            >
              <ItemIcon className="size-4 shrink-0" style={{ color: cfg.color }} />
              <div className="flex flex-col gap-0.5">
                <span className="text-sm font-medium" style={{ color: cfg.color }}>
                  {cfg.label}
                </span>
                <span className="text-[11px] text-muted-foreground">{cfg.description}</span>
              </div>
              {key === mode && (
                <Shield className="size-3.5 ml-auto text-muted-foreground" />
              )}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
