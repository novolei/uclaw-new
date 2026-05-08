import * as React from 'react'
import Markdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import remarkMath from 'remark-math'
import rehypeKatex from 'rehype-katex'
import { cn } from '@/lib/utils'
import { MarkdownCodeBlock } from '@/components/shared/code-block/CodeBlock'

// ===== Message 原语组件 =====

interface MessageProps {
  from: 'user' | 'assistant'
  children: React.ReactNode
}

export function Message({ from, children }: MessageProps): React.ReactElement {
  return (
    <div className={cn('px-4 py-3', from === 'user' ? 'bg-transparent' : '')}>
      {children}
    </div>
  )
}

export function MessageHeader({
  model,
  time,
  logo,
}: {
  model?: string
  time?: string
  logo?: React.ReactNode
}): React.ReactElement {
  return (
    <div className="flex items-start gap-2.5 mb-2.5">
      {logo}
      <div className="flex flex-col justify-between h-[35px]">
        <span className="text-sm font-semibold text-foreground/60 leading-none">{model || 'Assistant'}</span>
        {time && <span className="text-[10px] text-foreground/[0.38] leading-none">{time}</span>}
      </div>
    </div>
  )
}

export function MessageContent({ children, className }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return <div className={cn('pl-[46px]', className)}>{children}</div>
}

export function MessageActions({
  children,
  className,
}: {
  children: React.ReactNode
  className?: string
}): React.ReactElement {
  return (
    <div className={cn('flex items-center gap-0.5 opacity-0 hover:opacity-100 transition-opacity', className)}>
      {children}
    </div>
  )
}

export function MessageAction({
  children,
  onClick,
  tooltip,
  disabled,
}: {
  children: React.ReactNode
  onClick?: () => void
  tooltip?: string
  disabled?: boolean
}): React.ReactElement {
  return (
    <button
      type="button"
      className="p-1 rounded text-muted-foreground/60 hover:text-foreground transition-colors disabled:opacity-40"
      onClick={onClick}
      title={tooltip}
      disabled={disabled}
    >
      {children}
    </button>
  )
}

// ===== mdast 工具：保留换行（user 消息中的 \n 转为 <br>）=====

interface MdastTextNode { type: 'text'; value: string }
interface MdastBreakNode { type: 'break' }
interface MdastGenericNode { type: string; children?: MdastNode[]; value?: string }
type MdastNode = MdastTextNode | MdastBreakNode | MdastGenericNode
interface MdastParent { type: string; children: MdastNode[] }

function walkMdastText(
  node: MdastParent,
  visitor: (node: MdastTextNode, index: number, parent: MdastParent) => number | void,
): void {
  if (!node.children) return
  for (let i = 0; i < node.children.length; i++) {
    const child = node.children[i]!
    if (child.type === 'text') {
      const result = visitor(child as MdastTextNode, i, node)
      if (typeof result === 'number') i = result - 1
    } else if (child.type !== 'code' && child.type !== 'inlineCode') {
      const asParent = child as MdastParent
      if (asParent.children) walkMdastText(asParent, visitor)
    }
  }
}

/** 将 text 节点中的 \n 转为 break 节点（跳过代码块） */
function remarkPreserveBreaks() {
  return (tree: MdastParent) => {
    walkMdastText(tree, (node, index, parent) => {
      const text = node.value
      if (!text.includes('\n')) return
      const lines = text.split('\n')
      const parts: MdastNode[] = []
      for (let i = 0; i < lines.length; i++) {
        if (i > 0) parts.push({ type: 'break' })
        if (lines[i]) parts.push({ type: 'text', value: lines[i] })
      }
      parent.children.splice(index, 1, ...parts)
      return index + parts.length
    })
  }
}

// ===== Markdown 渲染器组件 =====

const REMARK_PLUGINS = [remarkGfm, remarkMath]
const REMARK_PLUGINS_WITH_BREAKS = [remarkGfm, remarkMath, remarkPreserveBreaks]
const REHYPE_PLUGINS = [rehypeKatex]

/** 链接渲染器：外部 URL 用 Tauri openExternal 打开 */
const MarkdownLink = React.memo(function MarkdownLink({
  href,
  children: linkChildren,
  ...linkProps
}: React.AnchorHTMLAttributes<HTMLAnchorElement>): React.ReactElement {
  return (
    <a
      {...linkProps}
      href={href}
      onClick={(e) => {
        if (href && (href.startsWith('http://') || href.startsWith('https://'))) {
          e.preventDefault()
          // 通过 Tauri 后端打开外部链接（避免在 webview 内导航）
          import('@/lib/tauri-bridge').then((m) => m.openExternal(href)).catch(() => {
            window.open(href, '_blank', 'noopener,noreferrer')
          })
        }
      }}
      title={href}
    >
      {linkChildren}
    </a>
  )
})

/** 代码块渲染器 */
const MarkdownPre = React.memo(function MarkdownPre({
  children: preChildren,
}: { children?: React.ReactNode }): React.ReactElement {
  return <MarkdownCodeBlock>{preChildren}</MarkdownCodeBlock>
})

/** 行内代码渲染器（仅在没有 language- 类时生效；有则交给 MarkdownPre 处理） */
const MarkdownInlineCode = React.memo(function MarkdownInlineCode({
  children: codeChildren,
  className: codeClassName,
  ...codeProps
}: React.HTMLAttributes<HTMLElement>): React.ReactElement {
  // 代码块（fenced code）的 <code> 走 MarkdownPre，这里只处理行内代码
  if (codeClassName) {
    return <code className={codeClassName} {...codeProps}>{codeChildren}</code>
  }
  return (
    <code
      className="rounded bg-foreground/10 px-[0.35em] py-[0.15em] text-[0.875em] font-medium"
      {...codeProps}
    >
      {codeChildren}
    </code>
  )
})

const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
} as const

interface MessageResponseProps {
  children: React.ReactNode
  className?: string
  /** 是否在 text 节点中保留换行（user 消息常用） */
  preserveBreaks?: boolean
  /** 兼容 Agent 视图：基础目录路径（当前未启用文件路径 chip，仅占位以避免类型破坏） */
  basePath?: string
  basePaths?: string[]
}

/**
 * 使用 react-markdown 渲染 assistant 消息内容。
 * 支持 GFM、数学公式（KaTeX）、代码语法高亮（Shiki）。
 */
export const MessageResponse = React.memo(
  function MessageResponse({ children, className, preserveBreaks = false }: MessageResponseProps): React.ReactElement {
    const remarkPlugins = preserveBreaks ? REMARK_PLUGINS_WITH_BREAKS : REMARK_PLUGINS
    const content = typeof children === 'string' ? children : ''

    return (
      <div
        className={cn(
          'chat-content prose prose-sm dark:prose-invert max-w-none text-[15px] leading-relaxed',
          'prose-p:my-1.5 prose-p:leading-[1.6] prose-li:leading-[1.6]',
          'prose-pre:my-0 prose-pre:bg-transparent prose-pre:p-0',
          'prose-headings:my-2 prose-headings:font-semibold',
          'prose-a:text-primary prose-a:no-underline hover:prose-a:underline',
          'prose-blockquote:border-l-2 prose-blockquote:border-foreground/20 prose-blockquote:text-foreground/70',
          'prose-table:my-2 prose-th:bg-muted/40 prose-th:font-semibold',
          '[&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
          className,
        )}
      >
        {content ? (
          <Markdown
            remarkPlugins={remarkPlugins}
            rehypePlugins={REHYPE_PLUGINS}
            urlTransform={defaultUrlTransform}
            components={MARKDOWN_COMPONENTS}
          >
            {content}
          </Markdown>
        ) : (
          typeof children !== 'string' ? children : null
        )}
      </div>
    )
  },
  (prev, next) => prev.children === next.children && prev.preserveBreaks === next.preserveBreaks && prev.className === next.className,
)

/** 用户消息：通过 MessageResponse 渲染 markdown，但保留换行 */
export function UserMessageContent({ children }: { children: React.ReactNode }): React.ReactElement {
  if (typeof children !== 'string') {
    return <div className="chat-content text-[15px] leading-relaxed whitespace-pre-wrap break-words">{children}</div>
  }
  return (
    <MessageResponse preserveBreaks className="break-words">
      {children}
    </MessageResponse>
  )
}

export function BasePathsProvider({
  basePaths,
  children,
}: {
  basePaths?: string[]
  children: React.ReactNode
}): React.ReactElement {
  return <>{children}</>
}

// ===== 流式辅助组件 =====

export function MessageLoading({ startedAt }: { startedAt?: number }): React.ReactElement {
  return (
    <div className="flex items-center gap-1.5 py-1">
      <div className="flex gap-0.5">
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" />
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" style={{ animationDelay: '150ms' }} />
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" style={{ animationDelay: '300ms' }} />
      </div>
    </div>
  )
}

export function StreamingIndicator(): React.ReactElement {
  return (
    <span className="inline-block ml-0.5 w-2 h-4 bg-foreground/40 animate-pulse rounded-sm" />
  )
}

export function MessageStopped(): React.ReactElement {
  return (
    <div className="text-sm text-foreground/40 italic">
      已中止生成
    </div>
  )
}

export function MessageAttachments({ attachments }: { attachments: Array<{ filename: string; mediaType: string; localPath: string }> }): React.ReactElement {
  return (
    <div className="flex flex-wrap gap-2 mt-2">
      {attachments.map((att, i) => (
        <div key={i} className="text-xs text-muted-foreground bg-muted/50 rounded px-2 py-1">
          {att.filename}
        </div>
      ))}
    </div>
  )
}
