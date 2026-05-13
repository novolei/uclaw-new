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
import { useAtomValue, useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import {
  lastSelfWriteMtimeAtom,
  setConflictAction,
  clearConflictAction,
  recordSelfWriteAction,
} from '@/atoms/preview-editor-atoms'
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
/** Mirror of preview/commands.rs FORCE_OVERWRITE_SENTINEL. Tells the
 *  backend "skip the optimistic mtime check, write my content as-is". */
const FORCE_OVERWRITE_SENTINEL = -2

export function EditorSurface({ target, initialContent, mtimeMs: initialMtimeMs, isMarkdown, language }: Props): React.ReactElement {
  const setConflict = useSetAtom(setConflictAction)
  const clearConflict = useSetAtom(clearConflictAction)
  const recordSelfWrite = useSetAtom(recordSelfWriteAction)
  const lastSelfWriteMap = useAtomValue(lastSelfWriteMtimeAtom)
  // `baseline` is the on-disk content at last load/save. Updated ONLY on:
  //   - filePath change (re-snap from props)
  //   - successful save (promote current content to new baseline)
  //   - "Discard mine" (replace with external content)
  // `content` is the LIVE mutating state from the editor's onContentChange.
  // useDirtyBuffer (inside TextEditor) compares content !== baseline to flag dirty.
  const [baseline, setBaseline] = React.useState(initialContent)
  const [content, setContent] = React.useState(initialContent)
  const [mtimeMs, setMtimeMs] = React.useState(initialMtimeMs)
  const [saving, setSaving] = React.useState(false)
  const [diffOpen, setDiffOpen] = React.useState(false)
  const [diffPayload, setDiffPayload] = React.useState<{ local: string; external: string } | null>(null)

  const filePath = target.absolutePath ?? `${target.mountId}::${target.relPath}`
  const saveMode: 'explicit' | 'auto' = isMarkdown ? 'auto' : 'explicit'

  // mtimeMs ALSO lives in a ref so handleSave reads the absolute-latest
  // value synchronously, with no React-commit timing window. The state
  // version is only used by render-time consumers (toolbar pill, etc.).
  // Bug repro before this ref existed: save A returned T1 + setMtimeMs(T1)
  // queued; before React committed, save B fired with the still-T0
  // useCallback closure; backend returned conflict because disk was T1.
  const mtimeMsRef = React.useRef(initialMtimeMs)
  const setMtimeBoth = React.useCallback((next: number) => {
    mtimeMsRef.current = next
    setMtimeMs(next)
  }, [])

  // Only reset baseline/content/mtime when the file ITSELF changes (filePath).
  // Don't reset on every initialContent / initialMtimeMs prop update —
  // useFileBytes refetches on watcher events would otherwise silently
  // destroy the user's unsaved edits AND break conflict detection.
  React.useEffect(() => {
    setBaseline(initialContent)
    setContent(initialContent)
    mtimeMsRef.current = initialMtimeMs
    setMtimeMs(initialMtimeMs)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath])

  // lastSelfWriteMap also goes through a ref so the conflict-echo check
  // sees the absolute-latest entry — the React-state version trails the
  // jotai atom by one render in the worst case.
  const lastSelfWriteMapRef = React.useRef(lastSelfWriteMap)
  React.useEffect(() => {
    lastSelfWriteMapRef.current = lastSelfWriteMap
  }, [lastSelfWriteMap])

  const handleSave = React.useCallback(
    async (latest: string): Promise<SaveOutcome> => {
      setSaving(true)
      try {
        // Read mtime from the ref (fresh) instead of useState closure.
        const expected = mtimeMsRef.current === 0 ? NEW_FILE_MTIME_SENTINEL : mtimeMsRef.current
        const result = await invoke<WriteResultIpc>('preview_write_text', {
          mountId: target.mountId,
          relPath: target.relPath,
          sessionId: target.sessionId ?? null,
          content: latest,
          expectedMtimeMs: expected,
        })
        if (result.kind === 'saved') {
          const newMtime = result.mtimeMs ?? 0
          setMtimeBoth(newMtime)
          setBaseline(latest)
          recordSelfWrite({ filePath, mtimeMs: newMtime })
          return { kind: 'saved', mtimeMs: newMtime }
        }
        if (result.kind === 'conflict') {
          // Self-write echo guard: if the "external" mtime the backend
          // reported equals one we just wrote ourselves, this isn't a
          // real conflict — it's the editor's own previous save round-
          // tripping back through a stale cached mtime. Resync silently.
          const externalMtime = result.currentMtimeMs ?? 0
          const lastSelf = lastSelfWriteMapRef.current.get(filePath)
          if (lastSelf !== undefined && lastSelf === externalMtime) {
            setMtimeBoth(externalMtime)
            // Retry the save with the fresh mtime so the user's edit lands.
            const retry = await invoke<WriteResultIpc>('preview_write_text', {
              mountId: target.mountId,
              relPath: target.relPath,
              sessionId: target.sessionId ?? null,
              content: latest,
              expectedMtimeMs: externalMtime,
            })
            if (retry.kind === 'saved') {
              const retryMtime = retry.mtimeMs ?? 0
              setMtimeBoth(retryMtime)
              setBaseline(latest)
              recordSelfWrite({ filePath, mtimeMs: retryMtime })
              return { kind: 'saved', mtimeMs: retryMtime }
            }
            // If retry also conflicts, fall through to surface the banner.
          }
          setConflict({
            filePath,
            conflict: {
              externalContent: result.currentContent ?? '',
              externalMtimeMs: externalMtime,
            },
          })
          return {
            kind: 'conflict',
            externalContent: result.currentContent ?? '',
            externalMtimeMs: externalMtime,
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
    // mtimeMs / lastSelfWriteMap removed from deps — they're read via
    // refs above, so handleSave can stay STABLE across saves. Without
    // this stability, `onSaveRef.current` updates raced React commits
    // and led to phantom "file changed on disk" warnings on the second
    // save attempt, even when only the editor was writing.
    [filePath, target, setConflict, recordSelfWrite, setMtimeBoth],
  )

  const handleOverwrite = React.useCallback(async () => {
    // The user clicked 覆盖 — pass FORCE_OVERWRITE_SENTINEL so the
    // backend skips the optimistic mtime check. Earlier versions tried
    // to compute the right `expected` (editor mtime, then conflict's
    // external mtime); both still raced against in-flight writes from
    // concurrent auto-saves and looped right back into another conflict.
    // Force-overwrite removes the loop.
    setSaving(true)
    try {
      const result = await invoke<WriteResultIpc>('preview_write_text', {
        mountId: target.mountId,
        relPath: target.relPath,
        sessionId: target.sessionId ?? null,
        content,
        expectedMtimeMs: FORCE_OVERWRITE_SENTINEL,
      })
      if (result.kind === 'saved') {
        const newMtime = result.mtimeMs ?? 0
        setMtimeBoth(newMtime)
        setBaseline(content)
        recordSelfWrite({ filePath, mtimeMs: newMtime })
        clearConflict(filePath)
      } else if (result.kind === 'conflict') {
        // Force-overwrite shouldn't return Conflict (sentinel skips the
        // check) — surface anything that does as a refresh of the banner.
        setConflict({
          filePath,
          conflict: {
            externalContent: result.currentContent ?? '',
            externalMtimeMs: result.currentMtimeMs ?? 0,
          },
        })
      }
    } catch (err) {
      console.error('[preview] overwrite failed', err)
    } finally {
      setSaving(false)
    }
  }, [filePath, content, target, recordSelfWrite, clearConflict, setConflict])

  const handleDiscard = React.useCallback((externalContent: string, externalMtimeMs: number) => {
    setBaseline(externalContent)  // also update baseline on discard
    setContent(externalContent)
    setMtimeBoth(externalMtimeMs)  // keep ref + state in sync so the next save reads the discarded mtime
    // Editor will re-mount with new initialContent via the key prop below.
  }, [setMtimeBoth])

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
          initialContent={baseline}
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
