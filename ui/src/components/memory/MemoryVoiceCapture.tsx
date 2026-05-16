/**
 * MemoryVoiceCapture — 语音记忆录制浮层。
 *
 * 粉色温暖脉冲、小型紧凑浮层、单个环形脉冲。
 * 与 STT 语音输入 Modal（蓝色冷酷辉光、大尺寸居中、7段EQ频谱条）在视觉上完全不同。
 *
 * 监听 window 上的 `uclaw:memory-voice-start` 事件来触发启动，
 * 使用 useMemoryVoiceSession() hook 驱动状态机。
 */
import * as React from 'react'
import { Brain, Check, Loader2, ShieldAlert, AlertCircle } from 'lucide-react'
import { listen } from '@tauri-apps/api/event'
import { useMemoryVoiceSession } from '@/hooks/useMemoryVoiceSession'
import { cn } from '@/lib/utils'
import './MemoryVoiceCapture.css'

// ─── 国际化文案常量 ────────────────────────────────────────────────────────────
const MEMORY_VOICE_I18N = {
  zh: {
    mainPrompt: '说出你要记住的事',
    subPrompt: '停顿后自动保存',
    saving: '正在记住…',
    saved: '已记住',
    preparing: '准备记录…',
    permissionSub: '正在请求麦克风权限',
    permissionNeeded: '需要麦克风权限',
    permissionHint: '请在系统设置中授权',
    errorTitle: '记录失败',
    hint: '停顿即保存 · Esc 取消',
    tooltip: '语音记忆',
  },
  en: {
    mainPrompt: 'Say what you want to remember',
    subPrompt: 'Pause to save automatically',
    saving: 'Remembering...',
    saved: 'Remembered',
    preparing: 'Preparing...',
    permissionSub: 'Requesting microphone permission',
    permissionNeeded: 'Microphone permission needed',
    permissionHint: 'Please allow in system settings',
    errorTitle: 'Recording failed',
    hint: 'Pause to save · Esc to cancel',
    tooltip: 'Voice Memory',
  },
} as const

// 当前使用中文
const t = MEMORY_VOICE_I18N.zh

export function MemoryVoiceCapture(): React.ReactElement | null {
  const { start, cancel, state, volume } = useMemoryVoiceSession()

  // ── 监听全局事件 uclaw:memory-voice-start ──────────────────────────────────
  React.useEffect(() => {
    const onStart = () => {
      void start()
    }
    window.addEventListener('uclaw:memory-voice-start', onStart)
    return () => window.removeEventListener('uclaw:memory-voice-start', onStart)
  }, [start])

  // ── 监听 Tauri 全局快捷键触发事件 ─────────────────────────────────────
  React.useEffect(() => {
    const unlisten = listen('memory-voice-global-trigger', () => {
      void start()
    })
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [start])

  // ── Esc 取消录制 ───────────────────────────────────────────────────────────
  React.useEffect(() => {
    if (state.kind === 'idle') return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        cancel()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [state.kind, cancel])

  // ── 空闲时不渲染 ───────────────────────────────────────────────────────────
  if (state.kind === 'idle') return null

  // ── 根据状态计算文案和图标 ─────────────────────────────────────────────────
  let mainText = ''
  let subText: string | null = null
  let icon: React.ReactNode = null
  let iconClass = 'memory-voice-icon'
  let showPulseRing = false
  let pulseRingActive = false

  switch (state.kind) {
    case 'requesting-permission':
      mainText = t.preparing
      subText = t.permissionSub
      icon = <Loader2 className={cn(iconClass, 'animate-spin')} />
      break

    case 'recording':
      mainText = t.mainPrompt
      subText = t.subPrompt
      icon = <Brain className={iconClass} />
      showPulseRing = true
      pulseRingActive = true
      break

    case 'saving':
      mainText = t.saving
      subText = null
      icon = <Brain className={cn(iconClass, 'animate-spin')} />
      break

    case 'saved':
      mainText = t.saved
      subText = state.title ? `${state.title} · ${state.subtype}` : null
      iconClass = cn(iconClass, 'memory-voice-icon--success', 'memory-voice-icon--check-anim')
      icon = <Check className={iconClass} />
      break

    case 'permission-denied':
      mainText = t.permissionNeeded
      subText = t.permissionHint
      iconClass = cn(iconClass, 'memory-voice-icon--error')
      icon = <ShieldAlert className={iconClass} />
      break

    case 'error':
      mainText = t.errorTitle
      subText = state.message
      iconClass = cn(iconClass, 'memory-voice-icon--error')
      icon = <AlertCircle className={iconClass} />
      break
  }

  return (
    <div
      className="memory-voice-overlay"
      onClick={() => cancel()}
      data-testid="memory-voice-overlay"
    >
      <div className="memory-voice-panel" onClick={(e) => e.stopPropagation()}>
        {/* 图标区 */}
        <div className="memory-voice-icon-area">
          {/* 环形音量脉冲 */}
          {showPulseRing && (
            <div
              className={cn(
                'memory-voice-pulse-ring',
                pulseRingActive && 'memory-voice-pulse-ring--active',
              )}
              style={{ transform: `scale(${1 + volume * 0.3})` }}
              aria-hidden
            />
          )}
          {/* 中心图标 */}
          {icon}
        </div>

        {/* 主文案 */}
        <p className="memory-voice-main-text">{mainText}</p>

        {/* 副文案 */}
        {subText && <p className="memory-voice-sub-text">{subText}</p>}

        {/* 实时转写区（仅 recording 状态） */}
        {state.kind === 'recording' && state.interimText && (
          <div className="memory-voice-transcript">
            &ldquo;{state.interimText}&rdquo;
          </div>
        )}

        {/* 底部提示（仅 recording 状态） */}
        {state.kind === 'recording' && (
          <p className="memory-voice-hint">{t.hint}</p>
        )}
      </div>
    </div>
  )
}
