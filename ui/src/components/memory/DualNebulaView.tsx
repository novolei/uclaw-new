/**
 * DualNebulaView — 融合 3D 星云画布，同时渲染记忆层（暖色）与 gbrain 知识层（冷色）。
 *
 * 两层节点共享同一 Canvas，通过固定颜色常量区分层身份：
 *   memory   → 暖橙色 (#f4a259 / #e0883a)
 *   knowledge → 冷蓝色 (#5b8def / #3a6fe0)
 *
 * 复用 nebula/ 原语（StarNode, EdgeLines, NebulaDust, AutoRotateControls）+
 * dual-nebula/buildUnifiedScene 纯映射器。
 */
import * as React from 'react'
import { Canvas } from '@react-three/fiber'
import { useAtomValue } from 'jotai'
import { resolvedThemeAtom, themeStyleAtom } from '@/atoms/theme'
import { cn } from '@/lib/utils'
import type { MemoryGraphData } from '@/lib/types'
import type { KnowledgeGraph } from '@/lib/gbrain-browse'
import { StarNode, EdgeLines, NebulaDust, AutoRotateControls } from './nebula/primitives'
import type { NodePosition } from './nebula/layout'
import { getNebulaThemeConfig } from './nebula/theme'
import { buildUnifiedScene, type NebulaLayer } from './dual-nebula/buildUnifiedScene'

// ─── 层颜色常量（层身份唯一允许的语义硬编码色） ──────────────────────────

const LAYER_COLOR: Record<NebulaLayer, { color: string; emissive: string }> = {
  memory:    { color: '#f4a259', emissive: '#e0883a' },
  knowledge: { color: '#5b8def', emissive: '#3a6fe0' },
}

const STAR_RADIUS = 6

// ─── Props ────────────────────────────────────────────────────────────────

export interface DualNebulaViewProps {
  memory: MemoryGraphData | null
  knowledge: KnowledgeGraph | null
  onSelect?: (id: string, layer: NebulaLayer) => void
  className?: string
}

// ─── Scene content (inside Canvas) ───────────────────────────────────────

interface DualSceneProps {
  nodes: ReturnType<typeof buildUnifiedScene>['nodes']
  edges: ReturnType<typeof buildUnifiedScene>['edges']
  positions: NodePosition[]
  layerOf: Map<string, NebulaLayer>
  hoveredId: string | null
  onHover: (id: string | null) => void
  onSelect?: (id: string, layer: NebulaLayer) => void
  themeConfig: ReturnType<typeof getNebulaThemeConfig>
  resolvedTheme: 'light' | 'dark'
}

function DualScene({
  nodes,
  edges,
  positions,
  layerOf,
  hoveredId,
  onHover,
  onSelect,
  themeConfig,
  resolvedTheme,
}: DualSceneProps): React.ReactElement {
  return (
    <>
      <ambientLight intensity={resolvedTheme === 'dark' ? 0.08 : 0.3} color={themeConfig.ambientColor} />
      <pointLight position={[0, 0, 200]} intensity={0.8} />
      <AutoRotateControls />
      <NebulaDust themeConfig={themeConfig} />
      {nodes.map((n) => {
        const c = LAYER_COLOR[n.layer]
        return (
          <StarNode
            key={n.id}
            id={n.id}
            label={n.title}
            position={[n.x, n.y, n.z]}
            color={c.color}
            emissive={c.emissive}
            radius={STAR_RADIUS}
            isHovered={hoveredId === n.id}
            onHover={onHover}
            onClick={(id: string) => onSelect?.(id, layerOf.get(id) ?? 'memory')}
            themeConfig={themeConfig}
          />
        )
      })}
      <EdgeLines
        edges={edges}
        positions={positions}
        hoveredNodeId={hoveredId}
        themeConfig={themeConfig}
        resolvedTheme={resolvedTheme}
      />
    </>
  )
}

// ─── 主组件 ───────────────────────────────────────────────────────────────

export function DualNebulaView({
  memory,
  knowledge,
  onSelect,
  className,
}: DualNebulaViewProps): React.ReactElement {
  const [hoveredId, setHoveredId] = React.useState<string | null>(null)
  const resolvedTheme = useAtomValue(resolvedThemeAtom)
  const themeStyle = useAtomValue(themeStyleAtom)
  const themeConfig = React.useMemo(
    () => getNebulaThemeConfig(resolvedTheme, themeStyle),
    [resolvedTheme, themeStyle],
  )

  const scene = React.useMemo(() => buildUnifiedScene(memory, knowledge), [memory, knowledge])

  // NodePosition[] — shape expected by EdgeLines (positions: NodePosition[])
  const positions = React.useMemo<NodePosition[]>(
    () => scene.nodes.map((n) => ({ id: n.id, x: n.x, y: n.y, z: n.z })),
    [scene.nodes],
  )

  // Fast layer lookup for onClick callback
  const layerOf = React.useMemo(() => {
    const m = new Map<string, NebulaLayer>()
    for (const n of scene.nodes) m.set(n.id, n.layer)
    return m
  }, [scene.nodes])

  if (scene.nodes.length === 0) {
    return (
      <div
        className={cn('flex items-center justify-center text-muted-foreground text-sm', className)}
        data-testid="dual-nebula-empty"
      >
        暂无可视化数据
      </div>
    )
  }

  const cameraZ = Math.max(400, Math.sqrt(scene.nodes.length) * 30)

  return (
    <div className={cn('relative', className)} data-testid="dual-nebula-view">
      <Canvas
        camera={{ position: [0, 0, cameraZ], fov: 60 }}
        style={{ background: 'transparent' }}
      >
        <DualScene
          nodes={scene.nodes}
          edges={scene.edges}
          positions={positions}
          layerOf={layerOf}
          hoveredId={hoveredId}
          onHover={setHoveredId}
          onSelect={onSelect}
          themeConfig={themeConfig}
          resolvedTheme={resolvedTheme}
        />
      </Canvas>
    </div>
  )
}
