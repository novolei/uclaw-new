# Phase 6-C — Cost Dashboard + Budget Alerts

> **Sub-feature of [Phase 6](./2026-05-11-workspace-phase6-design.md).**
> Last to ship (after 6-A and 6-B). Adds per-workspace and per-model
> cost rollups + a configurable monthly budget with toast warnings.

## 1. Problem

V13 (PR #39) shipped `cost_records` — every agent turn writes a row
with `(session_id, model, input_tokens, output_tokens, cost_usd,
created_at)`. The Settings page has a rudimentary cost view, but
there's no:
- Per-workspace spend breakdown
- Per-model spend breakdown
- Monthly budget anchor (current month total vs. limit)
- Threshold warnings before the bill arrives

For users who run agents heavily, this is the difference between
"I think I'm spending too much" and "I'm at $X / $Y this month, time
to throttle."

## 2. Goal

- A `Settings → 用量与预算` page with three sections:
  1. **This month total** + budget progress bar
  2. **Per-workspace** rollup (sorted by spend descending)
  3. **Per-model** rollup (sorted by spend descending)
- A monthly budget input (USD, persisted via existing `Settings`
  shape — no new IPC).
- Toast alerts when the running monthly total crosses 80% / 100% of
  the budget. Toast-only — no blocking.

## 3. Non-Goals

- Per-workspace budgets (only a global monthly budget — revisit if
  asked).
- Hard-blocking budget enforcement.
- Yearly / weekly / daily rollups (just the current month).
- Cost CSV export.
- Multi-currency support (USD only).
- Historical month browsing (only the current month visible; raw data
  stays in the DB).

## 4. Data Model

**No schema changes.** V13's `cost_records` schema already has
everything needed. Per-workspace rollups query via a JOIN with
`agent_sessions`:

```sql
SELECT
    s.space_id AS workspace_id,
    SUM(c.cost_usd) AS total_cost_usd,
    SUM(c.input_tokens + c.output_tokens) AS total_tokens
FROM cost_records c
JOIN agent_sessions s ON c.session_id = s.id
WHERE c.created_at >= ?1   -- start of current month (ms)
GROUP BY s.space_id
ORDER BY total_cost_usd DESC
```

Per-model rollup:

```sql
SELECT
    model,
    SUM(cost_usd) AS total_cost_usd,
    SUM(input_tokens) AS in_tokens,
    SUM(output_tokens) AS out_tokens,
    COUNT(*) AS turn_count
FROM cost_records
WHERE created_at >= ?1
GROUP BY model
ORDER BY total_cost_usd DESC
```

JOIN performance: read-only queries, cost is ~one user click. No
denormalization needed.

## 5. Backend (Rust)

### New Tauri commands

```rust
// src-tauri/src/tauri_commands.rs

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCostRollup {
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_icon: String,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
}

/// Sum cost_records for the current month, grouped by workspace.
/// `since_ms` is the start of the current month in user-local time
/// (computed in the frontend and passed down — keeps timezone logic
/// out of Rust).
#[tauri::command]
pub async fn list_workspace_cost_rollup(
    state: State<'_, AppState>,
    since_ms: i64,
) -> Result<Vec<WorkspaceCostRollup>, Error> {
    // SQL above; join with spaces table to resolve workspace name + icon.
    // Returns empty Vec when no records.
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostRollup {
    pub model: String,
    pub total_cost_usd: f64,
    pub in_tokens: i64,
    pub out_tokens: i64,
    pub turn_count: i64,
}

#[tauri::command]
pub async fn list_model_cost_rollup(
    state: State<'_, AppState>,
    since_ms: i64,
) -> Result<Vec<ModelCostRollup>, Error> { /* ... */ }

#[tauri::command]
pub async fn get_month_cost_total(
    state: State<'_, AppState>,
    since_ms: i64,
) -> Result<f64, Error> { /* SUM(cost_usd) WHERE created_at >= since_ms */ }
```

Register all three in `main.rs` invoke_handler!.

### Budget alert emission (existing `emit_turn_cost`)

`src-tauri/src/agent/dispatcher.rs::emit_turn_cost` already persists
cost records. Add a threshold check after each insert:

```rust
// Pseudocode after the cost_records INSERT:
let monthly_total = compute_monthly_total(&conn, month_start_ms)?;
if let Some(budget) = settings.monthly_budget_usd {
    let pct = monthly_total / budget;
    let crossed_80 = (monthly_total - cost) / budget < 0.80 && pct >= 0.80;
    let crossed_100 = (monthly_total - cost) / budget < 1.00 && pct >= 1.00;
    if crossed_80 || crossed_100 {
        let _ = app_handle.emit("budget:threshold", BudgetThresholdPayload {
            threshold: if crossed_100 { 100 } else { 80 },
            current: monthly_total,
            budget,
        });
    }
}
```

Performance: one SUM query per turn. Cheap (V13 has `idx_cost_records_created`).
Skip the query entirely when `monthly_budget_usd` is None.

## 6. Settings Shape

`Settings` (existing IPC type) extends with:

```ts
interface Settings {
  // ... existing fields ...
  monthlyBudgetUsd: number | null  // null = no budget, no alerts
}
```

Rust side: same field with serde rename. `patch_settings` already
supports partial updates so the new field round-trips for free.

## 7. Frontend

### Atoms

`ui/src/atoms/cost.ts` (new file):

```ts
import { atom } from 'jotai'
import {
  listWorkspaceCostRollup,
  listModelCostRollup,
  getMonthCostTotal,
} from '@/lib/tauri-bridge'

export interface WorkspaceCostRollup { /* matches Rust shape */ }
export interface ModelCostRollup { /* matches Rust shape */ }

export const monthStartMsAtom = atom<number>(() => {
  const now = new Date()
  return new Date(now.getFullYear(), now.getMonth(), 1).getTime()
})

export const monthTotalUsdAtom = atom<number | null>(null)
export const workspaceRollupAtom = atom<WorkspaceCostRollup[]>([])
export const modelRollupAtom = atom<ModelCostRollup[]>([])

export const refreshCostsAtom = atom(null, async (get, set) => {
  const since = get(monthStartMsAtom)
  const [total, ws, models] = await Promise.all([
    getMonthCostTotal(since),
    listWorkspaceCostRollup(since),
    listModelCostRollup(since),
  ])
  set(monthTotalUsdAtom, total)
  set(workspaceRollupAtom, ws)
  set(modelRollupAtom, models)
})

// Budget threshold de-duplication — store which thresholds fired
// this session so we don't spam.
export const firedBudgetThresholdsAtom = atom<Set<80 | 100>>(new Set())
```

### Tauri-bridge wrappers

`ui/src/lib/tauri-bridge.ts`:

```ts
export const getMonthCostTotal = (sinceMs: number): Promise<number> =>
  invoke('get_month_cost_total', { sinceMs })

export const listWorkspaceCostRollup = (sinceMs: number): Promise<WorkspaceCostRollup[]> =>
  invoke('list_workspace_cost_rollup', { sinceMs })

export const listModelCostRollup = (sinceMs: number): Promise<ModelCostRollup[]> =>
  invoke('list_model_cost_rollup', { sinceMs })

export const onBudgetThreshold = (cb: (payload: BudgetThresholdPayload) => void): Promise<() => void> =>
  listen('budget:threshold', ({ payload }) => cb(payload))
```

### Component: CostDashboard

`ui/src/components/settings/CostDashboard.tsx` (new):

```tsx
export function CostDashboard(): React.ReactElement {
  const total = useAtomValue(monthTotalUsdAtom)
  const wsRollup = useAtomValue(workspaceRollupAtom)
  const modelRollup = useAtomValue(modelRollupAtom)
  const refresh = useSetAtom(refreshCostsAtom)
  const settings = useAtomValue(settingsAtom)
  const budget = settings.monthlyBudgetUsd

  React.useEffect(() => { void refresh() }, [refresh])

  return (
    <div className="space-y-6">
      <BudgetHeader total={total ?? 0} budget={budget} onSaveBudget={...} />
      <Section title="按工作区">
        <BarList items={wsRollup.map(r => ({
          label: r.workspaceName,
          icon: getWorkspaceIcon(r.workspaceIcon),
          value: r.totalCostUsd,
          tone: 'primary',
        }))} />
      </Section>
      <Section title="按模型">
        <BarList items={modelRollup.map(r => ({
          label: r.model,
          value: r.totalCostUsd,
          sublabel: `${r.turnCount} turns · ${r.inTokens + r.outTokens} tokens`,
          tone: 'neutral',
        }))} />
      </Section>
    </div>
  )
}
```

`BudgetHeader` shows the running total + a horizontal bar fill (filled
portion = used; remainder = headroom). When `budget == null`, render
a "设置月度预算" inline form. When budget exceeded, the bar turns
red/danger tone.

`BarList` is a simple component: list of rows with a label, a
horizontal bar (relative width = value / max), and the value as text.
No charting library needed — Recharts is finicky under jsdom and
overkill for two bars.

### Budget alert listener

In `AppShell.tsx` (sibling of `TabSessionSyncer`):

```tsx
React.useEffect(() => {
  let unlisten: (() => void) | undefined
  onBudgetThreshold((payload) => {
    const fired = store.get(firedBudgetThresholdsAtom)
    if (fired.has(payload.threshold)) return
    store.set(firedBudgetThresholdsAtom, new Set([...fired, payload.threshold]))
    if (payload.threshold === 80) {
      toast.warning(`本月已使用预算 $${payload.current.toFixed(2)} / $${payload.budget.toFixed(2)} (80%)`)
    } else {
      toast.error(`本月已超出预算: $${payload.current.toFixed(2)} / $${payload.budget.toFixed(2)}. AI 调用将继续但请留意。`)
    }
  }).then((fn) => { unlisten = fn })
  return () => unlisten?.()
}, [])
```

### Settings page wiring

`ui/src/components/settings/SettingsPanel.tsx` (or equivalent) gains a
new tab/section "用量与预算" that renders `<CostDashboard />`.

## 8. Edge Cases

- **No cost records yet**: dashboard renders zeros + "本月暂无记录" hint.
- **No budget set**: alerts disabled; UI shows "设置预算以接收提醒".
- **Budget changed mid-month**: thresholds re-evaluate on next turn.
  If user lowers budget below current spend, both 80% and 100% can
  fire in immediate succession (within the same turn); the dedup set
  prevents re-firing.
- **Month rollover**: `monthStartMsAtom` is computed at hook time, so
  it stays stable through a session. If the user keeps the app open
  across midnight on the 1st of a new month, the dashboard reflects
  the wrong month until next refresh. Acceptable — power users
  restart the app or click refresh; a future tweak could re-derive
  on focus/visibility events.
- **`firedBudgetThresholdsAtom` resets on app restart** — that's
  intentional: a fresh boot in a new month should re-alert if still
  over. Persisting this state would be over-engineering.
- **Currency**: `cost_usd` stays in USD. Display formatting uses
  `Intl.NumberFormat` with USD.
- **Negative budget** (typo): clamp to 0 in the input form.
- **Very small spends** (sub-cent): display `$0.001` granularity for
  rollups, `$0.01` granularity for the budget bar.

## 9. Tests

Vitest:

- `monthStartMsAtom` returns the first-of-month-at-midnight in local
  time.
- `refreshCostsAtom` parallel-fetches the three IPCs and writes the
  three atoms.
- Threshold dedup: emit budget:threshold 80% twice; second fire is a
  no-op (toast called once).
- `BarList` renders rows in order, with relative widths matching
  ratios.
- `BudgetHeader`: when `budget=null`, renders the inline form. When
  total > budget, bar shows danger tone.

Rust unit:
- `list_workspace_cost_rollup` returns empty Vec for empty
  `cost_records`.
- Sum is correct across multiple sessions, multiple workspaces.
- Threshold detection in `emit_turn_cost`: fires once per crossing,
  not on each subsequent turn that stays above threshold (achieved
  by checking the previous-total vs new-total crossing).

## 10. Commit Shape (6 commits)

1. `feat(settings): monthlyBudgetUsd field in Settings shape`
2. `feat(cost): three Tauri commands — month total, per-workspace, per-model rollups`
3. `feat(cost): emit budget:threshold event on 80%/100% crossings`
4. `feat(atoms): cost-related atoms (totals, rollups, dedup set)`
5. `feat(settings): CostDashboard component with BudgetHeader + BarList`
6. `feat(app-shell): budget-threshold toast listener + Settings tab wiring`

Each commit bisects to a meaningful surface: 1 lets you save a budget
(no display); 2 lets you query rollups via dev console; 3 lets you
see toasts (but no dashboard); 4 has working atoms; 5 has a dashboard
that renders (no alerts); 6 is full feature.

## 11. Risks

- **Threshold race condition**: if two concurrent agent turns finish
  in the same millisecond and both push the total past 80%, both
  fire. Mitigation: the `firedBudgetThresholdsAtom` dedup catches
  the second one client-side. Rust-side dedup is harder (would need
  a persisted "last_alerted_threshold" column) — leave for later if
  it becomes noisy.
- **JOIN cost on huge `cost_records` tables**: V13 has the
  `created_at` index; a month's worth of records is bounded. If a
  user accumulates a year of cost records and queries them, the
  rollup queries stay fast because of the WHERE filter.
- **Settings field add**: changes the `Settings` type. Any frontend
  consumer that destructures Settings might need to update — most
  use spread or just the fields they care about. Audit during
  implementation.
- **Currency drift**: if Anthropic changes pricing mid-month,
  `cost_usd` is whatever the agent computed at turn time. We don't
  recompute. Acceptable; this is a usage tracker, not an
  authoritative bill.
