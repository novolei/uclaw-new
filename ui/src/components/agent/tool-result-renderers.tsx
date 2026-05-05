// [PLACEHOLDER] agent/tool-result-renderers — 工具结果渲染器
import * as React from 'react'

interface ToolResultRendererProps {
  toolName: string
  input: Record<string, unknown>
  result: string
  isError?: boolean
}

export function ToolResultRenderer({ toolName, input, result, isError }: ToolResultRendererProps): React.ReactElement {
  return (
    <div className="text-xs">
      {isError ? (
        <pre className="text-destructive whitespace-pre-wrap break-all">{result}</pre>
      ) : (
        <pre className="text-muted-foreground whitespace-pre-wrap break-all max-h-[200px] overflow-y-auto">{result}</pre>
      )}
    </div>
  )
}
