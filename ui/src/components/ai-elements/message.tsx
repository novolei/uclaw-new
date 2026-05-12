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

/** 标题渲染器：h1/h2 带左侧实心条做视觉锚点；h3 纯文本 */
const MarkdownH1 = React.memo(function MarkdownH1({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h1 className="flex items-center gap-2.5 text-[22px] font-semibold tracking-[-0.015em] mt-7 mb-3.5 first:mt-0 leading-[1.3]">
      <span className="w-[3px] h-[18px] bg-foreground rounded-sm shrink-0" aria-hidden />
      <span>{children}</span>
    </h1>
  )
})

const MarkdownH2 = React.memo(function MarkdownH2({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h2 className="flex items-center gap-2.5 text-[19px] font-semibold tracking-[-0.012em] mt-[22px] mb-3 first:mt-0 leading-[1.35]">
      <span className="w-[3px] h-4 bg-foreground rounded-sm shrink-0" aria-hidden />
      <span>{children}</span>
    </h2>
  )
})

const MarkdownH3 = React.memo(function MarkdownH3({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h3 className="text-[16px] font-semibold mt-[18px] mb-2 first:mt-0 leading-[1.4] tracking-[-0.005em]">
      {children}
    </h3>
  )
})

/** 表格渲染器：包一层 card 容器，让表格有清晰边界 */
const MarkdownTable = React.memo(function MarkdownTable({
  children,
}: React.HTMLAttributes<HTMLTableElement>): React.ReactElement {
  // `not-prose` opts the entire table subtree out of @tailwindcss/typography
  // defaults. Without it, prose injects:
  //   - `table { margin-top: 2em; margin-bottom: 2em }`
  //     → renders as empty cream bands above/below the table inside our
  //       `bg-card` wrapper (user-visible "blank rows" bug).
  //   - `tbody tr { border-bottom: 1px }`
  //     → competes with our `border-border/40` row borders.
  //   - `thead th { vertical-align: bottom }`
  //     → fights our `align-middle`.
  // Once not-prose is set, our MarkdownTr/Th/Td classes have unambiguous
  // control of every visual property.
  return (
    <div className="not-prose my-3 rounded-[10px] border border-border overflow-hidden bg-card">
      <table className="w-full border-collapse text-[14px]">{children}</table>
    </div>
  )
})

const MarkdownThead = React.memo(function MarkdownThead({
  children,
}: React.HTMLAttributes<HTMLTableSectionElement>): React.ReactElement {
  // Theme-agnostic tint via `--foreground` — warm-paper / qingye etc.
  // have such low-contrast `--muted` against their `--card` that
  // bg-muted/* is invisible. A foreground-derived tint is identical
  // visual weight on every theme.
  return <thead className="bg-foreground/[0.05]">{children}</thead>
})

const MarkdownTr = React.memo(function MarkdownTr({
  children,
}: React.HTMLAttributes<HTMLTableRowElement>): React.ReactElement {
  // Polish set, all theme-agnostic via foreground tint:
  //   - Zebra (3.5% foreground tint) — readable on every theme without
  //     competing with status badges or text legibility
  //   - Hover (8% foreground tint) — clearly tactile on light AND dark
  //   - Border on every td except in the last row keeps the card
  //     bottom edge clean
  // Why foreground/<alpha> and not muted/<alpha>: `--muted` is ~95-98%
  // of `--card` on warm themes (paper, sepia), so a 30% overlay of one
  // beige on another beige produces ~1% rendered difference — invisible.
  // Foreground tints are derived from the text color, so contrast is
  // guaranteed on every theme.
  return (
    <tr
      className={cn(
        '[&:not(:last-child)>td]:border-b [&>td]:border-border/40',
        '[&:nth-child(even)]:bg-foreground/[0.035]',
        'hover:bg-foreground/[0.075] transition-colors',
      )}
    >
      {children}
    </tr>
  )
})

const MarkdownTh = React.memo(function MarkdownTh({
  children,
}: React.HTMLAttributes<HTMLTableCellElement>): React.ReactElement {
  return (
    <th className="text-left px-3.5 py-2 text-[11px] font-semibold uppercase tracking-[0.07em] text-muted-foreground/85 border-b border-border align-middle">
      {children}
    </th>
  )
})

/** 递归收集 React 子树里的文本节点（用于 td 内的状态识别） */
function extractText(node: React.ReactNode): string {
  if (node == null || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(extractText).join('')
  if (React.isValidElement(node)) {
    const props = node.props as { children?: React.ReactNode }
    return extractText(props.children)
  }
  return ''
}

type StatusVariant = 'success' | 'warning' | 'danger'

const STATUS_PATTERNS: Array<{ re: RegExp; variant: StatusVariant }> = [
  // Order matters: explicit failure / negation wins over completion.
  { re: /❌|✗|未开始|尚未|failed\b|error\b/i, variant: 'danger' },
  { re: /⏳|未完成|in[\s-]?progress\b|pending\b|wip\b/i, variant: 'warning' },
  { re: /✅|✓|已完成/, variant: 'success' },
]

function detectStatus(text: string): StatusVariant | null {
  for (const { re, variant } of STATUS_PATTERNS) {
    if (re.test(text)) return variant
  }
  return null
}

const STATUS_CLASS: Record<StatusVariant, string> = {
  success: 'bg-[hsl(var(--success-bg))] text-[hsl(var(--success))]',
  warning: 'bg-[hsl(var(--warning-bg))] text-[hsl(var(--warning))]',
  danger:  'bg-[hsl(var(--danger-bg))] text-[hsl(var(--danger))]',
}

/** 单元格渲染器：检测状态文本，自动包成 badge */
const MarkdownTd = React.memo(function MarkdownTd({
  children,
}: React.HTMLAttributes<HTMLTableCellElement>): React.ReactElement {
  const text = extractText(children).trim()
  const variant = text ? detectStatus(text) : null
  // Polish: tighter vertical padding + middle align + zero out any
  // `prose-p:my-1.5` margin that would otherwise stretch cells when
  // the content is wrapped in <p> by react-markdown. `align-middle`
  // matters when one cell wraps to two lines and the neighbour doesn't.
  const tdClass =
    'px-3.5 py-2 text-[14px] align-middle leading-[1.55] [&>p]:my-0 [&_p]:my-0'
  if (variant) {
    return (
      <td className={tdClass}>
        <span
          data-status={variant}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[12px] font-medium',
            STATUS_CLASS[variant],
          )}
        >
          {text}
        </span>
      </td>
    )
  }
  return <td className={tdClass}>{children}</td>
})

const MarkdownBlockquote = React.memo(function MarkdownBlockquote({
  children,
}: React.HTMLAttributes<HTMLQuoteElement>): React.ReactElement {
  return (
    <blockquote className="my-3 pl-3 border-l-2 border-foreground/20 text-foreground/75 not-italic [&>p]:my-1">
      {children}
    </blockquote>
  )
})

const MarkdownHr = React.memo(function MarkdownHr(): React.ReactElement {
  return <hr className="my-6 border-0 border-t border-border/60" />
})

/**
 * `<strong>` renderer applied uniformly to ALL `**bold**` spans, regardless
 * of whether they live inside a `not-prose` subtree (e.g. our tables).
 *
 * Why a dedicated component instead of `prose-strong:` utilities:
 *   1. `MarkdownTable` wraps the `<table>` in `not-prose` (so prose's
 *      default table styling doesn't fight our card layout). That also
 *      strips `prose-strong:*` styles from `<strong>` inside cells —
 *      browser default `font-weight: bold` (700) takes over, which reads
 *      as a typeface change in mixed CJK + Latin prose.
 *   2. The previous setup also forced `text-foreground` on bold, which
 *      broke `<blockquote>`'s dimmed (`text-foreground/75`) color —
 *      bold spans popped via color contrast even when their weight
 *      matched, again reading as a different font.
 *
 * The fix is purely additive: `font-medium` (500) for a subtle emphasis,
 * inherit color from the parent so bold blends with whatever container
 * it lives in.
 */
const MarkdownStrong = React.memo(function MarkdownStrong({
  children,
}: React.HTMLAttributes<HTMLElement>): React.ReactElement {
  return <strong className="font-medium text-inherit">{children}</strong>
})

const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
  h1: MarkdownH1,
  h2: MarkdownH2,
  h3: MarkdownH3,
  table: MarkdownTable,
  thead: MarkdownThead,
  tr: MarkdownTr,
  th: MarkdownTh,
  td: MarkdownTd,
  blockquote: MarkdownBlockquote,
  hr: MarkdownHr,
  strong: MarkdownStrong,
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
          'chat-content prose prose-sm dark:prose-invert max-w-none',
          'prose-p:my-1.5 prose-p:leading-[1.65]',
          'prose-pre:my-0 prose-pre:bg-transparent prose-pre:p-0',
          'prose-a:text-primary prose-a:no-underline hover:prose-a:underline',
          // `<strong>` styling moved to the MarkdownStrong component
          // override (font-medium + inherit color), so it applies inside
          // `not-prose` table cells too. Don't restore prose-strong:*
          // here — they would re-introduce color contrast inside
          // blockquotes.
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
