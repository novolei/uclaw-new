import { invoke } from '@tauri-apps/api/core'
import {
  dryRunBrowserRuntimeAction,
  executeBrowserRuntimeAction,
  getBrowserRuntimeStatus,
} from './tauri-bridge'

declare global {
  interface Window {
    __UCLAW_DEBUG__?: {
      getBrowserRuntimeStatus: typeof getBrowserRuntimeStatus
      dryRunBrowserRuntimeAction: typeof dryRunBrowserRuntimeAction
      executeBrowserRuntimeAction: typeof executeBrowserRuntimeAction
      invoke: typeof invoke
    }
    uclaw_invoke?: typeof invoke
  }
}

export function installUclawDebugBridge(): void {
  if (!import.meta.env.DEV || typeof window === 'undefined') return

  const debugInvoke = async (cmd: string, args?: Record<string, unknown>, options?: any) => {
    console.info(`[uClaw Debug Bridge] Invoking command: "${cmd}"`, args ?? {})
    try {
      const result = await invoke(cmd, args, options)
      console.info(`[uClaw Debug Bridge] Command "${cmd}" successfully resolved to:`, result)
      return result
    } catch (error) {
      console.error(`[uClaw Debug Bridge] Command "${cmd}" failed with error:`, error)
      throw error
    }
  }

  Object.defineProperty(window, '__UCLAW_DEBUG__', {
    configurable: true,
    enumerable: false,
    value: {
      getBrowserRuntimeStatus,
      dryRunBrowserRuntimeAction,
      executeBrowserRuntimeAction,
      invoke: debugInvoke,
    },
  })

  Object.defineProperty(window, 'uclaw_invoke', {
    configurable: true,
    enumerable: false,
    value: debugInvoke,
  })
}
