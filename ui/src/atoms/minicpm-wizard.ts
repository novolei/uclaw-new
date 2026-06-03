import { atom } from 'jotai'
import type { EnvReport, ProbedSource, MiniCpmDownloadProgress } from '@/lib/tauri-bridge'

export type MiniCpmWizardStep =
  | 'intro' | 'envcheck' | 'source' | 'download' | 'warmup' | 'smoketest' | 'done' | 'error' | null

export interface MiniCpmWizardState {
  step: MiniCpmWizardStep
  env: EnvReport | null
  sources: ProbedSource[] | null
  chosenSource: string | null      // host id, or null = auto (probe-ranked)
  progress: MiniCpmDownloadProgress | null
  smokeOutput: string | null
  error: string | null
}

export const INITIAL_WIZARD: MiniCpmWizardState = {
  step: null, env: null, sources: null, chosenSource: null,
  progress: null, smokeOutput: null, error: null,
}

export const minicpmWizardAtom = atom<MiniCpmWizardState>(INITIAL_WIZARD)
