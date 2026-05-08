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

// ===== MarkdownCodeBlock — react-markdown 的 <pre> 渲染器 =====

/**
 * 不规则语言显示名称映射（无法通过首字母大写自动生成）
 */
const DISPLAY_NAMES: Record<string, string> = {
  js: 'JavaScript', javascript: 'JavaScript',
  ts: 'TypeScript', typescript: 'TypeScript',
  tsx: 'TSX', jsx: 'JSX',
  py: 'Python', rb: 'Ruby',
  cpp: 'C++', 'c++': 'C++',
  cs: 'C#', csharp: 'C#',
  kt: 'Kotlin', rs: 'Rust',
  sh: 'Shell', zsh: 'Shell', bash: 'Bash',
  yml: 'YAML', yaml: 'YAML', md: 'Markdown',
  html: 'HTML', css: 'CSS', scss: 'SCSS',
  json: 'JSON', xml: 'XML', sql: 'SQL',
  graphql: 'GraphQL', php: 'PHP',
  plaintext: 'Text', text: 'Text',
  dockerfile: 'Dockerfile', toml: 'TOML',
  rust: 'Rust', go: 'Go', swift: 'Swift', kotlin: 'Kotlin',
}

function getDisplayName(lang: string): string {
  if (!lang) return 'Code'
  const key = lang.toLowerCase()
  return DISPLAY_NAMES[key] ?? key.charAt(0).toUpperCase() + key.slice(1)
}

/** 递归提取 ReactNode 中的纯文本 */
function extractText(node: React.ReactNode): string {
  if (typeof node === 'string') return node
  if (typeof node === 'number') return String(node)
  if (!node) return ''
  if (Array.isArray(node)) return node.map(extractText).join('')
  if (React.isValidElement(node)) {
    return extractText((node.props as { children?: React.ReactNode }).children)
  }
  return ''
}

interface MarkdownCodeBlockProps {
  /** react-markdown 传入的 <pre> 子元素（内含 <code className="language-xxx">） */
  children?: React.ReactNode
}

/**
 * 供 react-markdown 的 `pre` 组件覆盖使用。
 * 从 children 中提取语言和代码文本，使用 Shiki 渲染语法高亮。
 */
export function MarkdownCodeBlock({ children }: MarkdownCodeBlockProps): React.ReactElement {
  // 从 react-markdown 传入的 children 中提取 <code> 元素
  const codeElement = React.Children.toArray(children).find(
    (child): child is React.ReactElement =>
      React.isValidElement(child) && (child as React.ReactElement).type === 'code'
  ) as React.ReactElement | undefined

  const codeProps = codeElement?.props as { className?: string; children?: React.ReactNode } | undefined
  const langMatch = codeProps?.className?.match(/language-(\S+)/)
  const language = langMatch?.[1] ?? 'text'
  const code = extractText(codeProps?.children ?? children).replace(/\n$/, '')

  const [highlightedHtml, setHighlightedHtml] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const copyTimeoutRef = useRef<ReturnType<typeof setTimeout>>()

  // 检测当前主题
  const isDark = typeof document !== 'undefined' && document.documentElement.classList.contains('dark')
  const resolvedTheme: 'light' | 'dark' = isDark ? 'dark' : 'light'

  useEffect(() => {
    let cancelled = false
    highlightCode(code, language, resolvedTheme).then((html) => {
      if (!cancelled) setHighlightedHtml(html)
    })
    return () => { cancelled = true }
  }, [code, language, resolvedTheme])

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
      if (copyTimeoutRef.current) clearTimeout(copyTimeoutRef.current)
      copyTimeoutRef.current = setTimeout(() => setCopied(false), 2000)
    } catch (err) {
      console.error('[MarkdownCodeBlock] 复制失败:', err)
    }
  }, [code])

  const fallbackHtml = useMemo(
    () => `<pre class="shiki"><code>${escapeHtml(code)}</code></pre>`,
    [code],
  )

  return (
    <div className="code-block-wrapper group/code my-3 rounded-lg border border-border/50 overflow-hidden">
      {/* 头部栏：语言标签 + 复制按钮 */}
      <div className="flex items-center justify-between h-[34px] px-3 border-b border-border/50 bg-muted/60 text-xs text-muted-foreground">
        <span className="font-mono font-medium select-none">{getDisplayName(language)}</span>
        <button
          type="button"
          onClick={handleCopy}
          className="flex items-center gap-1.5 px-1.5 py-0.5 rounded hover:bg-foreground/10 transition-colors hover:text-foreground"
        >
          {copied ? '已复制 ✓' : '复制'}
        </button>
      </div>

      {/* 代码区域 */}
      <div
        className="overflow-x-auto text-[13px] leading-relaxed [&_pre]:!bg-transparent [&_pre]:!m-0 [&_pre]:p-4 [&_code]:!text-[13px] [&_code]:!leading-relaxed"
        dangerouslySetInnerHTML={{ __html: highlightedHtml ?? fallbackHtml }}
      />
    </div>
  )
}
