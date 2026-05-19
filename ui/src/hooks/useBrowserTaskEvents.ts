import * as React from 'react'
import { useSetAtom } from 'jotai'
import { browserTaskRunAtom, type BrowserTaskRunEntry } from '@/atoms/browser-atoms'
import { listenBrowserTaskRun, listenBrowserTaskStep } from '@/lib/tauri-bridge'

export function useBrowserTaskEvents(sessionId: string): void {
  const setTaskRun = useSetAtom(browserTaskRunAtom)

  React.useEffect(() => {
    let unlistenRun: (() => void) | null = null
    let unlistenStep: (() => void) | null = null

    listenBrowserTaskRun((payload) => {
      if (payload.sessionId !== sessionId) return
      setTaskRun((prev) => {
        const next = new Map(prev)
        next.set(sessionId, payload as BrowserTaskRunEntry)
        return next
      })
    }).then((fn) => { unlistenRun = fn })

    listenBrowserTaskStep((payload) => {
      if (payload.sessionId !== sessionId) return
      setTaskRun((prev) => {
        const existing = prev.get(sessionId)
        const run: BrowserTaskRunEntry = existing ?? {
          runId: payload.runId,
          sessionId: payload.sessionId,
          task: '',
          status: payload.status,
          steps: [],
        }
        const steps = [...run.steps]
        const idx = steps.findIndex((step) => step.stepIndex === payload.step.stepIndex)
        if (idx >= 0) {
          steps[idx] = payload.step
        } else {
          steps.push(payload.step)
          steps.sort((a, b) => a.stepIndex - b.stepIndex)
        }
        const next = new Map(prev)
        next.set(sessionId, { ...run, status: payload.status, steps })
        return next
      })
    }).then((fn) => { unlistenStep = fn })

    return () => {
      if (unlistenRun) unlistenRun()
      if (unlistenStep) unlistenStep()
    }
  }, [sessionId, setTaskRun])
}
