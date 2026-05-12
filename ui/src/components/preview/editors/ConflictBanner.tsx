/**
 * ConflictBanner — sticky banner shown above the editor when the file
 * was modified externally (preview_write_text returned Conflict).
 *
 * 3 actions:
 *   - View diff: opens a modal with <DiffRenderer />
 *   - Overwrite: re-save with expected_mtime_ms = externalMtimeMs
 *   - Discard mine: replace editor content with externalContent
 *   - ✕: dismiss banner only (editor keeps user's edits, mtime stays stale)
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { AlertTriangle, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { conflictsAtom, clearConflictAction } from '@/atoms/preview-editor-atoms'

interface Props {
  filePath: string
  /** Current local content (for diff view). */
  localContent: string
  /** Called when user picks "Overwrite". Implementer re-saves with
   *  expected_mtime_ms = externalMtimeMs and clears the conflict on success. */
  onOverwrite: () => void
  /** Called when user picks "Discard mine". Implementer replaces editor
   *  content with externalContent and clears the conflict. */
  onDiscard: (externalContent: string, externalMtimeMs: number) => void
  /** Called when user picks "View diff". Implementer opens a modal
   *  containing <DiffRenderer left={localContent} right={externalContent} />. */
  onViewDiff: (localContent: string, externalContent: string) => void
}

export function ConflictBanner({ filePath, localContent, onOverwrite, onDiscard, onViewDiff }: Props): React.ReactElement | null {
  const conflict = useAtomValue(conflictsAtom).get(filePath)
  const clearConflict = useSetAtom(clearConflictAction)
  if (!conflict) return null

  return (
    <div className={cn(
      'sticky top-0 z-10 flex items-center gap-2 px-3 py-1.5 border-b',
      'bg-amber-50/90 border-amber-200/70 text-amber-900',
      'dark:bg-amber-900/30 dark:border-amber-700/40 dark:text-amber-100',
      'text-[11.5px]',
    )}>
      <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
      <span>文件已在磁盘上更改</span>
      <span className="flex-1" />
      <button
        type="button"
        onClick={() => onViewDiff(localContent, conflict.externalContent)}
        className="rounded px-2 py-0.5 text-[11px] hover:bg-amber-100/60 dark:hover:bg-amber-800/30"
      >
        查看差异
      </button>
      <button
        type="button"
        onClick={onOverwrite}
        className="rounded bg-amber-600 px-2 py-0.5 text-[11px] font-medium text-white hover:opacity-90"
      >
        覆盖
      </button>
      <button
        type="button"
        onClick={() => onDiscard(conflict.externalContent, conflict.externalMtimeMs)}
        className="rounded px-2 py-0.5 text-[11px] hover:bg-amber-100/60 dark:hover:bg-amber-800/30"
      >
        丢弃我的修改
      </button>
      <button
        type="button"
        onClick={() => clearConflict(filePath)}
        aria-label="dismiss"
        className="rounded p-0.5 hover:bg-amber-100/60 dark:hover:bg-amber-800/30"
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  )
}
