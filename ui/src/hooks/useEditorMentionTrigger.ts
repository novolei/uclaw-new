/**
 * useEditorMentionTrigger — TipTap port of the pre-PR #130
 * `useComposerMentionTrigger`. Same state machine, same external contract,
 * but operates on the ProseMirror document instead of a textarea string +
 * selectionStart.
 *
 * Detection rules (preserved from PR #130 for behavioral continuity):
 *   - Trigger char fires only at start-of-document OR after whitespace
 *     (avoids `email@x.com`, `src/foo`).
 *   - Active state ends on whitespace inside the query, Esc (caller-driven),
 *     cursor moving before the trigger position, or the trigger char being
 *     deleted.
 *   - Esc explicitly dismisses the trigger at that position; the popup stays
 *     closed until the trigger position changes (different start or char).
 *
 * What changes vs the textarea version:
 *   - Reads `editor.state.selection.from` (a ProseMirror integer position)
 *     instead of `textareaRef.current.selectionStart` (DOM offset).
 *   - Reads text via `editor.state.doc.textBetween(0, cursor, '\n', '\0')` —
 *     the `\0` placeholder for leaf nodes (mention chips) prevents the
 *     trigger from being detected INSIDE a chip's wire-format text. A chip's
 *     `/<name>` payload would otherwise be misread as a real trigger.
 *
 * The popup component (ComposerMentionPopup) doesn't change.
 */
import * as React from 'react'
import type { Editor } from '@tiptap/core'

export type MentionTriggerChar = '/' | '@'

export interface MentionTrigger {
  char: MentionTriggerChar
  /** ProseMirror position of the trigger char. The chip-insert command
   *  uses `{ from: triggerStart, to: cursorPos }` to wipe the trigger +
   *  query span before inserting the chip. */
  triggerStart: number
  /** ProseMirror position of the cursor (exclusive end of query span). */
  cursorPos: number
  /** Text the user has typed after the trigger char. Empty string is
   *  valid — the popup opens immediately on `/` or `@`. */
  query: string
}

interface Options {
  editor: Editor | null
  chars?: readonly MentionTriggerChar[]
  onChange?: (trigger: MentionTrigger | null) => void
}

interface Result {
  trigger: MentionTrigger | null
  /** Imperatively close the popup; stays closed at this trigger position
   *  until the position changes (user moves to a different `/` or `@`). */
  close: () => void
}

/** Placeholder character used to flatten leaf nodes (mention chips) when
 *  reading text out of the doc. \0 is unlikely to appear in real input
 *  and we explicitly bail when we encounter it during the backward walk. */
const LEAF_PLACEHOLDER = '\0'

function isTriggerBoundary(text: string, triggerIdx: number): boolean {
  if (triggerIdx === 0) return true
  const prev = text[triggerIdx - 1]
  return /\s/.test(prev)
}

export function useEditorMentionTrigger({
  editor,
  chars = ['/', '@'] as const,
  onChange,
}: Options): Result {
  const [trigger, setTrigger] = React.useState<MentionTrigger | null>(null)
  const [dismissed, setDismissed] = React.useState<{ char: MentionTriggerChar; triggerStart: number } | null>(null)
  const onChangeRef = React.useRef(onChange)
  React.useEffect(() => { onChangeRef.current = onChange }, [onChange])

  React.useEffect(() => {
    if (!editor) return

    const recompute = (): void => {
      const { selection, doc } = editor.state
      const cursor = selection.from
      // Read text from doc-start to cursor, flattening leaf nodes (chips)
      // to a single \0 sentinel. This way we never see a chip's internal
      // wire-format `/skill-name` and false-trigger inside it.
      const text = doc.textBetween(0, cursor, '\n', LEAF_PLACEHOLDER)

      // Walk back to find the nearest trigger char. Stop on whitespace OR
      // a chip-placeholder (= we're "after" a previous chip; a trigger
      // inside that span is not addressable).
      let foundIdx = -1
      let foundChar: MentionTriggerChar | null = null
      for (let i = text.length - 1; i >= 0; i--) {
        const c = text[i]
        if (c === LEAF_PLACEHOLDER) break
        if (/\s/.test(c)) break
        if ((chars as readonly string[]).includes(c)) {
          foundIdx = i
          foundChar = c as MentionTriggerChar
          break
        }
      }

      if (foundIdx < 0 || foundChar == null) {
        if (trigger != null) {
          setTrigger(null)
          onChangeRef.current?.(null)
        }
        if (dismissed != null) setDismissed(null)
        return
      }
      if (!isTriggerBoundary(text, foundIdx)) {
        if (trigger != null) {
          setTrigger(null)
          onChangeRef.current?.(null)
        }
        return
      }

      // Translate text-offset back to a ProseMirror position. text was
      // produced with leaf-placeholder=1-char, hardBreak='\n', so the
      // text length matches the doc span exactly: triggerStart in text
      // === triggerStart in PM coordinates measured from doc start.
      // BUT: doc positions are 1-indexed (position 0 is the position
      // *before* the doc's first node). For a typical paragraph-doc the
      // first text char is at PM position 1, so we add 1.
      const triggerStartPM = foundIdx + 1

      // Honor explicit dismissal.
      if (
        dismissed
        && dismissed.char === foundChar
        && dismissed.triggerStart === triggerStartPM
      ) {
        if (trigger != null) {
          setTrigger(null)
          onChangeRef.current?.(null)
        }
        return
      }
      if (
        dismissed
        && (dismissed.char !== foundChar || dismissed.triggerStart !== triggerStartPM)
      ) {
        setDismissed(null)
      }

      const next: MentionTrigger = {
        char: foundChar,
        triggerStart: triggerStartPM,
        cursorPos: cursor,
        query: text.slice(foundIdx + 1),
      }
      if (
        trigger == null
        || trigger.char !== next.char
        || trigger.triggerStart !== next.triggerStart
        || trigger.cursorPos !== next.cursorPos
        || trigger.query !== next.query
      ) {
        setTrigger(next)
        onChangeRef.current?.(next)
      }
    }

    // Recompute on every selection / doc change. TipTap fires both as
    // unified `transaction`/`selectionUpdate`/`update` events.
    editor.on('transaction', recompute)
    editor.on('selectionUpdate', recompute)
    recompute() // initial
    return () => {
      editor.off('transaction', recompute)
      editor.off('selectionUpdate', recompute)
    }
  }, [editor, chars, trigger, dismissed])

  const close = React.useCallback(() => {
    if (trigger != null) {
      setDismissed({ char: trigger.char, triggerStart: trigger.triggerStart })
      setTrigger(null)
      onChangeRef.current?.(null)
    }
  }, [trigger])

  return { trigger, close }
}
