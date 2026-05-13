/**
 * useFocusModeAutoExit — when the preview panel closes, Focus Mode
 * loses its reason to exist; force-exit so the user isn't stranded
 * in a sidebars-hidden state with no preview to focus on.
 *
 * Also runs the same check on mount to scrub any orphan state left
 * over from a previous session (e.g. preview was closed in another
 * workspace while Focus Mode was still on globally).
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  focusModeAtom,
  exitFocusModeAction,
} from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'

export function useFocusModeAutoExit(): void {
  const focusMode = useAtomValue(focusModeAtom)
  const previewOpen = useAtomValue(previewPanelOpenAtom)
  const exit = useSetAtom(exitFocusModeAction)

  React.useEffect(() => {
    if (focusMode && !previewOpen) exit()
  }, [focusMode, previewOpen, exit])
}
