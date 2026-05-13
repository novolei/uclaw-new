/**
 * EditorSurface — top-level for editable preview files.
 *
 * Wraps:
 *   - EditorToolbar (top)
 *   - TextEditor or MarkdownEditor (body)
 *
 * Owns the save IPC call (preview_write_text) and the dirty-buffer
 * registration (via useDirtyBuffer). Single source of truth for current
 * content; editors call onContentChange to keep it in sync.
 *
 * The mtime-based conflict banner was removed (2026-05-13). See
 * preview-editor-atoms.ts and preview/commands.rs for the rationale —
 * tldr: dirty-guard pattern (if2Ai-style) eliminates the race class that
 * the mtime check kept misclassifying as conflicts.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import { clearDirtyBufferAction } from '@/atoms/preview-editor-atoms'
import { TextEditor, type SaveOutcome } from './TextEditor'
import { MarkdownEditor } from './MarkdownEditor'
import { EditorToolbar } from './EditorToolbar'
import { useDirtyBuffer } from './useDirtyBuffer'
import type { PreviewFileTarget } from '@/atoms/preview-panel-atoms'

interface Props {
  target: PreviewFileTarget
  initialContent: string
  mtimeMs: number
  isMarkdown: boolean
  /** Shiki language id (for TextEditor). */
  language?: string
}

interface WriteResultIpc {
  kind: 'saved' | 'needsApproval'
  // Saved
  mtimeMs?: number
  size?: number
  // NeedsApproval
  approvalId?: string
}

export function EditorSurface({ target, initialContent, mtimeMs: initialMtimeMs, isMarkdown, language }: Props): React.ReactElement {
  const clearDirty = useSetAtom(clearDirtyBufferAction)
  // `baseline` is the on-disk content at last load/save. Updated on:
  //   - filePath change (re-snap from props)
  //   - successful save (promote current content to new baseline)
  // `content` is the LIVE mutating state from the editor's onContentChange.
  // useDirtyBuffer compares content !== baseline to flag dirty.
  const [baseline, setBaseline] = React.useState(initialContent)
  const [content, setContent] = React.useState(initialContent)
  const [mtimeMs, setMtimeMs] = React.useState(initialMtimeMs)
  const [saving, setSaving] = React.useState(false)

  const filePath = target.absolutePath ?? `${target.mountId}::${target.relPath}`
  const saveMode: 'explicit' | 'auto' = isMarkdown ? 'auto' : 'explicit'

  // Dirty tracking — fires for BOTH save modes now. usePreviewRefresh's
  // dirty-guard reads from the same atom, so an unsaved local edit
  // blocks watcher/focus refetches from clobbering the draft.
  useDirtyBuffer({
    filePath,
    baselineContent: baseline,
    baselineMtimeMs: mtimeMs,
    currentContent: content,
  })

  // Only reset baseline/content/mtime when the file ITSELF changes (filePath).
  // Don't reset on every initialContent / initialMtimeMs prop update —
  // useFileBytes refetches on watcher events would otherwise silently
  // destroy the user's unsaved edits (the dirty-guard already prevents
  // refetches while dirty, but a clean → bumped → ready transition for a
  // DIFFERENT file shouldn't accidentally promote into the current one).
  React.useEffect(() => {
    setBaseline(initialContent)
    setContent(initialContent)
    setMtimeMs(initialMtimeMs)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath])

  // If the baseline content changes for the same filePath (i.e. the
  // file was refreshed from disk while NOT dirty — usePreviewRefresh
  // only bumps when clean), promote the fresh content as the new
  // baseline. This is the analogue of if2Ai's "refresh draft when not
  // dirty" branch in useProjectPreviewState.
  React.useEffect(() => {
    if (content === baseline) {
      // Clean — accept the disk refresh.
      setBaseline(initialContent)
      setContent(initialContent)
      setMtimeMs(initialMtimeMs)
    }
    // Intentionally narrow deps: we only react to baseline updates that
    // arrive via prop, not to local edits.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialContent, initialMtimeMs])

  const handleSave = React.useCallback(
    async (latest: string): Promise<SaveOutcome> => {
      setSaving(true)
      try {
        const result = await invoke<WriteResultIpc>('preview_write_text', {
          mountId: target.mountId,
          relPath: target.relPath,
          sessionId: target.sessionId ?? null,
          content: latest,
        })
        if (result.kind === 'saved') {
          const newMtime = result.mtimeMs ?? 0
          setMtimeMs(newMtime)
          setBaseline(latest)
          // Clearing the dirty buffer is also handled by useDirtyBuffer
          // when content returns to baseline on the next render, but
          // doing it eagerly here makes the dirty-flip atomic with the
          // save success (no transient render with content === baseline
          // but the atom still flagged dirty).
          clearDirty(filePath)
          return { kind: 'saved', mtimeMs: newMtime }
        }
        if (result.kind === 'needsApproval') {
          return { kind: 'needs-approval', approvalId: result.approvalId ?? '' }
        }
        return { kind: 'error', message: 'unknown WriteResult' }
      } catch (err) {
        return { kind: 'error', message: err instanceof Error ? err.message : String(err) }
      } finally {
        setSaving(false)
      }
    },
    [filePath, target, clearDirty],
  )

  const EditorComponent = isMarkdown ? MarkdownEditor : TextEditor

  return (
    <div className="flex flex-col h-full">
      <EditorToolbar filePath={filePath} isMarkdown={isMarkdown} saveMode={saveMode} saving={saving} />
      <div className="flex-1 min-h-0">
        <EditorComponent
          // key forces remount on file change so initialContent re-applies
          key={filePath}
          initialContent={baseline}
          language={language}
          mtimeMs={mtimeMs}
          filePath={filePath}
          saveMode={saveMode}
          onSave={handleSave}
          onContentChange={(next) => setContent(next)}
        />
      </div>
    </div>
  )
}
