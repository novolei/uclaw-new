// [PLACEHOLDER] ai-elements/rich-text-input — paste hooks wired in W1.
// A real TipTap port lives in W4's Preview Engine; for now this stays
// a thin textarea that nonetheless honors onPasteFiles + onPasteLongText.
//
// 2026-05-13: extended with `textareaRef` + `onKeyDownIntercept` so the
// composer's `<ComposerMentionController>` can drive `/` and `@` autocomplete
// without owning the textarea itself. When the real TipTap port replaces
// this file, it should expose equivalent surface: an editor ref + a
// pre-handler that returns `true` to consume the key event.
import * as React from 'react'

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
  /** Forward ref to the underlying <textarea>. Required by composer-level
   *  features (ComposerMentionController) that need direct DOM access for
   *  caret tracking + popup anchoring. */
  textareaRef?: React.Ref<HTMLTextAreaElement>
  /** Pre-handler invoked before the built-in keyboard logic. Returning
   *  `true` signals the event was consumed; the built-in submit shortcut
   *  is then skipped. Drives the `/` and `@` popup keyboard nav. */
  onKeyDownIntercept?: (e: React.KeyboardEvent<HTMLTextAreaElement>) => boolean
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
  textareaRef,
  onKeyDownIntercept,
}: RichTextInputProps): React.ReactElement {
  const handleKeyDown = React.useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Intercept first — mention popup needs ArrowUp/Down/Enter/Tab/Esc
      // before the textarea-level Enter-to-submit. Returning true means
      // the event is fully handled and we must NOT also submit.
      if (onKeyDownIntercept?.(e)) return

      if (sendWithCmdEnter) {
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
          e.preventDefault()
          onSubmit()
        }
      } else {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault()
          onSubmit()
        }
      }
    },
    [onSubmit, sendWithCmdEnter, onKeyDownIntercept],
  )

  const handlePaste = React.useCallback(
    (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const files = Array.from(e.clipboardData?.files ?? [])
      if (files.length > 0 && onPasteFiles) {
        e.preventDefault()
        onPasteFiles(files)
        return
      }
      const text = e.clipboardData?.getData('text/plain') ?? ''
      if (text.length >= longTextPasteThreshold && onPasteLongText) {
        e.preventDefault()
        onPasteLongText(text)
        return
      }
      // fall through to default paste
    },
    [onPasteFiles, onPasteLongText, longTextPasteThreshold],
  )

  return (
    <textarea
      ref={textareaRef}
      className="w-full resize-none bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground/50 min-h-[44px] max-h-[200px]"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={handleKeyDown}
      onPaste={handlePaste}
      placeholder={placeholder}
      disabled={disabled}
      rows={1}
    />
  )
}
