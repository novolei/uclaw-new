import * as React from 'react'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

export function EditResultRenderer(_props: Props): React.ReactElement {
  return <div className="text-xs text-muted-foreground italic">edit placeholder</div>
}
