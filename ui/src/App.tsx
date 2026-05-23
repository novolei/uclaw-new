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

/** localStorage 键：语言偏好 */
const LANGUAGE_CACHE_KEY = 'uclaw:language'

export default function App(): React.ReactElement {
  const [isLoading, setIsLoading] = React.useState(true)
  const setStickyUserMessageEnabled = useSetAtom(stickyUserMessageEnabledAtom)
  const setActiveProviderModel = useSetAtom(activeProviderModelAtom)

  useGlobalChatListeners()
  useGlobalAgentListeners()
  usePetStateSync()

  // 从 Tauri 后端加载初始设置
  React.useEffect(() => {
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
        setIsLoading(false)
      }
    }
    initialize()
  }, [setStickyUserMessageEnabled, setActiveProviderModel])

  // 加载中状态
  if (isLoading) {
    return <StartupSplash />
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
