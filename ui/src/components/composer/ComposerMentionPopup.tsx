/**
 * ComposerMentionPopup — dropdown rendered above the composer textarea
 * when `useComposerMentionTrigger` is active. Generic over the row
 * shape; the consumer supplies `items`, `renderItem`, and `onSelect`.
 *
 * Kept deliberately small + standalone so it survives a future TipTap
 * port — TipTap's Suggestion plugin can drive the same component
 * (only the trigger detection layer would be swapped out).
 *
 * Keyboard contract (must match what the textarea's onKeyDown intercepts):
 *   - ArrowUp / ArrowDown — move selectedIndex
 *   - Enter / Tab — select highlighted item
 *   - Escape — close (caller's responsibility — popup just fires onClose)
 */
import * as React from 'react'
import { cn } from '@/lib/utils'

interface Props<T> {
  /** Items to render, already filtered + sorted by the parent. */
  items: T[]
  /** Highlighted index — caller owns this state because the same hook
   *  manages key events on the textarea (popup itself isn't focused). */
  selectedIndex: number
  /** Called when the user clicks a row or presses Enter on it. */
  onSelect: (item: T) => void
  /** Called when the popup wants to close (Esc-style). */
  onClose: () => void
  /** Render one row; the parent's render is what differentiates skill
   *  vs file rows. */
  renderItem: (item: T, isSelected: boolean) => React.ReactNode
  /** Stable key per row. */
  keyFor: (item: T) => string
  /** Text shown when `items` is empty (e.g. "No matches"). */
  emptyText: string
  /** Whether to render at all. Caller can keep the hook open while
   *  still hiding the popup (e.g. mid-fetch). */
  open: boolean
  /** A small label like "Skill" / "File" rendered as the popup's
   *  header chip. Pure cosmetic. */
  headerLabel?: string
}

export function ComposerMentionPopup<T>({
  items,
  selectedIndex,
  onSelect,
  onClose: _onClose,
  renderItem,
  keyFor,
  emptyText,
  open,
  headerLabel,
}: Props<T>): React.ReactElement | null {
  // Auto-scroll the selected row into view. Important when items > visible
  // window (5-6 rows in our default size).
  const listRef = React.useRef<HTMLDivElement | null>(null)
  React.useEffect(() => {
    if (!open) return
    const list = listRef.current
    if (!list) return
    const child = list.children[selectedIndex] as HTMLElement | undefined
    if (child) {
      child.scrollIntoView({ block: 'nearest', behavior: 'auto' })
    }
  }, [selectedIndex, open])

  if (!open) return null

  return (
    // bottom-full pins the popup *above* the textarea — the composer is
    // typically anchored to the bottom of the screen, so opening upward
    // is the only direction that doesn't overflow. mb-1 gives breathing
    // room. left-0 + max-w aligns to the textarea's left edge.
    <div
      className={cn(
        'absolute bottom-full left-0 mb-1 z-30',
        'w-[360px] max-w-[calc(100vw-3rem)]',
        'rounded-lg border border-border bg-popover text-popover-foreground shadow-md',
        'overflow-hidden',
      )}
      // Stop pointerdown so clicking the popup doesn't blur the textarea
      // before the click handler fires.
      onMouseDown={(e) => e.preventDefault()}
    >
      {headerLabel && (
        <div className="px-2 py-1 text-[10px] uppercase tracking-wider text-muted-foreground/60 border-b border-border/40">
          {headerLabel}
        </div>
      )}
      <div ref={listRef} className="max-h-[220px] overflow-y-auto py-1">
        {items.length === 0 ? (
          <div className="px-3 py-2 text-xs text-muted-foreground/60">{emptyText}</div>
        ) : (
          items.map((item, idx) => (
            <button
              type="button"
              key={keyFor(item)}
              onClick={(e) => {
                e.preventDefault()
                onSelect(item)
              }}
              className={cn(
                'w-full text-left px-2.5 py-1.5 transition-colors',
                'flex items-start gap-2',
                idx === selectedIndex
                  ? 'bg-primary/10 text-foreground'
                  : 'hover:bg-muted/40 text-foreground/85',
              )}
            >
              {renderItem(item, idx === selectedIndex)}
            </button>
          ))
        )}
      </div>
    </div>
  )
}
