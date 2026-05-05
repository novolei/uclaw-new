// [PLACEHOLDER] proma-ui — @proma/ui 本地化替代
// 待后续任务迁移完整 useSmoothStream 实现

import * as React from 'react'

interface UseSmoothStreamOptions {
  content: string
  isStreaming: boolean
}

interface UseSmoothStreamResult {
  displayedContent: string
}

/**
 * useSmoothStream — 平滑流式文本输出
 *
 * [PLACEHOLDER] 简化实现：直接返回原始内容
 */
export function useSmoothStream({ content }: UseSmoothStreamOptions): UseSmoothStreamResult {
  return { displayedContent: content }
}
