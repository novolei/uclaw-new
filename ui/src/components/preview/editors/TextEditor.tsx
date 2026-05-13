/**
 * TextEditor — CodeMirror 6 host for plain text + code formats.
 *
 * Common `EditorProps` API shared with MarkdownRichEditor:
 *   - initialContent / language / mtimeMs / filePath / saveMode
 *   - onSave(content) → SaveOutcome
 *   - onContentChange(content, isDirty) — invoked on every keystroke
 *
 * Save trigger:
 *   - saveMode === 'explicit': Cmd-S / Ctrl-S triggers onSave
 *   - saveMode === 'auto': 300 ms debounced auto-save
 *
 * Dirty tracking lives in EditorSurface via useDirtyBuffer — this
 * component only emits content changes through `onContentChange`.
 */

import * as React from 'react'
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view'
import { EditorState, Compartment } from '@codemirror/state'
import { defaultKeymap, historyKeymap, history } from '@codemirror/commands'
import { uclawCmTheme, uclawSyntaxHighlight } from './codemirror-theme'
import { loadLanguage } from './codemirror-langs'

export type SaveOutcome =
  | { kind: 'saved'; mtimeMs: number }
  | { kind: 'needs-approval'; approvalId: string }
  | { kind: 'error'; message: string }

export interface EditorProps {
  initialContent: string
  language?: string
  mtimeMs: number
  filePath: string
  saveMode: 'explicit' | 'auto'
  onSave: (content: string) => Promise<SaveOutcome>
  onContentChange?: (content: string, isDirty: boolean) => void
  readOnly?: boolean
}

const AUTO_SAVE_DEBOUNCE_MS = 300

export function TextEditor(props: EditorProps): React.ReactElement {
  const {
    initialContent,
    language = 'text',
    mtimeMs,
    filePath,
    saveMode,
    onSave,
    onContentChange,
    readOnly,
  } = props

  const containerRef = React.useRef<HTMLDivElement>(null)
  const viewRef = React.useRef<EditorView | null>(null)
  const langCompartment = React.useRef(new Compartment())
  const [currentContent, setCurrentContent] = React.useState<string>(initialContent)
  void mtimeMs // baseline mtime tracked by parent's useDirtyBuffer; prop retained for future use

  // Auto-save debounce timer
  const autoSaveTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // Save handler (called by Cmd-S or auto-debounce). Keeps a stable
  // closure via refs so the listener doesn't capture stale `onSave`.
  const onSaveRef = React.useRef(onSave)
  React.useEffect(() => {
    onSaveRef.current = onSave
  }, [onSave])

  // In-flight save guard: serializes saves against the same file so the
  // latest typed content always wins. Even without OCC, two parallel
  // IPC calls can return out-of-order; queuing one follow-up keeps the
  // baseline promotion deterministic.
  const savingRef = React.useRef(false)
  const pendingContentRef = React.useRef<string | null>(null)

  const handleSave = React.useCallback(async (): Promise<SaveOutcome> => {
    const content = viewRef.current?.state.doc.toString() ?? ''
    if (savingRef.current) {
      // Already saving — stash the latest snapshot for the loop owner.
      pendingContentRef.current = content
      return { kind: 'saved', mtimeMs: 0 }
    }
    savingRef.current = true
    let lastOutcome: SaveOutcome = { kind: 'saved', mtimeMs: 0 }
    try {
      let toSave: string | null = content
      while (toSave !== null) {
        const cur: string = toSave
        const outcome = await onSaveRef.current(cur)
        lastOutcome = outcome
        if (outcome.kind !== 'saved') break
        const next: string | null = pendingContentRef.current
        pendingContentRef.current = null
        toSave = next !== null && next !== cur ? next : null
      }
    } finally {
      savingRef.current = false
    }
    return lastOutcome
  }, [])

  // Build the initial state ONCE on filePath change (re-mount on file switch)
  React.useEffect(() => {
    if (!containerRef.current) return

    const onChange = EditorView.updateListener.of((u) => {
      if (!u.docChanged) return
      const next = u.state.doc.toString()
      setCurrentContent(next)
      const isDirty = next !== initialContent
      onContentChange?.(next, isDirty)

      if (saveMode === 'auto') {
        if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current)
        autoSaveTimer.current = setTimeout(() => {
          void handleSave()
        }, AUTO_SAVE_DEBOUNCE_MS)
      }
    })

    const saveKey = keymap.of([
      {
        key: 'Mod-s',
        run: () => {
          if (saveMode === 'explicit') {
            void handleSave()
          }
          return true
        },
      },
    ])

    const state = EditorState.create({
      doc: initialContent,
      extensions: [
        lineNumbers(),
        highlightActiveLine(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        saveKey,
        langCompartment.current.of([]),
        uclawCmTheme,
        uclawSyntaxHighlight,
        EditorView.editable.of(!readOnly),
        EditorState.readOnly.of(!!readOnly),
        onChange,
      ],
    })

    const view = new EditorView({ state, parent: containerRef.current })
    viewRef.current = view

    // Lazy-load language and reconfigure
    void loadLanguage(language).then((lang) => {
      if (lang && viewRef.current) {
        viewRef.current.dispatch({
          effects: langCompartment.current.reconfigure(lang),
        })
      }
    })

    return () => {
      if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current)
      view.destroy()
      viewRef.current = null
    }
    // Re-init only on file change. handleSave/onContentChange/etc.
    // are captured via refs to avoid re-mounting on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath])

  // Re-load language when `language` prop changes (e.g. ext changed)
  React.useEffect(() => {
    void loadLanguage(language).then((lang) => {
      if (lang && viewRef.current) {
        viewRef.current.dispatch({
          effects: langCompartment.current.reconfigure(lang),
        })
      }
    })
  }, [language])

  return <div ref={containerRef} className="h-full w-full overflow-hidden" />
}
