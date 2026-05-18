import * as React from 'react'
import { WriteResultRenderer } from './write-result'
import { EditResultRenderer } from './edit-result'
import { ReadResultRenderer } from './read-result'
import { BashResultRenderer } from './bash-result'
import { DefaultResultRenderer } from './default-result'

export interface ToolResultRendererProps {
  toolName: string
  input: Record<string, unknown>
  result: string
  isError: boolean
}

/**
 * Dispatcher for tool result rendering. Switches by uClaw's
 * snake_case tool names (not Proma's PascalCase). Phase 1 covers
 * the four highest-traffic tools + a JSON-aware fallback.
 * Phase 2 will add grep / glob / web_fetch / web_search.
 */
export function ToolResultRenderer({
  toolName,
  input,
  result,
  isError,
}: ToolResultRendererProps): React.ReactElement {
  const props = { input, result, isError }
  switch (toolName) {
    case 'write_file':
      return <WriteResultRenderer {...props} />
    case 'edit':
      return <EditResultRenderer {...props} />
    case 'read_file':
      return <ReadResultRenderer {...props} />
    case 'bash':
      return <BashResultRenderer {...props} />
    default:
      return <DefaultResultRenderer toolName={toolName} {...props} />
  }
}
