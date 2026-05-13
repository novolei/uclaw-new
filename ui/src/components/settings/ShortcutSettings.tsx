/**
 * ShortcutSettings — keybinding management UI.
 *
 * Replaces the old hardcoded read-only list with a data-driven view of
 * SHORTCUT_DEFINITIONS. Each row shows the current effective binding
 * (override > default), lets the user enter capture mode to record a
 * new combo, surfaces conflicts in real-time, and persists overrides
 * to `shortcutOverridesAtom` (atomWithStorage → localStorage).
 *
 *   - Click a row's binding chip → "press a combo…" mode
 *   - Press any combo → captured + persisted, capture mode closes
 *   - Press Escape (while capturing) → cancel, no change
 *   - Reset icon next to the chip clears any override → falls back to default
 *
 * Conflict policy: if the captured combo is already used by ANOTHER
 * shortcut (default or overridden), we show a warning row beneath the
 * chip listing the conflicting action and a "Replace anyway" button
 * that swaps — overrides the conflict's binding to its default first,
 * then assigns the new combo here. No silent dual-bound state.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { RotateCcw, KeyRound, AlertTriangle } from 'lucide-react'
import { shortcutOverridesAtom } from '@/atoms/shortcut-atoms'
import {
  SHORTCUT_DEFINITIONS,
  formatShortcut,
  getShortcutsByGroup,
  type ShortcutDefinition,
} from '@/lib/shortcut-defaults'
import { useShortcutCapture } from '@/hooks/useShortcutCapture'
import { cn } from '@/lib/utils'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import type { ShortcutOverrides } from '@/lib/chat-types'

const isMac = typeof navigator !== 'undefined' && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)

/** Resolve a definition's effective binding on the current platform,
 *  honoring an override map. */
function effectiveBinding(
  def: ShortcutDefinition,
  overrides: ShortcutOverrides,
): string {
  const override = overrides[def.id]
  if (override) {
    const overriddenPlatform = isMac ? override.mac : override.win
    if (overriddenPlatform) return overriddenPlatform
  }
  return (isMac ? def.mac : def.win) ?? ''
}

/** Find another shortcut (by id) that already binds the same combo on
 *  the current platform — both override-driven and default values are
 *  considered. Returns `undefined` for no conflict. */
function findConflict(
  combo: string,
  selfId: string,
  overrides: ShortcutOverrides,
): ShortcutDefinition | undefined {
  for (const d of SHORTCUT_DEFINITIONS) {
    if (d.id === selfId) continue
    if (effectiveBinding(d, overrides) === combo) return d
  }
  return undefined
}

interface RowProps {
  def: ShortcutDefinition
}

function ShortcutRow({ def }: RowProps): React.ReactElement {
  const [overrides, setOverrides] = useAtom(shortcutOverridesAtom)
  const [capturing, setCapturing] = React.useState(false)
  const [conflictCombo, setConflictCombo] = React.useState<string | null>(null)

  const binding = effectiveBinding(def, overrides)
  const defaultBinding = isMac ? def.mac : def.win
  const isOverridden = binding !== defaultBinding

  // Conflict resolution + write helpers — declared up here so the
  // `useShortcutCapture` callback below can read them via closure.
  const writeOverride = React.useCallback(
    (combo: string) => {
      setOverrides((prev) => ({
        ...prev,
        [def.id]: {
          ...prev[def.id],
          ...(isMac ? { mac: combo } : { win: combo }),
        },
      }))
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
  }, [def.id, setOverrides])

  useShortcutCapture({
    active: capturing,
    onCapture: (combo) => {
      setCapturing(false)
      if (combo === null) return  // user cancelled with Escape
      const conflict = findConflict(combo, def.id, overrides)
      if (conflict) {
        setConflictCombo(combo)
        return  // don't write; surface conflict UI, user picks Replace anyway / Cancel
      }
      writeOverride(combo)
    },
  })

  const conflictDef = conflictCombo ? findConflict(conflictCombo, def.id, overrides) : undefined

  const acceptConflictReplace = () => {
    if (!conflictCombo || !conflictDef) return
    // Reset the conflicting row to its default first so it doesn't keep
    // pointing at conflictCombo (the new combo we're about to claim).
    // If the conflicting row IS already at its default (i.e. the default
    // value IS the conflict), then we have to override THAT row away
    // from its default — pick the empty string sentinel? Actually if the
    // conflict is the OTHER row's default, we override it to "" (cleared)
    // so it has no binding at all. The user can then rebind it later.
    setOverrides((prev) => {
      const next = { ...prev }
      const otherDefault = isMac ? conflictDef.mac : conflictDef.win
      if (otherDefault === conflictCombo) {
        // Their default conflicts; clear their binding entirely.
        next[conflictDef.id] = {
          ...next[conflictDef.id],
          ...(isMac ? { mac: '' } : { win: '' }),
        }
      } else {
        // They had an override that conflicts; remove the override → back to default.
        const otherEntry = { ...(next[conflictDef.id] ?? {}) }
        if (isMac) delete otherEntry.mac
        else delete otherEntry.win
        if (otherEntry.mac === undefined && otherEntry.win === undefined) {
          delete next[conflictDef.id]
        } else {
          next[conflictDef.id] = otherEntry
        }
      }
      // Claim the combo for ourselves.
      next[def.id] = {
        ...next[def.id],
        ...(isMac ? { mac: conflictCombo } : { win: conflictCombo }),
      }
      return next
    })
    setConflictCombo(null)
  }

  const handleChipClick = () => {
    setConflictCombo(null)
    setCapturing((c) => !c)
  }

  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <div className="flex items-center justify-between gap-3">
        <span className="text-sm text-foreground">{def.label}</span>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={handleChipClick}
            aria-label={capturing ? '取消录入' : '点击录入新组合'}
            title={capturing ? '取消（Esc）' : '点击录入新组合'}
            className={cn(
              'inline-flex items-center gap-1.5 px-2 py-1 rounded border text-xs font-mono',
              'transition-colors min-w-[110px] justify-center',
              capturing
                ? 'border-primary/60 bg-primary/10 text-primary'
                : 'border-border bg-muted text-foreground/85 hover:bg-foreground/[0.04]',
            )}
          >
            {capturing ? (
              <>
                <KeyRound className="size-3" />
                <span>按下组合键…</span>
              </>
            ) : binding ? (
              <kbd className="font-mono">{formatShortcut(binding)}</kbd>
            ) : (
              <span className="text-muted-foreground italic">未绑定</span>
            )}
          </button>
          <button
            type="button"
            onClick={clearOverride}
            disabled={!isOverridden}
            aria-label="重置为默认"
            title={isOverridden ? '重置为默认' : '已是默认值'}
            className={cn(
              'inline-flex size-6 items-center justify-center rounded-md',
              'transition-colors',
              isOverridden
                ? 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.06]'
                : 'text-foreground/25 cursor-not-allowed',
            )}
          >
            <RotateCcw className="size-3.5" />
          </button>
        </div>
      </div>
      {conflictCombo && conflictDef && (
        <div className="flex items-start gap-2 rounded-md bg-amber-50/80 dark:bg-amber-900/20 border border-amber-200/60 dark:border-amber-700/30 px-2.5 py-2 text-[11.5px] text-amber-900 dark:text-amber-200">
          <AlertTriangle className="size-3.5 shrink-0 mt-0.5" aria-hidden />
          <div className="flex-1 leading-relaxed">
            <span className="font-mono">{formatShortcut(conflictCombo)}</span>
            <span className="mx-1">已被</span>
            <span className="font-medium">「{conflictDef.label}」</span>
            <span>使用。要替换吗？被替换方将清除其当前绑定。</span>
          </div>
          <div className="flex gap-1.5 shrink-0">
            <button
              type="button"
              onClick={acceptConflictReplace}
              className="px-2 py-0.5 rounded bg-amber-600 text-white text-[11px] font-medium hover:opacity-90"
            >
              替换
            </button>
            <button
              type="button"
              onClick={() => setConflictCombo(null)}
              className="px-2 py-0.5 rounded text-[11px] hover:bg-amber-100/70 dark:hover:bg-amber-800/30"
            >
              取消
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

function ResetAllButton(): React.ReactElement {
  const [overrides, setOverrides] = useAtom(shortcutOverridesAtom)
  const hasAny = Object.keys(overrides).length > 0
  return (
    <button
      type="button"
      onClick={() => setOverrides({})}
      disabled={!hasAny}
      className={cn(
        'inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs',
        'transition-colors',
        hasAny
          ? 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.06]'
          : 'text-foreground/25 cursor-not-allowed',
      )}
      aria-label="重置全部"
      title={hasAny ? '清除全部自定义快捷键，恢复默认' : '没有自定义项'}
    >
      <RotateCcw className="size-3.5" />
      重置全部
    </button>
  )
}

export function ShortcutSettings(): React.ReactElement {
  const groups = React.useMemo(() => getShortcutsByGroup(), [])
  const groupNames = Object.keys(groups)
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">快捷键</h2>
        <ResetAllButton />
      </div>
      {groupNames.map((group) => (
        <SettingsSection
          key={group}
          title={group}
          description="点击右侧的组合键卡片可录入新组合；Esc 取消录入。"
        >
          <SettingsCard>
            <div className="divide-y divide-border">
              {groups[group]!.map((def) => (
                <ShortcutRow key={def.id} def={def} />
              ))}
            </div>
          </SettingsCard>
        </SettingsSection>
      ))}
    </div>
  )
}
