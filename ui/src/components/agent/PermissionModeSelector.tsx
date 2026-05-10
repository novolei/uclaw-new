/**
 * PermissionModeSelector — global SafetyMode quick-toggle in the input bar.
 *
 * Cycles between `supervised` (smart-approval, default) and `yolo`
 * (auto-approve everything). Backed by the real `SafetyManager` (see
 * `src-tauri/src/safety/mod.rs`) — clicking actually persists to
 * `~/.uclaw/safety_policy.json`.
 *
 * For per-session overrides + per-command rules + audit log, use
 * Settings → 工具权限 (P6).
 *
 * History: this previously called `get_permission_mode`/`set_permission_mode`
 * Tauri commands that never existed; the bridge silenced the IPC failures via
 * `.catch()`. The selector visibly cycled but the backend never received the
 * value. Now properly wired.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Compass, Zap } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { safetyModeAtom } from '@/atoms/safety-atoms'
import { getSafetyPolicy, setSafetyMode, type SafetyModeWire } from '@/lib/tauri-bridge'

const MODE_CONFIG: Record<SafetyModeWire, {
  icon: React.ComponentType<{ className?: string }>
  label: string
  description: string
}> = {
  ask: {
    icon: Compass,
    label: '逐次确认',
    description: '所有需要审批的工具调用都弹窗询问',
  },
  supervised: {
    icon: Compass,
    label: '自动模式',
    description: '低风险工具自动通过，高风险弹窗确认',
  },
  yolo: {
    icon: Zap,
    label: '完全自动',
    description: '所有工具调用自动允许（不推荐）',
  },
}

/**
 * Cycle order. `ask` is intentionally excluded from the quick-cycle — it's
 * the strictest mode and rarely needed; users wanting it can pick from the
 * Settings → 工具权限 tab. The toggle here is the everyday "be careful" /
 * "trust me" switch.
 */
const CYCLE_ORDER: SafetyModeWire[] = ['supervised', 'yolo']

export interface PermissionModeSelectorProps {
  /** Kept for prop compat with the previous selector; unused — global SafetyMode is workspace-agnostic. */
  sessionId?: string
}

export function PermissionModeSelector(_: PermissionModeSelectorProps): React.ReactElement | null {
  const [mode, setMode] = useAtom(safetyModeAtom)
  const [busy, setBusy] = React.useState(false)

  // Hydrate from backend on mount.
  React.useEffect(() => {
    getSafetyPolicy()
      .then((p) => setMode(p.globalMode as SafetyModeWire))
      .catch((e) => console.error('[PermissionModeSelector] getSafetyPolicy failed:', e))
    // eslint-disable-next-line react-hooks/exhaustive-deps -- run once
  }, [])

  const cycleMode = React.useCallback(async () => {
    if (busy) return
    const idx = CYCLE_ORDER.indexOf(mode)
    const next = CYCLE_ORDER[(idx === -1 ? 0 : idx + 1) % CYCLE_ORDER.length]!
    setBusy(true)
    try {
      await setSafetyMode({ mode: next })
      setMode(next)
    } catch (err) {
      console.error('[PermissionModeSelector] setSafetyMode failed:', err)
    } finally {
      setBusy(false)
      requestAnimationFrame(() => document.querySelector<HTMLElement>('.ProseMirror')?.focus())
    }
  }, [mode, busy, setMode])

  // `ask` is a valid wire value but we display it via supervised's icon if
  // it ever sneaks in (e.g. set via Settings tab) — defensively map.
  const displayMode = mode === 'ask' ? 'ask' : mode
  const config = MODE_CONFIG[displayMode] ?? MODE_CONFIG.supervised
  const Icon = config.icon

  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={cycleMode}
            disabled={busy}
            className="flex items-center gap-1 px-1.5 py-1 rounded text-xs font-medium transition-colors text-muted-foreground hover:text-foreground disabled:opacity-50"
          >
            <Icon className="size-3.5" />
            <span className="hidden sm:inline">{config.label}</span>
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom" className="max-w-[220px]">
          <p className="font-medium">{config.label}</p>
          <p className="text-xs text-muted-foreground mt-0.5">{config.description}</p>
          <p className="text-xs text-muted-foreground mt-1">点击切换 · 详细规则在 设置 → 工具权限</p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}
