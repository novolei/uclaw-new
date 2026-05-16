/**
 * ShortcutSettings — keybinding management UI (v3).
 *
 * Data-driven from SHORTCUT_DEFINITIONS. Each row renders the current
 * effective binding (override > default) as a kbd cluster: every modifier
 * and the final key occupy their own "key cap" with a subtle 3-D look
 * (top highlight + bottom inset shadow), inspired by macOS System
 * Settings → Keyboard Shortcuts.
 *
 * Layout contract:
 *   - The kbd cluster is the RIGHTMOST element in every row, flush
 *     against the row's right padding — guarantees vertical alignment
 *     of the binding column regardless of whether a row shows a reset
 *     icon (reset sits to the LEFT of the cluster).
 *   - Group headers use UPPERCASE / muted typography + item count.
 *   - Sizing intentionally one notch SMALLER than other Settings pages
 *     because keybinding panels are reference material — should feel
 *     light, not shouty.
 *
 * Interaction:
 *   - Click the kbd cluster → enter capture mode. Press any combo →
 *     captured; Esc → cancel; Backspace alone → clear binding (sets
 *     override to empty string = "未绑定" state).
 *   - Conflict detection: if the captured combo is already used by
 *     another shortcut, show an amber inline banner with Replace /
 *     Cancel choices.
 *   - "重置全部" header button wipes the entire override map.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { RotateCcw, AlertTriangle } from 'lucide-react'
import { shortcutOverridesAtom } from '@/atoms/shortcut-atoms'
import {
  SHORTCUT_DEFINITIONS,
  getShortcutsByGroup,
  parseShortcutTokens,
  type ShortcutDefinition,
  type ShortcutToken,
} from '@/lib/shortcut-defaults'
import { useShortcutCapture } from '@/hooks/useShortcutCapture'
import { cn } from '@/lib/utils'
import { updateGlobalShortcut } from '@/lib/tauri-bridge'
import type { ShortcutOverrides } from '@/lib/chat-types'

const isMac = typeof navigator !== 'undefined' && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)

/** 全局快捷键 ID 列表：这些快捷键通过系统级全局注册，修改时需同步后端 */
const GLOBAL_SHORTCUT_IDS = ['quick-memory-voice', 'clipboard-capture-silent']

function effectiveBinding(def: ShortcutDefinition, overrides: ShortcutOverrides): string {
  const override = overrides[def.id]
  if (override) {
    const v = isMac ? override.mac : override.win
    if (v !== undefined) return v  // empty string is a legitimate "unbound" override
  }
  return (isMac ? def.mac : def.win) ?? ''
}

function findConflict(
  combo: string,
  selfId: string,
  overrides: ShortcutOverrides,
): ShortcutDefinition | undefined {
  if (!combo) return undefined
  for (const d of SHORTCUT_DEFINITIONS) {
    if (d.id === selfId) continue
    if (effectiveBinding(d, overrides) === combo) return d
  }
  return undefined
}

// ─── KbdCluster + KeyCap ────────────────────────────────────────

function KeyCap({ token }: { token: ShortcutToken }): React.ReactElement {
  return (
    <span
      className={cn(
        'inline-flex items-center justify-center h-[17px] rounded-[3.5px]',
        'bg-card border border-foreground/15',
        'shadow-[0_0.5px_0_rgba(0,0,0,0.04),inset_0_-0.5px_0_rgba(0,0,0,0.02)]',
        'text-foreground leading-none font-medium',
        token.kind === 'mod'
          ? 'min-w-[17px] px-[3px] text-[11.5px]'
          : 'min-w-[17px] px-[4px] text-[10.5px]',
      )}
    >
      {token.display}
    </span>
  )
}

interface KbdClusterProps {
  binding: string
  capturing: boolean
  onClick: () => void
}

function KbdCluster({ binding, capturing, onClick }: KbdClusterProps): React.ReactElement {
  const tokens = React.useMemo(() => parseShortcutTokens(binding), [binding])

  if (capturing) {
    return (
      <button
        type="button"
        onClick={onClick}
        aria-label="取消录入"
        className={cn(
          'inline-flex items-center gap-1.5 h-[24px] px-[10px] rounded-md',
          'bg-primary/10 border border-dashed border-primary/70',
          'text-primary text-[10.5px] font-medium leading-none cursor-text',
        )}
      >
        <span>按下组合键</span>
        <span className="inline-block w-[1.5px] h-[10px] bg-primary animate-pulse" />
      </button>
    )
  }

  if (tokens.length === 0) {
    return (
      <button
        type="button"
        onClick={onClick}
        aria-label="点击录入新组合"
        title="点击录入新组合"
        className={cn(
          'inline-flex items-center h-[24px] px-[10px] rounded-md',
          'bg-transparent border border-dashed border-foreground/20',
          'text-foreground/40 text-[10.5px] italic leading-none',
          'hover:border-foreground/35 hover:text-foreground/55 transition-colors',
        )}
      >
        未绑定
      </button>
    )
  }

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="点击录入新组合"
      title="点击录入新组合"
      className={cn(
        'inline-flex items-center gap-[2px] h-[24px] px-[4px] rounded-md',
        'bg-foreground/[0.025] border border-foreground/[0.06]',
        'shadow-[inset_0_-1px_0_rgba(0,0,0,0.025)]',
        'hover:bg-foreground/[0.045] hover:border-foreground/[0.10] transition-colors',
      )}
    >
      {tokens.map((t, i) => (
        <KeyCap key={i} token={t} />
      ))}
    </button>
  )
}

// ─── Row ────────────────────────────────────────────────────────

function ShortcutRow({ def }: { def: ShortcutDefinition }): React.ReactElement {
  const [overrides, setOverrides] = useAtom(shortcutOverridesAtom)
  const [capturing, setCapturing] = React.useState(false)
  const [conflictCombo, setConflictCombo] = React.useState<string | null>(null)

  const binding = effectiveBinding(def, overrides)
  const defaultBinding = isMac ? def.mac : def.win
  const isCustomized =
    overrides[def.id] !== undefined &&
    ((isMac && overrides[def.id]!.mac !== undefined) ||
      (!isMac && overrides[def.id]!.win !== undefined))

  const writeOverride = React.useCallback(
    (combo: string) => {
      setOverrides((prev) => ({
        ...prev,
        [def.id]: {
          ...prev[def.id],
          ...(isMac ? { mac: combo } : { win: combo }),
        },
      }))
      // 同步全局快捷键到后端
      if (GLOBAL_SHORTCUT_IDS.includes(def.id)) {
        updateGlobalShortcut(def.id, combo).catch((e) =>
          console.error('[ShortcutSettings] Failed to sync global shortcut:', e),
        )
      }
    },
    [def.id, setOverrides],
  )

  const clearOverride = React.useCallback(() => {
    setOverrides((prev) => {
      if (!prev[def.id]) return prev
      const { [def.id]: _drop, ...rest } = prev
      return rest
    })
    setConflictCombo(null)
    // 重置全局快捷键为默认值
    if (GLOBAL_SHORTCUT_IDS.includes(def.id)) {
      const defaultCombo = (isMac ? def.mac : def.win) ?? ''
      updateGlobalShortcut(def.id, defaultCombo).catch((e) =>
        console.error('[ShortcutSettings] Failed to reset global shortcut:', e),
      )
    }
  }, [def.id, def.mac, def.win, setOverrides])

  useShortcutCapture({
    active: capturing,
    onCapture: (combo) => {
      setCapturing(false)
      if (combo === null) return  // Esc cancel
      if (combo === 'Backspace') {
        // Backspace alone (no modifiers) → clear binding entirely.
        writeOverride('')
        return
      }
      const conflict = findConflict(combo, def.id, overrides)
      if (conflict) {
        setConflictCombo(combo)
        return
      }
      writeOverride(combo)
    },
  })

  const conflictDef = conflictCombo ? findConflict(conflictCombo, def.id, overrides) : undefined

  const acceptConflictReplace = () => {
    if (!conflictCombo || !conflictDef) return
    setOverrides((prev) => {
      const next = { ...prev }
      const otherDefault = isMac ? conflictDef.mac : conflictDef.win
      if (otherDefault === conflictCombo) {
        next[conflictDef.id] = {
          ...next[conflictDef.id],
          ...(isMac ? { mac: '' } : { win: '' }),
        }
      } else {
        const otherEntry = { ...(next[conflictDef.id] ?? {}) }
        if (isMac) delete otherEntry.mac
        else delete otherEntry.win
        if (otherEntry.mac === undefined && otherEntry.win === undefined) {
          delete next[conflictDef.id]
        } else {
          next[conflictDef.id] = otherEntry
        }
      }
      next[def.id] = {
        ...next[def.id],
        ...(isMac ? { mac: conflictCombo } : { win: conflictCombo }),
      }
      return next
    })
    // 同步全局快捷键变更
    if (GLOBAL_SHORTCUT_IDS.includes(conflictDef.id)) {
      // 被替换方的全局快捷键需要清除
      updateGlobalShortcut(conflictDef.id, '').catch((e) =>
        console.error('[ShortcutSettings] Failed to clear conflicting global shortcut:', e),
      )
    }
    if (GLOBAL_SHORTCUT_IDS.includes(def.id)) {
      // 当前快捷键重新注册新组合键
      updateGlobalShortcut(def.id, conflictCombo).catch((e) =>
        console.error('[ShortcutSettings] Failed to sync global shortcut after replace:', e),
      )
    }
    setConflictCombo(null)
  }

  return (
    <div className="flex flex-col group">
      <div
        className={cn(
          'flex items-center justify-between gap-3 px-3 py-[7px] min-h-[30px]',
          'transition-colors hover:bg-foreground/[0.012]',
        )}
      >
        <span className="text-[12px] text-foreground inline-flex items-center gap-1.5 min-w-0">
          <span className="truncate">{def.label}</span>
          {isCustomized && (
            <span className="shrink-0 px-[5px] py-[0.5px] rounded-full bg-primary/10 text-primary text-[9px] font-medium leading-[1.5] tracking-wide">
              已自定义
            </span>
          )}
        </span>
        <div className="flex items-center gap-[5px] shrink-0">
          <button
            type="button"
            onClick={clearOverride}
            disabled={!isCustomized}
            aria-label="重置为默认"
            title={isCustomized ? `重置为默认（${defaultBinding}）` : '已是默认值'}
            className={cn(
              'inline-flex w-[22px] h-[22px] items-center justify-center rounded-[5px]',
              'transition-all',
              isCustomized
                ? 'text-muted-foreground opacity-0 group-hover:opacity-70 hover:!opacity-100 hover:bg-foreground/[0.05] hover:text-foreground cursor-pointer'
                : 'text-foreground/15 opacity-0 cursor-not-allowed',
            )}
          >
            <RotateCcw className="size-[11px]" />
          </button>
          <KbdCluster
            binding={binding}
            capturing={capturing}
            onClick={() => {
              setConflictCombo(null)
              setCapturing((c) => !c)
            }}
          />
        </div>
      </div>
      {conflictCombo && conflictDef && (
        <div
          className={cn(
            'mx-3 mb-2 flex items-start gap-2 rounded-md px-2.5 py-1.5',
            'bg-amber-50/85 dark:bg-amber-900/20',
            'border border-amber-200/70 dark:border-amber-700/35',
            'text-amber-900 dark:text-amber-200 text-[10.5px] leading-relaxed',
          )}
        >
          <AlertTriangle className="size-3 shrink-0 mt-[2px]" aria-hidden />
          <div className="flex-1">
            <span className="font-mono">{conflictCombo}</span>
            <span className="mx-1">已被</span>
            <span className="font-medium">「{conflictDef.label}」</span>
            <span>使用。要替换吗？被替换方将清除其当前绑定。</span>
          </div>
          <div className="flex gap-1 shrink-0">
            <button
              type="button"
              onClick={acceptConflictReplace}
              className="px-2 py-[1px] rounded bg-amber-600 text-white text-[10px] font-medium hover:opacity-90"
            >
              替换
            </button>
            <button
              type="button"
              onClick={() => setConflictCombo(null)}
              className="px-2 py-[1px] rounded text-[10px] hover:bg-amber-100/70 dark:hover:bg-amber-800/30"
            >
              取消
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

// ─── Header pieces ──────────────────────────────────────────────

function ResetAllButton(): React.ReactElement {
  const [overrides, setOverrides] = useAtom(shortcutOverridesAtom)
  const hasAny = Object.keys(overrides).length > 0

  const handleResetAll = () => {
    setOverrides({})
    // 将所有全局快捷键重置为默认值
    for (const id of GLOBAL_SHORTCUT_IDS) {
      if (overrides[id]) {
        const def = SHORTCUT_DEFINITIONS.find((d) => d.id === id)
        const defaultCombo = def ? (isMac ? def.mac : def.win) ?? '' : ''
        updateGlobalShortcut(id, defaultCombo).catch((e) =>
          console.error('[ShortcutSettings] Failed to reset global shortcut on reset-all:', e),
        )
      }
    }
  }

  return (
    <button
      type="button"
      onClick={handleResetAll}
      disabled={!hasAny}
      title={hasAny ? '清除全部自定义快捷键，恢复默认' : '没有自定义项'}
      aria-label="重置全部"
      className={cn(
        'inline-flex items-center gap-[5px] px-2 py-[3px] rounded-md text-[11px]',
        'border border-transparent transition-all whitespace-nowrap',
        hasAny
          ? 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.04] hover:border-border/60 cursor-pointer'
          : 'text-foreground/25 cursor-not-allowed',
      )}
    >
      <RotateCcw className="size-[11px]" />
      重置全部
    </button>
  )
}

// ─── Panel root ─────────────────────────────────────────────────

export function ShortcutSettings(): React.ReactElement {
  const groups = React.useMemo(() => getShortcutsByGroup(), [])
  const groupNames = Object.keys(groups)
  return (
    <div className="space-y-4">
      {/* No h2 — the settings nav rail already labels this section "快捷键". */}
      <div className="flex items-center justify-between gap-3 pb-3 border-b border-border/50">
        <p className="text-[11px] text-muted-foreground leading-[1.55] m-0">
          点击组合键卡片可录入新组合 ·{' '}
          <kbd className="font-mono text-[10px] bg-foreground/[0.06] px-[5px] py-[0.5px] rounded">Esc</kbd>{' '}
          取消 ·{' '}
          <kbd className="font-mono text-[10px] bg-foreground/[0.06] px-[5px] py-[0.5px] rounded">⌫</kbd>{' '}
          清除绑定
        </p>
        <ResetAllButton />
      </div>
      {groupNames.map((group) => (
        <section key={group}>
          <div className="flex items-baseline gap-[7px] px-[2px] mb-1.5">
            <span className="text-[10.5px] uppercase tracking-[0.55px] font-semibold text-muted-foreground">
              {group}
            </span>
            <span className="text-[10px] text-foreground/30 tabular-nums">
              {groups[group]!.length} 项
            </span>
          </div>
          <div className="rounded-[10px] bg-card border border-border/60 overflow-hidden">
            {groups[group]!.map((def, i) => (
              <React.Fragment key={def.id}>
                {i > 0 && <div className="border-t border-border/40" />}
                <ShortcutRow def={def} />
              </React.Fragment>
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}
