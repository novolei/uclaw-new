import * as React from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import type { EscalationRow } from '@/lib/tauri-bridge'

interface EscalationChoice {
  id: string
  label: string
  description?: string
}

interface Props {
  escalation: EscalationRow
  onResolve: (choiceId: string) => void | Promise<void>
}

function parseChoices(json: string): EscalationChoice[] {
  try {
    const parsed = JSON.parse(json)
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

export function EscalationModal({ escalation, onResolve }: Props): React.ReactElement {
  const choices = parseChoices(escalation.choicesJson)
  const [submitting, setSubmitting] = React.useState<string | null>(null)

  const handleClick = async (choiceId: string) => {
    setSubmitting(choiceId)
    try {
      await onResolve(choiceId)
    } finally {
      setSubmitting(null)
    }
  }

  return (
    <Dialog open onOpenChange={() => {}}>
      <DialogContent hideClose>
        <DialogHeader>
          <DialogTitle>{escalation.question}</DialogTitle>
          <DialogDescription>
            数字员工请求确认（spec {escalation.specId.slice(0, 8)}）
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-2 mt-2">
          {choices.length === 0 && (
            <p className="text-sm text-muted-foreground">选项缺失（choices_json 解析失败）</p>
          )}
          {choices.map((c) => (
            <Button
              key={c.id}
              variant="outline"
              disabled={submitting !== null}
              onClick={() => handleClick(c.id)}
              className="w-full justify-start"
            >
              <span className="font-medium">{c.label}</span>
              {c.description && (
                <span className="text-muted-foreground ml-2">— {c.description}</span>
              )}
              {submitting === c.id && <span className="ml-auto text-muted-foreground">…</span>}
            </Button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
