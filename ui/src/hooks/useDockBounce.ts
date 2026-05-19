import { useEffect, type RefObject } from 'react'
import { useSetAtom } from 'jotai'
import { dockBounceKeysAtom } from '@/atoms/dock-atoms'
import { onNeedApproval } from '@/lib/tauri-bridge'
import type { BottomDockHoverRegionHandle } from '@/components/dock/BottomDockHoverRegion'

/**
 * Attention-signal hook for the BottomDock.
 *
 * Subscribes to `agent:need_approval` IPC events; on each event:
 *   1. Calls `forceReveal()` on the hover region (slides the dock up if hidden)
 *   2. Calls `holdRevealed(1500)` so the dock stays visible ~1.5 s
 *   3. Increments `dockBounceKeysAtom['mode-agent']` so the Agent icon
 *      runs its one-shot bounce animation (DockItem listens via bounceKey)
 *
 * After the hold expires, the normal mouseLeave debounce takes over.
 *
 * Phase 2C scope: tool-approval only. The "non-active mode new message"
 * trigger from spec §2.4 is deferred — there is no clear backend event
 * for it today. The hook is structured so additional event subscriptions
 * can plug in alongside without refactor.
 */
export function useDockBounce(
  hoverRef: RefObject<BottomDockHoverRegionHandle>,
): void {
  const setBounceKeys = useSetAtom(dockBounceKeysAtom)

  useEffect(() => {
    let unlisten: (() => void) | null = null
    let active = true

    onNeedApproval(() => {
      if (!active) return
      hoverRef.current?.forceReveal()
      hoverRef.current?.holdRevealed(1500)
      setBounceKeys((prev) => ({
        ...prev,
        'mode-agent': (prev['mode-agent'] ?? 0) + 1,
      }))
    }).then((fn) => {
      if (active) unlisten = fn
      else fn() // mount race — unlisten immediately
    })

    return () => {
      active = false
      if (unlisten) unlisten()
    }
  }, [hoverRef, setBounceKeys])
}
