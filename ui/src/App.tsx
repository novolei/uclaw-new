/**
 * App — uClaw 应用根组件
 *
 * [Migration] 从 Proma App.tsx 迁移，移除 Electron 依赖：
 * - window.electronAPI.getSettings → tauri-bridge 兼容层
 * - OnboardingView / EnvironmentCheckDialog → 占位组件
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { AppShell } from './components/app-shell/AppShell'
import { StartupSplash } from './components/startup/StartupSplash'
import { TooltipProvider } from './components/ui/tooltip'
import type { AppShellContextType } from './contexts/AppShellContext'
import * as bridge from './lib/tauri-bridge'
import { stickyUserMessageEnabledAtom, initializeUiPreferences } from './atoms/ui-preferences'
import { activeProviderModelAtom } from './atoms/active-model'
import { useGlobalChatListeners } from './hooks/useGlobalChatListeners'
import { useGlobalAgentListeners } from './hooks/useGlobalAgentListeners'
import { usePetStateSync } from './hooks/usePetStateSync'
import {
  DEFAULT_STARTUP_DOCTOR_CHECKS,
  deriveStartupDoctorViewModel,
  deriveStartupDoctorViewModelFromRuntimePackStatus,
  type StartupDoctorViewModel,
  type StartupRuntimePackStatusReport,
} from './lib/startup/startup-doctor'

/** localStorage 键：语言偏好 */
const LANGUAGE_CACHE_KEY = 'uclaw:language'
export const STARTUP_SPLASH_MIN_VISIBLE_MS = 1800
export const STARTUP_SPLASH_EXIT_TRANSITION_MS = 220
export const STARTUP_BROWSER_RUNTIME_STATUS_TIMEOUT_MS = 5000
type StartupBrowserRuntimeStatusState = 'loading' | 'ready' | 'failed'

export default function App(): React.ReactElement {
  const [initializationComplete, setInitializationComplete] = React.useState(false)
  const [minimumSplashElapsed, setMinimumSplashElapsed] = React.useState(false)
  const [showStartupSplash, setShowStartupSplash] = React.useState(true)
  const [isSplashExiting, setIsSplashExiting] = React.useState(false)
  const [browserRuntimeStatusState, setBrowserRuntimeStatusState] =
    React.useState<StartupBrowserRuntimeStatusState>('loading')
  const [browserRuntimeStatus, setBrowserRuntimeStatus] =
    React.useState<StartupRuntimePackStatusReport | undefined>()
  const [browserRuntimeStatusError, setBrowserRuntimeStatusError] = React.useState<string | undefined>()
  const setStickyUserMessageEnabled = useSetAtom(stickyUserMessageEnabledAtom)
  const setActiveProviderModel = useSetAtom(activeProviderModelAtom)

  useGlobalChatListeners()
  useGlobalAgentListeners()
  usePetStateSync()

  React.useEffect(() => {
    const timer = window.setTimeout(() => {
      setMinimumSplashElapsed(true)
    }, STARTUP_SPLASH_MIN_VISIBLE_MS)

    return () => {
      window.clearTimeout(timer)
    }
  }, [])

  React.useEffect(() => {
    let cancelled = false
    let timeoutId: number | undefined
    const timeout = new Promise<never>((_, reject) => {
      timeoutId = window.setTimeout(() => {
        reject(
          new Error(
            `Rust Browser Runtime status did not respond within ${STARTUP_BROWSER_RUNTIME_STATUS_TIMEOUT_MS}ms`,
          ),
        )
      }, STARTUP_BROWSER_RUNTIME_STATUS_TIMEOUT_MS)
    })

    void Promise.race([bridge.getBrowserRuntimeStatus(), timeout])
      .then((report) => {
        if (cancelled) return
        setBrowserRuntimeStatus(report)
        setBrowserRuntimeStatusError(undefined)
        setBrowserRuntimeStatusState('ready')
      })
      .catch((error) => {
        if (cancelled) return
        console.error('[App] Browser Runtime 状态读取失败:', error)
        setBrowserRuntimeStatus(undefined)
        setBrowserRuntimeStatusError(error instanceof Error ? error.message : String(error))
        setBrowserRuntimeStatusState('failed')
      })
      .finally(() => {
        if (timeoutId !== undefined) {
          window.clearTimeout(timeoutId)
        }
      })

    return () => {
      cancelled = true
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId)
      }
    }
  }, [])

  // 从 Tauri 后端加载初始设置
  React.useEffect(() => {
    let cancelled = false

    const initialize = async () => {
      try {
        // 从后端加载设置（language、theme 等）
        const settings = await bridge.getSettings()

        // 持久化 language 到 localStorage，供 i18n 层读取
        if (settings.language) {
          try {
            localStorage.setItem(LANGUAGE_CACHE_KEY, settings.language)
          } catch {
            // localStorage 不可用时忽略
          }
        }

        // 初始化 UI 偏好（stickyUserMessage 等）
        await initializeUiPreferences(setStickyUserMessageEnabled)

        // 从 providers.json 同步活跃模型（权威来源）
        try {
          const activeModel = await bridge.getActiveModel()
          if (activeModel) {
            setActiveProviderModel({ providerId: activeModel.providerId, modelId: activeModel.modelId })
          }
        } catch {
          // getActiveModel 失败时保持 localStorage 缓存值
        }
      } catch (error) {
        console.error('[App] 初始化失败:', error)
      } finally {
        if (!cancelled) {
          setInitializationComplete(true)
        }
      }
    }
    initialize()

    return () => {
      cancelled = true
    }
  }, [setStickyUserMessageEnabled, setActiveProviderModel])

  const browserRuntimeStatusComplete = browserRuntimeStatusState !== 'loading'
  const startupViewModel = React.useMemo(
    () =>
      startupDoctorViewModelFromBrowserRuntimeStatus(
        browserRuntimeStatusState,
        browserRuntimeStatus,
        browserRuntimeStatusError,
      ),
    [browserRuntimeStatusState, browserRuntimeStatus, browserRuntimeStatusError],
  )
  const startupReadyToHandoff = initializationComplete && minimumSplashElapsed && browserRuntimeStatusComplete

  React.useEffect(() => {
    if (!startupReadyToHandoff) return

    setIsSplashExiting(true)
    const timer = window.setTimeout(() => {
      setShowStartupSplash(false)
    }, STARTUP_SPLASH_EXIT_TRANSITION_MS)

    return () => {
      window.clearTimeout(timer)
    }
  }, [startupReadyToHandoff])

  // 加载中状态
  if (showStartupSplash) {
    return (
      <div
        data-startup-splash-state={isSplashExiting ? 'exiting' : 'visible'}
        className={
          isSplashExiting
            ? 'opacity-0 transition-opacity duration-200 ease-out motion-reduce:transition-none'
            : 'opacity-100 transition-opacity duration-200 ease-out motion-reduce:transition-none'
        }
      >
        <StartupSplash viewModel={startupViewModel} />
      </div>
    )
  }

  // Placeholder context value
  const contextValue: AppShellContextType = {}

  // 显示主界面
  return (
    <TooltipProvider delayDuration={200}>
      <AppShell contextValue={contextValue} />
    </TooltipProvider>
  )
}

function startupDoctorViewModelFromBrowserRuntimeStatus(
  state: StartupBrowserRuntimeStatusState,
  report: StartupRuntimePackStatusReport | undefined,
  errorMessage: string | undefined,
): StartupDoctorViewModel {
  if (state === 'ready') {
    return deriveStartupDoctorViewModelFromRuntimePackStatus(report)
  }

  if (state === 'failed') {
    const detail = errorMessage
      ? `Rust Browser Runtime status is unavailable: ${errorMessage}`
      : 'Rust Browser Runtime status is unavailable.'
    return deriveStartupDoctorViewModel(
      DEFAULT_STARTUP_DOCTOR_CHECKS.map((check) => {
        if (
          check.id === 'browser-runtime-manifest' ||
          check.id === 'browser-runtime-pack' ||
          check.id === 'last-runtime-status'
        ) {
          return { ...check, status: 'warning', detail }
        }

        return { ...check }
      }),
    )
  }

  return deriveStartupDoctorViewModel()
}
