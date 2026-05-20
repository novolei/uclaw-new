import * as React from 'react'
import { X } from 'lucide-react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { BrowserPanel } from '@/components/browser/BrowserPanel'

function readLoginParams(): { specId: string; label: string; targetUrl: string } {
  const params = new URLSearchParams(window.location.search)
  return {
    specId: params.get('specId') || 'automation-login',
    label: params.get('label') || 'Browser',
    targetUrl: params.get('targetUrl') || '',
  }
}

export function AutomationLoginBrowserWindow(): React.ReactElement {
  const { specId, label, targetUrl } = React.useMemo(readLoginParams, [])
  const sessionId = `automation-login:${specId}`

  return (
    <div className="flex h-screen w-screen flex-col bg-background text-foreground">
      <div className="titlebar-drag-region flex h-11 shrink-0 items-center justify-between border-b border-border/60 px-4">
        <div className="flex min-w-0 flex-col">
          <span className="truncate text-sm font-medium">{label} 登录</span>
          <span className="truncate text-[11px] text-muted-foreground">登录状态仅保存在 AI Browser 会话中</span>
        </div>
        <button
          type="button"
          aria-label="关闭登录窗口"
          className="titlebar-no-drag rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
          onClick={() => {
            getCurrentWebviewWindow().close().catch(console.error)
          }}
        >
          <X size={15} />
        </button>
      </div>
      <div className="min-h-0 flex-1">
        <BrowserPanel agentSessionId={sessionId} initialUrl={targetUrl} />
      </div>
    </div>
  )
}
