/**
 * MemoryGraphView - 记忆图网络可视化
 *
 * 使用 Canvas 渲染力导向布局的记忆图。
 * 节点按 kind 配色，支持拖拽、缩放、点击选择。
 */

import * as React from 'react'
import { Loader2, ZoomIn, ZoomOut, Maximize2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { memoryGraphGetFullGraph } from '@/lib/tauri-bridge'
import type { MemoryNode, MemoryEdge, MemoryGraphData, MemoryNodeKind } from '@/lib/types'

// ─── Kind 配色 ──────────────────────────────────────────────────────────

const KIND_COLORS: Record<MemoryNodeKind, string> = {
  boot: '#ef4444',
  identity: '#a855f7',
  value: '#3b82f6',
  user_profile: '#22c55e',
  directive: '#f97316',
  curated: '#0ea5e9',
  episode: '#eab308',
  procedure: '#14b8a6',
  reference: '#6b7280',
}

// ─── Simulation Types ───────────────────────────────────────────────────

interface SimNode {
  id: string
  kind: MemoryNodeKind
  title: string
  x: number
  y: number
  vx: number
  vy: number
  radius: number
}

interface SimEdge {
  source: string
  target: string
}

// ─── Barnes-Hut QuadTree ──────────────────────────────────────────────

interface QTRect {
  x: number
  y: number
  width: number
  height: number
}

interface QTPoint {
  x: number
  y: number
  mass: number
  id: string
}

class QuadTree {
  bounds: QTRect
  points: QTPoint[] = []
  divided = false
  nw: QuadTree | null = null
  ne: QuadTree | null = null
  sw: QuadTree | null = null
  se: QuadTree | null = null
  totalMass = 0
  comX = 0
  comY = 0

  constructor(bounds: QTRect) {
    this.bounds = bounds
  }

  private contains(p: QTPoint): boolean {
    const b = this.bounds
    return p.x >= b.x && p.x < b.x + b.width && p.y >= b.y && p.y < b.y + b.height
  }

  private subdivide(): void {
    const { x, y, width, height } = this.bounds
    const hw = width / 2
    const hh = height / 2
    this.nw = new QuadTree({ x, y, width: hw, height: hh })
    this.ne = new QuadTree({ x: x + hw, y, width: hw, height: hh })
    this.sw = new QuadTree({ x, y: y + hh, width: hw, height: hh })
    this.se = new QuadTree({ x: x + hw, y: y + hh, width: hw, height: hh })
    this.divided = true

    for (const pt of this.points) {
      this.nw.insert(pt) || this.ne.insert(pt) || this.sw.insert(pt) || this.se.insert(pt)
    }
    this.points = []
  }

  insert(point: QTPoint): boolean {
    if (!this.contains(point)) return false

    const newTotal = this.totalMass + point.mass
    this.comX = (this.comX * this.totalMass + point.x * point.mass) / newTotal
    this.comY = (this.comY * this.totalMass + point.y * point.mass) / newTotal
    this.totalMass = newTotal

    if (!this.divided && this.points.length < 1) {
      this.points.push(point)
      return true
    }

    if (!this.divided) this.subdivide()
    return this.nw!.insert(point) || this.ne!.insert(point) || this.sw!.insert(point) || this.se!.insert(point)
  }

  calculateForce(node: { x: number; y: number; id: string }, repulsion: number, theta = 0.7): { fx: number; fy: number } {
    if (this.totalMass === 0) return { fx: 0, fy: 0 }

    const dx = this.comX - node.x
    const dy = this.comY - node.y
    const distSq = dx * dx + dy * dy + 1
    const dist = Math.sqrt(distSq)
    const size = this.bounds.width

    if (!this.divided || size / dist < theta) {
      if (this.totalMass === 1 && this.points.length === 1 && this.points[0].id === node.id) {
        return { fx: 0, fy: 0 }
      }
      const force = this.totalMass * repulsion / distSq
      return { fx: -(dx / dist) * force, fy: -(dy / dist) * force }
    }

    let fx = 0
    let fy = 0
    for (const child of [this.nw, this.ne, this.sw, this.se]) {
      if (child && child.totalMass > 0) {
        const f = child.calculateForce(node, repulsion, theta)
        fx += f.fx
        fy += f.fy
      }
    }
    return { fx, fy }
  }
}

function computeQuadTreeBounds(nodes: SimNode[]): QTRect {
  if (nodes.length === 0) return { x: -500, y: -500, width: 1000, height: 1000 }
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  for (const n of nodes) {
    if (n.x < minX) minX = n.x
    if (n.y < minY) minY = n.y
    if (n.x > maxX) maxX = n.x
    if (n.y > maxY) maxY = n.y
  }
  const padding = 50
  const size = Math.max(maxX - minX, maxY - minY) + padding * 2
  return { x: minX - padding, y: minY - padding, width: size, height: size }
}

// ─── Props ──────────────────────────────────────────────────────────────

interface MemoryGraphViewProps {
  onSelectNode?: (nodeId: string) => void
  className?: string
}

export function MemoryGraphView({
  onSelectNode,
  className,
}: MemoryGraphViewProps): React.ReactElement {
  const canvasRef = React.useRef<HTMLCanvasElement>(null)
  const containerRef = React.useRef<HTMLDivElement>(null)
  const [loading, setLoading] = React.useState(true)
  const [graphData, setGraphData] = React.useState<MemoryGraphData | null>(null)

  // Simulation state refs (to avoid re-renders during animation)
  const nodesRef = React.useRef<SimNode[]>([])
  const edgesRef = React.useRef<SimEdge[]>([])
  const transformRef = React.useRef({ x: 0, y: 0, scale: 1 })
  const dragRef = React.useRef<{
    nodeId: string | null
    panning: boolean
    startX: number
    startY: number
    startTx: number
    startTy: number
  }>({ nodeId: null, panning: false, startX: 0, startY: 0, startTx: 0, startTy: 0 })
  const animFrameRef = React.useRef<number>(0)
  const hoveredRef = React.useRef<string | null>(null)

  // Load graph data
  React.useEffect(() => {
    setLoading(true)
    memoryGraphGetFullGraph()
      .then((res) => {
        const data = res as MemoryGraphData
        setGraphData(data)
      })
      .catch((err) => console.error('[MemoryGraphView] 加载图数据失败:', err))
      .finally(() => setLoading(false))
  }, [])

  // Initialize simulation nodes from graph data
  React.useEffect(() => {
    if (!graphData) return

    const { nodes, edges } = graphData
    const simNodes: SimNode[] = nodes.map((n, i) => {
      const angle = (2 * Math.PI * i) / Math.max(nodes.length, 1)
      const r = 150 + Math.random() * 100
      return {
        id: n.id,
        kind: n.kind,
        title: n.title,
        x: Math.cos(angle) * r,
        y: Math.sin(angle) * r,
        vx: 0,
        vy: 0,
        radius: n.kind === 'boot' ? 10 : 7,
      }
    })
    const simEdges: SimEdge[] = edges
      .filter((e) => e.parentNodeId)
      .map((e) => ({ source: e.parentNodeId!, target: e.childNodeId }))

    nodesRef.current = simNodes
    edgesRef.current = simEdges
  }, [graphData])

  // ─── Force simulation + canvas render loop ────────────────────────────

  React.useEffect(() => {
    const canvas = canvasRef.current
    const container = containerRef.current
    if (!canvas || !container) return

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    let running = true

    const resizeCanvas = (): void => {
      const rect = container.getBoundingClientRect()
      const dpr = window.devicePixelRatio || 1
      canvas.width = rect.width * dpr
      canvas.height = rect.height * dpr
      canvas.style.width = `${rect.width}px`
      canvas.style.height = `${rect.height}px`
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    }

    resizeCanvas()
    const ro = new ResizeObserver(resizeCanvas)
    ro.observe(container)

    // Simple force simulation step
    const simulate = (): void => {
      const nodes = nodesRef.current
      const edges = edgesRef.current
      if (nodes.length === 0) return

      const alpha = 0.3
      const repulsion = 3000
      const linkDist = 80
      const linkStrength = 0.05
      const centerStrength = 0.01
      const damping = 0.85

      // Center force
      for (const n of nodes) {
        n.vx -= n.x * centerStrength
        n.vy -= n.y * centerStrength
      }

      // Repulsion (Barnes-Hut O(N log N))
      const tree = new QuadTree(computeQuadTreeBounds(nodes))
      for (const n of nodes) {
        tree.insert({ x: n.x, y: n.y, mass: 1, id: n.id })
      }
      for (const n of nodes) {
        const { fx, fy } = tree.calculateForce(n, repulsion)
        n.vx += fx
        n.vy += fy
      }

      // Link forces
      const nodeMap = new Map(nodes.map((n) => [n.id, n]))
      for (const edge of edges) {
        const s = nodeMap.get(edge.source)
        const t = nodeMap.get(edge.target)
        if (!s || !t) continue
        const dx = t.x - s.x
        const dy = t.y - s.y
        const dist = Math.sqrt(dx * dx + dy * dy) || 1
        const displacement = (dist - linkDist) * linkStrength
        const fx = (dx / dist) * displacement
        const fy = (dy / dist) * displacement
        s.vx += fx
        s.vy += fy
        t.vx -= fx
        t.vy -= fy
      }

      // Integrate
      for (const n of nodes) {
        if (dragRef.current.nodeId === n.id) continue
        n.vx *= damping
        n.vy *= damping
        n.x += n.vx * alpha
        n.y += n.vy * alpha
      }
    }

    // Render
    const render = (): void => {
      const w = canvas.width / (window.devicePixelRatio || 1)
      const h = canvas.height / (window.devicePixelRatio || 1)
      const { x: tx, y: ty, scale } = transformRef.current
      const nodes = nodesRef.current
      const edges = edgesRef.current
      const nodeMap = new Map(nodes.map((n) => [n.id, n]))
      const hovered = hoveredRef.current

      ctx.clearRect(0, 0, w, h)
      ctx.save()
      ctx.translate(w / 2 + tx, h / 2 + ty)
      ctx.scale(scale, scale)

      // Edges
      ctx.strokeStyle = 'rgba(148, 163, 184, 0.2)'
      ctx.lineWidth = 1
      for (const edge of edges) {
        const s = nodeMap.get(edge.source)
        const t = nodeMap.get(edge.target)
        if (!s || !t) continue
        ctx.beginPath()
        ctx.moveTo(s.x, s.y)
        ctx.lineTo(t.x, t.y)
        ctx.stroke()
      }

      // Nodes
      for (const n of nodes) {
        const color = KIND_COLORS[n.kind] ?? '#6b7280'
        const isHovered = hovered === n.id

        ctx.beginPath()
        ctx.arc(n.x, n.y, n.radius * (isHovered ? 1.3 : 1), 0, Math.PI * 2)
        ctx.fillStyle = color
        ctx.globalAlpha = isHovered ? 1 : 0.85
        ctx.fill()
        ctx.globalAlpha = 1

        if (isHovered) {
          ctx.strokeStyle = color
          ctx.lineWidth = 2
          ctx.stroke()
        }

        // Label
        ctx.fillStyle = 'rgba(255,255,255,0.9)'
        ctx.font = `${isHovered ? 11 : 9}px system-ui, sans-serif`
        ctx.textAlign = 'center'
        ctx.textBaseline = 'top'
        const label = n.title.length > 16 ? n.title.slice(0, 15) + '…' : n.title
        ctx.fillText(label, n.x, n.y + n.radius + 3)
      }

      ctx.restore()
    }

    const loop = (): void => {
      if (!running) return
      simulate()
      render()
      animFrameRef.current = requestAnimationFrame(loop)
    }
    loop()

    // ─── Interaction handlers ─────────────────────────────────────────

    const screenToWorld = (clientX: number, clientY: number): { wx: number; wy: number } => {
      const rect = canvas.getBoundingClientRect()
      const { x: tx, y: ty, scale } = transformRef.current
      const cx = clientX - rect.left - rect.width / 2 - tx
      const cy = clientY - rect.top - rect.height / 2 - ty
      return { wx: cx / scale, wy: cy / scale }
    }

    const findNodeAt = (wx: number, wy: number): SimNode | null => {
      const nodes = nodesRef.current
      for (let i = nodes.length - 1; i >= 0; i--) {
        const n = nodes[i]
        const dx = wx - n.x
        const dy = wy - n.y
        if (dx * dx + dy * dy < (n.radius + 4) * (n.radius + 4)) return n
      }
      return null
    }

    const onMouseDown = (e: MouseEvent): void => {
      const { wx, wy } = screenToWorld(e.clientX, e.clientY)
      const node = findNodeAt(wx, wy)
      if (node) {
        dragRef.current = { nodeId: node.id, panning: false, startX: wx, startY: wy, startTx: 0, startTy: 0 }
      } else {
        dragRef.current = {
          nodeId: null,
          panning: true,
          startX: e.clientX,
          startY: e.clientY,
          startTx: transformRef.current.x,
          startTy: transformRef.current.y,
        }
      }
    }

    const onMouseMove = (e: MouseEvent): void => {
      const { wx, wy } = screenToWorld(e.clientX, e.clientY)
      const drag = dragRef.current

      if (drag.nodeId) {
        const n = nodesRef.current.find((n) => n.id === drag.nodeId)
        if (n) {
          n.x = wx
          n.y = wy
          n.vx = 0
          n.vy = 0
        }
      } else if (drag.panning) {
        transformRef.current.x = drag.startTx + (e.clientX - drag.startX)
        transformRef.current.y = drag.startTy + (e.clientY - drag.startY)
      } else {
        const node = findNodeAt(wx, wy)
        hoveredRef.current = node?.id ?? null
        canvas.style.cursor = node ? 'pointer' : 'grab'
      }
    }

    const onMouseUp = (e: MouseEvent): void => {
      const drag = dragRef.current
      if (drag.nodeId && !drag.panning) {
        const { wx, wy } = screenToWorld(e.clientX, e.clientY)
        const moved = Math.abs(wx - drag.startX) + Math.abs(wy - drag.startY)
        if (moved < 3) {
          onSelectNode?.(drag.nodeId)
        }
      }
      dragRef.current = { nodeId: null, panning: false, startX: 0, startY: 0, startTx: 0, startTy: 0 }
    }

    const onWheel = (e: WheelEvent): void => {
      e.preventDefault()
      const factor = e.deltaY > 0 ? 0.92 : 1.08
      transformRef.current.scale = Math.max(0.1, Math.min(5, transformRef.current.scale * factor))
    }

    canvas.addEventListener('mousedown', onMouseDown)
    canvas.addEventListener('mousemove', onMouseMove)
    canvas.addEventListener('mouseup', onMouseUp)
    canvas.addEventListener('mouseleave', onMouseUp)
    canvas.addEventListener('wheel', onWheel, { passive: false })

    return () => {
      running = false
      cancelAnimationFrame(animFrameRef.current)
      ro.disconnect()
      canvas.removeEventListener('mousedown', onMouseDown)
      canvas.removeEventListener('mousemove', onMouseMove)
      canvas.removeEventListener('mouseup', onMouseUp)
      canvas.removeEventListener('mouseleave', onMouseUp)
      canvas.removeEventListener('wheel', onWheel)
    }
  }, [graphData, onSelectNode])

  // ─── Zoom controls ────────────────────────────────────────────────

  const zoom = (factor: number): void => {
    transformRef.current.scale = Math.max(0.1, Math.min(5, transformRef.current.scale * factor))
  }

  const resetView = (): void => {
    transformRef.current = { x: 0, y: 0, scale: 1 }
  }

  return (
    <div ref={containerRef} className={cn('relative w-full h-full min-h-[300px]', className)}>
      {loading ? (
        <div className="absolute inset-0 flex items-center justify-center">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <>
          <canvas ref={canvasRef} className="absolute inset-0 cursor-grab" />

          {/* 缩放控制 */}
          <div className="absolute bottom-3 right-3 flex flex-col gap-1">
            <Button size="icon" variant="outline" className="h-7 w-7 bg-background/80" onClick={() => zoom(1.2)}>
              <ZoomIn className="size-3.5" />
            </Button>
            <Button size="icon" variant="outline" className="h-7 w-7 bg-background/80" onClick={() => zoom(0.8)}>
              <ZoomOut className="size-3.5" />
            </Button>
            <Button size="icon" variant="outline" className="h-7 w-7 bg-background/80" onClick={resetView}>
              <Maximize2 className="size-3.5" />
            </Button>
          </div>

          {/* 图例 */}
          <div className="absolute top-3 left-3 flex flex-wrap gap-2 bg-background/70 rounded-md px-2 py-1.5">
            {(Object.entries(KIND_COLORS) as [MemoryNodeKind, string][]).map(([kind, color]) => (
              <div key={kind} className="flex items-center gap-1 text-[10px] text-muted-foreground">
                <span className="inline-block size-2 rounded-full" style={{ backgroundColor: color }} />
                {kind}
              </div>
            ))}
          </div>

          {/* 空状态 */}
          {(!graphData || (graphData.nodes.length === 0)) && !loading && (
            <div className="absolute inset-0 flex items-center justify-center">
              <p className="text-sm text-muted-foreground">暂无记忆图数据</p>
            </div>
          )}
        </>
      )}
    </div>
  )
}
