import { WebviewWindow } from '@tauri-apps/api/webviewWindow'

export interface AutomationLoginWindowRequest {
  specId: string
  label: string
  url: string
}

function safeWindowSegment(value: string): string {
  const cleaned = value
    .trim()
    .replace(/[^a-zA-Z0-9:_/-]+/g, '-')
    .replace(/-+/g, '-')
    .slice(0, 80)
  return cleaned || 'login'
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
    return
  }

  const params = new URLSearchParams({
    uclawWindow: 'automation-login-browser',
    specId,
    label,
    targetUrl: url,
  })

  new WebviewWindow(windowLabel, {
    title: `${label} 登录`,
    url: `/?${params.toString()}`,
    width: 1180,
    height: 820,
    minWidth: 860,
    minHeight: 620,
    resizable: true,
    center: true,
    focus: true,
  })
}
