import {
  dryRunBrowserRuntimeAction,
  executeBrowserRuntimeAction,
  getBrowserRuntimeStatus,
} from './browser-runtime/browser-runtime-adapter'

declare global {
  interface Window {
    __UCLAW_DEBUG__?: {
      getBrowserRuntimeStatus: typeof getBrowserRuntimeStatus
      dryRunBrowserRuntimeAction: typeof dryRunBrowserRuntimeAction
      executeBrowserRuntimeAction: typeof executeBrowserRuntimeAction
    }
  }
}

export function installUclawDebugBridge(): void {
  if (!import.meta.env.DEV || typeof window === 'undefined') return

  Object.defineProperty(window, '__UCLAW_DEBUG__', {
    configurable: true,
    enumerable: false,
    value: {
      getBrowserRuntimeStatus,
      dryRunBrowserRuntimeAction,
      executeBrowserRuntimeAction,
    },
  })
}
