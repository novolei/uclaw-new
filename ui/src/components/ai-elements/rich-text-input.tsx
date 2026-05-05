// [PLACEHOLDER] ai-elements/rich-text-input — 待后续任务迁移
import * as React from 'react'

interface RichTextInputProps {
  value: string
  onChange: (value: string) => void
  onSubmit: () => void
  onPasteFiles?: (files: File[]) => void
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
}

export function RichTextInput({
  value,
  onChange,
  onSubmit,
  placeholder,
  disabled,
  sendWithCmdEnter,
}: RichTextInputProps): React.ReactElement {
  const handleKeyDown = React.useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
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
    [onSubmit, sendWithCmdEnter],
  )

  return (
    <textarea
      className="w-full resize-none bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground/50 min-h-[44px] max-h-[200px]"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={handleKeyDown}
      placeholder={placeholder}
      disabled={disabled}
      rows={1}
    />
  )
}
