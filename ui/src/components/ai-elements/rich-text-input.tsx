/**
 * RichTextInput — composer-level rich-text editor for chat + agent modes.
 *
 * Scope (post-2026-05-13 TipTap-chip-container port): paragraphs +
 * mention chips + paste hooks. Bold/italic/markdown formatting are
 * **out of scope** — this is a chip container, not a Notion clone.
 *
 * Spec: docs/superpowers/specs/2026-05-13-composer-tiptap-chip-design.md
 *
 * Wire-format contract: the `onChange(value: string)` callback emits the
 * plain-text serialization a textarea would have produced — chips are
 * flattened to their `/<name>` / `@<absPath>` inline form by
 * `serializeDocToWireText`. Backend sees identical strings to pre-TipTap
 * uClaw, so `send_agent_message` + `agent_messages.content TEXT` work
 * unchanged. Chip atomicity is purely a UI sugar on top.
 */
import * as React from 'react'
import { useEditor, EditorContent } from '@tiptap/react'
import type { Editor } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import { MentionChipNode } from '@/components/composer/MentionChipNode'
import { serializeDocToWireText } from '@/components/composer/composer-serialize'

interface RichTextInputProps {
  value: string
  onChange: (value: string) => void
  onSubmit: () => void
  onPasteFiles?: (files: File[]) => void
  /** Called when pasted plain text is >= longTextPasteThreshold. Receives the text. */
  onPasteLongText?: (text: string) => void
  /** Override the default threshold for onPasteLongText. Defaults to 500. */
  longTextPasteThreshold?: number
  placeholder?: string
  disabled?: boolean
  autoFocusTrigger?: string
  collapsible?: boolean
  workspacePath?: string | null
  workspaceSlug?: string | null
  attachedDirs?: string[]
  htmlValue?: string
  onHtmlChange?: (html: string) => void
  sendWithCmdEnter?: boolean
  /** Expose the TipTap Editor instance to the parent. Replaces the
   *  pre-TipTap `textareaRef` — `ComposerMentionController` reads the
   *  editor's selection state through this. */
  editorRef?: React.MutableRefObject<Editor | null>
  /** Pre-handler invoked before the built-in submit/newline keymap.
   *  Returning `true` consumes the event; the built-in handlers are
   *  skipped. Drives `/` and `@` popup keyboard nav. */
  onKeyDownIntercept?: (e: React.KeyboardEvent<HTMLDivElement>) => boolean
  /** Called when the editor gains focus. Used by PetWidget for composer state. */
  onFocus?: () => void
  /** Called when the editor loses focus. Used by PetWidget for composer state. */
  onBlur?: () => void
}

export function RichTextInput({
  value,
  onChange,
  onSubmit,
  onPasteFiles,
  onPasteLongText,
  longTextPasteThreshold = 500,
  placeholder,
  disabled,
  sendWithCmdEnter,
  editorRef,
  onKeyDownIntercept,
  onFocus,
  onBlur,
}: RichTextInputProps): React.ReactElement {
  // Keep callback refs so the editor's once-created handlers see the
  // latest props without re-creating the editor (which would lose
  // focus + history). Standard TipTap idiom.
  const onSubmitRef = React.useRef(onSubmit)
  const onChangeRef = React.useRef(onChange)
  const onPasteFilesRef = React.useRef(onPasteFiles)
  const onPasteLongTextRef = React.useRef(onPasteLongText)
  const longTextThresholdRef = React.useRef(longTextPasteThreshold)
  const sendWithCmdEnterRef = React.useRef(sendWithCmdEnter)
  const onFocusRef = React.useRef(onFocus)
  const onBlurRef = React.useRef(onBlur)
  React.useEffect(() => { onSubmitRef.current = onSubmit }, [onSubmit])
  React.useEffect(() => { onChangeRef.current = onChange }, [onChange])
  React.useEffect(() => { onPasteFilesRef.current = onPasteFiles }, [onPasteFiles])
  React.useEffect(() => { onPasteLongTextRef.current = onPasteLongText }, [onPasteLongText])
  React.useEffect(() => { longTextThresholdRef.current = longTextPasteThreshold }, [longTextPasteThreshold])
  React.useEffect(() => { sendWithCmdEnterRef.current = sendWithCmdEnter }, [sendWithCmdEnter])
  React.useEffect(() => { onFocusRef.current = onFocus }, [onFocus])
  React.useEffect(() => { onBlurRef.current = onBlur }, [onBlur])

  const editor = useEditor({
    extensions: [
      // Aggressively trim StarterKit — chat messages don't need formatting.
      // Keeps: Document, Paragraph, Text, HardBreak, History, Dropcursor,
      // Gapcursor (the structural minimum + undo).
      StarterKit.configure({
        heading: false,
        bold: false,
        italic: false,
        strike: false,
        blockquote: false,
        code: false,
        codeBlock: false,
        horizontalRule: false,
        bulletList: false,
        orderedList: false,
        listItem: false,
        link: false,
      }),
      Placeholder.configure({
        placeholder: ({ editor: ed }) => (ed.isEmpty ? (placeholder ?? '') : ''),
        showOnlyWhenEditable: true,
      }),
      MentionChipNode,
    ],
    // Initial content: plain text from the caller's `value` (drafts are
    // strings, see spec §"Migration to draft strings"). Chips appear
    // only on fresh popup selection — we deliberately don't auto-chipify
    // hydrated draft text.
    content: value,
    editable: !disabled,
    immediatelyRender: false,
    autofocus: false,
    onUpdate: ({ editor: ed }) => {
      const text = serializeDocToWireText(ed.getJSON())
      onChangeRef.current(text)
    },
    onFocus: () => { onFocusRef.current?.() },
    onBlur: () => { onBlurRef.current?.() },
    editorProps: {
      attributes: {
        // Match the pre-TipTap textarea visual rhythm: same paddings,
        // same min/max height, same outline behavior. Theme tokens so
        // the 11-theme palette still applies.
        class: 'w-full bg-transparent px-3 py-2 text-sm outline-none min-h-[44px] max-h-[200px] overflow-y-auto',
      },
      handleKeyDown: (_view, ev) => {
        // Submit shortcuts. The popup's intercept runs at the React
        // synthetic-event level above (onKeyDownCapture on the wrapper
        // div) — by the time we get here, the popup has declined.
        if (ev.key === 'Enter') {
          if (sendWithCmdEnterRef.current) {
            if (ev.metaKey || ev.ctrlKey) {
              ev.preventDefault()
              onSubmitRef.current()
              return true
            }
            // Plain Enter → newline (ProseMirror default)
            return false
          }
          // Shift+Enter → newline; bare Enter → submit
          if (!ev.shiftKey) {
            ev.preventDefault()
            onSubmitRef.current()
            return true
          }
          return false
        }
        return false
      },
      handlePaste: (_view, ev) => {
        const files = Array.from(ev.clipboardData?.files ?? [])
        if (files.length > 0 && onPasteFilesRef.current) {
          ev.preventDefault()
          onPasteFilesRef.current(files)
          return true
        }
        const text = ev.clipboardData?.getData('text/plain') ?? ''
        if (text.length >= longTextThresholdRef.current && onPasteLongTextRef.current) {
          ev.preventDefault()
          onPasteLongTextRef.current(text)
          return true
        }
        // Fall through to default plain-text paste (TipTap inserts it
        // as a Text node automatically).
        return false
      },
    },
  })

  // Expose the editor instance through the parent's ref.
  React.useEffect(() => {
    if (editorRef) editorRef.current = editor
    return () => {
      if (editorRef) editorRef.current = null
    }
  }, [editor, editorRef])

  // External `value` change → reset editor content. Only fires when the
  // caller's value diverges from what the editor would serialize (e.g.
  // draft cleared after send, or a different session loaded). Without
  // this guard, every `onUpdate → onChange → re-render` cycle would
  // clobber mid-IME state.
  React.useEffect(() => {
    if (!editor) return
    const current = serializeDocToWireText(editor.getJSON())
    if (current !== value) {
      editor.commands.setContent(value)
    }
  }, [value, editor])

  // Sync the editable state with the disabled prop.
  React.useEffect(() => {
    if (!editor) return
    editor.setEditable(!disabled)
  }, [disabled, editor])

  return (
    <div onKeyDownCapture={(e) => {
      if (onKeyDownIntercept?.(e)) {
        // The popup consumed this event — stop ProseMirror from
        // also processing it (e.g. Enter inserting a hard break).
        e.preventDefault()
        e.stopPropagation()
      }
    }}>
      <EditorContent editor={editor} />
    </div>
  )
}
