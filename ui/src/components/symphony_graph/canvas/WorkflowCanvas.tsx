/**
 * WorkflowCanvas — @xyflow/react host for one workflow.
 *
 * Translates `SymphonyWorkflowDef` into ReactFlow nodes/edges. The Design
 * mode lets the user re-arrange; the Run mode is read-only and reflects
 * `NodeRun.status` via the NodeCard rendering.
 *
 * Auto-layout: nodes without an explicit position are laid out left-to-right
 * by topological depth. This keeps unauthored workflows readable without
 * forcing the user to drag boxes around.
 */

import * as React from 'react'
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  type Edge,
  type Node,
  type NodeTypes,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'

import type {
  SymphonyNodeRunRow,
  SymphonyNodeStatus,
  SymphonyWorkflowDetailDto,
} from '@/lib/tauri-bridge'

import { NodeCard, type NodeCardData, type SymphonyNodeType } from './NodeCard'

// `as any` here: xyflow's NodeTypes map is loosely typed (Record<string,
// ComponentType<NodeProps<Node<any>>>>) and matching the generic exactly
// adds ceremony without typing benefit. The component itself is fully typed.
const NODE_TYPES: NodeTypes = { symphony: NodeCard as any }

export interface WorkflowCanvasProps {
  detail: SymphonyWorkflowDetailDto
  mode: 'design' | 'run'
  runId: string | null
  nodeRuns: SymphonyNodeRunRow[]
}

export function WorkflowCanvas({
  detail,
  mode,
  runId: _runId,
  nodeRuns,
}: WorkflowCanvasProps): React.ReactElement {
  const { nodes, edges } = React.useMemo(() => {
    const positions = layoutByTopoDepth(detail.definition.nodes)
    const statusByNodeId = new Map<string, SymphonyNodeStatus>()
    const costByNodeId = new Map<string, number>()
    for (const nr of nodeRuns) {
      // Latest attempt wins.
      const existing = statusByNodeId.get(nr.nodeId)
      if (!existing || nr.attempt >= 1) {
        statusByNodeId.set(nr.nodeId, nr.status)
        costByNodeId.set(nr.nodeId, nr.costUsd)
      }
    }

    const rfNodes: SymphonyNodeType[] = detail.definition.nodes.map(
      (n): SymphonyNodeType => ({
        id: n.id,
        type: 'symphony',
        position: positions.get(n.id) ?? { x: 0, y: 0 },
        data: {
          label: n.label,
          kind: n.kind,
          status: statusByNodeId.get(n.id) ?? 'pending',
          costUsd: costByNodeId.get(n.id) ?? 0,
          mode,
        } satisfies NodeCardData,
      }),
    )

    const rfEdges: Edge[] = (detail.definition.edges.length > 0
      ? detail.definition.edges
      : detail.definition.nodes.flatMap((n) =>
          n.deps.map((d) => ({ from: d, to: n.id, label: null })),
        )
    ).map((e, idx) => ({
      id: `e-${e.from}-${e.to}-${idx}`,
      source: e.from,
      target: e.to,
      label: e.label ?? undefined,
      animated:
        statusByNodeId.get(e.from) === 'running' ||
        statusByNodeId.get(e.to) === 'running',
      style: { stroke: 'hsl(var(--border))' },
    }))

    return { nodes: rfNodes, edges: rfEdges }
  }, [detail, nodeRuns, mode])

  return (
    <div className="h-full w-full">
      <ReactFlowProvider>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={NODE_TYPES}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          minZoom={0.3}
          maxZoom={2.0}
          panOnScroll
          nodesDraggable={mode === 'design'}
          nodesConnectable={mode === 'design'}
          elementsSelectable
          proOptions={{ hideAttribution: true }}
        >
          <Background gap={16} color="hsl(var(--border))" />
          <MiniMap pannable zoomable className="!bg-muted/40" />
          <Controls />
        </ReactFlow>
      </ReactFlowProvider>
    </div>
  )
}

/** Compute simple {x, y} positions by topological depth (left → right). */
function layoutByTopoDepth(
  nodes: SymphonyWorkflowDetailDto['definition']['nodes'],
): Map<string, { x: number; y: number }> {
  const depth = new Map<string, number>()
  const queue: string[] = []
  const indeg = new Map<string, number>()
  for (const n of nodes) {
    indeg.set(n.id, n.deps.length)
    if (n.deps.length === 0) {
      depth.set(n.id, 0)
      queue.push(n.id)
    }
  }
  const consumers = new Map<string, string[]>()
  for (const n of nodes) {
    for (const d of n.deps) {
      const arr = consumers.get(d) ?? []
      arr.push(n.id)
      consumers.set(d, arr)
    }
  }
  while (queue.length > 0) {
    const cur = queue.shift()!
    const d = depth.get(cur) ?? 0
    for (const c of consumers.get(cur) ?? []) {
      depth.set(c, Math.max(depth.get(c) ?? 0, d + 1))
      indeg.set(c, (indeg.get(c) ?? 0) - 1)
      if ((indeg.get(c) ?? 0) === 0) queue.push(c)
    }
  }
  // Group by depth, lay out top-to-bottom.
  const byDepth = new Map<number, string[]>()
  for (const n of nodes) {
    const d = depth.get(n.id) ?? 0
    const arr = byDepth.get(d) ?? []
    arr.push(n.id)
    byDepth.set(d, arr)
  }
  const positions = new Map<string, { x: number; y: number }>()
  const COL_W = 240
  const ROW_H = 120
  for (const [d, ids] of byDepth.entries()) {
    ids.forEach((id, idx) => {
      positions.set(id, { x: d * COL_W, y: idx * ROW_H })
    })
  }
  return positions
}
