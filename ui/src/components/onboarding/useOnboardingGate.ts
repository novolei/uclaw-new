import * as React from 'react'
import { useSetAtom } from 'jotai'
import { getOnboardingState } from '@/lib/tauri-bridge'
import { minicpmWizardAtom } from '@/atoms/minicpm-wizard'

/** On mount, open the wizard at `intro` iff onboarding is neither completed
 * nor skipped (i.e. pending or deferred). Non-blocking: failures are swallowed. */
export function useOnboardingGate(): void {
  const setWizard = useSetAtom(minicpmWizardAtom)
  React.useEffect(() => {
    let cancelled = false
    getOnboardingState()
      .then((state) => {
        if (cancelled) return
        if (state !== 'completed' && state !== 'skipped') {
          setWizard((s) => ({ ...s, step: 'intro' }))
        }
      })
      .catch(() => { /* non-blocking */ })
    return () => { cancelled = true }
  }, [setWizard])
}
