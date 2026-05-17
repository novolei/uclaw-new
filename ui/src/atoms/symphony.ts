/**
 * Symphony Atoms — workflow + run state on the frontend side.
 *
 * Mirror of the data flowing through `tauri-bridge.ts` symphony_* commands +
 * the `symphony:*` IPC events emitted by `RunActor` and `SymphonyService`.
 *
 * Update flow:
 *   - on mount, SymphonyCanvas fetches workflows and runs once,
 *   - listeners on `symphony:node_update` / `symphony:run_completed` /
 *     `symphony:node_log` update these atoms,
 *   - any save / trigger Tauri call ends with an atom write so the UI
 *     reflects the persisted state without needing a re-fetch.
 */

import { atom } from 'jotai'
import type {
  SymphonyNodeRunRow,
  SymphonyNodeUpdateEvent,
  SymphonyRunRow,
  SymphonyWorkflowDetailDto,
  SymphonyWorkflowSummary,
} from '@/lib/tauri-bridge'

// ─── workflow + run primary atoms ──────────────────────────────────────────

export const symphonyWorkflowsAtom = atom<SymphonyWorkflowSummary[]>([])

/** id of the workflow currently open in the canvas */
export const currentSymphonyWorkflowIdAtom = atom<string | null>(null)

/** Cached `SymphonyWorkflowDetailDto` keyed by workflow id. */
export const symphonyWorkflowDetailsAtom = atom<
  Record<string, SymphonyWorkflowDetailDto>
>({})

/** All recent runs keyed by workflow id. */
export const symphonyRunsByWorkflowAtom = atom<
  Record<string, SymphonyRunRow[]>
>({})

/** The currently-selected run id (null = "Design" view, set = "Run" view). */
export const currentSymphonyRunIdAtom = atom<string | null>(null)

/** Node-runs keyed by run id. */
export const symphonyNodeRunsByRunAtom = atom<
  Record<string, SymphonyNodeRunRow[]>
>({})

// ─── reducer-style writers — call these from IPC listeners ─────────────────

export const applyNodeUpdateAtom = atom(
  null,
  (get, set, e: SymphonyNodeUpdateEvent) => {
    const map = { ...get(symphonyNodeRunsByRunAtom) }
    const nodes = map[e.runId] ?? []
    let touched = false
    const next = nodes.map((n) => {
      if (n.nodeId === e.nodeId) {
        touched = true
        return { ...n, status: e.status }
      }
      return n
    })
    map[e.runId] = touched
      ? next
      : [
          ...nodes,
          // Synthesize a minimal row so the canvas can render before the
          // detail fetch lands (no attempts, no cost yet).
          {
            id: `${e.runId}-${e.nodeId}-placeholder`,
            runId: e.runId,
            nodeId: e.nodeId,
            attempt: 1,
            status: e.status,
            sessionId: null,
            costUsd: 0,
            iterations: 0,
            lastHeartbeatMs: null,
            errorText: null,
            outputJson: null,
          },
        ]
    set(symphonyNodeRunsByRunAtom, map)
  },
)

export const upsertRunAtom = atom(null, (get, set, r: SymphonyRunRow) => {
  const map = { ...get(symphonyRunsByWorkflowAtom) }
  const arr = map[r.workflowId] ?? []
  const idx = arr.findIndex((x) => x.id === r.id)
  if (idx >= 0) {
    arr[idx] = r
  } else {
    arr.unshift(r)
  }
  map[r.workflowId] = [...arr]
  set(symphonyRunsByWorkflowAtom, map)
})

export const finalizeRunAtom = atom(
  null,
  (
    get,
    set,
    e: { runId: string; status: SymphonyRunRow['status']; totalCostUsd: number },
  ) => {
    const map = { ...get(symphonyRunsByWorkflowAtom) }
    for (const wid of Object.keys(map)) {
      const arr = map[wid]
      const idx = arr.findIndex((x) => x.id === e.runId)
      if (idx >= 0) {
        arr[idx] = {
          ...arr[idx],
          status: e.status,
          totalCostUsd: e.totalCostUsd,
          completedAt: Date.now(),
        }
        map[wid] = [...arr]
      }
    }
    set(symphonyRunsByWorkflowAtom, map)
  },
)
