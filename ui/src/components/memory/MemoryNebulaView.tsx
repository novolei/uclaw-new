/**
 * MemoryNebulaView — 3D 宇宙星云可视化（增强版）。
 *
 * 使用 @react-three/fiber + drei 渲染记忆图谱节点为星球，
 * 边为连线。螺旋星系布局，Fresnel 光晕，主题自适应星云雾气。
 */
import * as React from 'react'
import { Canvas, useFrame, type ThreeEvent } from '@react-three/fiber'
import { OrbitControls, Stars, Line, Html, Billboard } from '@react-three/drei'
import * as THREE from 'three'
import { useAtomValue } from 'jotai'
import { resolvedThemeAtom, themeStyleAtom } from '@/atoms/theme'
import { cn } from '@/lib/utils'
import type { MemoryGraphData, MemoryNode, MemoryEdge, MemoryNodeKind } from '@/lib/types'
import type { ThemeStyle } from '@/lib/chat-types'

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

// ─── 主题自适应配置 ──────────────────────────────────────────────────────

interface NebulaThemeConfig {
  fogColor: string
  ambientColor: string
  ambientIntensity: number
  pointLightColor: string
  pointLightIntensity: number
  starsFactor: number
  starsFade: boolean
  edgeOpacity: number
  edgeHighlightOpacity: number
  emissiveScale: number
  showNebulaDust: boolean
}

function getNebulaThemeConfig(resolved: 'light' | 'dark', style: ThemeStyle): NebulaThemeConfig {
  const darkDefault: NebulaThemeConfig = {
    fogColor: '#0a0a1a',
    ambientColor: '#1a1a3a',
    ambientIntensity: 0.3,
    pointLightColor: '#ffffff',
    pointLightIntensity: 0.6,
    starsFactor: 5,
    starsFade: true,
    edgeOpacity: 0.12,
    edgeHighlightOpacity: 0.45,
    emissiveScale: 1.0,
    showNebulaDust: true,
  }
  const lightDefault: NebulaThemeConfig = {
    fogColor: '#e8eaf0',
    ambientColor: '#f0f4ff',
    ambientIntensity: 0.7,
    pointLightColor: '#ffffff',
    pointLightIntensity: 0.4,
    starsFactor: 2,
    starsFade: false,
    edgeOpacity: 0.08,
    edgeHighlightOpacity: 0.30,
    emissiveScale: 0.5,
    showNebulaDust: false,
  }

  if (resolved === 'dark') {
    switch (style) {
      case 'ocean-dark':
        return { ...darkDefault, fogColor: '#0a1628', ambientColor: '#1a3050' }
      case 'forest-dark':
        return { ...darkDefault, fogColor: '#0a1a0f', ambientColor: '#1a3a20', ambientIntensity: 0.35, starsFactor: 4, emissiveScale: 0.9 }
      case 'qingye':
        return { ...darkDefault, fogColor: '#0f1a12', ambientColor: '#1a3522', ambientIntensity: 0.35, starsFactor: 4, emissiveScale: 0.9 }
      case 'black':
        return { ...darkDefault, fogColor: '#000000', ambientColor: '#0a0a0a', ambientIntensity: 0.2, starsFactor: 7, emissiveScale: 1.2 }
      case 'the-finals':
        return { ...darkDefault, fogColor: '#1a0a1e', ambientColor: '#2a1030', emissiveScale: 1.1 }
      case 'slate-dark':
        return { ...darkDefault, fogColor: '#0f1419', ambientColor: '#1a2530' }
      default:
        return darkDefault
    }
  } else {
    switch (style) {
      case 'ocean-light':
        return { ...lightDefault, fogColor: '#e0f0ff', ambientColor: '#c8e8ff' }
      case 'forest-light':
        return { ...lightDefault, fogColor: '#e8f5e8', ambientColor: '#d0f0d0' }
      case 'warm-paper':
        return { ...lightDefault, fogColor: '#f5f0e8', ambientColor: '#faf5e8', ambientIntensity: 0.8, starsFactor: 1.5, emissiveScale: 0.4 }
      case 'slate-light':
        return { ...lightDefault, fogColor: '#e8ecf0', ambientColor: '#d0d8e0' }
      default:
        return lightDefault
    }
  }
}

// ─── 螺旋星系布局 ────────────────────────────────────────────────────────

interface NodePosition {
  id: string
  x: number
  y: number
  z: number
}

function gaussian(): number {
  const u1 = Math.random() || 0.001
  const u2 = Math.random()
  return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2)
}

function computeGalaxyLayout(nodes: MemoryNode[], edges: MemoryEdge[]): NodePosition[] {
  if (nodes.length === 0) return []

  const coreKinds: MemoryNodeKind[] = ['boot', 'identity', 'directive']
  const midKinds: MemoryNodeKind[] = ['value', 'user_profile', 'curated']

  const coreNodes: MemoryNode[] = []
  const midNodes: MemoryNode[] = []
  const outerNodes: MemoryNode[] = []

  for (const n of nodes) {
    if (coreKinds.includes(n.kind)) coreNodes.push(n)
    else if (midKinds.includes(n.kind)) midNodes.push(n)
    else outerNodes.push(n)
  }

  const armCount = 3

  function placeGroup(group: MemoryNode[], baseRadius: number, spread: number): { id: string; x: number; y: number; z: number }[] {
    const groupSize = group.length || 1
    return group.map((node, idx) => {
      const arm = idx % armCount
      const armBaseAngle = (arm * 2 * Math.PI) / armCount
      const progress = idx / groupSize
      const spiralAngle = armBaseAngle + progress * Math.PI * 2.0
      const r = baseRadius + spread * Math.sqrt(progress)
      const jitter = spread * 0.25

      const x = r * Math.cos(spiralAngle) + gaussian() * jitter
      const y = (Math.random() - 0.5) * spread * 0.3
      const z = r * Math.sin(spiralAngle) + gaussian() * jitter
      return { id: node.id, x, y, z }
    })
  }

  const allPositions = [
    ...placeGroup(coreNodes, 0, 60),
    ...placeGroup(midNodes, 70, 70),
    ...placeGroup(outerNodes, 140, 80),
  ]

  // 力导向迭代优化
  const idxMap = new Map<string, number>()
  const sim = allPositions.map((p, i) => {
    idxMap.set(p.id, i)
    return { ...p, vx: 0, vy: 0, vz: 0 }
  })

  const iterations = 50
  const repulsion = 2000
  const attraction = 0.003
  const centerForce = 0.005
  const damping = 0.8

  for (let iter = 0; iter < iterations; iter++) {
    for (let i = 0; i < sim.length; i++) {
      for (let j = i + 1; j < sim.length; j++) {
        const dx = sim[i].x - sim[j].x
        const dy = sim[i].y - sim[j].y
        const dz = sim[i].z - sim[j].z
        const distSq = dx * dx + dy * dy + dz * dz + 1
        const force = repulsion / distSq
        const dist = Math.sqrt(distSq)
        const fx = (dx / dist) * force
        const fy = (dy / dist) * force
        const fz = (dz / dist) * force
        sim[i].vx += fx; sim[i].vy += fy; sim[i].vz += fz
        sim[j].vx -= fx; sim[j].vy -= fy; sim[j].vz -= fz
      }
    }

    for (const edge of edges) {
      const si = edge.parentNodeId ? idxMap.get(edge.parentNodeId) : undefined
      const ti = idxMap.get(edge.childNodeId)
      if (si === undefined || ti === undefined) continue
      const dx = sim[ti].x - sim[si].x
      const dy = sim[ti].y - sim[si].y
      const dz = sim[ti].z - sim[si].z
      const fx = dx * attraction
      const fy = dy * attraction
      const fz = dz * attraction
      sim[si].vx += fx; sim[si].vy += fy; sim[si].vz += fz
      sim[ti].vx -= fx; sim[ti].vy -= fy; sim[ti].vz -= fz
    }

    for (const p of sim) {
      p.vx -= p.x * centerForce
      p.vy -= p.y * centerForce
      p.vz -= p.z * centerForce
      p.vx *= damping; p.vy *= damping; p.vz *= damping
      p.x += p.vx; p.y += p.vy; p.z += p.vz
    }
  }

  return sim.map(({ id, x, y, z }) => ({ id, x, y, z }))
}

// ─── Fresnel 光晕 Shader ─────────────────────────────────────────────────

const glowVertexShader = `
varying vec3 vNormal;
varying vec3 vPosition;
void main() {
  vNormal = normalize(normalMatrix * normal);
  vPosition = (modelViewMatrix * vec4(position, 1.0)).xyz;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`

const glowFragmentShader = `
uniform vec3 glowColor;
uniform float intensity;
varying vec3 vNormal;
varying vec3 vPosition;
void main() {
  vec3 viewDir = normalize(-vPosition);
  float rim = 1.0 - max(dot(viewDir, vNormal), 0.0);
  float softGlow = pow(rim, 1.5) * 0.6;
  float coreGlow = pow(rim, 4.0) * 1.0;
  float combined = (softGlow + coreGlow) * intensity;
  gl_FragColor = vec4(glowColor, combined * 0.5);
}
`

// ─── Star Surface Shader ─────────────────────────────────────────────────

const starVertexShader = `
varying vec3 vNormal;
varying vec3 vPosition;
varying vec2 vUv;
varying vec3 vWorldPos;

void main() {
  vNormal = normalize(normalMatrix * normal);
  vPosition = (modelViewMatrix * vec4(position, 1.0)).xyz;
  vUv = uv;
  vWorldPos = position;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`

const starFragmentShader = `
uniform vec3 baseColor;
uniform vec3 emissiveColor;
uniform float emissiveIntensity;
uniform float time;
uniform float noiseFreq;
uniform float noiseAmp;
uniform float flowSpeed;
uniform float opacity;

varying vec3 vNormal;
varying vec3 vPosition;
varying vec2 vUv;
varying vec3 vWorldPos;

vec4 permute(vec4 x) { return mod(((x*34.0)+1.0)*x, 289.0); }
vec4 taylorInvSqrt(vec4 r) { return 1.79284291400159 - 0.85373472095314 * r; }

float snoise(vec3 v) {
  const vec2 C = vec2(1.0/6.0, 1.0/3.0);
  const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
  vec3 i  = floor(v + dot(v, C.yyy));
  vec3 x0 = v - i + dot(i, C.xxx);
  vec3 g  = step(x0.yzx, x0.xyz);
  vec3 l  = 1.0 - g;
  vec3 i1 = min(g.xyz, l.zxy);
  vec3 i2 = max(g.xyz, l.zxy);
  vec3 x1 = x0 - i1 + C.xxx;
  vec3 x2 = x0 - i2 + C.yyy;
  vec3 x3 = x0 - D.yyy;
  i = mod(i, 289.0);
  vec4 p = permute(permute(permute(
    i.z + vec4(0.0, i1.z, i2.z, 1.0))
    + i.y + vec4(0.0, i1.y, i2.y, 1.0))
    + i.x + vec4(0.0, i1.x, i2.x, 1.0));
  float n_ = 1.0/7.0;
  vec3 ns = n_ * D.wyz - D.xzx;
  vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
  vec4 x_ = floor(j * ns.z);
  vec4 y_ = floor(j - 7.0 * x_);
  vec4 x  = x_ * ns.x + ns.yyyy;
  vec4 y  = y_ * ns.x + ns.yyyy;
  vec4 h  = 1.0 - abs(x) - abs(y);
  vec4 b0 = vec4(x.xy, y.xy);
  vec4 b1 = vec4(x.zw, y.zw);
  vec4 s0 = floor(b0)*2.0 + 1.0;
  vec4 s1 = floor(b1)*2.0 + 1.0;
  vec4 sh = -step(h, vec4(0.0));
  vec4 a0 = b0.xzyw + s0.xzyw*sh.xxyy;
  vec4 a1 = b1.xzyw + s1.xzyw*sh.zzww;
  vec3 p0 = vec3(a0.xy, h.x);
  vec3 p1 = vec3(a0.zw, h.y);
  vec3 p2 = vec3(a1.xy, h.z);
  vec3 p3 = vec3(a1.zw, h.w);
  vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2,p2), dot(p3,p3)));
  p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
  vec4 m = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
  m = m * m;
  return 42.0 * dot(m*m, vec4(dot(p0,x0), dot(p1,x1), dot(p2,x2), dot(p3,x3)));
}

float fbm(vec3 p) {
  float f = 0.0;
  f += 0.5 * snoise(p);
  f += 0.25 * snoise(p * 2.0);
  f += 0.125 * snoise(p * 4.0);
  return f;
}

void main() {
  vec3 samplePos = vWorldPos * noiseFreq + vec3(time * flowSpeed, time * flowSpeed * 0.7, time * flowSpeed * 0.3);
  float noise = fbm(samplePos) * noiseAmp;

  float brightness = 0.6 + noise * 0.4;
  vec3 hotColor = emissiveColor * emissiveIntensity * brightness;

  vec3 finalColor = mix(baseColor * 0.3, hotColor, brightness);

  vec3 viewDir = normalize(-vPosition);
  float rim = 1.0 - max(dot(viewDir, vNormal), 0.0);
  float rimBoost = pow(rim, 2.0) * 0.3;
  finalColor += emissiveColor * rimBoost;

  gl_FragColor = vec4(finalColor, opacity);
}
`

// ─── StarNode 组件 ───────────────────────────────────────────────────────

interface StarNodeProps {
  node: MemoryNode
  position: [number, number, number]
  isHovered: boolean
  onHover: (id: string | null) => void
  onClick: (id: string) => void
  themeConfig: NebulaThemeConfig
}

function hashCode(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) {
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0
  }
  return Math.abs(h)
}

function StarNode({ node, position, isHovered, onHover, onClick, themeConfig }: StarNodeProps): React.ReactElement {
  const groupRef = React.useRef<THREE.Group>(null)
  const coreRef = React.useRef<THREE.Mesh>(null)
  const coreMaterialRef = React.useRef<THREE.ShaderMaterial>(null)
  const glowRef = React.useRef<THREE.ShaderMaterial>(null)
  const config = getNodeConfig(node)
  const phase = React.useMemo(() => hashCode(node.id) % 100, [node.id])
  const baseEmissive = config.emissiveIntensity * themeConfig.emissiveScale

  const baseColorVec = React.useMemo(() => new THREE.Color(config.color), [config.color])
  const emissiveColorVec = React.useMemo(() => new THREE.Color(config.emissive), [config.emissive])
  const glowColor = React.useMemo(() => new THREE.Color(config.color), [config.color])

  const isLargeNode = node.kind === 'boot' || node.kind === 'directive' || node.kind === 'identity' || node.kind === 'procedure'
  const segments = isLargeNode ? 16 : 12

  // Uniforms for core star shader — created once, updated via ref
  const coreUniforms = React.useMemo(() => ({
    baseColor: { value: baseColorVec },
    emissiveColor: { value: emissiveColorVec },
    emissiveIntensity: { value: baseEmissive },
    time: { value: 0.0 },
    noiseFreq: { value: config.noiseFreq },
    noiseAmp: { value: config.noiseAmp },
    flowSpeed: { value: config.flowSpeed },
    opacity: { value: node.kind === 'reference' ? 0.6 : 1.0 },
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
        onPointerOver={(e: ThreeEvent<PointerEvent>) => { e.stopPropagation(); onHover(node.id) }}
        onPointerOut={() => onHover(null)}
        onClick={(e: ThreeEvent<MouseEvent>) => { e.stopPropagation(); onClick(node.id) }}
      >
        <sphereGeometry args={[config.radius, segments, segments]} />
        <shaderMaterial
          ref={coreMaterialRef}
          vertexShader={starVertexShader}
          fragmentShader={starFragmentShader}
          uniforms={coreUniforms}
          transparent={node.kind === 'reference'}
          toneMapped={false}
        />
      </mesh>

      {/* Fresnel 光晕外壳 */}
      <mesh>
        <sphereGeometry args={[config.radius * 2.0, 16, 16]} />
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
            {node.title}
          </div>
        </Html>
      )}
    </group>
  )
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
        return (
          <StarNode
            key={node.id}
            node={node}
            position={pos}
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

// ─── 边连线 ─────────────────────────────────────────────────────────────

interface MemoryEdgeLinesProps {
  edges: MemoryEdge[]
  positions: NodePosition[]
  hoveredNodeId: string | null
  themeConfig: NebulaThemeConfig
  resolvedTheme: 'light' | 'dark'
}

function MemoryEdgeLines({ edges, positions, hoveredNodeId, themeConfig, resolvedTheme }: MemoryEdgeLinesProps): React.ReactElement {
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
      {edges.map((edge) => {
        if (!edge.parentNodeId) return null
        const from = posMap.get(edge.parentNodeId)
        const to = posMap.get(edge.childNodeId)
        if (!from || !to) return null
        const isHighlighted = hoveredNodeId === edge.parentNodeId || hoveredNodeId === edge.childNodeId
        return (
          <Line
            key={edge.id}
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

// ─── 星云雾气装饰 ────────────────────────────────────────────────────────

const createNebulaTexture = (): THREE.CanvasTexture => {
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

function NebulaDust({ themeConfig }: { themeConfig: NebulaThemeConfig }): React.ReactElement | null {
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

function AutoRotateControls(): React.ReactElement {
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
      <MemoryEdgeLines
        edges={graphData.edges}
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
    return computeGalaxyLayout(graphData.nodes, graphData.edges)
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
