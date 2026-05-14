/**
 * SkillDetail — 技能模块右栏:选中技能的详情。
 *
 * 学得技能:场景 / 原则 / 步骤 / 陷阱 + 可展开的演化历史(SkillEvolutionTab)。
 * 内置技能:描述 / 版本 / 作者 / 分类 / provenance 徽章 + Fork(仅 bundled)。
 * 顶部右侧「Agent 可调用」开关。渲染逻辑迁自原 SkillsSettings 的 SkillCard 展开体。
 */
import * as React from 'react'
import Markdown from 'react-markdown'
import { History } from 'lucide-react'
import { Switch } from '@/components/ui/switch'
import { Button } from '@/components/ui/button'
import { SkillEvolutionTab } from '@/components/settings/SkillEvolutionTab'
import { cn } from '@/lib/utils'
import type { UnifiedSkill } from './SkillsModule'

function formatDate(s: string): string {
  if (!s) return ''
  const d = new Date(s)
  if (isNaN(d.getTime())) return s
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
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

export interface SkillDetailProps {
  skill: UnifiedSkill | null
  forking: boolean
  onToggleEnabled: (skill: UnifiedSkill, next: boolean) => void
  onRequestDelete: (skill: UnifiedSkill) => void
  onFork: (name: string) => void
}

export function SkillDetail({
  skill,
  forking,
  onToggleEnabled,
  onRequestDelete,
  onFork,
}: SkillDetailProps): React.ReactElement {
  const [showTimeline, setShowTimeline] = React.useState(false)

  // 切换选中技能时收起演化历史。
  React.useEffect(() => {
    setShowTimeline(false)
  }, [skill?.id])

  if (!skill) {
    return (
      <div className="flex-1 min-w-0 flex items-center justify-center bg-content-area">
        <div className="text-[13px] text-muted-foreground">选择左侧一个技能查看详情</div>
      </div>
    )
  }

  return (
    <div className="flex-1 min-w-0 overflow-y-auto px-7 py-6 bg-content-area">
      {/* 头部:名称 + 类型徽章 + 启用开关 */}
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-[18px] font-semibold text-foreground truncate">{skill.name}</span>
            <span className="rounded-full bg-accent/15 border border-accent/35 px-2 py-0.5 text-[10px] text-accent-foreground">
              {skill.kind === 'learned' ? '学得' : '内置'}
            </span>
          </div>
          {skill.kind === 'learned' ? (
            <div className="mt-1 text-[11px] text-muted-foreground tabular-nums">
              使用 {skill.raw.usageCount} 次 · 创建于 {formatDate(skill.raw.createdAt)}
            </div>
          ) : (
            <div className="mt-1 text-[11px] text-muted-foreground">
              v{skill.raw.version} · {skill.raw.author} · {skill.raw.category || '未分类'}
            </div>
          )}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {skill.kind === 'builtin' && skill.raw.provenance === 'bundled' && (
            <Button
              size="sm"
              variant="outline"
              disabled={forking}
              onClick={() => onFork(skill.name)}
              className="h-7 px-2 text-[11.5px]"
            >
              {forking ? 'Fork 中…' : 'Fork 到我的'}
            </Button>
          )}
          <span className="text-[11px] text-muted-foreground">Agent 可调用</span>
          <Switch
            checked={skill.enabled}
            onCheckedChange={(next) => onToggleEnabled(skill, next)}
            aria-label="Agent 可调用"
          />
        </div>
      </div>

      {/* 主体 */}
      {skill.kind === 'builtin' ? (
        <div className="mt-5 space-y-3 text-[12.5px] text-foreground/90">
          {skill.raw.description && (
            <Section label="描述">
              <p className="leading-relaxed text-muted-foreground">{skill.raw.description}</p>
            </Section>
          )}
        </div>
      ) : (
        <div className="mt-5 space-y-3 text-[12.5px] text-foreground/90">
          <div className="flex items-center justify-between">
            <button
              type="button"
              onClick={() => setShowTimeline((v) => !v)}
              className={cn(
                'flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-[11.5px] transition-colors',
                showTimeline
                  ? 'border-border bg-muted/60 text-foreground'
                  : 'border-border/40 text-muted-foreground hover:border-border hover:text-foreground',
              )}
            >
              <History className="size-3.5" />
              演化历史
            </button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => onRequestDelete(skill)}
              className="h-7 px-2 text-[11.5px] text-destructive hover:text-destructive"
            >
              删除
            </Button>
          </div>
          {showTimeline ? (
            <SkillEvolutionTab skillId={skill.raw.id} />
          ) : (
            <>
              {skill.raw.context && (
                <Section label="场景">
                  <p className="leading-relaxed text-muted-foreground">{skill.raw.context}</p>
                </Section>
              )}
              {skill.raw.principles && (
                <Section label="原则">
                  <MarkdownBlock text={skill.raw.principles} />
                </Section>
              )}
              {skill.raw.steps && (
                <Section label="步骤">
                  <MarkdownBlock text={skill.raw.steps} />
                </Section>
              )}
              {skill.raw.pitfalls && (
                <Section label="陷阱">
                  <MarkdownBlock text={skill.raw.pitfalls} />
                </Section>
              )}
            </>
          )}
        </div>
      )}
    </div>
  )
}
