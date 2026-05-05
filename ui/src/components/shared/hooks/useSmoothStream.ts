/**
 * useSmoothStream — 流式输出平滑钩子
 *
 * 在 AI 流式输出时，将突发的文本增量平滑化为均匀的渲染帧，
 * 避免大块文本一次性渲染导致的视觉跳动。
 *
 * 从 @proma/ui 迁移。
 */

import { useCallback, useEffect, useRef, useState } from 'react'

export interface UseSmoothStreamOptions {
  /** 目标文本（流式增量） */
  targetText: string
  /** 是否正在流式输出 */
  isStreaming: boolean
  /** 每帧显示的最大字符数（默认 3） */
  charsPerFrame?: number
  /** 帧间隔（ms，默认 16 ≈ 60fps） */
  frameInterval?: number
  /** 是否启用平滑（默认 true） */
  enabled?: boolean
}

export interface UseSmoothStreamReturn {
  /** 当前应该渲染的文本 */
  displayText: string
  /** 是否正在追赶目标文本 */
  isCatchingUp: boolean
}

/**
 * 流式输出平滑 Hook。
 *
 * 在流式模式下，逐字符/逐块地将 targetText 渐进显示为 displayText，
 * 使文本渲染看起来更流畅。当流式结束时，立即显示完整文本。
 */
export function useSmoothStream({
  targetText,
  isStreaming,
  charsPerFrame = 3,
  frameInterval = 16,
  enabled = true,
}: UseSmoothStreamOptions): UseSmoothStreamReturn {
  const [displayText, setDisplayText] = useState('')
  const displayTextRef = useRef('')
  const targetTextRef = useRef('')
  const rafIdRef = useRef<number | null>(null)
  const lastFrameTimeRef = useRef(0)

  targetTextRef.current = targetText

  // 流式结束时，立即显示完整文本
  useEffect(() => {
    if (!isStreaming && targetText) {
      setDisplayText(targetText)
      displayTextRef.current = targetText
      if (rafIdRef.current) {
        cancelAnimationFrame(rafIdRef.current)
        rafIdRef.current = null
      }
    }
  }, [isStreaming, targetText])

  // 流式开始时重置
  useEffect(() => {
    if (isStreaming && targetText === '') {
      setDisplayText('')
      displayTextRef.current = ''
    }
  }, [isStreaming, targetText])

  // 平滑追赶动画
  useEffect(() => {
    if (!isStreaming || !enabled) return

    const tick = (timestamp: number) => {
      const elapsed = timestamp - lastFrameTimeRef.current

      if (elapsed >= frameInterval) {
        lastFrameTimeRef.current = timestamp

        const current = displayTextRef.current
        const target = targetTextRef.current

        if (current.length < target.length) {
          const nextLength = Math.min(
            current.length + charsPerFrame,
            target.length,
          )
          const nextText = target.slice(0, nextLength)
          displayTextRef.current = nextText
          setDisplayText(nextText)
        }
      }

      rafIdRef.current = requestAnimationFrame(tick)
    }

    rafIdRef.current = requestAnimationFrame(tick)

    return () => {
      if (rafIdRef.current) {
        cancelAnimationFrame(rafIdRef.current)
        rafIdRef.current = null
      }
    }
  }, [isStreaming, enabled, charsPerFrame, frameInterval])

  const isCatchingUp = isStreaming && displayText.length < targetText.length

  return { displayText: enabled ? displayText : targetText, isCatchingUp }
}
