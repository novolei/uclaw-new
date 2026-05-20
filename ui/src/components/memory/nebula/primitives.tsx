import * as React from 'react'
import { useFrame, type ThreeEvent } from '@react-three/fiber'
import { OrbitControls, Line, Html, Billboard } from '@react-three/drei'
import * as THREE from 'three'
import { glowVertexShader, glowFragmentShader, starVertexShader, starFragmentShader } from './shaders'
import { hashCode, type NodePosition } from './layout'
import type { NebulaThemeConfig } from './theme'

// ─── 星云纹理 ────────────────────────────────────────────────────────────

export const createNebulaTexture = (): THREE.CanvasTexture => {
  const size = 128
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const ctx = canvas.getContext('2d')!
  const gradient = ctx.createRadialGradient(size / 2, size / 2, 0, size / 2, size / 2, size / 2)
  gradient.addColorStop(0, 'rgba(255,255,255,0.3)')
  gradient.addColorStop(0.5, 'rgba(255,255,255,0.05)')
  gradient.addColorStop(1, 'rgba(255,255,255,0)')
  ctx.fillStyle = gradient
  ctx.fillRect(0, 0, size, size)
  return new THREE.CanvasTexture(canvas)
}

// ─── 星云雾气装饰 ────────────────────────────────────────────────────────

export function NebulaDust({ themeConfig }: { themeConfig: NebulaThemeConfig }): React.ReactElement | null {
  const groupRef = React.useRef<THREE.Group>(null)
  const texture = React.useMemo(() => createNebulaTexture(), [])
  const dustColor = React.useMemo(() => new THREE.Color(themeConfig.ambientColor), [themeConfig.ambientColor])

  // Dispose the manually created CanvasTexture on unmount
  React.useEffect(() => {
    return () => {
      texture.dispose()
    }
  }, [texture])

  const dustConfigs = React.useMemo(() => [
    { size: 280, opacity: 0.04, pos: [30, 10, -50] as [number, number, number] },
    { size: 220, opacity: 0.05, pos: [-60, -20, 40] as [number, number, number] },
    { size: 250, opacity: 0.03, pos: [10, 40, 60] as [number, number, number] },
  ], [])

  useFrame(() => {
    if (groupRef.current) {
      groupRef.current.rotation.z += 0.0003
    }
  })

  if (!themeConfig.showNebulaDust) return null

  return (
    <group ref={groupRef}>
      {dustConfigs.map((dust, i) => (
        <Billboard key={i} position={dust.pos}>
          <mesh>
            <planeGeometry args={[dust.size, dust.size]} />
            <meshBasicMaterial
              map={texture}
              color={dustColor}
              transparent
              opacity={dust.opacity}
              depthWrite={false}
              side={THREE.DoubleSide}
            />
          </mesh>
        </Billboard>
      ))}
    </group>
  )
}

// ─── 自动旋转控制 ────────────────────────────────────────────────────────

export function AutoRotateControls(): React.ReactElement {
  const controlsRef = React.useRef<any>(null)
  const userInteracted = React.useRef(false)
  const idleTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  const handleInteractionStart = React.useCallback(() => {
    userInteracted.current = true
    if (controlsRef.current) {
      controlsRef.current.autoRotate = false
    }
    if (idleTimer.current) clearTimeout(idleTimer.current)
    idleTimer.current = setTimeout(() => {
      if (controlsRef.current) {
        controlsRef.current.autoRotate = true
      }
      userInteracted.current = false
    }, 3000)
  }, [])

  return (
    <OrbitControls
      ref={controlsRef}
      enableDamping
      dampingFactor={0.05}
      autoRotate
      autoRotateSpeed={0.3}
      onStart={handleInteractionStart}
    />
  )
}

// ─── 边连线 ─────────────────────────────────────────────────────────────

export interface EdgeLinesProps {
  edges: { from: string; to: string; id?: string }[]
  positions: NodePosition[]
  hoveredNodeId: string | null
  themeConfig: NebulaThemeConfig
  resolvedTheme: 'light' | 'dark'
}

export function EdgeLines({ edges, positions, hoveredNodeId, themeConfig, resolvedTheme }: EdgeLinesProps): React.ReactElement {
  const posMap = React.useMemo(() => {
    const map = new Map<string, [number, number, number]>()
    for (const p of positions) {
      map.set(p.id, [p.x, p.y, p.z])
    }
    return map
  }, [positions])

  const edgeColor = resolvedTheme === 'dark' ? '#94a3b8' : '#64748b'

  return (
    <group>
      {edges.map((edge, idx) => {
        if (!edge.from) return null
        const from = posMap.get(edge.from)
        const to = posMap.get(edge.to)
        if (!from || !to) return null
        const isHighlighted = hoveredNodeId === edge.from || hoveredNodeId === edge.to
        return (
          <Line
            key={edge.id ?? idx}
            points={[from, to]}
            color={edgeColor}
            opacity={isHighlighted ? themeConfig.edgeHighlightOpacity : themeConfig.edgeOpacity}
            transparent
            lineWidth={isHighlighted ? 2.0 : 0.8}
          />
        )
      })}
    </group>
  )
}

// ─── StarNode 组件 ───────────────────────────────────────────────────────

export interface StarNodeProps {
  id: string
  label?: string                 // tooltip text; falls back to id when absent
  position: [number, number, number]
  color: string
  emissive: string
  radius: number
  emissiveIntensity?: number     // default: 1.0; multiplied by themeConfig.emissiveScale
  noiseFreq?: number             // default 1.5
  noiseAmp?: number              // default 0.7
  flowSpeed?: number             // default 0.2
  opacity?: number               // default 1.0
  segments?: number              // sphere segments; default 12, pass 16 for large-kind nodes
  isHovered: boolean
  onHover: (id: string | null) => void
  onClick: (id: string) => void
  themeConfig: NebulaThemeConfig
}

export function StarNode({ id, label, position, color, emissive, radius, emissiveIntensity, noiseFreq, noiseAmp, flowSpeed, opacity, segments, isHovered, onHover, onClick, themeConfig }: StarNodeProps): React.ReactElement {
  const groupRef = React.useRef<THREE.Group>(null)
  const coreRef = React.useRef<THREE.Mesh>(null)
  const coreMaterialRef = React.useRef<THREE.ShaderMaterial>(null)
  const glowRef = React.useRef<THREE.ShaderMaterial>(null)
  const phase = React.useMemo(() => hashCode(id) % 100, [id])

  const resolvedSegments = segments ?? 12
  const resolvedNoiseFreq = noiseFreq ?? 1.5
  const resolvedNoiseAmp = noiseAmp ?? 0.7
  const resolvedFlowSpeed = flowSpeed ?? 0.2
  const resolvedOpacity = opacity ?? 1.0
  const resolvedEmissiveIntensity = emissiveIntensity ?? 1.0
  const baseEmissive = resolvedEmissiveIntensity * themeConfig.emissiveScale

  const baseColorVec = React.useMemo(() => new THREE.Color(color), [color])
  const emissiveColorVec = React.useMemo(() => new THREE.Color(emissive), [emissive])
  const glowColor = React.useMemo(() => new THREE.Color(color), [color])

  // Uniforms for core star shader — created once, updated via ref
  const coreUniforms = React.useMemo(() => ({
    baseColor: { value: baseColorVec },
    emissiveColor: { value: emissiveColorVec },
    emissiveIntensity: { value: baseEmissive },
    time: { value: 0.0 },
    noiseFreq: { value: resolvedNoiseFreq },
    noiseAmp: { value: resolvedNoiseAmp },
    flowSpeed: { value: resolvedFlowSpeed },
    opacity: { value: resolvedOpacity },
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }), [])

  useFrame((state) => {
    // Group-level hover scale
    if (groupRef.current) {
      const target = isHovered ? 1.3 : 1.0
      const current = groupRef.current.scale.x
      if (Math.abs(current - target) > 0.001) {
        groupRef.current.scale.setScalar(current + (target - current) * 0.12)
      }
    }
    // Update time uniform (drives surface texture flow)
    if (coreMaterialRef.current) {
      coreMaterialRef.current.uniforms.time.value = state.clock.getElapsedTime() + phase * 0.1
      coreMaterialRef.current.uniforms.emissiveIntensity.value = baseEmissive + Math.sin(state.clock.getElapsedTime() * 1.5 + phase) * 0.08
    }
    // Glow intensity
    if (glowRef.current) {
      glowRef.current.uniforms.intensity.value = isHovered ? 0.7 : 0.35
    }
  })

  return (
    <group ref={groupRef} position={position}>
      {/* 核心几何体 — 自定义星体表面 shader */}
      <mesh
        ref={coreRef}
        onPointerOver={(e: ThreeEvent<PointerEvent>) => { e.stopPropagation(); onHover(id) }}
        onPointerOut={() => onHover(null)}
        onClick={(e: ThreeEvent<MouseEvent>) => { e.stopPropagation(); onClick(id) }}
      >
        <sphereGeometry args={[radius, resolvedSegments, resolvedSegments]} />
        <shaderMaterial
          ref={coreMaterialRef}
          vertexShader={starVertexShader}
          fragmentShader={starFragmentShader}
          uniforms={coreUniforms}
          transparent={resolvedOpacity < 1.0}
          toneMapped={false}
        />
      </mesh>

      {/* Fresnel 光晕外壳 */}
      <mesh>
        <sphereGeometry args={[radius * 2.0, 16, 16]} />
        <shaderMaterial
          ref={glowRef}
          vertexShader={glowVertexShader}
          fragmentShader={glowFragmentShader}
          blending={THREE.AdditiveBlending}
          transparent
          side={THREE.BackSide}
          depthWrite={false}
          depthTest={true}
          uniforms={{
            glowColor: { value: glowColor },
            intensity: { value: 0.35 },
          }}
        />
      </mesh>

      {/* Hover 浮层 */}
      {isHovered && (
        <Html center distanceFactor={200} style={{ pointerEvents: 'none' }}>
          <div className="bg-popover/95 backdrop-blur-md text-popover-foreground text-[11px] px-2 py-1 rounded-md shadow-lg whitespace-nowrap border border-border/50 max-w-[180px] truncate">
            {label ?? id}
          </div>
        </Html>
      )}
    </group>
  )
}
