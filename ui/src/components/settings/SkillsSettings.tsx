/**
 * SkillsSettings — Settings → 已学技能 tab.
 *
 * Lists learned procedure skills extracted by the proactive `skill_extraction`
 * scenario. Lets users:
 *   - search / filter by title or context
 *   - toggle enabled / disabled
 *   - delete (with confirm)
 *   - expand a card to read context / principles / steps / pitfalls
 */

import * as React from 'react'
import Markdown from 'react-markdown'
import { toast } from 'sonner'
import { ChevronDown, ChevronRight, Trash2, Sparkles, RefreshCw, Search, Combine } from 'lucide-react'
import {
  listLearnedSkills,
  toggleLearnedSkill,
  deleteLearnedSkill,
  proposeSkillConsolidation,
  type SkillConsolidationProposal,
} from '@/lib/tauri-bridge'
import { SkillConsolidationDialog } from './SkillConsolidationDialog'
import type { LearnedSkill } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { Input } from '@/components/ui/input'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { cn } from '@/lib/utils'

function formatDate(s: string): string {
  if (!s) return ''
  const d = new Date(s)
  if (isNaN(d.getTime())) return s
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

interface SkillCardProps {
  skill: LearnedSkill
  expanded: boolean
  onToggleExpand: () => void
  onToggleEnabled: (next: boolean) => void
  onRequestDelete: () => void
}

function SkillCard({ skill, expanded, onToggleExpand, onToggleEnabled, onRequestDelete }: SkillCardProps): React.ReactElement {
  return (
    <div
      className={cn(
        'rounded-lg border bg-card transition-colors',
        skill.enabled ? 'border-border/60' : 'border-border/30 opacity-70',
      )}
    >
      {/* Header row */}
      <div className="flex items-center gap-2 px-3 py-2.5">
        <button
          type="button"
          onClick={onToggleExpand}
          className="flex-shrink-0 rounded p-0.5 text-muted-foreground/60 hover:text-foreground hover:bg-muted/50"
          aria-label={expanded ? '收起' : '展开'}
        >
          {expanded ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
        </button>
        <button
          type="button"
          onClick={onToggleExpand}
          className="flex-1 min-w-0 text-left"
        >
          <div className="text-[13px] font-semibold text-foreground truncate">
            {skill.name || '(未命名技能)'}
          </div>
          {!expanded && skill.context && (
            <div className="text-[11.5px] text-muted-foreground/80 truncate mt-0.5">
              {skill.context}
            </div>
          )}
        </button>
        <Switch
          checked={skill.enabled}
          onCheckedChange={onToggleEnabled}
          aria-label="启用"
        />
        <Button
          size="sm"
          variant="ghost"
          onClick={onRequestDelete}
          className="h-7 w-7 p-0"
          title="删除"
        >
          <Trash2 className="size-3.5 text-muted-foreground/70" />
        </Button>
      </div>

      {/* Expanded body */}
      {expanded && (
        <div className="border-t border-border/40 px-4 py-3 space-y-3 text-[12.5px] text-foreground/90">
          {skill.context && (
            <Section label="场景">
              <p className="leading-relaxed text-muted-foreground">{skill.context}</p>
            </Section>
          )}
          {skill.principles && (
            <Section label="原则">
              <MarkdownBlock text={skill.principles} />
            </Section>
          )}
          {skill.steps && (
            <Section label="步骤">
              <MarkdownBlock text={skill.steps} />
            </Section>
          )}
          {skill.pitfalls && (
            <Section label="陷阱">
              <MarkdownBlock text={skill.pitfalls} />
            </Section>
          )}
          <div className="pt-1 text-[11px] text-muted-foreground/60 tabular-nums">
            使用 {skill.usageCount} 次 · 创建于 {formatDate(skill.createdAt)}
          </div>
        </div>
      )}
    </div>
  )
}

function Section({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div>
      <div className="mb-1 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/70">
        {label}
      </div>
      {children}
    </div>
  )
}

function MarkdownBlock({ text }: { text: string }): React.ReactElement {
  return (
    <div className="prose prose-sm max-w-none text-[12.5px] text-foreground/90
                    prose-p:my-1 prose-ul:my-1 prose-ol:my-1 prose-li:my-0
                    prose-headings:text-foreground prose-strong:text-foreground
                    prose-code:text-foreground prose-code:bg-muted prose-code:px-1 prose-code:rounded">
      <Markdown>{text}</Markdown>
    </div>
  )
}

export function SkillsSettings(): React.ReactElement {
  const [skills, setSkills] = React.useState<LearnedSkill[]>([])
  const [loading, setLoading] = React.useState(true)
  const [query, setQuery] = React.useState('')
  const [expanded, setExpanded] = React.useState<Set<string>>(new Set())
  const [pendingDelete, setPendingDelete] = React.useState<LearnedSkill | null>(null)
  const [proposing, setProposing] = React.useState(false)
  const [proposal, setProposal] = React.useState<SkillConsolidationProposal | null>(null)
  const [consolidationOpen, setConsolidationOpen] = React.useState(false)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    try {
      const list = await listLearnedSkills()
      setSkills(list)
    } catch (err) {
      console.error('[SkillsSettings] load failed', err)
      toast.error('加载技能失败', { description: String(err) })
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void refetch()
  }, [refetch])

  const onToggleExpand = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const onToggleEnabled = async (skill: LearnedSkill, next: boolean) => {
    // Optimistic update
    setSkills((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: next } : s)))
    try {
      await toggleLearnedSkill(skill.id, next)
      toast.success(next ? `已启用「${skill.name}」` : `已禁用「${skill.name}」`)
    } catch (err) {
      console.error('[SkillsSettings] toggle failed', err)
      toast.error('切换状态失败', { description: String(err) })
      // Roll back
      setSkills((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: !next } : s)))
    }
  }

  const onConfirmDelete = async () => {
    if (!pendingDelete) return
    const target = pendingDelete
    setPendingDelete(null)
    // Optimistic remove
    const snapshot = skills
    setSkills((prev) => prev.filter((s) => s.id !== target.id))
    try {
      await deleteLearnedSkill(target.id)
      toast.success(`已删除「${target.name}」`)
    } catch (err) {
      console.error('[SkillsSettings] delete failed', err)
      toast.error('删除失败', { description: String(err) })
      setSkills(snapshot)
    }
  }

  const onPropose = async () => {
    setProposing(true)
    try {
      const result = await proposeSkillConsolidation()
      if (!result.clusters || result.clusters.length === 0) {
        toast.info('暂无可合并的重复技能')
        return
      }
      setProposal(result)
      setConsolidationOpen(true)
    } catch (err) {
      console.error('[SkillsSettings] propose consolidation failed', err)
      toast.error('无法分析技能整合方案', { description: String(err) })
    } finally {
      setProposing(false)
    }
  }

  const filtered = React.useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return skills
    return skills.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.context.toLowerCase().includes(q),
    )
  }, [skills, query])

  const total = skills.length
  const enabledCount = skills.filter((s) => s.enabled).length
  const usageSum = skills.reduce((acc, s) => acc + (s.usageCount || 0), 0)

  return (
    <div className="space-y-5 pb-8">
      {/* Intro */}
      <section>
        <div className="flex items-start gap-2.5">
          <div className="rounded-md bg-muted/50 p-1.5 text-muted-foreground/80 mt-0.5">
            <Sparkles className="size-4" />
          </div>
          <div className="flex-1">
            <h3 className="text-[13px] font-semibold text-foreground">已学技能</h3>
            <p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground/80">
              这些 procedure 节点由后台 <code className="px-1 rounded bg-muted/60 text-[11px]">skill_extraction</code> proactive 场景从你的对话中自动提炼而来。启用的技能会在 agent 遇到相似场景时被召回，作为参考流程注入上下文。可以在这里查看、临时禁用或永久删除。
            </p>
          </div>
        </div>
      </section>

      {/* Stats + search */}
      <section className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div className="text-[11.5px] text-muted-foreground/80 tabular-nums">
            共 <span className="text-foreground font-medium">{total}</span> 条 ·
            启用 <span className="text-foreground font-medium">{enabledCount}</span> 条 ·
            累计使用 <span className="text-foreground font-medium">{usageSum}</span> 次
          </div>
          <div className="flex items-center gap-1">
            {enabledCount >= 2 && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => void onPropose()}
                disabled={proposing || loading}
                className="h-7 px-2 text-[11.5px] gap-1.5"
                title="使用 LLM 分析并合并概念重复的技能"
              >
                <Combine className={cn('size-3.5', proposing && 'animate-pulse')} />
                {proposing ? '分析中…' : '整合现有技能'}
              </Button>
            )}
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void refetch()}
              disabled={loading}
              title="刷新"
              className="h-7 w-7 p-0"
            >
              <RefreshCw className={cn('size-3.5', loading && 'animate-spin')} />
            </Button>
          </div>
        </div>
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground/60 pointer-events-none" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索标题或场景…"
            className="pl-8 h-8 text-[12.5px]"
          />
        </div>
      </section>

      {/* List */}
      <section className="space-y-2">
        {loading && skills.length === 0 ? (
          <div className="rounded-lg border border-border/40 bg-muted/10 p-8 text-center text-[12px] text-muted-foreground/70">
            加载中…
          </div>
        ) : filtered.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/50 bg-muted/10 p-8 text-center">
            {skills.length === 0 ? (
              <>
                <Sparkles className="mx-auto mb-2 size-5 text-muted-foreground/50" />
                <div className="text-[12.5px] text-foreground/80">还没有提取到技能</div>
                <div className="mt-1 text-[11.5px] text-muted-foreground/70">
                  使用 agent 完成几个任务后会自动学到。
                </div>
              </>
            ) : (
              <div className="text-[12px] text-muted-foreground/70">
                没有匹配「{query}」的技能。
              </div>
            )}
          </div>
        ) : (
          filtered.map((skill) => (
            <SkillCard
              key={skill.id}
              skill={skill}
              expanded={expanded.has(skill.id)}
              onToggleExpand={() => onToggleExpand(skill.id)}
              onToggleEnabled={(next) => void onToggleEnabled(skill, next)}
              onRequestDelete={() => setPendingDelete(skill)}
            />
          ))
        )}
      </section>

      {/* Delete confirm */}
      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除技能？</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-2 text-sm text-muted-foreground">
                <p>
                  即将删除「{pendingDelete?.name ?? ''}」，连同它的版本、关键词和关联边都会被清除。
                </p>
                <p>这个操作无法撤销。</p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => void onConfirmDelete()}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Consolidation preview */}
      <SkillConsolidationDialog
        open={consolidationOpen}
        proposal={proposal}
        onOpenChange={(next) => {
          setConsolidationOpen(next)
          if (!next) setProposal(null)
        }}
        onApplied={() => {
          void refetch()
        }}
      />
    </div>
  )
}
