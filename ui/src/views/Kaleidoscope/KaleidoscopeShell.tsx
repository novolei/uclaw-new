/**
 * KaleidoscopeShell — 万花筒 surface 的根组件。
 *
 * 布局对齐 chat 窗口的 shell-bg 视觉系统：根容器无背景（让 AppShell 的
 * shell-bg 渐变透出 padding 间隙），rail 与主区各自是 rounded-2xl 浮卡。
 *
 * rail 在 p-2 pr-0 包裹里（与 chat 窗口 sidebar-wrapper 同款）；主区在 p-2
 * 包裹里、内层一张 rounded-2xl 卡片。主区按 kaleidoscopeModuleAtom 渲染模块。
 *
 * humaneSpecsAtom 在此加载一次（替代已退役的 AutomationsView 的同名 effect）
 * —— StoreView/StoreGrid 依赖它算"已安装"徽章，用户可能直接进应用商店模块。
 */
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import { humaneSpecsAtom } from '@/atoms/automation'
import { listAutomationsHumane } from '@/lib/tauri-bridge'
import { KaleidoscopeRail } from './KaleidoscopeRail'
import { HumansModule } from './modules/Humans/HumansModule'
import { StoreModule } from './modules/Store/StoreModule'
import { AppsModule } from './modules/Apps/AppsModule'
import { ComingSoonModule } from './modules/ComingSoonModule'

export function KaleidoscopeShell(): React.ReactElement {
  const moduleId = useAtomValue(kaleidoscopeModuleAtom)
  const setHumaneSpecs = useSetAtom(humaneSpecsAtom)

  // 加载已安装 specs 一次。StoreView/StoreGrid 读 humaneSpecsAtom 算"已安装"
  // 徽章但自己不 fetch（AutomationHub 有自己的 fetch，StoreView 没有）——
  // 用户可能直接进应用商店模块，所以在 Shell 层兜底加载一次。
  React.useEffect(() => {
    listAutomationsHumane()
      .then(setHumaneSpecs)
      .catch((err) => console.warn('[KaleidoscopeShell] failed to load installed specs:', err))
  }, [setHumaneSpecs])

  return (
    <div className="flex flex-1 min-w-0 min-h-0">
      {/* rail 浮卡 —— p-2 pr-0 对齐 chat 窗口 sidebar-wrapper */}
      <div className="p-2 pr-0 shrink-0">
        <KaleidoscopeRail />
      </div>
      {/* 主区浮卡 */}
      <div className="flex-1 min-w-0 min-h-0 p-2">
        <div className="h-full rounded-2xl shadow-xl bg-content-area overflow-hidden relative">
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
              ) : moduleId === 'store' ? (
                <StoreModule />
              ) : moduleId === 'apps' ? (
                <AppsModule />
              ) : (
                <ComingSoonModule moduleId={moduleId} />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </div>
    </div>
  )
}
