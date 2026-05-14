/**
 * SttModal — 流式语音 modal。
 *
 * 订阅 sttModalStateAtom 渲染：实时转写区 + 5 段音量条 + 控制行，
 * 外加 Claude Code 风格的辉光漫散射效果（CSS，主题色）。
 * 内部持有 useSttStreamingSession；onSegmentFinalized 透传给 hook，
 * 由调用方（composer）负责把定稿文本追加到聊天输入框。
 * modal 在 state.kind !== 'idle' 时挂载。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Square, X, Loader2, MicOff } from 'lucide-react'
import { cn } from '@/lib/utils'
import { sttModalStateAtom, type ComposerKind } from '@/atoms/stt-atoms'
import { useSttStreamingSession } from '@/hooks/useSttStreamingSession'
import './SttModal.css'

interface SttModalProps {
  composer: ComposerKind
  /** 每段定稿后调用，由调用方追加到聊天输入框。 */
  onSegmentFinalized: (text: string) => void
}

const BAR_HEIGHT_SCALES = [0.6, 1.0, 0.75, 0.9, 0.5]

export function SttModal({ composer, onSegmentFinalized }: SttModalProps): React.ReactElement | null {
  const state = useAtomValue(sttModalStateAtom)
  const session = useSttStreamingSession(composer, { onSegmentFinalized })

  // Esc 取消。
  React.useEffect(() => {
    if (state.kind === 'idle') return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') session.cancel()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [state.kind, session])

  // window 事件桥接：SpeechButton / FirstRunDialog 通过事件驱动会话开关。
  React.useEffect(() => {
    const onStart = () => {
      if (state.kind === 'idle') void session.start()
    }
    const onEnd = () => {
      if (state.kind !== 'idle') void session.end()
    }
    window.addEventListener('uclaw:stt-start', onStart)
    window.addEventListener('uclaw:stt-end', onEnd)
    window.addEventListener('uclaw:stt-start-after-ready', onStart)
    return () => {
      window.removeEventListener('uclaw:stt-start', onStart)
      window.removeEventListener('uclaw:stt-end', onEnd)
      window.removeEventListener('uclaw:stt-start-after-ready', onStart)
    }
  }, [state.kind, session])

  if (state.kind === 'idle') return null

  const volume =
    state.kind === 'listening' || state.kind === 'finalizing' ? state.volume : 0
  const interimText = state.kind === 'listening' ? state.interimText : ''

  let statusText = ''
  if (state.kind === 'requesting-permission') statusText = '正在请求麦克风权限…'
  else if (state.kind === 'listening') statusText = '正在聆听… 停顿即录入'
  else if (state.kind === 'finalizing') statusText = '录入中…'
  else if (state.kind === 'permission-denied') statusText = '麦克风权限被拒绝，请在系统设置中授权'
  else if (state.kind === 'error') statusText = state.message

  return (
    <div
      className="stt-modal-overlay"
      onClick={() => session.cancel()}
      data-testid="stt-modal-overlay"
    >
      <div className="stt-modal-panel" onClick={(e) => e.stopPropagation()}>
        <div className="stt-modal-glow" aria-hidden />
        <div className="stt-modal-grain" aria-hidden />
        <div className="stt-modal-content">
          {/* 状态行 */}
          <div className="flex items-center gap-2 mb-3">
            {state.kind === 'finalizing' || state.kind === 'requesting-permission' ? (
              <Loader2 className="size-4 animate-spin text-primary shrink-0" />
            ) : state.kind === 'permission-denied' ? (
              <MicOff className="size-4 text-destructive shrink-0" />
            ) : (
              <span className="spinner text-sm text-primary shrink-0" aria-hidden>
                {Array.from({ length: 9 }).map((_, i) => (
                  <span key={i} className="spinner-cube" />
                ))}
              </span>
            )}
            <span className="text-[13px] text-muted-foreground">{statusText}</span>
          </div>

          {/* 实时转写区 */}
          {(state.kind === 'listening' || state.kind === 'finalizing') && (
            <div className="min-h-[60px] text-[15px] leading-7 text-foreground whitespace-pre-wrap break-words mb-3">
              {interimText !== '' ? (
                interimText
              ) : (
                <span className="text-muted-foreground/60">请开始说话</span>
              )}
            </div>
          )}

          {/* 音量条 + 控制行 */}
          {(state.kind === 'listening' || state.kind === 'finalizing') && (
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-[3px] h-6" aria-label="音量">
                {BAR_HEIGHT_SCALES.map((scale, i) => {
                  // 感知曲线：RMS 集中在低值区，开平方把它拉开，说话时条更跳。
                  // 高度区间 3–22px：静音时是一条静止的细线，说话时明显起伏。
                  const shaped = Math.sqrt(Math.max(0, Math.min(1, volume)))
                  const h = Math.max(3, Math.round(shaped * scale * 22))
                  return (
                    <span
                      key={i}
                      data-testid="stt-volume-bar"
                      className="w-[3px] rounded-full bg-primary transition-all duration-100"
                      style={{ height: `${h}px` }}
                    />
                  )
                })}
              </div>
              <div className="flex items-center gap-3">
                <span className="text-[11px] text-muted-foreground/70">
                  Alt+S 结束 · Esc 取消
                </span>
                <button
                  type="button"
                  aria-label="结束语音输入"
                  onClick={() => void session.end()}
                  className={cn(
                    'size-7 rounded-full inline-flex items-center justify-center',
                    'bg-primary/15 text-primary hover:bg-primary/25 transition-colors',
                  )}
                >
                  <Square className="size-3.5" fill="currentColor" />
                </button>
              </div>
            </div>
          )}

          {/* 权限拒绝 / error 态的关闭按钮 */}
          {(state.kind === 'permission-denied' || state.kind === 'error') && (
            <div className="flex justify-end mt-2">
              <button
                type="button"
                aria-label="关闭"
                onClick={() => session.cancel()}
                className="size-7 rounded-full inline-flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-foreground/5 transition-colors"
              >
                <X className="size-4" />
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
