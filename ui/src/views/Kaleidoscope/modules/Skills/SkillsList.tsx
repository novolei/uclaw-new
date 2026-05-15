/**
 * SkillsList — 技能模块左栏:四个可折叠分组。
 *
 * 分组顺序:
 *  1. 最近使用 — top-5 most-used learned skills
 *  2. 自定义技能 — builtin skills with provenance='user'
 *  3. 学得 — learned skills from memory_graph
 *  4. 内建技能 — builtin skills with provenance='bundled'|'project'|'marketplace'
 *
 * 搜索框和 lifecycle 筛选已提升到 SkillsModule 的 header 区域。
 * 纯展示组件:数据、选中态、维护操作回调都由 SkillsModule 传入。
 * 分组折叠状态是本地 useState,不持久化。
 */
import * as React from 'react'
import { RefreshCw, Combine, KeyRound, ChevronDown, ChevronRight, Clock } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { UnifiedSkill } from './SkillsModule'

const LIFECYCLE_DOT: Record<string, string> = {
  draft:      'bg-yellow-500',
  promoted:   'bg-emerald-500',
  deprecated: 'bg-muted-foreground/50',
}

interface GroupProps {
  label: string
  count: number
  open: boolean
  onToggle: () => void
  icon?: React.ReactNode
  actions?: React.ReactNode
  children: React.ReactNode
}

function Group({ label, count, open, onToggle, icon, actions, children }: GroupProps): React.ReactElement {
  return (
    <div className="mb-1.5">
      <div
        className={cn(
          'flex w-full items-center gap-1.5 px-3 py-2 rounded-lg transition-all duration-200',
          open && 'bg-muted/30',
        )}
      >
        <button
          type="button"
          onClick={onToggle}
          className={cn(
            'flex items-center gap-2 flex-1 min-w-0 text-left',
            'text-[11px] font-semibold uppercase tracking-wider text-muted-foreground hover:text-foreground transition-colors duration-200',
          )}
        >
          <span className="transition-transform duration-200 shrink-0">
            {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          </span>
          {icon && <span className="text-muted-foreground/70 shrink-0">{icon}</span>}
          <span className="truncate">{label}</span>
        </button>
        {actions && <div className="flex items-center gap-1 shrink-0">{actions}</div>}
        <span className={cn(
          'text-[10px] font-medium tabular-nums rounded-full px-2 py-0.5 min-w-[1.25rem] text-center transition-colors duration-200 shrink-0',
          open ? 'bg-primary/15 text-primary' : 'bg-muted/70 text-muted-foreground/80',
        )}>
          {count}
        </span>
      </div>
      {open && <div className="mt-1 space-y-0.5 px-1 animate-in fade-in slide-in-from-top-1 duration-150">{children}</div>}
    </div>
  )
}

function Row({
  skill,
  selected,
  onSelect,
  compact,
}: {
  skill: UnifiedSkill
  selected: boolean
  onSelect: () => void
  compact?: boolean
}): React.ReactElement {
  const secondary =
    skill.kind === 'learned'
      ? skill.raw.context.split('\n')[0].slice(0, 120)
      : skill.raw.category || skill.raw.description
  const lifecycle = skill.kind === 'learned' ? (skill.raw.lifecycle || 'promoted') : null
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        'w-full text-left transition-all duration-200',
        'active:scale-[0.99]',
        compact
          ? cn(
              'rounded-lg border px-3 py-2',
              selected
                ? 'border-primary/30 bg-primary/8 shadow-[0_1px_4px_rgba(0,0,0,0.03)] ring-1 ring-primary/15'
                : 'border-transparent hover:bg-muted/30 hover:border-border/30',
              !skill.enabled && 'opacity-50',
            )
          : cn(
              'rounded-xl border px-3.5 py-3',
              selected
                ? 'border-primary/30 bg-primary/8 shadow-[0_2px_8px_rgba(0,0,0,0.04)] ring-1 ring-primary/15'
                : 'border-transparent hover:bg-muted/40 hover:border-border/40 hover:shadow-[0_1px_4px_rgba(0,0,0,0.03)]',
              !skill.enabled && 'opacity-50',
            ),
      )}
    >
      <div className="flex items-center gap-2.5">
        {lifecycle && (
          <span className={cn(
            'size-2.5 shrink-0 rounded-full ring-1 ring-offset-1 ring-offset-background transition-transform duration-200',
            LIFECYCLE_DOT[lifecycle] || 'bg-muted-foreground/50',
            selected && 'scale-110',
          )} />
        )}
        <span className={cn(
          'font-medium truncate transition-colors duration-200',
          compact ? 'text-[12px]' : 'text-[12.5px]',
          selected ? 'text-foreground' : 'text-foreground/80',
        )}>
          {skill.name}
        </span>
      </div>
      {!compact && secondary && (
        <div className={cn(
          'mt-1 pl-5 text-[10.5px] truncate leading-relaxed transition-colors duration-200',
          selected ? 'text-muted-foreground' : 'text-muted-foreground/70',
        )}>
          {secondary}
        </div>
      )}
    </button>
  )
}

export interface SkillsListProps {
  learned: UnifiedSkill[]
  userSkills: UnifiedSkill[]
  bundledSkills: UnifiedSkill[]
  selectedId: string | null
  loading: boolean
  canPropose: boolean
  proposing: boolean
  backfilling: boolean
  compact: boolean
  onSelect: (id: string) => void
  onReload: () => void
  onPropose: () => void
  onBackfill: () => void
}

export function SkillsList({
  learned,
  userSkills,
  bundledSkills,
  selectedId,
  loading,
  canPropose,
  proposing,
  backfilling,
  compact,
  onSelect,
  onReload,
  onPropose,
  onBackfill,
}: SkillsListProps): React.ReactElement {
  const [learnedOpen, setLearnedOpen] = React.useState(true)
  const [userOpen, setUserOpen] = React.useState(true)
  const [bundledOpen, setBundledOpen] = React.useState(true)
  const [recentOpen, setRecentOpen] = React.useState(true)

  // Top-5 most-used learned skills
  const recentlyUsed = React.useMemo(() => {
    return [...learned]
      .filter((s) => s.kind === 'learned' && s.raw.usageCount > 0)
      .sort((a, b) => {
        const aCount = a.kind === 'learned' ? a.raw.usageCount : 0
        const bCount = b.kind === 'learned' ? b.raw.usageCount : 0
        return bCount - aCount
      })
      .slice(0, 5)
  }, [learned])

  return (
    <div className="flex h-full w-full flex-col bg-background overflow-hidden">
      {/* 列表 */}
      <div className="flex-1 min-h-0 overflow-y-auto p-3.5 space-y-0.5">
        {/* ─── 最近使用 ─── */}
        {recentlyUsed.length > 0 && (
          <Group
            label="最近使用"
            count={recentlyUsed.length}
            open={recentOpen}
            onToggle={() => setRecentOpen((v) => !v)}
            icon={<Clock className="size-3.5" />}
          >
            <div className="flex flex-wrap gap-1.5 px-2 pb-2.5">
              {recentlyUsed.map((s) => (
                <button
                  key={`recent-${s.id}`}
                  type="button"
                  onClick={() => onSelect(s.id)}
                  className={cn(
                    'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1',
                    'text-[10.5px] font-medium transition-all duration-200',
                    'active:scale-95',
                    selectedId === s.id
                      ? 'border-primary/30 bg-primary/10 text-primary shadow-[0_1px_3px_rgba(0,0,0,0.04)]'
                      : 'border-border/30 text-muted-foreground hover:border-border/60 hover:text-foreground hover:bg-muted/40',
                  )}
                >
                  <span className="truncate max-w-[120px]">{s.name}</span>
                  <span className={cn(
                    'text-[9px] tabular-nums rounded-full px-1.5 py-px min-w-[1.25rem] text-center',
                    selectedId === s.id ? 'bg-primary/20 text-primary' : 'bg-muted/60 text-muted-foreground/70',
                  )}>
                    {s.kind === 'learned' ? s.raw.usageCount : 0}×
                  </span>
                </button>
              ))}
            </div>
          </Group>
        )}

        {/* ─── 自定义技能 ─── */}
        {userSkills.length > 0 && (
          <Group
            label="自定义技能"
            count={userSkills.length}
            open={userOpen}
            onToggle={() => setUserOpen((v) => !v)}
          >
            {userSkills.map((s) => (
              <Row key={s.id} skill={s} selected={s.id === selectedId} onSelect={() => onSelect(s.id)} compact={compact} />
            ))}
          </Group>
        )}

        {/* ─── 学得 ─── */}
        <Group
          label="学得"
          count={learned.length}
          open={learnedOpen}
          onToggle={() => setLearnedOpen((v) => !v)}
          actions={
            <>
              <Button
                size="sm"
                variant="ghost"
                onClick={onBackfill}
                disabled={backfilling || loading || learned.length === 0}
                title="回填关键词 — 为缺索引的旧技能补全关键词索引"
                className="h-6 w-6 p-0 text-muted-foreground/50 hover:text-muted-foreground rounded-md"
              >
                <KeyRound className={cn('size-3', backfilling && 'animate-pulse')} />
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={onPropose}
                disabled={proposing || loading || !canPropose}
                title="整合技能 — 用 LLM 分析并合并概念重复的技能"
                className="h-6 w-6 p-0 text-muted-foreground/50 hover:text-muted-foreground rounded-md"
              >
                <Combine className={cn('size-3', proposing && 'animate-pulse')} />
              </Button>
            </>
          }
        >
          {learned.map((s) => (
            <Row key={s.id} skill={s} selected={s.id === selectedId} onSelect={() => onSelect(s.id)} compact={compact} />
          ))}
        </Group>

        {/* ─── 内建技能 ─── */}
        <Group
          label="内建技能"
          count={bundledSkills.length}
          open={bundledOpen}
          onToggle={() => setBundledOpen((v) => !v)}
          actions={
            <Button
              size="sm"
              variant="ghost"
              onClick={onReload}
              disabled={loading}
              title="重新加载内置技能"
              className="h-6 w-6 p-0 text-muted-foreground/50 hover:text-muted-foreground rounded-md"
            >
              <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
            </Button>
          }
        >
          {bundledSkills.map((s) => (
            <Row key={s.id} skill={s} selected={s.id === selectedId} onSelect={() => onSelect(s.id)} compact={compact} />
          ))}
        </Group>
      </div>
    </div>
  )
}
