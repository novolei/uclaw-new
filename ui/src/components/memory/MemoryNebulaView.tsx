/**
 * MemoryNebulaView — 3D 宇宙星云可视化（增强版）。
 *
 * 使用 @react-three/fiber + drei 渲染记忆图谱节点为星球，
 * 边为连线。螺旋星系布局，Fresnel 光晕，主题自适应星云雾气。
 */
import * as React from 'react'
import { Canvas } from '@react-three/fiber'
import { Stars } from '@react-three/drei'
import { useAtomValue } from 'jotai'
import { resolvedThemeAtom, themeStyleAtom } from '@/atoms/theme'
import { cn } from '@/lib/utils'
import type { MemoryGraphData, MemoryNode, MemoryNodeKind } from '@/lib/types'
import { StarNode, EdgeLines, NebulaDust, AutoRotateControls } from './nebula/primitives'
import { computeGalaxyLayout, type NodePosition, type LayoutEdge } from './nebula/layout'
import { getNebulaThemeConfig, type NebulaThemeConfig } from './nebula/theme'

// ─── Kind 配色与尺寸 ────────────────────────────────────────────────────

const KIND_CONFIG: Record<MemoryNodeKind, { color: string; emissive: string; radius: number; emissiveIntensity: number; roughness: number; metalness: number; noiseFreq: number; noiseAmp: number; flowSpeed: number }> = {
  boot:         { color: '#ff6b6b', emissive: '#ff4444', radius: 4, emissiveIntensity: 1.5, roughness: 0.9, metalness: 0, noiseFreq: 2.5, noiseAmp: 1.0, flowSpeed: 0.4 },
  identity:     { color: '#c084fc', emissive: '#a855f7', radius: 3, emissiveIntensity: 1.2, roughness: 0.85, metalness: 0, noiseFreq: 1.8, noiseAmp: 0.8, flowSpeed: 0.25 },
  value:        { color: '#60a5fa', emissive: '#3b82f6', radius: 3, emissiveIntensity: 1.0, roughness: 0.8, metalness: 0, noiseFreq: 1.5, noiseAmp: 0.7, flowSpeed: 0.2 },
  user_profile: { color: '#4ade80', emissive: '#22c55e', radius: 3, emissiveIntensity: 1.0, roughness: 0.8, metalness: 0, noiseFreq: 1.5, noiseAmp: 0.7, flowSpeed: 0.2 },
  directive:    { color: '#fb923c', emissive: '#f97316', radius: 3.5, emissiveIntensity: 1.4, roughness: 0.9, metalness: 0, noiseFreq: 2.2, noiseAmp: 0.9, flowSpeed: 0.35 },
  curated:      { color: '#38bdf8', emissive: '#0ea5e9', radius: 2.2, emissiveIntensity: 0.8, roughness: 0.8, metalness: 0, noiseFreq: 1.2, noiseAmp: 0.5, flowSpeed: 0.15 },
  episode:      { color: '#fbbf24', emissive: '#eab308', radius: 2.2, emissiveIntensity: 0.8, roughness: 0.8, metalness: 0, noiseFreq: 1.2, noiseAmp: 0.5, flowSpeed: 0.15 },
  procedure:    { color: '#2dd4bf', emissive: '#14b8a6', radius: 2.8, emissiveIntensity: 1.0, roughness: 0.75, metalness: 0, noiseFreq: 1.8, noiseAmp: 0.7, flowSpeed: 0.2 },
  reference:    { color: '#9ca3af', emissive: '#6b7280', radius: 2, emissiveIntensity: 0.5, roughness: 1.0, metalness: 0, noiseFreq: 0.8, noiseAmp: 0.3, flowSpeed: 0.08 },
}

// Fragment 专属星云配色
const FRAGMENT_NEBULA_CONFIG = {
  color: '#FFB347',       // 温暖橙色核心
  emissive: '#FF8C00',    // 深橙光晕
  radius: 1.8,            // 比普通 Episode 略小
  emissiveIntensity: 0.9,
  roughness: 0.8,
  metalness: 0,
  noiseFreq: 1.4,
  noiseAmp: 0.6,
  flowSpeed: 0.18,
}

/** 根据节点 kind 和 metadata.subtype 选择配色 */
function getNodeConfig(node: MemoryNode) {
  if (node.kind === 'episode' && node.metadata?.subtype === 'fragment') {
    return FRAGMENT_NEBULA_CONFIG
  }
  return KIND_CONFIG[node.kind] ?? KIND_CONFIG.reference
}

// ─── 节点群组 ───────────────────────────────────────────────────────────

interface MemoryNodesMeshProps {
  nodes: MemoryNode[]
  positions: NodePosition[]
  hoveredId: string | null
  onHover: (id: string | null) => void
  onClick: (id: string) => void
  themeConfig: NebulaThemeConfig
}

function MemoryNodesMesh({ nodes, positions, hoveredId, onHover, onClick, themeConfig }: MemoryNodesMeshProps): React.ReactElement {
  const posMap = React.useMemo(() => {
    const map = new Map<string, [number, number, number]>()
    for (const p of positions) {
      map.set(p.id, [p.x, p.y, p.z])
    }
    return map
  }, [positions])

  return (
    <group>
      {nodes.map((node) => {
        const pos = posMap.get(node.id)
        if (!pos) return null
        const config = getNodeConfig(node)
        return (
          <StarNode
            key={node.id}
            id={node.id}
            position={pos}
            color={config.color}
            emissive={config.emissive}
            radius={config.radius}
            isHovered={hoveredId === node.id}
            onHover={onHover}
            onClick={onClick}
            themeConfig={themeConfig}
          />
        )
      })}
    </group>
  )
}

// ─── 场景内容 ────────────────────────────────────────────────────────────

interface SceneContentProps {
  graphData: MemoryGraphData
  positions: NodePosition[]
  hoveredId: string | null
  onHover: (id: string | null) => void
  onClick: (id: string) => void
  themeConfig: NebulaThemeConfig
  resolvedTheme: 'light' | 'dark'
}

function SceneContent({ graphData, positions, hoveredId, onHover, onClick, themeConfig, resolvedTheme }: SceneContentProps): React.ReactElement {
  const normalizedEdges = React.useMemo<LayoutEdge[]>(
    () => graphData.edges.map(e => ({ from: e.parentNodeId ?? '', to: e.childNodeId })),
    [graphData.edges],
  )

  return (
    <>
      <color attach="background" args={[themeConfig.fogColor]} />
      <fog attach="fog" args={[themeConfig.fogColor, 300, 800]} />
      <ambientLight intensity={resolvedTheme === 'dark' ? 0.08 : 0.3} color={themeConfig.ambientColor} />
      <directionalLight position={[200, 100, 150]} intensity={0.15} color="#e0e0ff" />
      <AutoRotateControls />
      <Stars radius={500} depth={80} count={3000} factor={themeConfig.starsFactor} fade={themeConfig.starsFade} speed={0.5} />
      <NebulaDust themeConfig={themeConfig} />
      <MemoryNodesMesh
        nodes={graphData.nodes}
        positions={positions}
        hoveredId={hoveredId}
        onHover={onHover}
        onClick={onClick}
        themeConfig={themeConfig}
      />
      <EdgeLines
        edges={normalizedEdges}
        positions={positions}
        hoveredNodeId={hoveredId}
        themeConfig={themeConfig}
        resolvedTheme={resolvedTheme}
      />
    </>
  )
}

// ─── 主组件 ─────────────────────────────────────────────────────────────

interface MemoryNebulaViewProps {
  graphData: MemoryGraphData | null
  onSelectNode?: (nodeId: string) => void
  className?: string
}

export function MemoryNebulaView({ graphData, onSelectNode, className }: MemoryNebulaViewProps): React.ReactElement {
  const [hoveredId, setHoveredId] = React.useState<string | null>(null)
  const resolvedTheme = useAtomValue(resolvedThemeAtom)
  const themeStyle = useAtomValue(themeStyleAtom)

  const themeConfig = React.useMemo(
    () => getNebulaThemeConfig(resolvedTheme, themeStyle),
    [resolvedTheme, themeStyle]
  )

  const positions = React.useMemo(() => {
    if (!graphData) return []
    return computeGalaxyLayout(
      graphData.nodes.map(n => ({ id: n.id, kind: n.kind })),
      graphData.edges.map(e => ({ from: e.parentNodeId ?? '', to: e.childNodeId })),
    )
  }, [graphData])

  const cameraZ = React.useMemo(() => {
    if (!graphData) return 350
    return Math.max(300, Math.sqrt(graphData.nodes.length) * 25)
  }, [graphData])

  const handleClick = React.useCallback((nodeId: string) => {
    onSelectNode?.(nodeId)
  }, [onSelectNode])

  if (!graphData || graphData.nodes.length === 0) {
    return (
      <div className={cn('flex items-center justify-center text-muted-foreground text-sm', className)}>
        {graphData ? '暂无记忆数据' : '加载中…'}
      </div>
    )
  }

  return (
    <div className={cn('relative', className)}>
      <Canvas
        camera={{ position: [0, 0, cameraZ], fov: 60 }}
        style={{ background: 'transparent' }}
      >
        <SceneContent
          graphData={graphData}
          positions={positions}
          hoveredId={hoveredId}
          onHover={setHoveredId}
          onClick={handleClick}
          themeConfig={themeConfig}
          resolvedTheme={resolvedTheme}
        />
      </Canvas>
    </div>
  )
}
