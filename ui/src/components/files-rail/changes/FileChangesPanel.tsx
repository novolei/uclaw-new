import * as React from 'react'
import { ChangeRow, type FileChangeBadge } from './ChangeRow'

/**
 * FileChangesPanel — Lists agent file edits within the current session.
 *
 * W3 implementation: reads from a stub data source that returns the empty
 * list. W4 wires this to the agent_turns table so each in-session file edit
 * surfaces as a row.
 */

interface ChangeEntry {
  badge: FileChangeBadge
  path: string
  newPath?: string
}

export function FileChangesPanel(): React.ReactElement {
  const changes: ChangeEntry[] = []

  if (changes.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center flex-1 min-h-[120px] p-6 text-center">
        <div className="text-[12px] text-muted-foreground">
          这个会话还没有文件改动
        </div>
        <div className="mt-1 text-[11px] text-muted-foreground/60">
          Agent 写入或修改文件后会出现在这里
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-y-auto py-1">
      {changes.map((c, idx) => (
        <ChangeRow
          key={`${c.badge}-${c.path}-${idx}`}
          badge={c.badge}
          path={c.path}
          newPath={c.newPath}
        />
      ))}
    </div>
  )
}
