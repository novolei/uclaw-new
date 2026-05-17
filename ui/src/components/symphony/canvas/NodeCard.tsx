/**
 * NodeCard — custom React Flow node renderer for a Symphony workflow node.
 *
 * Renders the status pill, cost chip, and a one-line label. Status colors
 * use theme tokens only (CLAUDE.md Part 1 hard rule) so warm-paper /
 * forest-* themes don't break.
 */

import * as React from 'react'
import { Handle, Position, type Node, type NodeProps } from '@xyflow/react'
import { cn } from '@/lib/utils'
import type { SymphonyNodeKind, SymphonyNodeStatus } from '@/lib/tauri-bridge'
import { Bot, GitBranch, Globe } from 'lucide-react'

// xyflow v12 NodeProps is generic over the full Node<Data> type, not just
// the data shape. A bare NodeProps<NodeCardData> compiles only on legacy
// reactflow; we use the v12-correct form so the type-check stays green.
export type NodeCardData = {
  label: string
  kind: SymphonyNodeKind
  status: SymphonyNodeStatus
  costUsd: number
  mode: 'design' | 'run'
}
export type SymphonyNodeType = Node<NodeCardData, 'symphony'>

// Status classes use theme tokens (`bg-primary`, `bg-accent`, `bg-muted`,
// `bg-destructive`) so warm-paper / forest-* / qingye themes don't break
// per CLAUDE.md Part 1 hard rule. Success uses bg-primary (themes ship
// distinct primaries) and stalled uses `bg-destructive/10` as a warning-
// tinted surface — we deliberately avoid raw Tailwind palette greens/ambers
// because they're not theme variables in this repo's globals.css.
const STATUS_CLASSES: Record<SymphonyNodeStatus, string> = {
  pending: 'bg-muted/50 text-muted-foreground border-border',
  ready: 'bg-accent/40 text-accent-foreground border-accent',
  running: 'bg-primary/15 text-primary border-primary animate-pulse',
  succeeded: 'bg-primary/20 text-primary-foreground border-primary/40',
  failed: 'bg-destructive/15 text-destructive border-destructive',
  stalled: 'bg-destructive/10 text-muted-foreground border-destructive/30',
  cancelled: 'bg-muted text-muted-foreground border-border',
}

const STATUS_LABELS: Record<SymphonyNodeStatus, string> = {
  pending: 'Pending',
  ready: 'Ready',
  running: 'Running',
  succeeded: 'Done',
  failed: 'Failed',
  stalled: 'Stalled',
  cancelled: 'Cancelled',
}

const KIND_ICONS: Record<SymphonyNodeKind, React.ReactNode> = {
  agent: <Bot size={12} />,
  shell: <GitBranch size={12} />,
  http: <Globe size={12} />,
}

export function NodeCard({
  data,
  selected,
}: NodeProps<SymphonyNodeType>): React.ReactElement {
  return (
    <div
      className={cn(
        'group flex w-56 flex-col rounded-lg border bg-card text-card-foreground shadow-sm transition-colors',
        STATUS_CLASSES[data.status],
        selected && 'ring-2 ring-primary/40',
      )}
    >
      <Handle type="target" position={Position.Left} className="!bg-border" />
      <div className="flex items-center gap-2 px-3 py-2">
        <span className="text-muted-foreground">{KIND_ICONS[data.kind]}</span>
        <span className="flex-1 truncate text-xs font-medium">
          {data.label}
        </span>
      </div>
      <div className="flex items-center justify-between border-t border-border/40 px-3 py-1.5 text-[10px]">
        <span className="font-medium">{STATUS_LABELS[data.status]}</span>
        {data.mode === 'run' && data.costUsd > 0 && (
          <span className="text-muted-foreground">
            ${data.costUsd.toFixed(4)}
          </span>
        )}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-border" />
    </div>
  )
}
