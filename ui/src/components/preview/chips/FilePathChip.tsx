/**
 * FilePathChip — Inline file reference in agent messages.
 *
 * Visual states:
 *   - ok       : full opacity, hover background
 *   - pending  : 70% opacity, no spinner (too noisy with many chips)
 *   - missing  : 45% opacity + strikethrough label, tooltip "文件未找到"
 *
 * Click semantics (uniform with FileTreeNode after Task 11):
 *   - Click       → openPreviewAction
 *   - Shift-click → addPendingAttachmentAction
 *   - Cmd/Ctrl    → reserved for W5; no-op today
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { FileIcon } from '@react-symbols/icons/utils'
import { cn } from '@/lib/utils'
import { openPreviewTabAction } from '@/atoms/preview-panel-atoms'
import { addPendingAttachmentAction } from '@/atoms/preview-chip-atoms'

export type ChipState = 'ok' | 'pending' | 'missing'

export interface FilePathChipProps {
  /** Input string the plugin matched (with :line:col stripped). */
  rawPath: string
  /** Display label — usually basename or the markdown link text. */
  label: string
  /** Resolution state (from useFileChipResolver). */
  state: ChipState
  /** Mount id (from resolver). Empty string when state !== 'ok'. */
  mountId: string
  /** Path inside the mount (from resolver). Empty string when state !== 'ok'. */
  relPath: string
  /** Absolute path when resolved; empty string otherwise. */
  absolutePath: string
  /** Active session id (used when re-opening the preview). */
  sessionId?: string | null
  /** Optional line/col from parser, preserved for future jump-to-line. */
  line?: number
  col?: number
}

export function FilePathChip(props: FilePathChipProps): React.ReactElement {
  const openPreview = useSetAtom(openPreviewTabAction)
  const addAttachment = useSetAtom(addPendingAttachmentAction)

  const isMissing = props.state === 'missing'

  const handleClick = React.useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      // Cmd/Ctrl reserved for W5 — no-op today.
      if (e.metaKey || e.ctrlKey) {
        e.preventDefault()
        return
      }
      e.preventDefault()
      if (e.shiftKey) {
        if (isMissing) return
        void addAttachment({
          mountId: props.mountId,
          relPath: props.relPath,
          name: props.label,
          sessionId: props.sessionId ?? null,
          absolutePath: props.absolutePath,
        })
        return
      }
      if (props.state !== 'ok') {
        // Still open so user sees the "not found" surface (master spec §6.8).
        openPreview({
          target: {
            mountId: props.mountId || 'workspace:default',
            relPath: props.relPath || props.rawPath,
            name: props.label,
            sessionId: props.sessionId ?? null,
            absolutePath: props.absolutePath,
          },
          source: 'manual',
        })
        return
      }
      openPreview({
        target: {
          mountId: props.mountId,
          relPath: props.relPath,
          name: props.label,
          sessionId: props.sessionId ?? null,
          absolutePath: props.absolutePath,
        },
        source: 'manual',
      })
    },
    [openPreview, addAttachment, isMissing, props],
  )

  const stateOpacity =
    props.state === 'ok'
      ? 'opacity-100'
      : props.state === 'pending'
        ? 'opacity-70'
        : 'opacity-45'

  return (
    <button
      type="button"
      onClick={handleClick}
      title={isMissing ? `文件未找到：${props.rawPath}` : props.rawPath}
      data-chip-state={props.state}
      className={cn(
        'inline-flex items-center gap-1 align-baseline',
        'h-[20px] px-1.5 mx-0.5 rounded-md',
        'text-[11.5px] font-mono tabular-nums leading-none',
        'bg-foreground/[0.04] hover:bg-foreground/[0.08]',
        'border border-border/60',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        stateOpacity,
      )}
    >
      <FileIcon fileName={props.label} autoAssign width={11} height={11} className="shrink-0" aria-hidden />
      <span className={cn(isMissing && 'line-through')}>{props.label}</span>
      {props.line !== undefined && (
        <span className="text-foreground/45">
          :{props.line}
          {props.col !== undefined ? `:${props.col}` : ''}
        </span>
      )}
    </button>
  )
}
