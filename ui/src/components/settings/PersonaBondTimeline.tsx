import * as React from 'react'
import {
  Award,
  BookOpen,
  Check,
  EyeOff,
  Loader2,
  Plus,
  ScrollText,
  Trash2,
} from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import {
  createPersonaJournalEntry,
  deletePersonaJournalEntry,
  getPersonaRelationshipTimeline,
  promotePersonaJournalEntry,
  updatePersonaBadgeVisibility,
  updatePersonaKeepsakeStatus,
  updatePersonaRelationshipSettings,
} from '@/lib/persona'
import type {
  BondProfile,
  PersonaBadge,
  PersonaBondField,
  PersonaJournalEntry,
  PersonaKeepsake,
  PersonaKeepsakeStatus,
  PersonaRelationshipTimeline,
} from '@/lib/persona-types'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsToggle } from './primitives/SettingsToggle'

export function PersonaBondTimeline(): React.ReactElement {
  const [timeline, setTimeline] = React.useState<PersonaRelationshipTimeline | null>(null)
  const [busyId, setBusyId] = React.useState<string | null>(null)
  const [journalObservation, setJournalObservation] = React.useState('')
  const [journalInterpretation, setJournalInterpretation] = React.useState('')

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
      console.error('[PersonaBondTimeline] update keepsake failed', error)
      toast.error('更新纪念物失败')
    } finally {
      setBusyId(null)
    }
  }

  const createJournal = async () => {
    const observation = journalObservation.trim()
    if (!observation) return
    setBusyId('journal:create')
    try {
      const next = await createPersonaJournalEntry({
        sessionId: null,
        taskId: null,
        observation,
        interpretation: journalInterpretation.trim() || null,
        confidence: 'medium',
      })
      setJournalObservation('')
      setJournalInterpretation('')
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] create journal failed', error)
      toast.error('记录内心层失败')
    } finally {
      setBusyId(null)
    }
  }

  const promoteJournal = async (id: string, field: PersonaBondField) => {
    setBusyId(`${id}:${field}`)
    try {
      const next = await promotePersonaJournalEntry({ id, field })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] promote journal failed', error)
      toast.error('沉淀关系档案失败')
    } finally {
      setBusyId(null)
    }
  }

  const deleteJournal = async (id: string) => {
    setBusyId(`${id}:delete`)
    try {
      const next = await deletePersonaJournalEntry(id)
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] delete journal failed', error)
      toast.error('删除内心层失败')
    } finally {
      setBusyId(null)
    }
  }

  const toggleGamification = async (gamificationEnabled: boolean) => {
    setBusyId('settings:gamification')
    try {
      const next = await updatePersonaRelationshipSettings({ gamificationEnabled })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] update settings failed', error)
      toast.error('更新关系奖励失败')
    } finally {
      setBusyId(null)
    }
  }

  const hideBadge = async (badgeKey: string) => {
    setBusyId(`badge:${badgeKey}`)
    try {
      const next = await updatePersonaBadgeVisibility({ badgeKey, hidden: true })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] update badge failed', error)
      toast.error('隐藏勋章失败')
    } finally {
      setBusyId(null)
    }
  }

  const gamificationEnabled = timeline?.settings.gamificationEnabled ?? true
  const score = timeline?.affinity.score ?? 0
  const scoreWidth = `${Math.max(0, Math.min(100, score))}%`

  return (
    <SettingsSection
      title="关系时间线"
      description="纪念物、亲密度和勋章只记录共同工作的经历，不改变 Agent 能力。"
    >
      <SettingsCard>
        <div className="space-y-4 p-3 text-sm">
          <SettingsToggle
            label="关系奖励"
            description="开启后显示亲密度和勋章，关闭后仍保留经历与内心层。"
            checked={gamificationEnabled}
            disabled={!timeline || busyId === 'settings:gamification'}
            onCheckedChange={(checked) => void toggleGamification(checked)}
          />

          {gamificationEnabled ? (
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
          ) : (
            <div className="rounded-md border border-border/50 bg-muted/20 p-3 text-xs text-muted-foreground">
              关系奖励已关闭。经历卡、内心层和关系档案仍会保留，界面不显示分数和勋章。
            </div>
          )}

          <div className="grid gap-3 lg:grid-cols-2">
            <Panel title="关系档案" icon={<BookOpen size={14} className="text-muted-foreground" />}>
              <BondProfileList bond={timeline?.bond} />
            </Panel>

            <Panel title="纪念物" icon={<ScrollText size={14} className="text-muted-foreground" />}>
              <KeepsakeList
                keepsakes={timeline?.keepsakes ?? []}
                busyId={busyId}
                onUpdate={(id, status) => void updateKeepsake(id, status)}
              />
            </Panel>
          </div>

          <Panel title="内心层日志" icon={<BookOpen size={14} className="text-muted-foreground" />}>
            <JournalComposer
              observation={journalObservation}
              interpretation={journalInterpretation}
              busy={busyId === 'journal:create'}
              onObservationChange={setJournalObservation}
              onInterpretationChange={setJournalInterpretation}
              onCreate={() => void createJournal()}
            />
            <JournalList
              entries={timeline?.journalEntries ?? []}
              busyId={busyId}
              onPromote={(id, field) => void promoteJournal(id, field)}
              onDelete={(id) => void deleteJournal(id)}
            />
          </Panel>

          {gamificationEnabled && (
            <Panel title="勋章" icon={<Award size={14} className="text-muted-foreground" />}>
              <BadgeList
                badges={timeline?.badges ?? []}
                busyId={busyId}
                onHide={(badgeKey) => void hideBadge(badgeKey)}
              />
            </Panel>
          )}
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}

function Panel({
  title,
  icon,
  children,
}: {
  title: string
  icon: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <div className="rounded-md border border-border/50 p-3">
      <div className="flex items-center gap-2 text-xs font-medium text-foreground">
        {icon}
        {title}
      </div>
      {children}
    </div>
  )
}

function BondProfileList({ bond }: { bond?: BondProfile }) {
  const rows = [
    ['协作节奏', bond?.collaborationRhythm ?? []],
    ['挑战契约', bond?.challengeContract ?? []],
    ['支持风格', bond?.supportStyle ?? []],
    ['不喜欢的表达', bond?.communicationDislikes ?? []],
  ] as const

  return (
    <div className="mt-3 grid gap-2 sm:grid-cols-2">
      {rows.map(([label, values]) => (
        <div key={label} className="rounded bg-muted/20 p-2">
          <div className="text-[11px] font-medium text-muted-foreground">{label}</div>
          <div className="mt-1 space-y-1 text-xs leading-5 text-foreground">
            {values.length > 0 ? (
              values.map((value) => <div key={value}>{value}</div>)
            ) : (
              <div className="text-muted-foreground">等待共同经历沉淀。</div>
            )}
          </div>
        </div>
      ))}
    </div>
  )
}

function JournalComposer({
  observation,
  interpretation,
  busy,
  onObservationChange,
  onInterpretationChange,
  onCreate,
}: {
  observation: string
  interpretation: string
  busy: boolean
  onObservationChange: (value: string) => void
  onInterpretationChange: (value: string) => void
  onCreate: () => void
}) {
  return (
    <div className="mt-3 grid gap-2 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
      <Textarea
        className="min-h-20 resize-none text-xs"
        value={observation}
        onChange={(event) => onObservationChange(event.target.value)}
        placeholder="记录一次合作中的观察"
      />
      <Textarea
        className="min-h-20 resize-none text-xs"
        value={interpretation}
        onChange={(event) => onInterpretationChange(event.target.value)}
        placeholder="可选：它可能说明的关系偏好"
      />
      <Button
        size="sm"
        className="h-9 self-start px-2 text-xs"
        disabled={busy || observation.trim().length === 0}
        onClick={onCreate}
      >
        {busy ? <Loader2 className="mr-1 size-3 animate-spin" /> : <Plus className="mr-1 size-3" />}
        记录
      </Button>
    </div>
  )
}

function JournalList({
  entries,
  busyId,
  onPromote,
  onDelete,
}: {
  entries: PersonaJournalEntry[]
  busyId: string | null
  onPromote: (id: string, field: PersonaBondField) => void
  onDelete: (id: string) => void
}) {
  if (entries.length === 0) {
    return <div className="mt-3 text-xs text-muted-foreground">还没有内心层日志。</div>
  }

  return (
    <div className="mt-3 space-y-2">
      {entries.map((entry) => (
        <div key={entry.id} className="rounded-md border border-border/40 bg-muted/20 p-2.5">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <div className="text-xs font-medium leading-5 text-foreground">
                {entry.observation}
              </div>
              {entry.interpretation && (
                <div className="mt-1 text-xs leading-5 text-muted-foreground">
                  {entry.interpretation}
                </div>
              )}
            </div>
            <span className="shrink-0 rounded border border-border/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {confidenceLabel(entry.confidence)}
            </span>
          </div>
          <div className="mt-2 flex flex-wrap justify-end gap-1.5">
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              disabled={busyId === `${entry.id}:support_style`}
              onClick={() => onPromote(entry.id, 'support_style')}
            >
              {busyId === `${entry.id}:support_style` && (
                <Loader2 className="mr-1 size-3 animate-spin" />
              )}
              提升为支持风格
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              disabled={busyId === `${entry.id}:collaboration_rhythm`}
              onClick={() => onPromote(entry.id, 'collaboration_rhythm')}
            >
              {busyId === `${entry.id}:collaboration_rhythm` && (
                <Loader2 className="mr-1 size-3 animate-spin" />
              )}
              提升为协作节奏
            </Button>
            <Button
              aria-label="删除内心层日志"
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs text-muted-foreground"
              disabled={busyId === `${entry.id}:delete`}
              onClick={() => onDelete(entry.id)}
            >
              {busyId === `${entry.id}:delete` ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Trash2 className="size-3" />
              )}
            </Button>
          </div>
        </div>
      ))}
    </div>
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

function BadgeList({
  badges,
  busyId,
  onHide,
}: {
  badges: PersonaBadge[]
  busyId: string | null
  onHide: (badgeKey: string) => void
}) {
  const visibleBadges = badges.filter((badge) => !badge.hidden)

  if (visibleBadges.length === 0) {
    return <div className="mt-2 text-xs text-muted-foreground">还没有解锁的关系勋章。</div>
  }

  return (
    <div className="mt-3 grid gap-2 sm:grid-cols-2">
      {visibleBadges.map((badge) => (
        <div key={badge.badgeKey} className="rounded-md border border-border/40 bg-muted/20 p-2.5">
          <div className="flex items-start justify-between gap-2">
            <div>
              <div className="text-xs font-medium text-foreground">{badge.label}</div>
              <div className="mt-1 text-xs leading-5 text-muted-foreground">
                {badge.unlockReason}
              </div>
            </div>
            <Button
              aria-label="隐藏勋章"
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs text-muted-foreground"
              disabled={busyId === `badge:${badge.badgeKey}`}
              onClick={() => onHide(badge.badgeKey)}
            >
              {busyId === `badge:${badge.badgeKey}` ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <EyeOff className="size-3" />
              )}
            </Button>
          </div>
        </div>
      ))}
    </div>
  )
}

function confidenceLabel(confidence: PersonaJournalEntry['confidence']): string {
  switch (confidence) {
    case 'high':
      return '高置信'
    case 'low':
      return '低置信'
    case 'medium':
    default:
      return '中置信'
  }
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
