/**
 * BrowserViewer — AI Browser (Phase 3) canvas component
 *
 * Displays browser UI and controls CDP functionality.
 * Launch button spins up the browser backend via Tauri IPC.
 * Full CDP tool interface is Phase 3 work.
 */

import React, { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useAtom } from 'jotai'
import { browserStateAtom, isBrowserLoadingAtom, type BrowserState } from '../../atoms/browser-atoms'

export const BrowserViewer: React.FC = () => {
  const [browserState, setBrowserState] = useAtom(browserStateAtom)
  const [isLoading, setIsLoading] = useAtom(isBrowserLoadingAtom)
  const [error, setError] = useState<string | null>(null)

  const handleLaunch = async () => {
    setIsLoading(true)
    setError(null)
    try {
      await invoke('browser_launch')
      const state = await invoke<BrowserState>('browser_get_state')
      setBrowserState(state as typeof browserState)
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setIsLoading(false)
    }
  }

  if (!browserState.running) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4 text-sm">
        <div className="text-4xl">🌐</div>
        <p className="text-muted-foreground">AI Browser (Phase 3)</p>
        {error && <p className="text-destructive text-xs max-w-xs text-center">{error}</p>}
        <button
          onClick={handleLaunch}
          disabled={isLoading}
          className="px-4 py-2 rounded-lg bg-primary text-primary-foreground text-xs disabled:opacity-50"
        >
          {isLoading ? '启动中...' : '启动 AI 浏览器'}
        </button>
        <p className="text-xs text-muted-foreground opacity-60">
          完整 CDP 功能将在 Phase 3 实现
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border text-xs text-muted-foreground">
        已连接 Chromium · {browserState.tabs.length} 个标签
      </div>
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        CDP 工具界面 — Phase 3 开发中
      </div>
    </div>
  )
}
