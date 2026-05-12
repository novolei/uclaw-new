/**
 * MarkdownRenderer — Renders a markdown file via react-markdown + remark-gfm.
 *
 * Uses uClaw's existing markdown deps + @tailwindcss/typography for styling.
 * No new packages. Safe: react-markdown does not execute scripts; we don't
 * enable raw HTML rendering.
 *
 * Typography uses `prose-zinc` so headings/code/tables inherit theme tokens
 * cleanly across uClaw's 11 themes. Code blocks inside markdown get muted
 * backgrounds — the user can still copy from them.
 */

import * as React from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

interface MarkdownRendererProps {
  /** Decoded file contents. */
  text: string
}

export function MarkdownRenderer({ text }: MarkdownRendererProps): React.ReactElement {
  return (
    <div className="flex-1 min-h-0 overflow-auto bg-popover">
      <div
        className={[
          'prose prose-sm prose-zinc dark:prose-invert',
          'max-w-3xl mx-auto px-6 py-5',
          // Code styling within markdown — defer to theme tokens, not literal grays.
          'prose-code:rounded prose-code:px-1 prose-code:py-0.5',
          'prose-code:font-mono prose-code:text-[12px]',
          'prose-code:bg-muted prose-code:text-foreground/85',
          'prose-code:before:content-none prose-code:after:content-none',
          'prose-pre:bg-muted prose-pre:border prose-pre:border-border/50',
          'prose-pre:text-[12px] prose-pre:leading-relaxed',
          // Headings get tighter rhythm in a narrow panel.
          'prose-headings:mt-5 prose-headings:mb-3',
          'prose-h1:text-[20px] prose-h2:text-[17px] prose-h3:text-[14px]',
          // Tables shouldn't blow out the panel.
          'prose-table:text-[12px] prose-th:px-2 prose-td:px-2',
          // Links pick up the accent color.
          'prose-a:text-[hsl(var(--primary))] hover:prose-a:opacity-80',
          // Blockquote uses theme border, not default gray.
          'prose-blockquote:border-l-2 prose-blockquote:border-border',
          'prose-blockquote:not-italic prose-blockquote:text-muted-foreground',
        ].join(' ')}
      >
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
      </div>
    </div>
  )
}
