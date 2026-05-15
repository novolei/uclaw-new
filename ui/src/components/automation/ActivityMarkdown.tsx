import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

interface Props {
  content: string
  className?: string
}

export function ActivityMarkdown({ content, className = '' }: Props) {
  return (
    <div
      className={[
        'prose prose-sm prose-zinc dark:prose-invert max-w-none',
        'prose-p:my-1 prose-headings:mt-2 prose-headings:mb-1',
        'prose-h1:text-sm prose-h2:text-sm prose-h3:text-xs',
        'prose-ul:my-1 prose-ol:my-1 prose-li:my-0',
        'prose-code:text-[11px] prose-code:bg-muted prose-code:px-1',
        'prose-code:py-0.5 prose-code:rounded',
        'prose-code:before:content-none prose-code:after:content-none',
        'prose-table:text-[11px] prose-th:px-2 prose-td:px-2',
        'prose-a:text-primary hover:prose-a:opacity-80',
        className,
      ].join(' ')}
    >
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
    </div>
  )
}
