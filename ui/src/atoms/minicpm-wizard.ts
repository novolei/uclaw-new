import { atom } from 'jotai'

export type MiniCpmWizardStep =
  | 'intro' | 'envcheck' | 'source' | 'download' | 'warmup' | 'smoketest' | 'done' | 'error' | null

export interface MiniCpmWizardState {
  step: MiniCpmWizardStep
  error: string | null
}

export const minicpmWizardAtom = atom<MiniCpmWizardState>({ step: null, error: null })
