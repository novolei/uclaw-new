/**
 * ComposerMentionController — glues `useComposerMentionTrigger` to the
 * data sources (skills + files) and renders the popup. Provides:
 *
 *   - An imperative ref the parent calls in textarea's onKeyDown to
 *     intercept ↑↓ Enter Esc when the popup is open.
 *   - A `commitReplacement(text)` callback the parent triggers when a
 *     row is selected; it splices the textarea value.
 *
 * Why is this a component and not a hook? It needs to render React
 * (the popup itself). A "headless hook + popup component" split would
 * duplicate the items+selectedIndex state across two surfaces. Keeping
 * it bundled here means AgentView + ChatInput each just drop in one
 * component + one ref + one onKeyDown call.
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { listInvocableSkills, searchWorkspaceFilesForMention } from '@/lib/tauri-bridge'
import type { InvocableSkill, WorkspaceFileMatch } from '@/lib/types'
import { useComposerMentionTrigger, type MentionTrigger } from '@/hooks/useComposerMentionTrigger'
import { ComposerMentionPopup } from './ComposerMentionPopup'
import { Sparkles, FileText, AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'

interface Props {
  /** Ref to the textarea this controller drives. */
  textareaRef: React.RefObject<HTMLTextAreaElement | null>
  /** Current textarea value. */
  value: string
  /** Setter for the textarea value (caller's controlled state). */
  setValue: (v: string) => void
  /** The agent session id — required to resolve workspace root +
   *  attached_dirs on the backend for the `@` file search. */
  sessionId: string | null
  /** If true, controller is disabled — popup never opens. Used when
   *  the textarea itself is disabled (no model selected, mid-stream
   *  with no interrupt, etc.). */
  disabled?: boolean
}

export interface ComposerMentionControllerHandle {
  /** Returns true if the keyboard event was consumed by the popup.
   *  Caller's onKeyDown must early-return when true. */
  handleKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => boolean
}

type Row =
  | { kind: 'skill'; data: InvocableSkill }
  | { kind: 'file'; data: WorkspaceFileMatch }

/** Format a skill's slash insertion. Falls back to title-as-is for
 *  learned skills whose title isn't ASCII-slash-able — the backend
 *  resolves Chinese / unicode titles via `normalize_title_for_dedup`
 *  so any title is technically valid; readability is the only concern. */
function skillInsertText(s: InvocableSkill): string {
  return `/${s.name}`
}

/** Format a file insertion. We insert the absolute path so the agent
 *  loop's path-policy can reason about it; the visual representation
 *  in chat (when a file_path chip renderer is wired) can shorten it. */
function fileInsertText(f: WorkspaceFileMatch): string {
  return `@${f.absolutePath}`
}

export const ComposerMentionController = React.forwardRef<
  ComposerMentionControllerHandle,
  Props
>(function ComposerMentionController(
  { textareaRef, value, setValue, sessionId, disabled },
  ref,
) {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const { trigger, close, commitReplacement } = useComposerMentionTrigger({
    textareaRef,
    value,
  })

  const [rows, setRows] = React.useState<Row[]>([])
  const [selectedIndex, setSelectedIndex] = React.useState(0)
  const fetchSeqRef = React.useRef(0)

  // Fetch rows whenever the trigger changes. Each fetch is sequence-
  // tagged so out-of-order responses don't clobber newer queries.
  React.useEffect(() => {
    if (disabled || trigger == null) {
      setRows([])
      setSelectedIndex(0)
      return
    }
    const seq = ++fetchSeqRef.current

    void (async () => {
      try {
        if (trigger.char === '/') {
          const skills = await listInvocableSkills(activeWorkspaceId ?? undefined)
          if (seq !== fetchSeqRef.current) return
          const q = trigger.query.toLowerCase()
          const filtered = q
            ? skills.filter(
                (s) =>
                  s.name.toLowerCase().includes(q)
                  || (s.description?.toLowerCase().includes(q) ?? false),
              )
            : skills
          setRows(filtered.slice(0, 30).map((s) => ({ kind: 'skill' as const, data: s })))
          setSelectedIndex(0)
        } else {
          // '@' — files + learned-skill fallback (the spec'd dual-mode
          // selected by the user). Run both in parallel; files first
          // in the popup since they're the primary use case.
          if (!sessionId) {
            setRows([])
            return
          }
          const [files, skills] = await Promise.all([
            searchWorkspaceFilesForMention(sessionId, trigger.query, 25),
            listInvocableSkills(activeWorkspaceId ?? undefined),
          ])
          if (seq !== fetchSeqRef.current) return
          const q = trigger.query.toLowerCase()
          // Only show learned skills under @ — static/borrowed are
          // already slash-typeable and don't need the fallback.
          const matchedSkills = (
            q
              ? skills.filter(
                  (s) =>
                    s.provenance === 'learned'
                    && (s.name.toLowerCase().includes(q)
                      || s.description.toLowerCase().includes(q)),
                )
              : skills.filter((s) => s.provenance === 'learned')
          ).slice(0, 5) // small slice — files dominate the popup
          const combined: Row[] = [
            ...files.map((f) => ({ kind: 'file' as const, data: f })),
            ...matchedSkills.map((s) => ({ kind: 'skill' as const, data: s })),
          ]
          setRows(combined)
          setSelectedIndex(0)
        }
      } catch (err) {
        if (seq !== fetchSeqRef.current) return
        // Don't toast — the popup just shows empty. Common cause is
        // an in-flight session_id that just got swapped out.
        console.warn('[composer-mention] fetch failed:', err)
        setRows([])
      }
    })()
  }, [trigger, disabled, sessionId, activeWorkspaceId])

  const open = !disabled && trigger != null
  const hasRows = rows.length > 0

  const commitRow = React.useCallback(
    (row: Row) => {
      const insertText
        = row.kind === 'skill' ? skillInsertText(row.data) : fileInsertText(row.data)
      const { newValue, newCursor } = commitReplacement(insertText)
      setValue(newValue)
      // Restore focus + caret on the next tick — setValue is async so
      // we wait for React to flush before touching selectionStart.
      requestAnimationFrame(() => {
        const ta = textareaRef.current
        if (ta) {
          ta.focus()
          ta.setSelectionRange(newCursor, newCursor)
        }
      })
    },
    [commitReplacement, setValue, textareaRef],
  )

  React.useImperativeHandle(
    ref,
    () => ({
      handleKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
        if (!open) return false
        if (e.key === 'Escape') {
          e.preventDefault()
          close()
          return true
        }
        if (!hasRows) {
          // No rows → only Escape is meaningful. Let Tab/Enter pass
          // through so the user can still submit / move focus.
          return false
        }
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setSelectedIndex((i) => (i + 1) % rows.length)
          return true
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setSelectedIndex((i) => (i - 1 + rows.length) % rows.length)
          return true
        }
        if (e.key === 'Enter' || e.key === 'Tab') {
          e.preventDefault()
          const row = rows[selectedIndex]
          if (row) commitRow(row)
          return true
        }
        return false
      },
    }),
    [open, hasRows, rows, selectedIndex, close, commitRow],
  )

  if (!open) return null

  const headerLabel = trigger.char === '/' ? 'Skill (/)' : 'File / Learned skill (@)'
  const emptyText
    = trigger.char === '/'
      ? trigger.query
        ? `没有匹配 "/${trigger.query}" 的 skill`
        : '没有可用的 skill'
      : trigger.query
        ? `没有匹配 "${trigger.query}" 的文件`
        : '开始输入文件名或 skill 名…'

  return (
    <ComposerMentionPopup<Row>
      open={open}
      items={rows}
      selectedIndex={selectedIndex}
      onSelect={commitRow}
      onClose={close}
      headerLabel={headerLabel}
      emptyText={emptyText}
      keyFor={(r) =>
        r.kind === 'skill' ? `s:${r.data.name}` : `f:${r.data.absolutePath}`}
      renderItem={(r, isSelected) =>
        r.kind === 'skill' ? (
          <SkillRow skill={r.data} isSelected={isSelected} />
        ) : (
          <FileRow file={r.data} isSelected={isSelected} />
        )
      }
    />
  )
})

function SkillRow({
  skill,
  isSelected: _isSelected,
}: {
  skill: InvocableSkill
  isSelected: boolean
}): React.ReactElement {
  const draftish = skill.lifecycle === 'draft' || skill.lifecycle === 'deprecated'
  return (
    <>
      <Sparkles
        className={cn(
          'size-3.5 flex-shrink-0 mt-0.5',
          draftish ? 'text-amber-500/70' : 'text-violet-500',
        )}
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-xs font-medium truncate">{skill.name}</span>
          <span className="text-[9px] px-1 py-px rounded border bg-muted/60 text-muted-foreground/70 border-border/50 flex-shrink-0">
            {skill.provenance}
          </span>
          {draftish && (
            <AlertTriangle className="size-2.5 text-amber-500/70 flex-shrink-0" />
          )}
        </div>
        {skill.description && (
          <div className="text-[10px] text-muted-foreground/70 mt-0.5 line-clamp-1">
            {skill.description}
          </div>
        )}
      </div>
    </>
  )
}

function FileRow({
  file,
  isSelected: _isSelected,
}: {
  file: WorkspaceFileMatch
  isSelected: boolean
}): React.ReactElement {
  return (
    <>
      <FileText className="size-3.5 flex-shrink-0 mt-0.5 text-blue-500/80" />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-xs font-medium truncate">{file.name}</span>
          {file.extension && (
            <span className="text-[9px] px-1 py-px rounded bg-muted/60 text-muted-foreground/70 flex-shrink-0">
              {file.extension}
            </span>
          )}
        </div>
        <div className="text-[10px] text-muted-foreground/70 mt-0.5 truncate">
          {file.relativePath}
        </div>
      </div>
    </>
  )
}

function _useTriggerRef(_t: MentionTrigger | null): void {
  // Reserved for future debug instrumentation hook.
}
