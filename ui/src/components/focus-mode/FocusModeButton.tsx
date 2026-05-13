/**
 * FocusModeButton — toggles Focus Mode from the preview header.
 *
 * Mounted in PreviewHeader to the LEFT of the Copy / Reveal / Close
 * action trio. Visual style matches the existing HeaderButton:
 * size-6, rounded-md, hover bg/text tokens. Icon flips Maximize2 →
 * Minimize2 when on; tooltip / aria-label reflect the current state
 * + shortcut hint.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Maximize2, Minimize2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  focusModeAtom,
  toggleFocusModeAction,
} from '@/atoms/focus-mode-atoms'

export function FocusModeButton(): React.ReactElement {
  const focusMode = useAtomValue(focusModeAtom)
  const toggle = useSetAtom(toggleFocusModeAction)
  const label = focusMode ? '退出专注模式 (Alt+F)' : '进入专注模式 (Alt+F)'
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={() => toggle()}
      className={cn(
        'size-6 inline-flex items-center justify-center rounded-md shrink-0',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        'text-foreground/55 hover:text-foreground hover:bg-foreground/[0.06] active:bg-foreground/[0.10]',
      )}
    >
      {focusMode ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
    </button>
  )
}
