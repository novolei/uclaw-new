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

/**
 * 递归查找带有 `language-xxx` 类名的 React 元素，返回语言标识和该元素的 children。
 * 兼容 react-markdown 用户自定义 `code` 组件覆盖的场景（此时 <code> 的 type
 * 不再是字符串 'code'，而是用户的组件函数）。
 */
function findLangChild(children: React.ReactNode): { language: string; node: React.ReactNode } | null {
  const arr = React.Children.toArray(children)
  for (const child of arr) {
    if (!React.isValidElement(child)) continue
    const props = child.props as { className?: string; children?: React.ReactNode }
    const m = props.className?.match(/language-([\w+-]+)/)
    if (m) return { language: m[1], node: props.children }
    if (props.children) {
      const nested = findLangChild(props.children)
      if (nested) return nested
    }
  }
  return null
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
  // 在 children 树中查找带 language- 类名的元素（兼容 code 组件被覆盖的情况）
  const found = findLangChild(children)
  const language = found?.language ?? 'text'
  const code = extractText(found?.node ?? children).replace(/\n$/, '')

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
    <div
      className={cn(
        'code-block-wrapper not-prose group/code my-3 overflow-hidden rounded-lg shadow-sm',
        'bg-[#282c34] ring-1 ring-white/5',
      )}
    >
      {/* 头部栏：左侧 macOS 风格圆点 + 语言名，右侧复制按钮 */}
      <div className="flex items-center justify-between h-9 pl-3 pr-2 border-b border-white/[0.06] bg-[#21252b] text-[11px]">
        <div className="flex items-center gap-2.5">
          <div className="flex items-center gap-1.5">
            <span className="size-2.5 rounded-full bg-[#ff5f56]/80" />
            <span className="size-2.5 rounded-full bg-[#ffbd2e]/80" />
            <span className="size-2.5 rounded-full bg-[#27c93f]/80" />
          </div>
          <span className="font-mono font-medium tracking-wide text-zinc-400 select-none">
            {getDisplayName(language)}
          </span>
        </div>
        <button
          type="button"
          onClick={handleCopy}
          className={cn(
            'flex items-center gap-1 px-2 py-1 rounded text-[11px] transition-colors',
            copied
              ? 'text-emerald-400'
              : 'text-zinc-500 hover:text-zinc-200 hover:bg-white/[0.06]',
          )}
        >
          {copied ? '已复制' : '复制'}
        </button>
      </div>

      {/* 代码区域 — 强制覆盖 prose 默认颜色，让 Shiki 的内联色彩生效 */}
      <div
        className={cn(
          'overflow-x-auto text-[13.5px] leading-[1.65] text-zinc-100',
          // 重置 Shiki 输出的 <pre>：透明背景 + 无边距 + 标准 padding
          '[&_pre]:!bg-transparent [&_pre]:!m-0 [&_pre]:!px-5 [&_pre]:!py-4 [&_pre]:!text-zinc-100',
          // 重置 <code>：无背景、字号锁定（前后反引号伪元素由 not-prose 自动屏蔽）
          '[&_code]:!bg-transparent [&_code]:!p-0 [&_code]:!text-[13.5px] [&_code]:!leading-[1.65]',
          '[&_code]:!font-mono [&_code]:![text-shadow:none]',
          // 自定义滚动条
          '[&::-webkit-scrollbar]:h-1.5 [&::-webkit-scrollbar-track]:bg-transparent',
          '[&::-webkit-scrollbar-thumb]:bg-white/10 [&::-webkit-scrollbar-thumb]:rounded-full',
          '[&::-webkit-scrollbar-thumb:hover]:bg-white/20',
        )}
        dangerouslySetInnerHTML={{ __html: highlightedHtml ?? fallbackHtml }}
      />
    </div>
  )
}
