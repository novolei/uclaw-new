import * as React from 'react'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

export function WriteResultRenderer(_props: Props): React.ReactElement {
  return <div className="text-xs text-muted-foreground italic">write placeholder</div>
}
