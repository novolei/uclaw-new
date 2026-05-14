/**
 * MemoryModule — 万花筒「记忆」模块。
 *
 * full-bleed wrap 现有 MemoryGraphView。MemoryGraphView 自包含
 * (根 relative w-full h-full,自己 fetch memory_graph_get_full_graph、
 * 自带筛选/缩放/节点详情),需要一个确定尺寸的父容器 —— 用 absolute
 * inset-0(KaleidoscopeShell 主区卡片是 relative)。与 HumansModule 同款,
 * 不叠 ModuleHeader。
 */
import * as React from 'react'
import { MemoryGraphView } from '@/components/memory/MemoryGraphView'

export function MemoryModule(): React.ReactElement {
  return (
    // titlebar-no-drag: MemoryGraphView is a full-bleed <canvas> with its own
    // mousedown/move/up drag-pan handlers — a window-drag region over it would
    // hijack every node-drag / pan gesture. The whole module opts out.
    <div className="titlebar-no-drag absolute inset-0">
      <MemoryGraphView />
    </div>
  )
}
