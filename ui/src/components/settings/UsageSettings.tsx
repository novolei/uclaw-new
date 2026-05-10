/**
 * UsageSettings — Settings → 用量 tab.
 *
 * Three views:
 *   - Daily total bar chart (last 30 days)
 *   - Per-model donut (last 30 days)
 *   - Per-session table (most recent first)
 *
 * Live update: subscribes to `agent:turn_cost` events; on each event
 * we re-fetch the daily rollup (debounced 1s) so the bar chart bumps
 * without manual reload.
 */

import * as React from 'react'
import {
  ResponsiveContainer,
  BarChart, Bar,
  PieChart, Pie, Cell,
  XAxis, YAxis, Tooltip,
  CartesianGrid, Legend,
} from 'recharts'
import {
  getDailyCosts, getModelCosts, getSessionCosts, onTurnCost,
} from '@/lib/tauri-bridge'
import type {
  DailyCostRollup, ModelCostRollup, SessionCostRollup,
} from '@/lib/types'

const PALETTE = ['hsl(220 70% 55%)', 'hsl(160 65% 45%)', 'hsl(30 80% 55%)',
                 'hsl(280 60% 60%)', 'hsl(0 70% 60%)', 'hsl(180 60% 45%)']

function formatUsd(v: number): string {
  if (v < 0.01) return `$${v.toFixed(4)}`
  return `$${v.toFixed(2)}`
}

function formatDateChip(epochMs: number): string {
  const d = new Date(epochMs)
  return `${d.getMonth() + 1}/${d.getDate()}`
}

export function UsageSettings(): React.ReactElement {
  const [daily, setDaily] = React.useState<DailyCostRollup[]>([])
  const [models, setModels] = React.useState<ModelCostRollup[]>([])
  const [sessions, setSessions] = React.useState<SessionCostRollup[]>([])
  const [loading, setLoading] = React.useState(true)

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

  React.useEffect(() => { void refetch() }, [refetch])

  // Debounced re-fetch on new turn_cost events
  React.useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null
    const unlistenP = onTurnCost(() => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => { void refetch() }, 1000)
    })
    return () => {
      if (timer) clearTimeout(timer)
      void unlistenP.then((u) => u())
    }
  }, [refetch])

  const totals = React.useMemo(() => {
    const cost = daily.reduce((a, d) => a + d.costUsd, 0)
    const inTok = daily.reduce((a, d) => a + d.inputTokens, 0)
    const outTok = daily.reduce((a, d) => a + d.outputTokens, 0)
    const turns = daily.reduce((a, d) => a + d.turnCount, 0)
    return { cost, inTok, outTok, turns }
  }, [daily])

  return (
    <div className="space-y-6 pb-8">
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
