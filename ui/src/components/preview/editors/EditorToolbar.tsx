/**
 * EditorToolbar — slim toolbar above the editor showing save state +
 * markdown mode toggle.
 *
 * Slot in EditorSurface (Task 17), which lays it out at the top of the
 * editor body.
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { Check, Circle, Pencil, Eye } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  markdownEditorModeAtom,
  dirtyBuffersAtom,
  conflictsAtom,
} from '@/atoms/preview-editor-atoms'

interface Props {
  filePath: string
  isMarkdown: boolean
  saveMode: 'explicit' | 'auto'
  /** True when a save is in flight (UI hint, not state). */
  saving?: boolean
}

export function EditorToolbar({ filePath, isMarkdown, saveMode, saving }: Props): React.ReactElement {
  const [mdMode, setMdMode] = useAtom(markdownEditorModeAtom)
  const dirty = useAtomValue(dirtyBuffersAtom).has(filePath)
  const conflicted = useAtomValue(conflictsAtom).has(filePath)

  // Save state pill
  let state: 'dirty' | 'saving' | 'saved' | 'auto' | 'conflict' = 'saved'
  if (conflicted) state = 'conflict'
  else if (saving) state = 'saving'
  else if (saveMode === 'explicit' && dirty) state = 'dirty'
  else if (saveMode === 'auto') state = 'auto'

  return (
    <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-popover/60 text-[11.5px]">
      <SaveStatePill state={state} />
      <div className="flex-1" />
      {isMarkdown && (
        // Single toggle: 富文本 (default) shows a Pencil to invite editing
        // the source; 源码 shows an Eye to invite returning to the preview.
        <button
          type="button"
          onClick={() => setMdMode(mdMode === 'rich' ? 'raw' : 'rich')}
          title={mdMode === 'rich' ? '编辑源码' : '返回富文本'}
          aria-label={mdMode === 'rich' ? '编辑源码' : '返回富文本'}
          className={cn(
            'inline-flex items-center justify-center size-6 rounded-md',
            'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.06]',
            'transition-colors',
          )}
        >
          {mdMode === 'rich' ? <Pencil className="size-3.5" /> : <Eye className="size-3.5" />}
        </button>
      )}
    </div>
  )
}

function SaveStatePill({ state }: { state: 'dirty' | 'saving' | 'saved' | 'auto' | 'conflict' }) {
  if (state === 'conflict') {
    return <span className="text-amber-600 dark:text-amber-300">⚠ 文件已更改</span>
  }
  if (state === 'saving') {
    return <span className="text-muted-foreground">保存中…</span>
  }
  if (state === 'dirty') {
    return (
      <span className="flex items-center gap-1 text-foreground/70">
        <Circle className="h-2 w-2 fill-current" />
        未保存
      </span>
    )
  }
  if (state === 'auto') {
    return <span className="text-muted-foreground">自动保存</span>
  }
  return (
    <span className="flex items-center gap-1 text-foreground/60">
      <Check className="h-3 w-3" />
      已保存
    </span>
  )
}
