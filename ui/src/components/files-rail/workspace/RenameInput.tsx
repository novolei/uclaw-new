/**
 * RenameInput — inline rename field for files-rail rows.
 *
 * Replaces the row's filename <span> while active. Synchronous
 * validation (empty / separator chars / duplicate sibling) shows an
 * inline error; Enter commits when valid, Escape cancels, blur
 * commits-or-cancels based on error state.
 */

import * as React from 'react'
import { cn } from '@/lib/utils'

interface Props {
  initialName: string
  /** Names of every sibling at the same depth (used to detect dup). */
  siblings: Set<string>
  onCommit: (newName: string) => void
  onCancel: () => void
}

const SEPARATOR_CHARS = /[/\\:]/

function validate(value: string, initialName: string, siblings: Set<string>): string | null {
  const trimmed = value.trim()
  if (trimmed.length === 0) return '名称不能为空'
  if (SEPARATOR_CHARS.test(trimmed)) return '名称不能包含 / \\ :'
  if (trimmed !== initialName && siblings.has(trimmed)) return '已存在同名文件'
  return null
}

export function RenameInput({ initialName, siblings, onCommit, onCancel }: Props): React.ReactElement {
  const [value, setValue] = React.useState(initialName)
  const [error, setError] = React.useState<string | null>(null)
  const inputRef = React.useRef<HTMLInputElement>(null)

  // Auto-focus + select basename (preserve extension).
  React.useEffect(() => {
    const el = inputRef.current
    if (!el) return
    el.focus()
    const dot = initialName.lastIndexOf('.')
    if (dot > 0) {
      el.setSelectionRange(0, dot)
    } else {
      el.select()
    }
  }, [initialName])

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
    const next = e.target.value
    setValue(next)
    setError(validate(next, initialName, siblings))
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === 'Enter') {
      e.preventDefault()
      const err = validate(value, initialName, siblings)
      if (err) {
        setError(err)
        return
      }
      onCommit(value.trim())
    } else if (e.key === 'Escape') {
      e.preventDefault()
      onCancel()
    }
  }

  const handleBlur = (): void => {
    const err = validate(value, initialName, siblings)
    if (err) {
      onCancel()
    } else {
      onCommit(value.trim())
    }
  }

  const errorId = 'rename-input-error'

  return (
    <div className="flex-1 min-w-0">
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onBlur={handleBlur}
        onClick={(e) => e.stopPropagation()}
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? errorId : undefined}
        className={cn(
          'w-full bg-transparent text-[12px] border-b outline-none py-0.5 px-0',
          error ? 'border-destructive' : 'border-primary/50',
        )}
        maxLength={255}
      />
      {error && (
        <div
          id={errorId}
          role="alert"
          aria-live="polite"
          aria-atomic="true"
          className="text-[10px] text-destructive mt-0.5 truncate"
        >
          {error}
        </div>
      )}
    </div>
  )
}
