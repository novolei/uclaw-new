/**
 * MainArea — 主内容区域
 *
 * 顶层 surface 切换：'workspace'（任务流，WorkspaceShell）↔ 'kaleidoscope'
 * （配置流，KaleidoscopeShell）。两个 surface 用 motion 的 AnimatePresence
 * 做 200ms cross-dissolve（与 AutomationsView.tsx 的子视图切换同模式）。
 * 设置以浮窗形式叠加显示。
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { Panel } from '@/components/app-shell/Panel'
import { SettingsDialog } from '@/components/settings/SettingsDialog'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { WorkspaceShell } from '@/views/Workspace/WorkspaceShell'
import { KaleidoscopeShell } from '@/views/Kaleidoscope/KaleidoscopeShell'

export function MainArea(): React.ReactElement {
  const topLevelView = useAtomValue(topLevelViewAtom)

  return (
    <>
      <Panel
        variant="grow"
        className="bg-content-area rounded-2xl shadow-xl"
      >
        <div className="relative flex-1 min-h-0 flex flex-col">
          <AnimatePresence mode="wait">
            <motion.div
              key={topLevelView}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.2, ease: [0.32, 0.72, 0, 1] }}
              className="absolute inset-0 flex flex-col min-h-0"
            >
              {topLevelView === 'kaleidoscope' ? (
                <KaleidoscopeShell />
              ) : (
                <WorkspaceShell />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </Panel>
      <SettingsDialog />
    </>
  )
}
