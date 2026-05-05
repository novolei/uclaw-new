/**
 * CodeBlock — 代码块高亮渲染组件
 *
 * 使用 Shiki 进行语法高亮渲染。
 * 支持复制按钮、语言标签、行号（可选）。
 *
 * 从 @proma/ui 迁移，依赖本地化的 highlight 模块。
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { highlightCode, escapeHtml } from '@/lib/highlight'
import { cn } from '@/lib/utils'

export interface CodeBlockProps {
  /** 代码内容 */
  code: string
  /** 语言标识 */
  language?: string
  /** 是否显示行号 */
  showLineNumbers?: boolean
  /** 是否显示复制按钮 */
  showCopyButton?: boolean
  /** 是否显示语言标签 */
  showLanguageLabel?: boolean
  /** 主题（跟随系统或手动指定） */
  theme?: 'light' | 'dark'
  /** 文件名（如果有） */
  filename?: string
  /** 额外 CSS 类名 */
  className?: string
  /** 最大高度（px），超出滚动 */
  maxHeight?: number
}

export function CodeBlock({
  code,
  language = 'plaintext',
  showLineNumbers = false,
  showCopyButton = true,
  showLanguageLabel = true,
  theme,
  filename,
  className,
  maxHeight,
}: CodeBlockProps) {
  const [highlightedHtml, setHighlightedHtml] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const copyTimeoutRef = useRef<ReturnType<typeof setTimeout>>()

  // 检测系统主题
  const resolvedTheme = theme ?? (
    typeof window !== 'undefined' &&
    window.matchMedia?.('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light'
  )

  // 异步高亮
  useEffect(() => {
    let cancelled = false

    highlightCode(code, language, resolvedTheme).then((html) => {
      if (!cancelled) {
        setHighlightedHtml(html)
      }
    })

    return () => {
      cancelled = true
    }
  }, [code, language, resolvedTheme])

  // 复制到剪贴板
  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
      if (copyTimeoutRef.current) clearTimeout(copyTimeoutRef.current)
      copyTimeoutRef.current = setTimeout(() => setCopied(false), 2000)
    } catch (err) {
      console.error('[CodeBlock] 复制失败:', err)
    }
  }, [code])

  // 行号（如果需要的话在 fallback 时手动添加）
  const lineCount = useMemo(() => code.split('\n').length, [code])

  // Fallback: 纯文本渲染
  const fallbackHtml = useMemo(
    () => `<pre class="shiki"><code>${escapeHtml(code)}</code></pre>`,
    [code],
  )

  return (
    <div
      className={cn(
        'group relative rounded-lg border border-border overflow-hidden',
        'bg-muted/30',
        className,
      )}
    >
      {/* 头部工具栏 */}
      {(showLanguageLabel || filename || showCopyButton) && (
        <div className="flex items-center justify-between px-3 py-1.5 border-b border-border bg-muted/50 text-xs text-muted-foreground">
          <span className="font-mono">
            {filename ?? (language !== 'plaintext' ? language : '')}
          </span>
          {showCopyButton && (
            <button
              onClick={handleCopy}
              className="opacity-0 group-hover:opacity-100 transition-opacity text-xs hover:text-foreground"
            >
              {copied ? '已复制 ✓' : '复制'}
            </button>
          )}
        </div>
      )}

      {/* 代码区域 */}
      <div
        className="overflow-x-auto text-sm"
        style={maxHeight ? { maxHeight, overflowY: 'auto' } : undefined}
      >
        <div className="flex">
          {showLineNumbers && (
            <div className="select-none pr-3 pl-3 py-3 text-right text-muted-foreground/50 font-mono text-xs leading-relaxed border-r border-border bg-muted/20">
              {Array.from({ length: lineCount }, (_, i) => (
                <div key={i}>{i + 1}</div>
              ))}
            </div>
          )}
          <div
            className="flex-1 py-3 px-4 overflow-x-auto [&_pre]:!bg-transparent [&_pre]:!m-0 [&_pre]:!p-0 [&_code]:!text-sm [&_code]:!leading-relaxed"
            dangerouslySetInnerHTML={{
              __html: highlightedHtml ?? fallbackHtml,
            }}
          />
        </div>
      </div>
    </div>
  )
}

export default CodeBlock
