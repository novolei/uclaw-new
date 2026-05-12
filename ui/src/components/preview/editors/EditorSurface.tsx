/**
 * EditorSurface — top-level for editable preview files.
 *
 * Wraps:
 *   - EditorToolbar (top)
 *   - ConflictBanner (sticky between toolbar and editor)
 *   - TextEditor or MarkdownEditor (body)
 *   - Diff modal for ConflictBanner's "View diff" action
 *
 * Owns the save IPC call (preview_write_text) and SaveOutcome dispatch.
 * Single source of truth for current content; editors call onContentChange
 * to keep it in sync.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import { setConflictAction } from '@/atoms/preview-editor-atoms'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { TextEditor, type SaveOutcome } from './TextEditor'
import { MarkdownEditor } from './MarkdownEditor'
import { EditorToolbar } from './EditorToolbar'
import { ConflictBanner } from './ConflictBanner'
import { DiffRenderer } from '../renderers/diff/DiffRenderer'
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
  kind: 'saved' | 'conflict' | 'needsApproval'
  // Saved
  mtimeMs?: number
  size?: number
  // Conflict
  currentMtimeMs?: number
  currentContent?: string
  // NeedsApproval
  approvalId?: string
}

const NEW_FILE_MTIME_SENTINEL = -1

export function EditorSurface({ target, initialContent, mtimeMs: initialMtimeMs, isMarkdown, language }: Props): React.ReactElement {
  const setConflict = useSetAtom(setConflictAction)
  const [content, setContent] = React.useState(initialContent)
  const [mtimeMs, setMtimeMs] = React.useState(initialMtimeMs)
  const [saving, setSaving] = React.useState(false)
  const [diffOpen, setDiffOpen] = React.useState(false)
  const [diffPayload, setDiffPayload] = React.useState<{ local: string; external: string } | null>(null)

  const filePath = target.absolutePath ?? `${target.mountId}::${target.relPath}`
  const saveMode: 'explicit' | 'auto' = isMarkdown ? 'auto' : 'explicit'

  // Reset local state when the target file changes.
  React.useEffect(() => {
    setContent(initialContent)
    setMtimeMs(initialMtimeMs)
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
          expectedMtimeMs: mtimeMs === 0 ? NEW_FILE_MTIME_SENTINEL : mtimeMs,
        })
        if (result.kind === 'saved') {
          setMtimeMs(result.mtimeMs ?? 0)
          return { kind: 'saved', mtimeMs: result.mtimeMs ?? 0 }
        }
        if (result.kind === 'conflict') {
          setConflict({
            filePath,
            conflict: {
              externalContent: result.currentContent ?? '',
              externalMtimeMs: result.currentMtimeMs ?? 0,
            },
          })
          return {
            kind: 'conflict',
            externalContent: result.currentContent ?? '',
            externalMtimeMs: result.currentMtimeMs ?? 0,
          }
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
    [filePath, target, mtimeMs, setConflict],
  )

  const handleOverwrite = React.useCallback(async () => {
    // Force-save with current local content. Server will check mtime;
    // because the user clicked "覆盖", we expect this to succeed against
    // the latest disk state. If it still conflicts (extremely rare race),
    // the banner will refresh.
    await handleSave(content)
  }, [content, handleSave])

  const handleDiscard = React.useCallback((externalContent: string, externalMtimeMs: number) => {
    setContent(externalContent)
    setMtimeMs(externalMtimeMs)
    // Editor will re-mount with new initialContent via the key prop below.
  }, [])

  const handleViewDiff = React.useCallback((local: string, external: string) => {
    setDiffPayload({ local, external })
    setDiffOpen(true)
  }, [])

  const EditorComponent = isMarkdown ? MarkdownEditor : TextEditor

  return (
    <div className="flex flex-col h-full">
      <EditorToolbar filePath={filePath} isMarkdown={isMarkdown} saveMode={saveMode} saving={saving} />
      <ConflictBanner
        filePath={filePath}
        localContent={content}
        onOverwrite={() => void handleOverwrite()}
        onDiscard={handleDiscard}
        onViewDiff={handleViewDiff}
      />
      <div className="flex-1 min-h-0">
        <EditorComponent
          // key forces remount on file/discard so initialContent re-applies
          key={`${filePath}::${mtimeMs}`}
          initialContent={content}
          language={language}
          mtimeMs={mtimeMs}
          filePath={filePath}
          saveMode={saveMode}
          onSave={handleSave}
          onContentChange={(next) => setContent(next)}
        />
      </div>

      <Dialog open={diffOpen} onOpenChange={setDiffOpen}>
        <DialogContent className="max-w-5xl h-[80vh] p-0">
          <DialogTitle className="sr-only">查看差异</DialogTitle>
          {diffPayload && (
            <DiffRenderer
              left={{ content: diffPayload.local, label: '我的修改' }}
              right={{ content: diffPayload.external, label: '磁盘上' }}
              language={language}
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}
