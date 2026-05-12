/**
 * useComposerMentionTrigger — detect `/` and `@` trigger characters in
 * a controlled textarea and emit a `MentionTrigger` state machine that
 * the composer renders a popup against.
 *
 * Why a hook (not just inline state in each composer): both AgentView
 * and ChatInput need identical trigger semantics per CLAUDE.md's
 * composer-parity rule. Keeping the state machine here also makes it
 * trivial to swap to TipTap's `Suggestion` plugin later — the popup
 * component consuming `MentionTrigger` won't change.
 *
 * Detection rules (kept small + predictable):
 *   - Trigger char fires only at start-of-string OR after whitespace.
 *     Avoids matching `email@example.com` or `path/to/file`.
 *   - Active state ends on: whitespace typed inside the query, Esc,
 *     cursor moves before the trigger position, or trigger char
 *     itself deleted.
 *   - `commitAndReset` inserts the chosen replacement into the
 *     surrounding text and snaps the cursor to the end of the insert.
 */
import * as React from 'react'

export type MentionTriggerChar = '/' | '@'

export interface MentionTrigger {
  /** Which char fired the trigger. */
  char: MentionTriggerChar
  /** Index of the trigger char in `value`. The query is everything
   *  between `triggerStart + 1` and `cursorPos`. */
  triggerStart: number
  /** Current cursor offset (exclusive end of the query span). */
  cursorPos: number
  /** Text the user has typed after the trigger char, trimmed of nothing
   *  (the popup decides how to interpret whitespace within a query). */
  query: string
}

interface Options {
  /** Required — the textarea ref the hook listens on for caret position. */
  textareaRef: React.RefObject<HTMLTextAreaElement | null>
  /** Current controlled value. The hook re-derives trigger state on
   *  every change rather than tracking it imperatively — cheaper than
   *  it sounds for a textarea, and immune to setState ordering bugs. */
  value: string
  /** Which trigger chars to listen for. Defaults to ['/', '@']. */
  chars?: readonly MentionTriggerChar[]
  /** Called when the trigger state opens, closes, or its query changes.
   *  `null` means closed. */
  onChange?: (trigger: MentionTrigger | null) => void
}

interface Result {
  trigger: MentionTrigger | null
  /** Imperatively close the popup without modifying the textarea (e.g.
   *  on Esc, on focus loss). */
  close: () => void
  /** Replace the trigger span (from `triggerStart` to `cursorPos`) with
   *  `insertText`. Returns the new full value so the caller can pipe it
   *  back through `setValue`. Cursor is positioned at the end of the
   *  insertion + a trailing space. */
  commitReplacement: (insertText: string) => { newValue: string; newCursor: number }
}

/** Boundary check: trigger char must be at start-of-line or preceded by
 *  whitespace. Prevents activation inside paths/emails/identifiers. */
function isTriggerBoundary(value: string, triggerIdx: number): boolean {
  if (triggerIdx === 0) return true
  const prev = value[triggerIdx - 1]
  return /\s/.test(prev)
}

export function useComposerMentionTrigger({
  textareaRef,
  value,
  chars = ['/', '@'] as const,
  onChange,
}: Options): Result {
  const [trigger, setTrigger] = React.useState<MentionTrigger | null>(null)
  // When the user dismisses (Esc) the popup, remember WHICH trigger
  // position they dismissed. The recompute then ignores that exact
  // position so a follow-up keystroke doesn't immediately reopen the
  // popup. Cleared automatically once the trigger position changes
  // (different `triggerStart` or different char) → if the user types
  // a fresh `@` somewhere else, the popup opens again.
  const [dismissed, setDismissed] = React.useState<{ char: MentionTriggerChar; triggerStart: number } | null>(null)
  const onChangeRef = React.useRef(onChange)
  React.useEffect(() => { onChangeRef.current = onChange }, [onChange])

  // Re-derive trigger state from (value, cursorPos) on every render.
  // The textarea's selectionStart is the source of truth — we read it
  // imperatively rather than mirroring it into React state.
  React.useEffect(() => {
    const ta = textareaRef.current
    if (!ta) return

    const recompute = (): void => {
      const cursor = ta.selectionStart ?? 0
      // Walk backwards from cursor to find the nearest unescaped trigger
      // char. Stop on whitespace — a query never spans whitespace.
      let foundIdx = -1
      let foundChar: MentionTriggerChar | null = null
      for (let i = cursor - 1; i >= 0; i--) {
        const c = value[i]
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
        // Clear dismissed when there's no trigger candidate at all.
        if (dismissed != null) setDismissed(null)
        return
      }
      if (!isTriggerBoundary(value, foundIdx)) {
        if (trigger != null) {
          setTrigger(null)
          onChangeRef.current?.(null)
        }
        return
      }
      // Honor an explicit dismissal: while the user is still inside
      // the dismissed trigger's span, suppress the popup. Clear the
      // dismissal once the trigger position moves (different start /
      // different char) so a fresh trigger elsewhere can open.
      if (dismissed && dismissed.char === foundChar && dismissed.triggerStart === foundIdx) {
        if (trigger != null) {
          setTrigger(null)
          onChangeRef.current?.(null)
        }
        return
      }
      if (dismissed && (dismissed.char !== foundChar || dismissed.triggerStart !== foundIdx)) {
        setDismissed(null)
      }
      const next: MentionTrigger = {
        char: foundChar,
        triggerStart: foundIdx,
        cursorPos: cursor,
        query: value.slice(foundIdx + 1, cursor),
      }
      // Only update if anything actually changed — avoids ping-pong renders
      // when the textarea fires `input` events that don't move the trigger.
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

    // Run once on mount + every value change, plus on selection changes
    // (cursor move via arrow keys / click).
    recompute()
    const handler = (): void => recompute()
    ta.addEventListener('keyup', handler)
    ta.addEventListener('click', handler)
    ta.addEventListener('select', handler)
    return () => {
      ta.removeEventListener('keyup', handler)
      ta.removeEventListener('click', handler)
      ta.removeEventListener('select', handler)
    }
  }, [textareaRef, value, chars, trigger, dismissed])

  const close = React.useCallback(() => {
    if (trigger != null) {
      setDismissed({ char: trigger.char, triggerStart: trigger.triggerStart })
      setTrigger(null)
      onChangeRef.current?.(null)
    }
  }, [trigger])

  const commitReplacement = React.useCallback(
    (insertText: string): { newValue: string; newCursor: number } => {
      if (trigger == null) {
        return { newValue: value, newCursor: value.length }
      }
      const before = value.slice(0, trigger.triggerStart)
      const after = value.slice(trigger.cursorPos)
      // Trailing space so the user can immediately keep typing without
      // accidentally re-triggering on the next character.
      const piece = `${insertText} `
      const newValue = `${before}${piece}${after}`
      const newCursor = before.length + piece.length
      // Caller is responsible for setting state; we just compute.
      setTrigger(null)
      onChangeRef.current?.(null)
      return { newValue, newCursor }
    },
    [trigger, value],
  )

  return { trigger, close, commitReplacement }
}
