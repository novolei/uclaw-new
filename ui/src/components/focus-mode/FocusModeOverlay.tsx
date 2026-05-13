/**
 * FocusModeOverlay — composition root for Focus Mode visuals.
 *
 * Responsibilities:
 *   - Mounts the hot zone listener (useFocusModeHotzone) and the
 *     auto-exit watcher (useFocusModeAutoExit). The shortcut binding
 *     (useFocusModeShortcut) is mounted at AppShell level instead so it
 *     stays alive even when this overlay returns null.
 *   - When focus mode is OFF, returns null (nothing on screen).
 *   - When focus mode is ON, renders the two GlowIndicators and the
 *     two FloatingIsland wrappers, with LeftSidebar / RightSidePanel
 *     as the islands' children. The right island is also gated on the
 *     existing `showRightPanel` condition (agent mode + active session).
 *
 * This file is the single place that imports LeftSidebar / RightSidePanel
 * for overlay use — their original AppShell inline mounts are conditionally
 * suppressed by AppShell when focusModeAtom is true.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { focusModeAtom } from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'
import { appModeAtom } from '@/atoms/app-mode'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { useFocusModeHotzone } from '@/hooks/useFocusModeHotzone'
import { useFocusModeAutoExit } from '@/hooks/useFocusModeAutoExit'
import { LeftSidebar } from '@/components/app-shell/LeftSidebar'
import { RightSidePanel } from '@/components/app-shell/RightSidePanel'
import { FloatingIsland } from './FloatingIsland'
import { GlowIndicator } from './GlowIndicator'

export function FocusModeOverlay(): React.ReactElement | null {
  const focusMode = useAtomValue(focusModeAtom)
  const previewOpen = useAtomValue(previewPanelOpenAtom)
  const appMode = useAtomValue(appModeAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const showRightPanel = appMode === 'agent' && !!currentSessionId

  // Always mount the hotzone + autoExit watchers — they cheaply no-op
  // when focusMode is false, but staying mounted means we don't miss
  // the moment focusMode flips on.
  useFocusModeHotzone()
  useFocusModeAutoExit()

  if (!focusMode || !previewOpen) return null

  return (
    <>
      <GlowIndicator side="left" />
      <GlowIndicator side="right" />
      <FloatingIsland side="left">
        <LeftSidebar />
      </FloatingIsland>
      {showRightPanel && (
        <FloatingIsland side="right">
          <RightSidePanel />
        </FloatingIsland>
      )}
    </>
  )
}
