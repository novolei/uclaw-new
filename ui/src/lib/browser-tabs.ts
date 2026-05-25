export function isRealBrowserTabId(tabId: string | null | undefined): tabId is string {
  return Boolean(tabId && tabId !== 'new')
}
