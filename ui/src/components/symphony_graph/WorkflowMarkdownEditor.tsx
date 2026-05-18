/**
 * WorkflowMarkdownEditor — Raw view for the SymphonyCanvas.
 *
 * Lets the user edit the WORKFLOW.md text directly. Saves through
 * `symphony_import_workflow_md` on a debounce so YAML errors don't
 * thrash the database. Uses a plain textarea today; a CodeMirror upgrade
 * with YAML + Markdown syntax highlighting is a follow-up.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import {
  symphonyImportWorkflowMd,
  type SymphonyWorkflowDetailDto,
} from '@/lib/tauri-bridge'
import { symphonyWorkflowDetailsAtom } from '@/atoms/symphony_graph'

export interface WorkflowMarkdownEditorProps {
  workflowId: string
  detail: SymphonyWorkflowDetailDto
}

export function WorkflowMarkdownEditor({
  workflowId,
  detail,
}: WorkflowMarkdownEditorProps): React.ReactElement {
  const [text, setText] = React.useState(detail.definitionMd)
  const [status, setStatus] = React.useState<
    'clean' | 'dirty' | 'saving' | 'error'
  >('clean')
  const [errMsg, setErrMsg] = React.useState<string | null>(null)
  const setDetails = useSetAtom(symphonyWorkflowDetailsAtom)
  const timer = React.useRef<number | null>(null)

  React.useEffect(() => {
    setText(detail.definitionMd)
    setStatus('clean')
  }, [detail.definitionMd])

  // Cleanup pending debounce on unmount so we don't fire setStatus on a
  // gone-component (React warns + can lead to stale state writes).
  React.useEffect(() => {
    return () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current)
        timer.current = null
      }
    }
  }, [])

  const onChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value)
    setStatus('dirty')
    if (timer.current !== null) window.clearTimeout(timer.current)
    timer.current = window.setTimeout(async () => {
      setStatus('saving')
      try {
        const result = await symphonyImportWorkflowMd(e.target.value)
        // Re-fetch isn't strictly necessary — we know the id + version, but
        // the version bump means the detail will be stale until we read.
        setStatus('clean')
        setErrMsg(null)
        setDetails((prev) => {
          const next = { ...prev }
          if (next[workflowId]) {
            next[workflowId] = {
              ...next[workflowId],
              definitionMd: e.target.value,
              summary: {
                ...next[workflowId].summary,
                currentVersion: result.version,
              },
            }
          }
          return next
        })
      } catch (err) {
        setStatus('error')
        setErrMsg(String(err))
      }
    }, 500)
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-border bg-muted/30 px-4 py-1.5">
        <span className="text-xs text-muted-foreground">WORKFLOW.md</span>
        <span className="text-xs">
          {status === 'clean' && (
            <span className="text-muted-foreground">Saved</span>
          )}
          {status === 'dirty' && (
            <span className="text-muted-foreground">Editing…</span>
          )}
          {status === 'saving' && <span className="text-primary">Saving…</span>}
          {status === 'error' && (
            <span className="text-destructive" title={errMsg ?? undefined}>
              YAML error
            </span>
          )}
        </span>
      </div>
      <textarea
        className="flex-1 resize-none bg-background p-4 font-mono text-xs text-foreground outline-none"
        value={text}
        onChange={onChange}
        spellCheck={false}
      />
    </div>
  )
}
