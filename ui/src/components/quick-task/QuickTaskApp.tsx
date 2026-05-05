/**
 * QuickTaskApp — 快捷任务窗口
 *
 * Tauri 模式下不需要独立窗口（Electron 使用 BrowserWindow 打开）。
 * 保留组件以兼容路由引用，显示不适用提示。
 */

import * as React from 'react'
import { Terminal } from 'lucide-react'

export function QuickTaskApp(): React.ReactElement {
  return (
    <div className="flex-1 flex flex-col items-center justify-center gap-3 text-muted-foreground p-8">
      <Terminal className="size-8 text-muted-foreground/40" />
      <div className="text-center">
        <p className="text-sm font-medium">快捷任务</p>
        <p className="text-xs text-muted-foreground/60 mt-1">
          在 Tauri 模式下，请使用主窗口的对话功能。
        </p>
      </div>
    </div>
  )
}
