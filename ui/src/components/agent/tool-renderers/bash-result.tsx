import * as React from 'react'
import { cn } from '@/lib/utils'
import { CollapsibleResult } from './collapsible-result'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

const ERROR_PATTERNS = /(error|exception|traceback|failed|fatal|panic|warning)/i

export function BashResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const command = (input.command as string | undefined) ?? ''
  // Normalize both actual newlines and escaped \n sequences (JSX string attrs pass literal \n)
  const normalized = result.replace(/\\n/g, '\n')
  const lines = normalized.split('\n')

  return (
    <CollapsibleResult charThreshold={2000} previewLines={20}>
      <div className="rounded-md bg-zinc-950 text-zinc-100 font-mono text-xs p-3 overflow-x-auto">
        <div className="text-emerald-400 mb-1.5">$ {command}</div>
        <pre className="whitespace-pre-wrap break-all">
          {lines.map((line, i) => {
            const isErrorLine = isError || ERROR_PATTERNS.test(line)
            return (
              <div key={i} className={cn(isErrorLine && 'text-red-400')}>
                {line || ' ' /* preserve blank lines */}
              </div>
            )
          })}
        </pre>
      </div>
    </CollapsibleResult>
  )
}
