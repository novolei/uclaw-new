import { useAtomValue } from 'jotai'
import { agentStreamingAtom } from '@/atoms/agent-atoms'

/**
 * Per-item liveness flags consumed by DockItem to render breathing halo,
 * streaming particles, and memory pulse.
 *
 * Phase 3 scope: only `mode-agent` and `mode-memory` get non-default values.
 * Other sortable ids (chat / kaleidoscope / pinned-*) always read default
 * (all-off). Consumers index by sortable id; missing keys are treated as
 * all-off via the `?? DEFAULT_LIVENESS` pattern at the call site.
 */
export interface LivenessState {
  /** Soft halo animation — "agent is alive and processing". */
  breathing: boolean
  /** Particles emit from top edge — "actively producing token output". */
  streaming: boolean
  /** Subtle scale wave — reserved for a memory-consolidation signal (currently always OFF). */
  pulsing: boolean
}

const OFF: LivenessState = { breathing: false, streaming: false, pulsing: false }

export type DockLivenessMap = Record<string, LivenessState>

export function useDockLiveness(): DockLivenessMap {
  const agentStreaming = useAtomValue(agentStreamingAtom)

  // Per spec §3.2:
  //   - breathing: broader "agent active" signal (we use agentStreaming
  //     today since there's no separate activeTasks atom)
  //   - streaming: narrow "currently producing tokens"
  //   - pulsing: memory consolidation in progress (memU removed; always OFF)
  // breathing and streaming share the same source atom in this phase;
  // they layer visually rather than competing.
  return {
    'mode-agent': agentStreaming
      ? { breathing: true, streaming: true, pulsing: false }
      : OFF,
    'mode-memory': OFF,
    // 'mode-chat' / 'mode-kaleidoscope' / pinned-* keys are absent —
    // BottomDock will pass `liveness ?? OFF` to those DockItems.
  }
}
