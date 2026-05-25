import {
  dryRunBrowserRuntimeAction,
  getBrowserRuntimeStatus,
} from './tauri-bridge'

declare global {
  interface Window {
    __UCLAW_DEBUG__?: {
      getBrowserRuntimeStatus: typeof getBrowserRuntimeStatus
      dryRunBrowserRuntimeAction: typeof dryRunBrowserRuntimeAction
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
    },
  })
}
