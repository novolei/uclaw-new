/**
 * UsageSettings — Settings → 用量与预算 tab.
 *
 * Sections (top to bottom):
 *   - BudgetHeader: month-to-date spend + progress bar vs. configured budget
 *   - WorkspaceRollupSection: per-workspace spend for the current month
 *   - KPI cards (last 30 days)
 *   - Daily total bar chart (last 30 days)
 *   - Per-model donut (last 30 days)
 *   - Per-session table (most recent first)
 *
 * Live update: subscribes to `agent:turn_cost` events; on each event
 * we re-fetch the daily rollup AND the monthly totals (debounced 1s).
 */

import * as React from 'react'
import {
  ResponsiveContainer,
  BarChart, Bar,
  PieChart, Pie, Cell,
  XAxis, YAxis, Tooltip,
  CartesianGrid, Legend,
} from 'recharts'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  monthTotalUsdAtom,
  workspaceRollupAtom,
  monthlyBudgetUsdAtom,
  refreshCostsAtom,
  loadBudgetAtom,
  setBudgetAtom,
} from '@/atoms/cost'
import {
  getDailyCosts, getModelCosts, getSessionCosts, onTurnCost,
} from '@/lib/tauri-bridge'
import type {
  DailyCostRollup, ModelCostRollup, SessionCostRollup, WorkspaceCostRollup,
} from '@/lib/types'
import { getWorkspaceIcon } from '@/lib/workspace-icons'

const PALETTE = ['hsl(220 70% 55%)', 'hsl(160 65% 45%)', 'hsl(30 80% 55%)',
                 'hsl(280 60% 60%)', 'hsl(0 70% 60%)', 'hsl(180 60% 45%)']

function formatUsd(v: number): string {
  if (v < 0.01) return `$${v.toFixed(4)}`
  return `$${v.toFixed(2)}`
}

function formatUsdShort(v: number): string {
  if (v < 0.01) return `$${v.toFixed(4)}`
  if (v < 1) return `$${v.toFixed(3)}`
  return `$${v.toFixed(2)}`
}

function formatDateChip(epochMs: number): string {
  const d = new Date(epochMs)
  return `${d.getMonth() + 1}/${d.getDate()}`
}

function BudgetHeader({
  total, budget, onSave,
}: {
  total: number
  budget: number | null
  onSave: (v: number | null) => void
}): React.ReactElement {
  const [editing, setEditing] = React.useState(false)
  const [draft, setDraft] = React.useState<string>(budget?.toString() ?? '')

  if (budget == null) {
    return (
      <div className="rounded-xl border border-border/60 bg-card p-4">
        <div className="text-[12.5px] font-medium text-foreground/80">本月已使用 {formatUsdShort(total)}</div>
        <div className="mt-1 text-[11px] text-muted-foreground/70">设置月度预算后，达到 80% / 100% 会收到提醒。</div>
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              const v = parseFloat(draft)
              if (Number.isFinite(v) && v > 0) {
                onSave(v)
                setEditing(false)
              }
            }}
            className="mt-3 flex items-center gap-2"
          >
            <span className="text-[12px] text-muted-foreground/80">$</span>
            <input
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              type="number" min="0" step="0.01" inputMode="decimal"
              className="w-24 rounded-md border border-border/60 bg-background px-2 py-1 text-[12.5px] outline-none focus:border-primary"
            />
            <button type="submit" className="rounded-md bg-primary px-2.5 py-1 text-[11.5px] text-primary-foreground hover:bg-primary/90">
              保存
            </button>
            <button type="button" onClick={() => setEditing(false)} className="text-[11.5px] text-muted-foreground/70 hover:text-foreground/80">
              取消
            </button>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="mt-3 rounded-md border border-dashed border-border/70 bg-transparent px-3 py-1.5 text-[11.5px] text-muted-foreground/85 hover:border-primary/50 hover:text-foreground/90"
          >
            设置月度预算
          </button>
        )}
      </div>
    )
  }

  const pct = Math.min(total / budget, 1.5)
  const isOver = total > budget
  const isWarn = !isOver && total / budget >= 0.8

  return (
    <div className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex items-baseline justify-between">
        <div>
          <div className="text-[14px] font-semibold text-foreground/90">本月用量</div>
          <div className="mt-0.5 text-[11.5px] text-muted-foreground/70">
            {formatUsdShort(total)} / {formatUsdShort(budget)} ·{' '}
            <span className={isOver ? 'text-destructive font-medium' : isWarn ? 'text-amber-500 font-medium' : ''}>
              {Math.round((total / budget) * 100)}%
            </span>
          </div>
        </div>
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              if (draft === '') { onSave(null); setEditing(false); return }
              const v = parseFloat(draft)
              if (Number.isFinite(v) && v > 0) { onSave(v); setEditing(false) }
            }}
            className="flex items-center gap-1.5"
          >
            <span className="text-[12px] text-muted-foreground/80">$</span>
            <input
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              type="number" min="0" step="0.01" inputMode="decimal"
              className="w-20 rounded-md border border-border/60 bg-background px-2 py-1 text-[12.5px] outline-none focus:border-primary"
              placeholder="预算"
            />
            <button type="submit" className="text-[11.5px] text-primary hover:underline">保存</button>
            <button type="button" onClick={() => setEditing(false)} className="text-[11.5px] text-muted-foreground/70 hover:text-foreground/80">×</button>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => { setDraft(budget.toString()); setEditing(true) }}
            className="text-[11px] text-muted-foreground/70 hover:text-foreground/80 underline-offset-2 hover:underline"
          >
            修改预算
          </button>
        )}
      </div>
      <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-muted">
        <div
          className={`h-full rounded-full transition-all duration-500 ${
            isOver ? 'bg-destructive' : isWarn ? 'bg-amber-500' : 'bg-primary'
          }`}
          style={{ width: `${(pct / 1.5) * 100}%` }}
        />
      </div>
    </div>
  )
}

function WorkspaceRollupSection({ items }: { items: WorkspaceCostRollup[] }): React.ReactElement | null {
  if (items.length === 0) return null
  const max = Math.max(...items.map((i) => i.totalCostUsd), 0.0001)
  return (
    <section>
      <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground/80">按工作区（本月）</h3>
      <div className="space-y-1.5">
        {items.map((r) => {
          const Icon = getWorkspaceIcon(r.workspaceIcon)
          return (
            <div key={r.workspaceId} className="flex items-center gap-2.5 rounded-md border border-border/40 bg-card/60 px-3 py-2">
              <span className="inline-flex items-center justify-center size-5 rounded bg-primary/15 text-primary shrink-0">
                <Icon className="size-3.5" />
              </span>
              <span className="flex-1 truncate text-[12.5px] text-foreground/85">{r.workspaceName || '默认工作区'}</span>
              <div className="relative h-1.5 w-24 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full bg-primary/70" style={{ width: `${(r.totalCostUsd / max) * 100}%` }} />
              </div>
              <span className="w-16 shrink-0 text-right text-[12px] tabular-nums text-foreground/80">{formatUsdShort(r.totalCostUsd)}</span>
            </div>
          )
        })}
      </div>
    </section>
  )
}

export function UsageSettings(): React.ReactElement {
  const [daily, setDaily] = React.useState<DailyCostRollup[]>([])
  const [models, setModels] = React.useState<ModelCostRollup[]>([])
  const [sessions, setSessions] = React.useState<SessionCostRollup[]>([])
  const [loading, setLoading] = React.useState(true)

  const monthTotal = useAtomValue(monthTotalUsdAtom)
  const wsRollup = useAtomValue(workspaceRollupAtom)
  const budget = useAtomValue(monthlyBudgetUsdAtom)
  const refreshCosts = useSetAtom(refreshCostsAtom)
  const loadBudget = useSetAtom(loadBudgetAtom)
  const saveBudget = useSetAtom(setBudgetAtom)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    try {
      const [d, m, s] = await Promise.all([
        getDailyCosts(30),
        getModelCosts(30),
        getSessionCosts(30, 50),
      ])
      setDaily(d)
      setModels(m)
      setSessions(s)
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void refetch()
    void refreshCosts()
    void loadBudget()
  }, [refetch, refreshCosts, loadBudget])

  // Debounced re-fetch on new turn_cost events
  React.useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null
    const unlistenP = onTurnCost(() => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => {
        void refetch()
        void refreshCosts()  // Phase 6-C: keep the monthly view live too
      }, 1000)
    })
    return () => {
      if (timer) clearTimeout(timer)
      void unlistenP.then((u) => u())
    }
  }, [refetch, refreshCosts])

  const totals = React.useMemo(() => {
    const cost = daily.reduce((a, d) => a + d.costUsd, 0)
    const inTok = daily.reduce((a, d) => a + d.inputTokens, 0)
    const outTok = daily.reduce((a, d) => a + d.outputTokens, 0)
    const turns = daily.reduce((a, d) => a + d.turnCount, 0)
    return { cost, inTok, outTok, turns }
  }, [daily])

  return (
    <div className="space-y-6 pb-8">
      <BudgetHeader total={monthTotal ?? 0} budget={budget} onSave={(v) => void saveBudget(v)} />
      <WorkspaceRollupSection items={wsRollup} />

      {/* KPI cards */}
      <section>
        <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          最近 30 天总览
        </h3>
        <div className="grid grid-cols-4 gap-2.5">
          <KPICard label="总花费" value={formatUsd(totals.cost)} />
          <KPICard label="LLM 调用" value={totals.turns.toLocaleString()} />
          <KPICard label="输入 token" value={(totals.inTok / 1000).toFixed(1) + 'k'} />
          <KPICard label="输出 token" value={(totals.outTok / 1000).toFixed(1) + 'k'} />
        </div>
      </section>

      {/* Daily bar */}
      <section>
        <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          每日花费（USD）
        </h3>
        <div className="h-56 rounded-lg border border-border/50 bg-muted/20 p-3">
          {daily.length === 0 ? (
            <EmptyState text={loading ? '加载中…' : '暂无数据'} />
          ) : (
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={daily} margin={{ top: 8, right: 8, left: 8, bottom: 4 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border) / 0.5)" vertical={false} />
                <XAxis dataKey="day" tick={{ fontSize: 10 }} stroke="hsl(var(--muted-foreground) / 0.7)" />
                <YAxis tick={{ fontSize: 10 }} stroke="hsl(var(--muted-foreground) / 0.7)"
                       tickFormatter={(v: number) => formatUsd(v)} />
                <Tooltip
                  contentStyle={{ background: 'hsl(var(--popover))', border: '1px solid hsl(var(--border))', fontSize: 12 }}
                  formatter={(v: number) => formatUsd(v)}
                />
                <Bar dataKey="costUsd" fill="hsl(var(--primary))" radius={[3, 3, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          )}
        </div>
      </section>

      {/* Model donut + Session table side by side on wide */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <section>
          <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            按模型
          </h3>
          <div className="h-56 rounded-lg border border-border/50 bg-muted/20 p-3">
            {models.length === 0 ? (
              <EmptyState text={loading ? '加载中…' : '暂无数据'} />
            ) : (
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={models}
                    dataKey="costUsd"
                    nameKey="model"
                    innerRadius={50}
                    outerRadius={75}
                    paddingAngle={2}
                  >
                    {models.map((_, i) => (
                      <Cell key={i} fill={PALETTE[i % PALETTE.length]} />
                    ))}
                  </Pie>
                  <Tooltip
                    contentStyle={{ background: 'hsl(var(--popover))', border: '1px solid hsl(var(--border))', fontSize: 12 }}
                    formatter={(v: number) => formatUsd(v)}
                  />
                  <Legend wrapperStyle={{ fontSize: 11 }} />
                </PieChart>
              </ResponsiveContainer>
            )}
          </div>
        </section>

        <section>
          <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            会话明细
          </h3>
          <div className="rounded-lg border border-border/50 bg-muted/20 max-h-56 overflow-y-auto">
            {sessions.length === 0 ? (
              <div className="p-3"><EmptyState text={loading ? '加载中…' : '暂无数据'} /></div>
            ) : (
              <table className="w-full text-[12px]">
                <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
                  <tr className="text-left text-muted-foreground/70">
                    <th className="px-3 py-2 font-normal">会话</th>
                    <th className="px-3 py-2 font-normal text-right">花费</th>
                    <th className="px-3 py-2 font-normal text-right">最近</th>
                  </tr>
                </thead>
                <tbody>
                  {sessions.map((s) => (
                    <tr key={s.sessionId} className="border-t border-border/30 hover:bg-muted/30">
                      <td className="px-3 py-1.5 truncate max-w-[200px]">
                        {s.title || s.sessionId.slice(0, 8)}
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">{formatUsd(s.costUsd)}</td>
                      <td className="px-3 py-1.5 text-right text-muted-foreground/60 tabular-nums">
                        {formatDateChip(s.lastUsedAt)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </section>
      </div>
    </div>
  )
}

function KPICard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border/50 bg-muted/20 px-3 py-2.5">
      <div className="text-[11px] text-muted-foreground/70">{label}</div>
      <div className="mt-0.5 text-[16px] font-semibold tabular-nums">{value}</div>
    </div>
  )
}

function EmptyState({ text }: { text: string }) {
  return (
    <div className="flex h-full items-center justify-center text-[12px] text-muted-foreground/60">
      {text}
    </div>
  )
}
