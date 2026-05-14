/**
 * SkillsList — 技能模块左栏:搜索 + 两个可折叠分组(学得 / 内置)。
 *
 * 纯展示组件:数据、选中态、维护操作回调都由 SkillsModule 传入。
 * 分组折叠状态是本地 useState,不持久化。
 */
import * as React from 'react'
import { Search, RefreshCw, Combine, KeyRound, ChevronDown, ChevronRight } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { UnifiedSkill } from './SkillsModule'

interface GroupProps {
  label: string
  count: number
  open: boolean
  onToggle: () => void
  children: React.ReactNode
}

function Group({ label, count, open, onToggle, children }: GroupProps): React.ReactElement {
  return (
    <div>
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center gap-1 px-1.5 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground hover:text-foreground"
      >
        {open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        {label} · {count}
      </button>
      {open && <div className="space-y-0.5">{children}</div>}
    </div>
  )
}

function Row({
  skill,
  selected,
  onSelect,
}: {
  skill: UnifiedSkill
  selected: boolean
  onSelect: () => void
}): React.ReactElement {
  const secondary =
    skill.kind === 'learned'
      ? skill.raw.context.split('\n')[0].slice(0, 120)
      : skill.raw.category || skill.raw.description
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        'w-full rounded-md border px-2.5 py-1.5 text-left transition-colors',
        selected
          ? 'border-accent/35 bg-accent/15'
          : 'border-transparent hover:bg-muted/40',
        !skill.enabled && 'opacity-60',
      )}
    >
      <div className="text-[12px] font-medium text-foreground truncate">{skill.name}</div>
      {secondary && (
        <div className="mt-0.5 text-[10px] text-muted-foreground truncate">{secondary}</div>
      )}
    </button>
  )
}

export interface SkillsListProps {
  learned: UnifiedSkill[]
  builtin: UnifiedSkill[]
  selectedId: string | null
  query: string
  loading: boolean
  canPropose: boolean
  proposing: boolean
  backfilling: boolean
  onSelect: (id: string) => void
  onQueryChange: (q: string) => void
  onReload: () => void
  onPropose: () => void
  onBackfill: () => void
}

export function SkillsList({
  learned,
  builtin,
  selectedId,
  query,
  loading,
  canPropose,
  proposing,
  backfilling,
  onSelect,
  onQueryChange,
  onReload,
  onPropose,
  onBackfill,
}: SkillsListProps): React.ReactElement {
  const [learnedOpen, setLearnedOpen] = React.useState(true)
  const [builtinOpen, setBuiltinOpen] = React.useState(true)

  return (
    <div className="flex w-64 shrink-0 flex-col border-r border-border bg-background">
      {/* header:搜索 + 自定义 */}
      <div className="border-b border-border/60 p-3 space-y-2">
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground pointer-events-none" />
          <Input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder="搜索技能…"
            className="h-8 pl-8 text-[12px]"
          />
        </div>
        <Button
          size="sm"
          variant="outline"
          disabled
          title="未来扩展"
          className="h-7 w-full text-[11.5px]"
        >
          + 自定义技能
        </Button>
      </div>

      {/* 列表 */}
      <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-2">
        <Group label="学得" count={learned.length} open={learnedOpen} onToggle={() => setLearnedOpen((v) => !v)}>
          <div className="flex gap-1 px-1 pb-1">
            <Button
              size="sm"
              variant="ghost"
              onClick={onBackfill}
              disabled={backfilling || loading || learned.length === 0}
              title="为缺关键词索引的旧技能补全索引"
              className="h-6 px-1.5 text-[10.5px] gap-1"
            >
              <KeyRound className={cn('size-3', backfilling && 'animate-pulse')} />
              回填关键词
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={onPropose}
              disabled={proposing || loading || !canPropose}
              title="用 LLM 分析并合并概念重复的技能"
              className="h-6 px-1.5 text-[10.5px] gap-1"
            >
              <Combine className={cn('size-3', proposing && 'animate-pulse')} />
              整合技能
            </Button>
          </div>
          {learned.map((s) => (
            <Row key={s.id} skill={s} selected={s.id === selectedId} onSelect={() => onSelect(s.id)} />
          ))}
        </Group>

        <Group label="内置" count={builtin.length} open={builtinOpen} onToggle={() => setBuiltinOpen((v) => !v)}>
          <div className="flex px-1 pb-1">
            <Button
              size="sm"
              variant="ghost"
              onClick={onReload}
              disabled={loading}
              title="重新加载内置技能"
              className="h-6 px-1.5 text-[10.5px] gap-1"
            >
              <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
              重新加载
            </Button>
          </div>
          {builtin.map((s) => (
            <Row key={s.id} skill={s} selected={s.id === selectedId} onSelect={() => onSelect(s.id)} />
          ))}
        </Group>
      </div>
    </div>
  )
}
