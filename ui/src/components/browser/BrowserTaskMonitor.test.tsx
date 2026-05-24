import { describe, expect, it } from 'vitest'
import { createStore } from 'jotai'
import { browserTaskRunAtom } from '@/atoms/browser-atoms'
import { renderWithProviders, screen } from '@/test-utils/render'
import { BrowserTaskMonitor } from './BrowserTaskMonitor'

describe('BrowserTaskMonitor', () => {
  it('renders paused-waiting runtime status without dropping recent steps', () => {
    const store = createStore()
    store.set(browserTaskRunAtom, new Map([
      ['sess-1', {
        runId: 'run-1',
        sessionId: 'sess-1',
        task: 'Collect page evidence',
        status: 'paused_waiting_for_browser_runtime',
        steps: [
          {
            stepIndex: 3,
            phase: 'done',
            observationSummary: '',
            reasoning: 'Browser runtime preparation was deferred.',
            actionName: 'checkpoint_pause',
            actionArgs: { checkpointStatus: 'paused_waiting_for_browser_runtime' },
            ok: false,
            message: null,
            error: 'Browser task is waiting for runtime preparation.',
            timestampMs: 123,
          },
        ],
      }],
    ]))

    renderWithProviders(<BrowserTaskMonitor sessionId="sess-1" />, { store })

    expect(screen.getByText('Collect page evidence')).toBeInTheDocument()
    expect(screen.getByText('waiting for runtime')).toBeInTheDocument()
    expect(screen.getByText('checkpoint_pause')).toBeInTheDocument()
    expect(screen.getByText('Browser task is waiting for runtime preparation.')).toBeInTheDocument()
  })
})
