/**
 * MermaidBlock — Mermaid 流程图渲染组件
 *
 * 动态加载 mermaid 库，将 mermaid 语法渲染为 SVG。
 * 支持暗色主题、错误降级。
 *
 * 从 @proma/ui 迁移。
 */

import React, { useEffect, useRef, useState, useMemo } from 'react'
import { cn } from '@/lib/utils'

export interface MermaidBlockProps {
  /** Mermaid 语法内容 */
  code: string
  /** 主题 */
  theme?: 'light' | 'dark'
  /** 额外 CSS 类名 */
  className?: string
}

/** 唯一 ID 计数器 */
let mermaidIdCounter = 0

export function MermaidBlock({
  code,
  theme,
  className,
}: MermaidBlockProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [svg, setSvg] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const resolvedTheme = theme ?? (
    typeof window !== 'undefined' &&
    window.matchMedia?.('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light'
  )

  const mermaidId = useMemo(() => `mermaid-${++mermaidIdCounter}`, [])

  useEffect(() => {
    let cancelled = false

    async function renderMermaid() {
      setLoading(true)
      setError(null)
      setSvg(null)

      try {
        // 动态导入 mermaid（避免 SSR 问题）
        const mermaid = await import('mermaid').then((m) => m.default).catch(() => null)

        if (cancelled) return

        if (!mermaid) {
          setError('Mermaid 库加载失败')
          setLoading(false)
          return
        }

        mermaid.initialize({
          startOnLoad: false,
          theme: resolvedTheme === 'dark' ? 'dark' : 'default',
          securityLevel: 'strict',
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        })

        const { svg: renderedSvg } = await mermaid.render(mermaidId, code.trim())

        if (!cancelled) {
          setSvg(renderedSvg)
          setLoading(false)
        }
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : '渲染失败'
          setError(message)
          setLoading(false)
        }
      }
    }

    renderMermaid()

    return () => {
      cancelled = true
    }
  }, [code, resolvedTheme, mermaidId])

  if (error) {
    return (
      <div
        className={cn(
          'rounded-lg border border-destructive/30 bg-destructive/5 p-4',
          className,
        )}
      >
        <div className="text-xs text-destructive font-medium mb-2">
          Mermaid 渲染错误
        </div>
        <pre className="text-xs text-muted-foreground overflow-x-auto whitespace-pre-wrap">
          {error}
        </pre>
        <details className="mt-2">
          <summary className="text-xs text-muted-foreground cursor-pointer hover:text-foreground">
            查看源代码
          </summary>
          <pre className="mt-1 text-xs overflow-x-auto whitespace-pre-wrap">
            {code}
          </pre>
        </details>
      </div>
    )
  }

  if (loading) {
    return (
      <div
        className={cn(
          'rounded-lg border border-border bg-muted/30 p-8 flex items-center justify-center',
          className,
        )}
      >
        <div className="text-sm text-muted-foreground animate-pulse">
          正在渲染图表...
        </div>
      </div>
    )
  }

  return (
    <div
      ref={containerRef}
      className={cn(
        'rounded-lg border border-border bg-background overflow-x-auto p-4',
        '[&_svg]:mx-auto [&_svg]:max-w-full',
        className,
      )}
      dangerouslySetInnerHTML={svg ? { __html: svg } : undefined}
    />
  )
}

export default MermaidBlock
