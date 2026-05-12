/**
 * MarkdownRenderer — Renders a markdown file via react-markdown + remark-gfm.
 *
 * Uses uClaw's existing markdown deps + @tailwindcss/typography for styling.
 * No new packages. Safe: react-markdown does not execute scripts; we don't
 * enable raw HTML rendering.
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
      <div className="prose prose-sm dark:prose-invert max-w-none p-5">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
      </div>
    </div>
  )
}
