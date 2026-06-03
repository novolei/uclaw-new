/**
 * usePetWizardBridge — listens for `pet://open-wizard` cross-window events
 * emitted by the pet window (e.g. on 503 / model-not-ready) and opens the
 * MiniCPM onboarding wizard in the main window.
 *
 * Extracted from App.tsx to keep the hook unit-testable in isolation.
 */
import * as React from 'react'
import { useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'

export function usePetWizardBridge(): void {
  const setWizard = useSetAtom(minicpmWizardAtom)

  React.useEffect(() => {
    let un: (() => void) | undefined
    let cancelled = false
    listen('pet://open-wizard', () => setWizard((s) => ({ ...s, step: 'intro' })))
      .then((fn) => { if (cancelled) fn(); else un = fn })
    return () => { cancelled = true; un?.() }
  }, [setWizard])
}
