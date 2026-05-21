/**
 * ComposerMentionController — glues `useEditorMentionTrigger` to the data
 * sources (skills + files) and renders the popup.
 *
 * 2026-05-13 TipTap port: editor-instance-driven instead of textarea-ref-
 * driven. The popup component (ComposerMentionPopup), data fetching, and
 * keyboard intercept all stay the same shape — only the commit path
 * changes from a string-splice to a `insertMentionChip` TipTap command.
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import type { Editor } from '@tiptap/core'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { listInvocableSkills, searchWorkspaceFilesForMention } from '@/lib/tauri-bridge'
import type { InvocableSkill, WorkspaceFileMatch } from '@/lib/types'
import { useEditorMentionTrigger } from '@/hooks/useEditorMentionTrigger'
import { ComposerMentionPopup } from './ComposerMentionPopup'
import type { MentionChipKind } from './MentionChipNode'
import { Sparkles, FileText, AlertTriangle, Layers } from 'lucide-react'
import { cn } from '@/lib/utils'

/**
 * Built-in slash commands that aren't backed by an InvocableSkill but
 * should still be discoverable via the `/` autocompletion popup.
 *
 * Each entry has a fixed `name` (matched against the `/foo` literal in
 * handleSend), a short `description`, and a Lucide icon component.
 * Adding more entries here surfaces them in the popup with zero
 * additional plumbing — `commitBuiltin` handles the text insert
 * uniformly.
 */
const BUILTIN_SLASH_COMMANDS = [
  {
    name: 'compact',
    description: '压缩历史对话为结构化摘要（M2-G StructuredFold）以节省 token',
    aliases: ['compact', '压缩', '归纳'],
  },
] as const
type BuiltinCommand = (typeof BUILTIN_SLASH_COMMANDS)[number]

function matchBuiltinCommands(query: string): BuiltinCommand[] {
  const q = query.toLowerCase().trim()
  if (!q) return [...BUILTIN_SLASH_COMMANDS]
  return BUILTIN_SLASH_COMMANDS.filter((cmd) =>
    cmd.aliases.some((alias) => alias.toLowerCase().includes(q)),
  )
}

interface Props {
  /** Ref to the TipTap editor this controller drives. Replaces the
   *  pre-TipTap `textareaRef` — the trigger detection now reads
   *  `editor.state.selection.from` instead of `textarea.selectionStart`. */
  editorRef: React.MutableRefObject<Editor | null>
  /** Current serialized value (unchanged role — `setValue` still gets
   *  called when the editor emits onUpdate, threaded through props). */
  value: string
  /** Setter for the serialized value (still string-typed — chips
   *  serialize back to their inline form). */
  setValue: (v: string) => void
  /** The agent session id — required to resolve workspace root +
   *  attached_dirs on the backend for the `@` file search. */
  sessionId: string | null
  /** If true, controller is disabled — popup never opens. Used when
   *  the editor itself is disabled (no model selected, mid-stream
   *  with no interrupt, etc.). */
  disabled?: boolean
}

export interface ComposerMentionControllerHandle {
  /** Returns true if the keyboard event was consumed by the popup.
   *  Caller's onKeyDownIntercept must return this. */
  handleKeyDown: (e: React.KeyboardEvent<HTMLElement>) => boolean
}

type Row =
  | { kind: 'skill'; data: InvocableSkill }
  | { kind: 'file'; data: WorkspaceFileMatch }
  | { kind: 'builtin'; data: BuiltinCommand }

export const ComposerMentionController = React.forwardRef<
  ComposerMentionControllerHandle,
  Props
>(function ComposerMentionController(
  { editorRef, value: _value, setValue: _setValue, sessionId, disabled },
  ref,
) {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  // Trigger detection is scoped to the current editor instance. The
  // hook reads ProseMirror positions through the editor's state.
  // We rebind whenever the editor instance changes (e.g. on first
  // mount when useEditor returns null then the real instance).
  const [editor, setEditor] = React.useState<Editor | null>(editorRef.current)
  React.useEffect(() => {
    // Poll the ref one frame after mount — useEditor's instance becomes
    // available after RichTextInput's useEffect runs. Cheaper than a
    // mutation observer; identical result.
    const id = requestAnimationFrame(() => setEditor(editorRef.current))
    return () => cancelAnimationFrame(id)
  })

  const { trigger, close } = useEditorMentionTrigger({ editor })

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
          // Built-in commands (e.g. /compact) are always candidates and
          // sit at the TOP of the result list — they don't depend on the
          // backend Skill registry and aren't subject to lifecycle
          // checks. Skills come below.
          const builtins = matchBuiltinCommands(trigger.query)
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
          const rows: Row[] = [
            ...builtins.map((b) => ({ kind: 'builtin' as const, data: b })),
            ...filtered.slice(0, 30).map((s) => ({ kind: 'skill' as const, data: s })),
          ]
          setRows(rows)
          setSelectedIndex(0)
        } else {
          // '@' — files + learned-skill fallback (the CJK-title fallback
          // selected in the PR #130 scope discussion).
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
          const matchedSkills = (
            q
              ? skills.filter(
                  (s) =>
                    s.provenance === 'learned'
                    && (s.name.toLowerCase().includes(q)
                      || s.description.toLowerCase().includes(q)),
                )
              : skills.filter((s) => s.provenance === 'learned')
          ).slice(0, 5)
          const combined: Row[] = [
            ...files.map((f) => ({ kind: 'file' as const, data: f })),
            ...matchedSkills.map((s) => ({ kind: 'skill' as const, data: s })),
          ]
          setRows(combined)
          setSelectedIndex(0)
        }
      } catch (err) {
        if (seq !== fetchSeqRef.current) return
        console.warn('[composer-mention] fetch failed:', err)
        setRows([])
      }
    })()
  }, [trigger, disabled, sessionId, activeWorkspaceId])

  const open = !disabled && trigger != null
  const hasRows = rows.length > 0

  // Commit: dispatch TipTap's insertMentionChip command for
  // skill/file rows. Builtin commands (e.g. /compact) commit as plain
  // text because they're intercepted by handleSend's slash-command
  // dispatcher, not by the skill mention chip path.
  const commitRow = React.useCallback(
    (row: Row) => {
      const ed = editorRef.current
      if (!ed || !trigger) return

      if (row.kind === 'builtin') {
        // Replace `/<query>` with `/<name> ` (trailing space so the
        // composer feels ready for follow-up text or Enter-to-send).
        // The mention chip path doesn't apply — handleSend checks the
        // raw text for `/<name>` and intercepts there.
        ed
          .chain()
          .focus()
          .deleteRange({ from: trigger.triggerStart, to: trigger.cursorPos })
          .insertContent(`/${row.data.name} `)
          .run()
        return
      }

      const kind: MentionChipKind = row.kind === 'skill' ? 'skill' : 'file'
      const display = row.kind === 'skill' ? row.data.name : row.data.name
      const value = row.kind === 'skill' ? row.data.name : row.data.absolutePath
      ed.commands.insertMentionChip({
        kind,
        display,
        value,
        from: trigger.triggerStart,
        to: trigger.cursorPos,
      })
      ed.commands.focus()
    },
    [editorRef, trigger],
  )

  React.useImperativeHandle(
    ref,
    () => ({
      handleKeyDown: (e: React.KeyboardEvent<HTMLElement>): boolean => {
        if (!open) return false
        if (e.key === 'Escape') {
          e.preventDefault()
          close()
          return true
        }
        if (!hasRows) {
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
        r.kind === 'skill'
          ? `s:${r.data.name}`
          : r.kind === 'file'
            ? `f:${r.data.absolutePath}`
            : `b:${r.data.name}`}
      renderItem={(r, isSelected) =>
        r.kind === 'skill' ? (
          <SkillRow skill={r.data} isSelected={isSelected} />
        ) : r.kind === 'file' ? (
          <FileRow file={r.data} isSelected={isSelected} />
        ) : (
          <BuiltinRow cmd={r.data} isSelected={isSelected} />
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

function BuiltinRow({
  cmd,
  isSelected: _isSelected,
}: {
  cmd: BuiltinCommand
  isSelected: boolean
}): React.ReactElement {
  return (
    <>
      <Layers className="size-3.5 flex-shrink-0 mt-0.5 text-emerald-500/85" />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-xs font-medium truncate">/{cmd.name}</span>
          <span className="text-[9px] px-1 py-px rounded border bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 border-emerald-500/30 flex-shrink-0">
            内置
          </span>
        </div>
        <div className="text-[10px] text-muted-foreground/70 mt-0.5 line-clamp-1">
          {cmd.description}
        </div>
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
