/**
 * useFocusModeShortcut — Alt+F → toggle Focus Mode.
 *
 * Mount once at AppShell top. preventDefault is true by default
 * (useShortcut.ts:80) so Mac's Option+F → ƒ character insertion is
 * blocked automatically — the binding works even when focus is
 * inside a code editor or chat input.
 */

import { useSetAtom } from 'jotai'
import { useShortcut } from './useShortcut'
import { toggleFocusModeAction } from '@/atoms/focus-mode-atoms'

export function useFocusModeShortcut(): void {
  const toggle = useSetAtom(toggleFocusModeAction)
  useShortcut({
    id: 'toggle-focus-mode',
    handler: () => toggle(),
  })
}
