import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { browserWebviewCompleteLogin } from './tauri-bridge'

export interface AutomationLoginWindowRequest {
  specId: string
  label: string
  url: string
}

function safeWindowSegment(value: string): string {
  const cleaned = value
    .trim()
    .replace(/[^a-zA-Z0-9_-]+/g, '-')
    .replace(/-+/g, '-')
    .slice(0, 80)
  return cleaned || 'login'
}

function startLoginCompletionPolling({
  windowLabel,
  specId,
  label,
  url,
}: AutomationLoginWindowRequest & { windowLabel: string }): void {
  let inFlight = false
  let attempts = 0
  const maxAttempts = 300
  const timer = window.setInterval(() => {
    if (inFlight) return
    attempts += 1
    if (attempts > maxAttempts) {
      window.clearInterval(timer)
      return
    }
    inFlight = true
    browserWebviewCompleteLogin(windowLabel, specId, label, url)
      .then(async (result) => {
        if (!result.completed && result.message) {
          console.debug(`[automation-login] ${label}: ${result.message}`)
        }
        if (!result.completed) return
        window.clearInterval(timer)
        const loginWindow = await WebviewWindow.getByLabel(windowLabel)
        await loginWindow?.close()
      })
      .catch((err) => {
        // The login window may still be starting; keep polling until timeout.
        console.debug(`[automation-login] ${label}: 登录态检查暂不可用`, err)
      })
      .finally(() => {
        inFlight = false
      })
  }, 2_000)
}

export async function openAutomationLoginWindow({
  specId,
  label,
  url,
}: AutomationLoginWindowRequest): Promise<void> {
  const windowLabel = `automation-login-${safeWindowSegment(specId)}-${safeWindowSegment(label)}`
  const existing = await WebviewWindow.getByLabel(windowLabel)
  if (existing) {
    await existing.focus()
    startLoginCompletionPolling({ windowLabel, specId, label, url })
    return
  }

  new WebviewWindow(windowLabel, {
    title: `${label} 登录`,
    url,
    width: 1180,
    height: 820,
    minWidth: 860,
    minHeight: 620,
    resizable: true,
    center: true,
    focus: true,
    acceptFirstMouse: true,
  })
  startLoginCompletionPolling({ windowLabel, specId, label, url })
}
