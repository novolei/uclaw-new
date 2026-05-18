import * as React from 'react'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

export function BashResultRenderer({ result, isError }: Props): React.ReactElement {
  return (
    <pre
      className={
        isError
          ? 'text-xs whitespace-pre-wrap break-all text-destructive'
          : 'text-xs whitespace-pre-wrap break-all text-muted-foreground'
      }
    >
      {result}
    </pre>
  )
}
