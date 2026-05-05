/**
 * useSmoothStream — 平滑流式输出 hook
 *
 * [PLACEHOLDER] 简化版 — 直接返回原始内容（无逐字动画）
 * 后续任务中迁移完整实现后替换
 */

import { useState, useEffect } from 'react'

interface UseSmoothStreamOptions {
  content: string
  isStreaming: boolean
}

interface UseSmoothStreamResult {
  displayedContent: string
}

export function useSmoothStream({ content, isStreaming }: UseSmoothStreamOptions): UseSmoothStreamResult {
  // 简化实现：直接返回原始内容
  const [displayedContent, setDisplayedContent] = useState(content)

  useEffect(() => {
    setDisplayedContent(content)
  }, [content])

  return { displayedContent }
}
