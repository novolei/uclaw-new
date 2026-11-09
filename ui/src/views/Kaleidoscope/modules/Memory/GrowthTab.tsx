/**
 * GrowthTab — 万花筒 → 记忆 → 成长：用户模型（含演化历史）和反思列表。
 * 反思可归档 / 恢复；「立即刷新」手动跑一次反思 + 用户模型更新。
 */
import * as React from 'react'
import { ChevronDown, ChevronRight, Loader2, RefreshCw } from 'lucide-react'
import {
  getGrowthUserModel,
  listGrowthReflections,
  listGrowthUserModelHistory,
  refreshGrowthNow,
  setGrowthReflectionArchived,
} from '@/lib/tauri-bridge'
import type { GrowthOutcome, GrowthReflection, GrowthUserModel, GrowthUserModelHistoryRow } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const SKIP_REASON: Record<string, string> = {
  disabled: '记忆成长已关闭（设置 → 记忆）',
  no_llm: '未开启学习模型，无法运行',
  budget: '今日 token 预算已用完',
  not_due: '新对话还不够',
  busy: '已有一次正在运行',
  backoff: '上次运行失败，约一小时后自动重试',
  error: '运行出错，请查看日志',
}

export function describeOutcome(o: GrowthOutcome): string {
  if (o.status === 'skipped') return `跳过：${SKIP_REASON[o.reason] ?? o.reason}`
  const done = [o.reflection && '新反思', o.promoted && '用户模型', o.consolidated && '整合'].filter(Boolean)
  return done.length ? `已更新：${done.join('、')}` : '没有新内容可更新'
}

function formatTime(ms: number): string {
  return ms > 0 ? new Date(ms).toLocaleString() : ''
}

export function GrowthTab(): React.ReactElement {
  const [model, setModel] = React.useState<GrowthUserModel | null>(null)
  const [history, setHistory] = React.useState<GrowthUserModelHistoryRow[]>([])
  const [reflections, setReflections] = React.useState<GrowthReflection[]>([])
  const [showArchived, setShowArchived] = React.useState(false)
  const [historyOpen, setHistoryOpen] = React.useState(false)
  const [loading, setLoading] = React.useState(true)
  const [refreshing, setRefreshing] = React.useState(false)
  const [message, setMessage] = React.useState<string | null>(null)

  const load = React.useCallback(async () => {
    try {
      const [m, h, r] = await Promise.all([
        getGrowthUserModel(),
        listGrowthUserModelHistory(10),
        listGrowthReflections(50, showArchived),
      ])
      setModel(m)
      setHistory(h)
      setReflections(r)
    } catch (e) {
      setMessage(`读取失败：${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [showArchived])

  React.useEffect(() => {
    void load()
  }, [load])

  const toggleArchived = async (r: GrowthReflection) => {
    try {
      await setGrowthReflectionArchived(r.id, r.archivedAt === null)
      await load()
    } catch (e) {
      setMessage(String(e))
    }
  }

  const refresh = async () => {
    setRefreshing(true)
    setMessage(null)
    try {
      const outcome = await refreshGrowthNow()
      setMessage(describeOutcome(outcome))
      await load()
    } catch (e) {
      setMessage(String(e))
    } finally {
      setRefreshing(false)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center gap-2 text-xs text-muted-foreground p-4">
        <Loader2 className="size-3.5 animate-spin" /> 加载中…
      </div>
    )
  }

  return (
    <div className="h-full overflow-y-auto p-4 space-y-6">
      <div className="flex items-center gap-3">
        <Button size="sm" variant="outline" disabled={refreshing} onClick={() => void refresh()} className="gap-1.5">
          {refreshing ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
          立即刷新
        </Button>
        {message && <span className="text-xs text-muted-foreground">{message}</span>}
      </div>

      <section className="space-y-2">
        <h3 className="text-sm font-medium">用户模型</h3>
        {model ? (
          <div className="rounded-lg border border-border bg-card/40 p-3">
            <p className="text-sm leading-relaxed whitespace-pre-wrap">{model.summary}</p>
            <p className="text-[10px] text-muted-foreground mt-2">更新于 {formatTime(model.updatedAt)}</p>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">还没有用户模型。学习模型积累了一些关于你的事实后，会在下一次反思时生成。</p>
        )}
        {history.length > 0 && (
          <div>
            <button
              type="button"
              onClick={() => setHistoryOpen((v) => !v)}
              className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
            >
              {historyOpen ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
              演化历史（{history.length}）
            </button>
            {historyOpen && (
              <ul className="mt-2 space-y-2">
                {history.map((h, i) => (
                  <li key={`${h.replacedAt}-${i}`} className="rounded border border-border/60 p-2">
                    <p className="text-xs text-muted-foreground whitespace-pre-wrap">{h.summary}</p>
                    <p className="text-[10px] text-muted-foreground/70 mt-1">替换于 {formatTime(h.replacedAt)}</p>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </section>

      <section className="space-y-2">
        <div className="flex items-center gap-3">
          <h3 className="text-sm font-medium">反思</h3>
          <label className="inline-flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer">
            <input type="checkbox" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} />
            显示已归档
          </label>
        </div>
        {reflections.length === 0 ? (
          <p className="text-xs text-muted-foreground">还没有反思。每积累若干轮 Agent 对话，会自动生成一条。</p>
        ) : (
          <ul className="space-y-1.5">
            {reflections.map((r) => (
              <li
                key={r.id}
                className={cn(
                  'flex items-start gap-3 rounded-lg border border-border px-3 py-2',
                  r.archivedAt !== null && 'opacity-60',
                )}
              >
                <span className="mt-0.5 shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-mono tabular-nums">
                  {r.confidence.toFixed(2)}
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-sm">{r.insight}</p>
                  <p className="text-[10px] text-muted-foreground mt-0.5">{formatTime(r.createdAt)}</p>
                </div>
                <button
                  type="button"
                  onClick={() => void toggleArchived(r)}
                  className="shrink-0 text-xs px-2 py-1 rounded bg-muted text-muted-foreground hover:bg-muted/70"
                >
                  {r.archivedAt === null ? '归档' : '恢复'}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}
