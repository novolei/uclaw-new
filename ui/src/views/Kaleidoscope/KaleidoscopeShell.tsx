/**
 * KaleidoscopeShell — 万花筒 surface 的根组件。
 *
 * 布局：左侧 120px KaleidoscopeRail + 右侧主区。主区按 kaleidoscopeModuleAtom
 * 渲染对应模块；Phase 1 只有 humans 是真实实现，其余走 ComingSoonModule。
 *
 * 模块间切换用 motion 的 AnimatePresence 做 80ms slide-fade（与
 * AutomationsView.tsx 的子视图切换同模式）。key={moduleId} 触发重挂载。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import { KaleidoscopeRail } from './KaleidoscopeRail'
import { HumansModule } from './modules/Humans/HumansModule'
import { ComingSoonModule } from './modules/ComingSoonModule'

export function KaleidoscopeShell(): React.ReactElement {
  const moduleId = useAtomValue(kaleidoscopeModuleAtom)

  return (
    <div className="flex h-full min-h-0 bg-background">
      <KaleidoscopeRail />
      <div className="flex-1 min-w-0 min-h-0 relative">
        <AnimatePresence mode="wait">
          <motion.div
            key={moduleId}
            initial={{ opacity: 0, x: 12 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.08, ease: [0.32, 0.72, 0, 1] }}
            className="absolute inset-0"
          >
            {moduleId === 'humans' ? (
              <HumansModule />
            ) : (
              <ComingSoonModule moduleId={moduleId} />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}
