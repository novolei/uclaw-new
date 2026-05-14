/**
 * SkillsModule — 万花筒「技能」模块。
 *
 * 左可折叠分组列表(SkillsList)+ 右详情(SkillDetail)双栏。数据来自两个
 * 来源 —— 学得技能(listLearnedSkills)+ 内置技能(listSkills)—— 在此 merge
 * 成 UnifiedSkill 列表。维护操作(回填关键词 / 整合技能 / 重新加载)与删除确认
 * 也由本容器持有。整体迁自原 components/settings/SkillsSettings.tsx。
 */
import * as React from 'react'
import { toast } from 'sonner'
import {
  listLearnedSkills,
  toggleLearnedSkill,
  deleteLearnedSkill,
  proposeSkillConsolidation,
  backfillSkillKeywords,
  listSkills,
  toggleSkill,
  forkSkillToUser,
  reloadSkills,
  type SkillConsolidationProposal,
} from '@/lib/tauri-bridge'
import type { LearnedSkill, SkillInfo } from '@/lib/types'
import { ModuleHeader } from '../../shared/ModuleHeader'
import { SkillsList } from './SkillsList'
import { SkillDetail } from './SkillDetail'
import { SkillConsolidationDialog } from '@/components/settings/SkillConsolidationDialog'
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

export type UnifiedSkill =
  | { kind: 'learned'; id: string; name: string; enabled: boolean; raw: LearnedSkill }
  | { kind: 'builtin'; id: string; name: string; enabled: boolean; raw: SkillInfo }

export function SkillsModule(): React.ReactElement {
  const [learnedRaw, setLearnedRaw] = React.useState<LearnedSkill[]>([])
  const [builtinRaw, setBuiltinRaw] = React.useState<SkillInfo[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState('')
  const [pendingDelete, setPendingDelete] = React.useState<UnifiedSkill | null>(null)
  const [forkingName, setForkingName] = React.useState<string | null>(null)
  const [proposing, setProposing] = React.useState(false)
  const [backfilling, setBackfilling] = React.useState(false)
  const [proposal, setProposal] = React.useState<SkillConsolidationProposal | null>(null)
  const [consolidationOpen, setConsolidationOpen] = React.useState(false)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    const [l, b] = await Promise.allSettled([listLearnedSkills(), listSkills()])
    if (l.status === 'fulfilled') setLearnedRaw(l.value)
    else toast.error('加载学得技能失败', { description: String(l.reason) })
    if (b.status === 'fulfilled') setBuiltinRaw(b.value)
    else toast.error('加载内置技能失败', { description: String(b.reason) })
    setLoading(false)
  }, [])

  React.useEffect(() => {
    void refetch()
  }, [refetch])

  const learned: UnifiedSkill[] = React.useMemo(
    () => learnedRaw.map((s) => ({ kind: 'learned', id: s.id, name: s.name, enabled: s.enabled, raw: s })),
    [learnedRaw],
  )
  const builtin: UnifiedSkill[] = React.useMemo(
    () => builtinRaw.map((s) => ({ kind: 'builtin', id: s.name, name: s.name, enabled: s.enabled, raw: s })),
    [builtinRaw],
  )

  const filterFn = React.useCallback(
    (s: UnifiedSkill) => {
      const q = query.trim().toLowerCase()
      return !q || s.name.toLowerCase().includes(q)
    },
    [query],
  )
  const learnedFiltered = learned.filter(filterFn)
  const builtinFiltered = builtin.filter(filterFn)

  const selected =
    [...learned, ...builtin].find((s) => s.id === selectedId) ?? null

  const onToggleEnabled = async (skill: UnifiedSkill, next: boolean) => {
    if (skill.kind === 'learned') {
      setLearnedRaw((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: next } : s)))
      try {
        await toggleLearnedSkill(skill.id, next)
      } catch (err) {
        toast.error('切换状态失败', { description: String(err) })
        setLearnedRaw((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: !next } : s)))
      }
    } else {
      setBuiltinRaw((prev) => prev.map((s) => (s.name === skill.id ? { ...s, enabled: next } : s)))
      try {
        await toggleSkill({ name: skill.name, enabled: next })
      } catch (err) {
        toast.error('切换状态失败', { description: String(err) })
        setBuiltinRaw((prev) => prev.map((s) => (s.name === skill.id ? { ...s, enabled: !next } : s)))
      }
    }
  }

  const onConfirmDelete = async () => {
    if (!pendingDelete || pendingDelete.kind !== 'learned') return
    const target = pendingDelete
    setPendingDelete(null)
    setLearnedRaw((prev) => prev.filter((s) => s.id !== target.id))
    if (selectedId === target.id) setSelectedId(null)
    try {
      await deleteLearnedSkill(target.id)
      toast.success(`已删除「${target.name}」`)
    } catch (err) {
      toast.error('删除失败', { description: String(err) })
      // 失败时从后端重新拉取 —— 比恢复 stale snapshot 安全(不会丢并发的 toggle 改动)。
      void refetch()
    }
  }

  const onFork = async (name: string) => {
    setForkingName(name)
    try {
      const destPath = await forkSkillToUser(name)
      toast.success(`已 Fork 到 ${destPath}`, {
        description: '现在可以在 ~/.uclaw/skills/ 下编辑这份 skill。',
      })
      const fresh = await reloadSkills()
      setBuiltinRaw(fresh)
    } catch (err) {
      toast.error('Fork 失败', { description: String(err) })
    } finally {
      setForkingName(null)
    }
  }

  const onReload = async () => {
    setLoading(true)
    try {
      const fresh = await reloadSkills()
      setBuiltinRaw(fresh)
    } catch (err) {
      toast.error('重新加载失败', { description: String(err) })
    } finally {
      setLoading(false)
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
      toast.error('无法分析技能整合方案', { description: String(err) })
    } finally {
      setProposing(false)
    }
  }

  const onBackfill = async () => {
    setBackfilling(true)
    try {
      const result = await backfillSkillKeywords()
      if (result.backfilledSkills === 0) {
        toast.info('关键词索引已完整', {
          description: `${result.totalLearnedSkills} 条技能全部已索引`,
        })
      } else {
        toast.success('关键词回填完成', {
          description: `${result.backfilledSkills}/${result.totalLearnedSkills} 条新增 · 共 ${result.keywordsInserted} 个关键词`,
        })
      }
    } catch (err) {
      toast.error('回填关键词失败', { description: String(err) })
    } finally {
      setBackfilling(false)
    }
  }

  const isEmpty = !loading && learned.length === 0 && builtin.length === 0
  const enabledLearnedCount = learned.filter((s) => s.enabled).length

  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader
        group="capability"
        title="技能"
        subtitle={`学得 ${learned.length} · 内置 ${builtin.length}`}
      />
      {/* titlebar-no-drag on the body branches: ModuleHeader above stays
          window-drag surface (no actions → fully draggable); the body holds
          SkillsList (search input, group toggles, skill rows) + SkillDetail
          (fork/delete buttons, enable switch, scrollable detail), all of
          which must stay interactive. See KaleidoscopeShell. */}
      {isEmpty ? (
        <div className="titlebar-no-drag flex-1 min-h-0 flex items-center justify-center">
          <div className="rounded-lg border border-dashed border-border bg-muted/10 px-8 py-10 text-center">
            <div className="text-[13px] text-foreground/80">你的 Agent 还没学到技能</div>
            <div className="mt-1 text-[11.5px] text-muted-foreground">
              让它处理几次任务就会积累。
            </div>
          </div>
        </div>
      ) : (
        <div className="titlebar-no-drag flex flex-1 min-h-0">
          <SkillsList
            learned={learnedFiltered}
            builtin={builtinFiltered}
            selectedId={selectedId}
            query={query}
            loading={loading}
            canPropose={enabledLearnedCount >= 2}
            proposing={proposing}
            backfilling={backfilling}
            onSelect={setSelectedId}
            onQueryChange={setQuery}
            onReload={() => void onReload()}
            onPropose={() => void onPropose()}
            onBackfill={() => void onBackfill()}
          />
          <SkillDetail
            skill={selected}
            forking={selected?.kind === 'builtin' && forkingName === selected.name}
            onToggleEnabled={(s, next) => void onToggleEnabled(s, next)}
            onRequestDelete={(s) => setPendingDelete(s)}
            onFork={(name) => void onFork(name)}
          />
        </div>
      )}

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
                <p>即将删除「{pendingDelete?.name ?? ''}」，连同它的版本、关键词和关联边都会被清除。</p>
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
