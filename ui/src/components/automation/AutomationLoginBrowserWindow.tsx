import * as React from 'react'
import { X } from 'lucide-react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { BrowserPanel } from '@/components/browser/BrowserPanel'
import { browserUICompleteLogin, listenNavState } from '@/lib/tauri-bridge'

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
  const [activeTabId, setActiveTabId] = React.useState<string | null>(null)
  const [status, setStatus] = React.useState('等待登录完成...')

  React.useEffect(() => {
    let unlisten: (() => void) | null = null
    listenNavState((payload) => {
      if (payload.sessionId !== sessionId) return
      setActiveTabId(payload.tabId)
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [sessionId])

  React.useEffect(() => {
    if (!activeTabId || !targetUrl) return
    let stopped = false
    let inFlight = false
    const probe = () => {
      if (stopped || inFlight) return
      inFlight = true
      browserUICompleteLogin(sessionId, activeTabId, specId, label, targetUrl)
        .then((result) => {
          if (stopped) return
          if (!result.completed) {
            setStatus(result.message ?? '请在网页中完成登录')
            return
          }
          setStatus('登录完成，正在关闭窗口...')
          window.setTimeout(() => {
            getCurrentWebviewWindow().close().catch(console.error)
          }, 250)
        })
        .catch((err) => {
          if (!stopped) setStatus((err as { message?: string })?.message ?? '登录状态检查失败')
        })
        .finally(() => { inFlight = false })
    }
    const timer = window.setInterval(probe, 2_000)
    const first = window.setTimeout(probe, 1_000)
    return () => {
      stopped = true
      window.clearInterval(timer)
      window.clearTimeout(first)
    }
  }, [activeTabId, label, sessionId, specId, targetUrl])

  return (
    <div className="flex h-screen w-screen flex-col bg-background text-foreground">
      <div className="titlebar-drag-region flex h-11 shrink-0 items-center justify-between border-b border-border/60 px-4">
        <div className="flex min-w-0 flex-col">
          <span className="truncate text-sm font-medium">{label} 登录</span>
          <span className="truncate text-[11px] text-muted-foreground">{status}</span>
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
