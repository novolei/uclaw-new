import * as React from 'react'
import { Award, Check, EyeOff, Loader2, ScrollText } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { getPersonaRelationshipTimeline, updatePersonaKeepsakeStatus } from '@/lib/persona'
import type {
  PersonaKeepsake,
  PersonaKeepsakeStatus,
  PersonaRelationshipTimeline,
} from '@/lib/persona-types'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'

export function PersonaBondTimeline(): React.ReactElement {
  const [timeline, setTimeline] = React.useState<PersonaRelationshipTimeline | null>(null)
  const [busyId, setBusyId] = React.useState<string | null>(null)

  React.useEffect(() => {
    let cancelled = false
    getPersonaRelationshipTimeline()
      .then((next) => {
        if (!cancelled) setTimeline(next)
      })
      .catch((error) => {
        console.error('[PersonaBondTimeline] load failed', error)
        toast.error('加载关系时间线失败')
      })
    return () => {
      cancelled = true
    }
  }, [])

  const updateKeepsake = async (id: string, status: PersonaKeepsakeStatus) => {
    setBusyId(id)
    try {
      const next = await updatePersonaKeepsakeStatus({ id, status })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] update failed', error)
      toast.error('更新纪念物失败')
    } finally {
      setBusyId(null)
    }
  }

  const score = timeline?.affinity.score ?? 0
  const scoreWidth = `${Math.max(0, Math.min(100, score))}%`

  return (
    <SettingsSection
      title="关系时间线"
      description="纪念物、亲密度和勋章只记录共同工作的经历，不改变 Agent 能力。"
    >
      <SettingsCard>
        <div className="space-y-4 p-3 text-sm">
          <div>
            <div className="text-xs text-muted-foreground">亲密度</div>
            <div className="mt-1 flex items-end gap-2">
              <div className="text-2xl font-semibold leading-none text-foreground">
                {timeline ? score : '加载中'}
              </div>
              <div className="text-xs text-muted-foreground">共同经历分</div>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full bg-primary transition-[width]"
                style={{ width: scoreWidth }}
              />
            </div>
            {timeline ? (
              <div className="mt-2 space-y-1 text-xs text-muted-foreground">
                {timeline.affinity.explanation.length > 0 ? (
                  timeline.affinity.explanation.map((line) => <div key={line}>{line}</div>)
                ) : (
                  <div>还没有足够的共同经历沉淀。</div>
                )}
              </div>
            ) : (
              <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                <Loader2 className="size-3 animate-spin" />
                读取中…
              </div>
            )}
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="rounded-md border border-border/50 p-3">
              <div className="flex items-center gap-2 text-xs font-medium text-foreground">
                <ScrollText size={14} className="text-muted-foreground" />
                纪念物
              </div>
              <KeepsakeList
                keepsakes={timeline?.keepsakes ?? []}
                busyId={busyId}
                onUpdate={(id, status) => void updateKeepsake(id, status)}
              />
            </div>

            <div className="rounded-md border border-border/50 p-3">
              <div className="flex items-center gap-2 text-xs font-medium text-foreground">
                <Award size={14} className="text-muted-foreground" />
                勋章
              </div>
              <div className="mt-2 text-xs text-muted-foreground">
                {timeline
                  ? `已确认 ${timeline.factors.acceptedKeepsakes} 个经历，后续可解锁关系勋章。`
                  : '勋章来自可解释的共同经历，只改变关系叙事，不提供额外能力。'}
              </div>
            </div>
          </div>
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}

function KeepsakeList({
  keepsakes,
  busyId,
  onUpdate,
}: {
  keepsakes: PersonaKeepsake[]
  busyId: string | null
  onUpdate: (id: string, status: PersonaKeepsakeStatus) => void
}) {
  if (keepsakes.length === 0) {
    return (
      <div className="mt-2 text-xs text-muted-foreground">
        成功合作后，UClaw 可以提议一张经历卡，由你确认后保存。
      </div>
    )
  }

  return (
    <div className="mt-3 space-y-2">
      {keepsakes.map((keepsake) => (
        <div key={keepsake.id} className="rounded-md border border-border/40 bg-muted/20 p-2.5">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <div className="truncate text-xs font-medium text-foreground">{keepsake.title}</div>
              <div className="mt-1 text-xs leading-5 text-muted-foreground">
                {keepsake.narrative}
              </div>
            </div>
            <span className="shrink-0 rounded border border-border/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {statusLabel(keepsake.status)}
            </span>
          </div>
          {keepsake.learnedText && (
            <div className="mt-2 rounded bg-background/50 px-2 py-1.5 text-[11px] text-muted-foreground">
              {keepsake.learnedText}
            </div>
          )}
          {keepsake.status === 'proposed' && (
            <div className="mt-2 flex justify-end gap-1.5">
              <Button
                size="sm"
                variant="ghost"
                className="h-7 px-2 text-xs"
                disabled={busyId === keepsake.id}
                onClick={() => onUpdate(keepsake.id, 'hidden')}
              >
                {busyId === keepsake.id ? (
                  <Loader2 className="mr-1 size-3 animate-spin" />
                ) : (
                  <EyeOff className="mr-1 size-3" />
                )}
                隐藏
              </Button>
              <Button
                size="sm"
                className="h-7 px-2 text-xs"
                disabled={busyId === keepsake.id}
                onClick={() => onUpdate(keepsake.id, 'accepted')}
              >
                {busyId === keepsake.id ? (
                  <Loader2 className="mr-1 size-3 animate-spin" />
                ) : (
                  <Check className="mr-1 size-3" />
                )}
                接受
              </Button>
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

function statusLabel(status: PersonaKeepsakeStatus): string {
  switch (status) {
    case 'accepted':
      return '已确认'
    case 'hidden':
      return '已隐藏'
    case 'discarded':
      return '已丢弃'
    case 'proposed':
    default:
      return '待确认'
  }
}
