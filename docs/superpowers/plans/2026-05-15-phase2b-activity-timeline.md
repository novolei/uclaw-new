# Phase 2b: Activity Timeline & Rich Report Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the automation 动态 tab to a vertical timeline with rich Markdown report cards, add a collapsible report card to RunSessionSubView, and fix the FilePathChip dead-click bug in Kaleidoscope.

**Architecture:** Seven sequential tasks: (1) Rust adds `working_dir` to `AutomationActivity` via a JOIN query; (2) TS interface picks up the new field; (3) new `ActivityMarkdown` compact renderer; (4) `ActivityListItem` rebuilt as a timeline card with Markdown body and artifact chips; (5) `ActivityHistoryView` wraps items in a dot+line timeline shell and passes `activity` to `RunSessionSubView`; (6) `RunSessionSubView` gains a collapsible report card above `AgentMessages`; (7) `KaleidoscopeShell` mounts `<PreviewPanel />` so FilePathChips work.

**Tech Stack:** Rust / rusqlite, React 18 + TypeScript, react-markdown + remark-gfm (already installed), Jotai, `openFile` / `openExternal` from tauri-bridge (already wired).

---

## File Structure

| File | Action |
|---|---|
| `src-tauri/src/automation/activity.rs` | Modify — add `working_dir` field, JOIN query |
| `ui/src/lib/tauri-bridge.ts` | Modify — add `workingDir: string` to interface |
| `ui/src/components/automation/ActivityMarkdown.tsx` | **Create** — compact Markdown renderer |
| `ui/src/components/automation/ActivityListItem.tsx` | Modify — timeline card, Markdown body, artifact chips |
| `ui/src/components/automation/ActivityListItem.test.tsx` | Modify — update mock, add new assertions |
| `ui/src/components/automation/ActivityHistoryView.tsx` | Modify — timeline shell, pass `activity` to RunSessionSubView |
| `ui/src/components/automation/RunSessionSubView.tsx` | Modify — collapsible report card above AgentMessages |
| `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx` | Modify — mount `<PreviewPanel />` |

---

## Task 1: Rust — `working_dir` in `AutomationActivity`

**Files:**
- Modify: `src-tauri/src/automation/activity.rs`

**Context:** The frontend needs an absolute path to open file artifacts. `working_dir` is derived from `spaces.path` for the spec's linked space, falling back to `~/Documents/workground/automations/<spec_id>`. It is computed via LEFT JOIN — never stored in the DB and never part of INSERT.

- [ ] **Step 1: Write the failing test**

Add this test to the `#[cfg(test)]` block at the bottom of `src-tauri/src/automation/activity.rs` (after the existing `activity_store_shim_complete_and_fail` test):

```rust
#[test]
fn list_activities_working_dir_fallback() {
    let conn = setup_test_db();
    let activity = make_activity("a1");
    insert_activity(&conn, &activity).unwrap();
    let rows = list_activities_for_spec(&conn, "s1", 10).unwrap();
    assert_eq!(rows.len(), 1);
    // No space attached to spec → fallback path must contain the spec_id
    assert!(
        rows[0].working_dir.contains("s1"),
        "expected working_dir to contain spec_id 's1', got: {}",
        rows[0].working_dir
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test --lib activity 2>&1 | grep -E "FAILED|error|working_dir"
```

Expected: compile error — `AutomationActivity` has no `working_dir` field, and `make_activity` is missing the field.

- [ ] **Step 3: Add `working_dir` to the struct**

In `src-tauri/src/automation/activity.rs`, add the field to `AutomationActivity` after `resumed_from_escalation_id`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationActivity {
    pub id: String,
    pub spec_id: String,
    pub subscription_id: Option<String>,
    pub trigger_source_type: TriggerSource,
    pub trigger_payload_json: String,
    pub status: ActivityStatus,
    pub error_text: Option<String>,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub duration_ms: i64,
    pub llm_iterations: i64,
    pub llm_tokens_in: i64,
    pub llm_tokens_out: i64,
    pub session_id: Option<String>,
    pub report_artifacts_json: String,
    pub report_text: Option<String>,
    pub report_outcome: Option<String>,
    pub escalation_id: Option<String>,
    pub resumed_from_activity_id: Option<String>,
    pub resumed_from_escalation_id: Option<String>,
    /// Absolute path of the spec's run working directory. Derived via JOIN on
    /// spaces; falls back to ~/Documents/workground/automations/<spec_id>.
    /// Not stored in the DB — computed at query time only.
    #[serde(default)]
    pub working_dir: String,
}
```

- [ ] **Step 4: Replace `SELECT_COLS` with `SELECT_WITH_WD` and update `row_to_activity`**

Replace the entire `SELECT_COLS` constant and `row_to_activity` function:

```rust
/// Full SELECT with LEFT JOIN to resolve working_dir from the spec's space.
/// Column index 21 = working_dir (COALESCE of spaces.path, '').
const SELECT_WITH_WD: &str =
    "SELECT a.id, a.spec_id, a.subscription_id, a.trigger_source_type,
            a.trigger_payload_json, a.status, a.error_text, a.queued_at,
            a.started_at, a.completed_at, a.duration_ms, a.llm_iterations,
            a.llm_tokens_in, a.llm_tokens_out, a.session_id,
            a.report_artifacts_json, a.report_text, a.report_outcome,
            a.escalation_id, a.resumed_from_activity_id,
            a.resumed_from_escalation_id,
            COALESCE(s.path, '') AS working_dir
     FROM automation_activities a
     LEFT JOIN automation_specs sp ON sp.id = a.spec_id
     LEFT JOIN spaces s ON s.id = sp.space_id";

fn row_to_activity(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationActivity> {
    let trigger_str: String = r.get(3)?;
    let status_str: String  = r.get(5)?;
    let spec_id: String     = r.get(1)?;

    // working_dir: col 21 = spaces.path from the JOIN; empty means no space
    // linked → fall back to ~/Documents/workground/automations/<spec_id>.
    let raw_wd: String = r.get(21).unwrap_or_default();
    let working_dir = if raw_wd.is_empty() {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("Documents/workground/automations")
            .join(&spec_id)
            .to_string_lossy()
            .into_owned()
    } else {
        raw_wd
    };

    Ok(AutomationActivity {
        id:                          r.get(0)?,
        spec_id,
        subscription_id:             r.get(2)?,
        trigger_source_type: TriggerSource::from_db_str(&trigger_str)
            .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(
                    format!("unknown trigger source: {trigger_str}")
                )),
            ))?,
        trigger_payload_json:        r.get(4)?,
        status: ActivityStatus::from_db_str(&status_str)
            .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(
                    format!("unknown status: {status_str}")
                )),
            ))?,
        error_text:                  r.get(6)?,
        queued_at:                   r.get(7)?,
        started_at:                  r.get(8)?,
        completed_at:                r.get(9)?,
        duration_ms:                 r.get(10)?,
        llm_iterations:              r.get(11)?,
        llm_tokens_in:               r.get(12)?,
        llm_tokens_out:              r.get(13)?,
        session_id:                  r.get(14)?,
        report_artifacts_json:       r.get(15)?,
        report_text:                 r.get(16)?,
        report_outcome:              r.get(17)?,
        escalation_id:               r.get(18)?,
        resumed_from_activity_id:    r.get(19)?,
        resumed_from_escalation_id:  r.get(20)?,
        working_dir,
    })
}
```

- [ ] **Step 5: Update `get_activity` and `list_activities_for_spec` to use `SELECT_WITH_WD`**

Replace both public query functions:

```rust
pub fn get_activity(
    conn: &rusqlite::Connection,
    id: &str,
) -> rusqlite::Result<Option<AutomationActivity>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT_WITH_WD} WHERE a.id = ?1"
    ))?;
    match stmt.query_row([id], row_to_activity) {
        Ok(a)                                     => Ok(Some(a)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e)                                    => Err(e),
    }
}

pub fn list_activities_for_spec(
    conn: &rusqlite::Connection,
    spec_id: &str,
    limit: u32,
) -> rusqlite::Result<Vec<AutomationActivity>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT_WITH_WD}
         WHERE a.spec_id = ?1
         ORDER BY a.queued_at DESC
         LIMIT ?2"
    ))?;
    let rows = stmt.query_map(rusqlite::params![spec_id, limit], row_to_activity)?;
    rows.collect()
}
```

- [ ] **Step 6: Update `make_activity` in the test helpers to include `working_dir`**

In the `#[cfg(test)]` block, update `make_activity`:

```rust
fn make_activity(id: &str) -> AutomationActivity {
    AutomationActivity {
        id:                          id.into(),
        spec_id:                     "s1".into(),
        subscription_id:             None,
        trigger_source_type:         TriggerSource::Manual,
        trigger_payload_json:        "{}".into(),
        status:                      ActivityStatus::Queued,
        error_text:                  None,
        queued_at:                   1,
        started_at:                  None,
        completed_at:                None,
        duration_ms:                 0,
        llm_iterations:              0,
        llm_tokens_in:               0,
        llm_tokens_out:              0,
        session_id:                  None,
        report_artifacts_json:       "[]".into(),
        report_text:                 None,
        report_outcome:              None,
        escalation_id:               None,
        resumed_from_activity_id:    None,
        resumed_from_escalation_id:  None,
        working_dir:                 String::new(),
    }
}
```

- [ ] **Step 7: Run all activity tests**

```bash
cd src-tauri && cargo test --lib activity 2>&1 | tail -15
```

Expected: all tests pass including the new `list_activities_working_dir_fallback`.

- [ ] **Step 8: Verify full Rust build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: no output (clean build).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/automation/activity.rs
git commit -m "feat(automation): add working_dir to AutomationActivity via spaces JOIN"
```

---

## Task 2: TypeScript — `workingDir` in `AutomationActivity` interface

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 1: Add `workingDir` to the interface**

In `ui/src/lib/tauri-bridge.ts`, find the `AutomationActivity` interface (around line 1232) and add `workingDir` after `resumedFromEscalationId`:

```typescript
export interface AutomationActivity {
  id: string
  specId: string
  subscriptionId: string | null
  triggerSourceType: 'schedule' | 'file' | 'webhook' | 'webpage' | 'rss' | 'wecom' | 'custom' | 'manual' | string
  triggerPayloadJson: string
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'waiting_user' | 'filtered_out' | 'deferred_phase_2' | string
  errorText: string | null
  queuedAt: number
  startedAt: number | null
  completedAt: number | null
  durationMs: number
  llmIterations: number
  llmTokensIn: number
  llmTokensOut: number
  sessionId: string | null
  reportArtifactsJson: string
  reportText: string | null
  reportOutcome: string | null
  escalationId: string | null
  resumedFromActivityId: string | null
  resumedFromEscalationId: string | null
  workingDir: string
}
```

- [ ] **Step 2: Run TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected: errors pointing to test fixtures that use `AutomationActivity` without `workingDir` — that's fine, they'll be fixed in Task 4.

- [ ] **Step 3: Commit**

```bash
git add ui/src/lib/tauri-bridge.ts
git commit -m "feat(automation): add workingDir to AutomationActivity TS interface"
```

---

## Task 3: Create `ActivityMarkdown` compact renderer

**Files:**
- Create: `ui/src/components/automation/ActivityMarkdown.tsx`
- Create: `ui/src/components/automation/ActivityMarkdown.test.tsx`

**Context:** The existing `MarkdownRenderer` at `ui/src/components/preview/renderers/MarkdownRenderer.tsx` is a full-page viewer (overflow-auto, max-w-3xl, heavy padding). `ActivityMarkdown` is a compact inline variant for card bodies.

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/automation/ActivityMarkdown.test.tsx`:

```typescript
import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { ActivityMarkdown } from './ActivityMarkdown'

describe('ActivityMarkdown', () => {
  it('renders bold text', () => {
    const { container } = render(<ActivityMarkdown content="**bold**" />)
    expect(container.querySelector('strong')).toBeTruthy()
  })

  it('renders inline code', () => {
    const { container } = render(<ActivityMarkdown content="`code`" />)
    expect(container.querySelector('code')).toBeTruthy()
  })

  it('renders a GFM table', () => {
    const { container } = render(
      <ActivityMarkdown content={'| A | B |\n|---|---|\n| 1 | 2 |'} />
    )
    expect(container.querySelector('table')).toBeTruthy()
  })

  it('accepts an extra className', () => {
    const { container } = render(
      <ActivityMarkdown content="hi" className="custom-class" />
    )
    expect(container.firstElementChild?.className).toContain('custom-class')
  })
})
```

- [ ] **Step 2: Run to verify failure**

```bash
cd ui && npm test -- --run ActivityMarkdown 2>&1 | tail -10
```

Expected: FAIL — `ActivityMarkdown` module not found.

- [ ] **Step 3: Create the component**

Create `ui/src/components/automation/ActivityMarkdown.tsx`:

```typescript
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

interface Props {
  content: string
  className?: string
}

export function ActivityMarkdown({ content, className = '' }: Props) {
  return (
    <div
      className={[
        'prose prose-sm prose-zinc dark:prose-invert max-w-none',
        'prose-p:my-1 prose-headings:mt-2 prose-headings:mb-1',
        'prose-h1:text-sm prose-h2:text-sm prose-h3:text-xs',
        'prose-ul:my-1 prose-ol:my-1 prose-li:my-0',
        'prose-code:text-[11px] prose-code:bg-muted prose-code:px-1',
        'prose-code:py-0.5 prose-code:rounded',
        'prose-code:before:content-none prose-code:after:content-none',
        'prose-table:text-[11px] prose-th:px-2 prose-td:px-2',
        'prose-a:text-primary hover:prose-a:opacity-80',
        className,
      ].join(' ')}
    >
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
    </div>
  )
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd ui && npm test -- --run ActivityMarkdown 2>&1 | tail -10
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/automation/ActivityMarkdown.tsx ui/src/components/automation/ActivityMarkdown.test.tsx
git commit -m "feat(automation): ActivityMarkdown compact inline Markdown renderer"
```

---

## Task 4: Rebuild `ActivityListItem` as a timeline card

**Files:**
- Modify: `ui/src/components/automation/ActivityListItem.tsx`
- Modify: `ui/src/components/automation/ActivityListItem.test.tsx`

**Context:** The new card shows: header row (timestamp, status label, outcome badge, duration, archive button, "查看进程 ›") + Markdown body (or running placeholder) + artifact chips. The timeline dot/connector line lives in `ActivityHistoryView` (Task 5), not here. `ArtifactChip` and `OUTCOME_CONFIG` are exported so `RunSessionSubView` (Task 6) can reuse them without duplication.

- [ ] **Step 1: Update the test file**

Replace the full contents of `ui/src/components/automation/ActivityListItem.test.tsx`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react'
import { ActivityListItem } from './ActivityListItem'
import type { AutomationActivity } from '@/lib/tauri-bridge'

const openFileMock = vi.fn().mockResolvedValue(undefined)
const openExternalMock = vi.fn().mockResolvedValue(undefined)

vi.mock('@/lib/tauri-bridge', () => ({
  toggleArchiveAgentSession: vi.fn().mockResolvedValue(undefined),
  openFile: openFileMock,
  openExternal: openExternalMock,
}))

// react-markdown renders async in jsdom; wrap in act where needed
const baseActivity: AutomationActivity = {
  id: 'act-1', specId: 'spec-1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status: 'completed', errorText: null,
  queuedAt: 1_700_000_000_000, startedAt: 1_700_000_000_000,
  completedAt: 1_700_000_042_000,
  durationMs: 42_000, llmIterations: 1, llmTokensIn: 100, llmTokensOut: 50,
  sessionId: 'sess-1', reportArtifactsJson: '[]',
  reportText: null, reportOutcome: null,
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
  workingDir: '/test/workdir',
}

beforeEach(() => {
  openFileMock.mockClear()
  openExternalMock.mockClear()
})

describe('ActivityListItem', () => {
  it('renders the testid and status label', () => {
    render(<ActivityListItem activity={baseActivity} />)
    expect(screen.getByTestId('activity-row-act-1')).toBeTruthy()
    expect(screen.getByText('已完成')).toBeTruthy()
  })

  it('shows outcome badge only when reportOutcome is set', async () => {
    const { rerender } = render(<ActivityListItem activity={baseActivity} />)
    expect(screen.queryByText('有效')).toBeNull()

    rerender(<ActivityListItem activity={{ ...baseActivity, reportOutcome: 'useful' }} />)
    expect(screen.getByText('有效')).toBeTruthy()
  })

  it('maps all outcome values to correct labels', () => {
    const cases: [string, string][] = [
      ['useful', '有效'], ['noop', '无操作'], ['skipped', '跳过'], ['error', '错误'],
    ]
    for (const [outcome, label] of cases) {
      const { unmount } = render(
        <ActivityListItem activity={{ ...baseActivity, reportOutcome: outcome }} />
      )
      expect(screen.getByText(label)).toBeTruthy()
      unmount()
    }
  })

  it('shows running placeholder when status is running and no reportText', () => {
    render(
      <ActivityListItem
        activity={{ ...baseActivity, status: 'running', reportText: null }}
      />
    )
    expect(screen.getByText(/运行中，暂无报告/)).toBeTruthy()
  })

  it('does not show body when status is completed and reportText is null', () => {
    render(<ActivityListItem activity={baseActivity} />)
    expect(screen.queryByText(/运行中，暂无报告/)).toBeNull()
  })

  it('renders reportText via ActivityMarkdown', async () => {
    await act(async () => {
      render(
        <ActivityListItem activity={{ ...baseActivity, reportText: '**bold result**' }} />
      )
    })
    const el = document.querySelector('strong')
    expect(el).toBeTruthy()
  })

  it('renders file artifact chip and calls openFile on click', async () => {
    const artifacts = JSON.stringify([{ kind: 'file', path: 'report.md', title: 'Report' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('Report', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openFileMock).toHaveBeenCalledWith('/test/workdir/report.md')
  })

  it('renders url artifact chip and calls openExternal on click', async () => {
    const artifacts = JSON.stringify([{ kind: 'url', path: 'https://example.com', title: 'Results' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('Results', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openExternalMock).toHaveBeenCalledWith('https://example.com')
  })

  it('renders url artifact chip using title as fallback URL when path is absent', async () => {
    const artifacts = JSON.stringify([{ kind: 'url', title: 'https://fallback.com' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('https://fallback.com', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openExternalMock).toHaveBeenCalledWith('https://fallback.com')
  })

  it('renders text artifact chip as non-clickable (no openFile or openExternal)', async () => {
    const artifacts = JSON.stringify([{ kind: 'text', title: 'Summary' }])
    render(
      <ActivityListItem activity={{ ...baseActivity, reportArtifactsJson: artifacts }} />
    )
    const chip = screen.getByText('Summary', { exact: false })
    await act(async () => { fireEvent.click(chip) })
    expect(openFileMock).not.toHaveBeenCalled()
    expect(openExternalMock).not.toHaveBeenCalled()
  })

  it('calls onArchived after archiving', async () => {
    const onArchived = vi.fn()
    render(<ActivityListItem activity={baseActivity} onArchived={onArchived} />)
    const btn = screen.getByLabelText('归档')
    await act(async () => { fireEvent.click(btn) })
    await waitFor(() => expect(onArchived).toHaveBeenCalledWith('sess-1'))
  })

  it('calls onOpenRunSession when 查看进程 is clicked', () => {
    const onOpen = vi.fn()
    render(<ActivityListItem activity={baseActivity} onOpenRunSession={onOpen} />)
    fireEvent.click(screen.getByText(/查看进程/))
    expect(onOpen).toHaveBeenCalledWith('sess-1')
  })

  it('escalation ring applied when status is waiting_user', () => {
    const { container } = render(
      <ActivityListItem activity={{ ...baseActivity, status: 'waiting_user' }} />
    )
    expect(container.firstElementChild?.className).toContain('ring-warning')
  })
})
```

- [ ] **Step 2: Run tests to confirm failures**

```bash
cd ui && npm test -- --run ActivityListItem 2>&1 | tail -15
```

Expected: multiple failures — missing exports, missing fields in fixture, etc.

- [ ] **Step 3: Replace `ActivityListItem.tsx` with the new implementation**

Replace the full contents of `ui/src/components/automation/ActivityListItem.tsx`:

```typescript
import { useState, useMemo } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { toggleArchiveAgentSession, openFile, openExternal } from '@/lib/tauri-bridge'
import { ActivityMarkdown } from './ActivityMarkdown'

// ─── Shared types and config (exported for RunSessionSubView) ─────────────────

export interface ReportArtifact {
  kind: string
  path?: string
  title: string
}

export const OUTCOME_CONFIG: Record<string, { label: string; className: string }> = {
  useful:  { label: '有效',   className: 'bg-green-500/15 text-green-600 dark:text-green-400' },
  noop:    { label: '无操作', className: 'bg-muted text-muted-foreground' },
  skipped: { label: '跳过',   className: 'bg-muted text-muted-foreground' },
  error:   { label: '错误',   className: 'bg-danger/10 text-danger' },
}

// ─── ArtifactChip (exported for RunSessionSubView) ────────────────────────────

interface ChipProps {
  artifact: ReportArtifact
  workingDir: string
}

export function ArtifactChip({ artifact, workingDir }: ChipProps) {
  const icon = artifact.kind === 'file' ? '📄' : artifact.kind === 'url' ? '🔗' : '📝'
  const clickable = artifact.kind === 'file' || artifact.kind === 'url'

  function handleClick() {
    if (artifact.kind === 'file' && artifact.path) {
      void openFile(`${workingDir}/${artifact.path}`)
    } else if (artifact.kind === 'url') {
      void openExternal(artifact.path ?? artifact.title)
    }
  }

  if (!clickable) {
    return (
      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] bg-muted text-muted-foreground">
        {icon} {artifact.title}
      </span>
    )
  }

  return (
    <button
      onClick={handleClick}
      className="titlebar-no-drag inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] bg-primary/10 text-primary hover:bg-primary/20 transition-colors"
    >
      {icon} {artifact.title}
    </button>
  )
}

// ─── Status config ────────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<string, { label: string; className: string }> = {
  completed:    { label: '已完成', className: 'text-success' },
  failed:       { label: '失败',   className: 'text-danger' },
  cancelled:    { label: '已取消', className: 'text-muted-foreground' },
  filtered_out: { label: '已跳过', className: 'text-muted-foreground' },
  waiting_user: { label: '待确认', className: 'text-warning' },
  running:      { label: '运行中', className: 'text-primary' },
  queued:       { label: '排队中', className: 'text-muted-foreground' },
}

function formatTs(ms: number | null): string {
  if (!ms) return '—'
  return new Date(ms).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  })
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

// ─── ActivityListItem ─────────────────────────────────────────────────────────

interface Props {
  activity: AutomationActivity
  onOpenRunSession?: (sessionId: string) => void
  onArchived?: (sessionId: string) => void
}

export function ActivityListItem({ activity, onOpenRunSession, onArchived }: Props) {
  const [archiving, setArchiving] = useState(false)

  const cfg = STATUS_CONFIG[activity.status] ?? {
    label: activity.status,
    className: 'text-muted-foreground',
  }
  const outcomeCfg = activity.reportOutcome
    ? (OUTCOME_CONFIG[activity.reportOutcome] ?? null)
    : null
  const isEscalation = activity.status === 'waiting_user'
  const isActive = activity.status === 'running' || activity.status === 'queued'

  const artifacts = useMemo<ReportArtifact[]>(() => {
    try { return JSON.parse(activity.reportArtifactsJson) as ReportArtifact[] }
    catch { return [] }
  }, [activity.reportArtifactsJson])

  async function handleArchive() {
    if (!activity.sessionId || archiving) return
    setArchiving(true)
    try {
      await toggleArchiveAgentSession(activity.sessionId)
      onArchived?.(activity.sessionId)
    } finally {
      setArchiving(false)
    }
  }

  return (
    <div
      data-testid={`activity-row-${activity.id}`}
      className={[
        'group rounded-lg border bg-background/60',
        isEscalation
          ? 'border-warning ring-1 ring-warning/20'
          : 'border-border/40',
      ].join(' ')}
    >
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 text-xs">
        <span className="text-muted-foreground shrink-0">
          {formatTs(activity.startedAt ?? activity.queuedAt)}
        </span>
        <span className={cfg.className}>{cfg.label}</span>
        {outcomeCfg && (
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${outcomeCfg.className}`}>
            {outcomeCfg.label}
          </span>
        )}
        {activity.durationMs > 0 && (
          <span className="text-muted-foreground">
            {formatDuration(activity.durationMs)}
          </span>
        )}
        <div className="ml-auto flex items-center gap-2 shrink-0">
          {activity.sessionId && (
            <button
              onClick={handleArchive}
              disabled={archiving}
              className="titlebar-no-drag text-xs text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
              aria-label="归档"
            >
              归档
            </button>
          )}
          {activity.sessionId && (
            <button
              onClick={() => onOpenRunSession?.(activity.sessionId!)}
              className="titlebar-no-drag text-xs text-primary hover:underline"
            >
              查看进程 &gt;
            </button>
          )}
        </div>
      </div>

      {/* Body: Markdown or running placeholder */}
      {(isActive || activity.reportText) && (
        <div className="px-3 pb-2">
          {isActive && !activity.reportText ? (
            <p className="text-xs text-muted-foreground italic">运行中，暂无报告…</p>
          ) : activity.reportText ? (
            <ActivityMarkdown content={activity.reportText} />
          ) : null}
        </div>
      )}

      {/* Artifact chips */}
      {artifacts.length > 0 && (
        <div className="flex flex-wrap gap-1.5 px-3 pb-2">
          {artifacts.map((a, i) => (
            <ArtifactChip key={i} artifact={a} workingDir={activity.workingDir} />
          ))}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Run the tests**

```bash
cd ui && npm test -- --run ActivityListItem 2>&1 | tail -15
```

Expected: all tests pass.

- [ ] **Step 5: Run full TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Fix any type errors before committing.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/automation/ActivityListItem.tsx ui/src/components/automation/ActivityListItem.test.tsx
git commit -m "feat(automation): rebuild ActivityListItem as timeline card with Markdown + artifact chips"
```

---

## Task 5: Timeline shell in `ActivityHistoryView`

**Files:**
- Modify: `ui/src/components/automation/ActivityHistoryView.tsx`

**Context:** `ActivityHistoryView` now wraps each `ActivityListItem` in a flex row containing a left-column dot+connector and a right-column card. It also passes the full `activity` object to `RunSessionSubView` (needed for Task 6's report card).

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/automation/ActivityHistoryView.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ActivityHistoryView } from './ActivityHistoryView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  toggleArchiveAgentSession: vi.fn().mockResolvedValue(undefined),
  openFile: vi.fn(),
  openExternal: vi.fn(),
}))
vi.mock('./RunSessionSubView', () => ({
  RunSessionSubView: () => <div data-testid="run-session" />,
}))

const makeAct = (id: string, status = 'completed'): AutomationActivity => ({
  id, specId: 'spec-1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status, errorText: null,
  queuedAt: 1_700_000_000_000, startedAt: null, completedAt: null,
  durationMs: 0, llmIterations: 0, llmTokensIn: 0, llmTokensOut: 0,
  sessionId: `sess-${id}`, reportArtifactsJson: '[]',
  reportText: null, reportOutcome: null,
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
  workingDir: '/workdir',
})

describe('ActivityHistoryView', () => {
  it('renders each activity as a timeline row', () => {
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={[makeAct('a1'), makeAct('a2')]}
      />
    )
    expect(screen.getByTestId('activity-row-a1')).toBeTruthy()
    expect(screen.getByTestId('activity-row-a2')).toBeTruthy()
  })

  it('shows empty-state when activities array is empty', () => {
    render(<ActivityHistoryView specId="spec-1" activities={[]} />)
    expect(screen.getByText(/还没有运行记录/)).toBeTruthy()
  })

  it('shows RunSessionSubView when activeRunSessionId matches a session', () => {
    render(
      <ActivityHistoryView
        specId="spec-1"
        activities={[makeAct('a1')]}
        activeRunSessionId="sess-a1"
        onCloseRunSession={() => {}}
      />
    )
    expect(screen.getByTestId('run-session')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd ui && npm test -- --run ActivityHistoryView 2>&1 | tail -10
```

Expected: FAIL — missing `workingDir` in fixture, and `RunSessionSubView` may not accept `activity` prop yet.

- [ ] **Step 3: Replace `ActivityHistoryView.tsx`**

Replace the full contents of `ui/src/components/automation/ActivityHistoryView.tsx`:

```typescript
import { useState } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { ActivityListItem } from './ActivityListItem'
import { RunSessionSubView } from './RunSessionSubView'

interface Props {
  specId: string
  activities: AutomationActivity[]
  onOpenRunSession?: (sessionId: string) => void
  activeRunSessionId?: string | null
  onCloseRunSession?: () => void
}

function dotColorClass(activity: AutomationActivity): string {
  const { status, reportOutcome } = activity
  if (status === 'running' || status === 'queued')
    return 'bg-primary animate-pulse'
  if (status === 'failed' || reportOutcome === 'error')
    return 'bg-red-500'
  if (status === 'waiting_user')
    return 'bg-yellow-500'
  if (status === 'cancelled')
    return 'bg-gray-400'
  if (status === 'completed') {
    if (reportOutcome === 'useful') return 'bg-green-500'
    return 'bg-gray-500'
  }
  return 'bg-gray-500'
}

export function ActivityHistoryView({
  specId: _specId,
  activities,
  onOpenRunSession,
  activeRunSessionId,
  onCloseRunSession,
}: Props) {
  const [archivedIds, setArchivedIds] = useState<Set<string>>(new Set())
  const [showArchived, setShowArchived] = useState(false)

  if (activeRunSessionId) {
    const activeActivity = activities.find((a) => a.sessionId === activeRunSessionId)
    const isRunning =
      activeActivity?.status === 'running' || activeActivity?.status === 'queued'
    return (
      <RunSessionSubView
        sessionId={activeRunSessionId}
        isRunning={isRunning}
        activity={activeActivity ?? null}
        onBack={() => onCloseRunSession?.()}
      />
    )
  }

  function handleArchived(sessionId: string) {
    setArchivedIds((prev) => new Set([...prev, sessionId]))
  }

  const visible = showArchived
    ? activities
    : activities.filter((a) => !a.sessionId || !archivedIds.has(a.sessionId))

  if (activities.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        还没有运行记录
      </div>
    )
  }

  return (
    <div className="flex flex-col flex-1 overflow-hidden">
      {archivedIds.size > 0 && (
        <div className="px-3 pt-2 shrink-0">
          <button
            onClick={() => setShowArchived((v) => !v)}
            className="titlebar-no-drag text-xs text-muted-foreground hover:text-foreground"
            aria-label={showArchived ? '隐藏已归档' : '显示已归档'}
          >
            {showArchived ? '隐藏已归档' : `显示已归档 (${archivedIds.size})`}
          </button>
        </div>
      )}

      {/* Timeline list */}
      <div className="flex-1 flex flex-col overflow-y-auto px-3 pt-3 pb-1">
        {visible.map((act, idx) => (
          <div key={act.id} className="flex gap-3">
            {/* Dot + connector */}
            <div className="flex flex-col items-center w-3 shrink-0 pt-1.5">
              <div className={`w-2.5 h-2.5 rounded-full shrink-0 ${dotColorClass(act)}`} />
              {idx < visible.length - 1 && (
                <div
                  className="w-px flex-1 bg-border/30 mt-1"
                  style={{ minHeight: '1.5rem' }}
                />
              )}
            </div>
            {/* Card */}
            <div className="flex-1 pb-3 group min-w-0">
              <ActivityListItem
                activity={act}
                onOpenRunSession={onOpenRunSession}
                onArchived={handleArchived}
              />
            </div>
          </div>
        ))}
        {visible.length === 0 && (
          <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
            所有记录已归档
          </div>
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run the tests**

```bash
cd ui && npm test -- --run ActivityHistoryView 2>&1 | tail -10
```

Expected: all 3 pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/automation/ActivityHistoryView.tsx ui/src/components/automation/ActivityHistoryView.test.tsx
git commit -m "feat(automation): timeline shell in ActivityHistoryView — dot+connector per run"
```

---

## Task 6: Collapsible report card in `RunSessionSubView`

**Files:**
- Modify: `ui/src/components/automation/RunSessionSubView.tsx`

**Context:** A collapsible card pinned above `AgentMessages` shows the run's `reportText` (Markdown), `reportOutcome` badge, and artifact chips. Reuses `ArtifactChip`, `OUTCOME_CONFIG`, and `ActivityMarkdown` from earlier tasks. Card is hidden entirely when `activity` is complete/failed and has no `reportText`.

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/automation/RunSessionSubView.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { RunSessionSubView } from './RunSessionSubView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  getAgentSessionMessages: vi.fn().mockResolvedValue([]),
  openFile: vi.fn(),
  openExternal: vi.fn(),
}))
vi.mock('@/components/agent/AgentMessages', () => ({
  AgentMessages: () => <div data-testid="agent-messages" />,
}))

const baseActivity: AutomationActivity = {
  id: 'act-1', specId: 'spec-1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status: 'completed', errorText: null,
  queuedAt: 1_700_000_000_000, startedAt: null, completedAt: null,
  durationMs: 0, llmIterations: 0, llmTokensIn: 0, llmTokensOut: 0,
  sessionId: 'sess-1', reportArtifactsJson: '[]',
  reportText: '**done**', reportOutcome: 'useful',
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
  workingDir: '/workdir',
}

describe('RunSessionSubView', () => {
  it('renders AgentMessages', async () => {
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={baseActivity} />
      )
    })
    expect(screen.getByTestId('agent-messages')).toBeTruthy()
  })

  it('shows report card when activity has reportText', async () => {
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={baseActivity} />
      )
    })
    expect(screen.getByText('运行报告')).toBeTruthy()
    expect(screen.getByText('有效')).toBeTruthy()
  })

  it('shows running placeholder when running and no reportText', async () => {
    const running = { ...baseActivity, status: 'running', reportText: null }
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" isRunning onBack={() => {}} activity={running} />
      )
    })
    expect(screen.getByText(/运行中，暂无报告/)).toBeTruthy()
  })

  it('hides report card when complete and no reportText', async () => {
    const noReport = { ...baseActivity, reportText: null }
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={noReport} />
      )
    })
    expect(screen.queryByText('运行报告')).toBeNull()
  })

  it('collapses report card on chevron click', async () => {
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={baseActivity} />
      )
    })
    // Before collapse: markdown content rendered
    expect(document.querySelector('strong')).toBeTruthy()
    // Click the header button to collapse
    fireEvent.click(screen.getByText('运行报告').closest('button')!)
    // After collapse: markdown content gone
    expect(document.querySelector('strong')).toBeNull()
  })
})
```

- [ ] **Step 2: Run to verify failure**

```bash
cd ui && npm test -- --run RunSessionSubView 2>&1 | tail -10
```

Expected: FAIL — `RunSessionSubView` doesn't accept `activity` prop yet.

- [ ] **Step 3: Replace `RunSessionSubView.tsx`**

Replace the full contents of `ui/src/components/automation/RunSessionSubView.tsx`:

```typescript
import { useEffect, useState, useMemo } from 'react'
import { getAgentSessionMessages } from '@/lib/tauri-bridge'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { AgentMessages } from '@/components/agent/AgentMessages'
import type { AgentMessage } from '@/lib/agent-types'
import { ActivityMarkdown } from './ActivityMarkdown'
import { ArtifactChip, OUTCOME_CONFIG } from './ActivityListItem'
import type { ReportArtifact } from './ActivityListItem'

interface Props {
  sessionId: string
  isRunning?: boolean
  activity?: AutomationActivity | null
  onBack: () => void
}

function ReportCard({ activity }: { activity: AutomationActivity }) {
  const [collapsed, setCollapsed] = useState(false)
  const isActive = activity.status === 'running' || activity.status === 'queued'

  const artifacts = useMemo<ReportArtifact[]>(() => {
    try { return JSON.parse(activity.reportArtifactsJson) as ReportArtifact[] }
    catch { return [] }
  }, [activity.reportArtifactsJson])

  if (!isActive && !activity.reportText) return null

  const outcomeCfg = activity.reportOutcome
    ? (OUTCOME_CONFIG[activity.reportOutcome] ?? null)
    : null

  return (
    <div className="mx-3 mt-3 mb-1 border border-border/50 rounded-lg overflow-hidden shrink-0">
      <button
        onClick={() => setCollapsed((v) => !v)}
        className="titlebar-no-drag w-full flex items-center gap-2 px-3 py-2 text-xs border-b border-border/40 bg-muted/30 hover:bg-muted/50 transition-colors"
      >
        <span className="font-medium text-foreground/80">运行报告</span>
        {outcomeCfg && (
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${outcomeCfg.className}`}>
            {outcomeCfg.label}
          </span>
        )}
        <span className="ml-auto text-muted-foreground">{collapsed ? '▸' : '▾'}</span>
      </button>
      {!collapsed && (
        <div className="px-3 py-2">
          {isActive && !activity.reportText ? (
            <p className="text-xs text-muted-foreground italic">运行中，暂无报告…</p>
          ) : (
            <>
              {activity.reportText && (
                <ActivityMarkdown content={activity.reportText} />
              )}
              {artifacts.length > 0 && (
                <div className="flex flex-wrap gap-1.5 mt-2">
                  {artifacts.map((a, i) => (
                    <ArtifactChip key={i} artifact={a} workingDir={activity.workingDir} />
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  )
}

export function RunSessionSubView({ sessionId, isRunning, activity, onBack }: Props) {
  const [messages, setMessages] = useState<AgentMessage[]>([])
  const [loaded, setLoaded] = useState(false)

  // Initial load
  useEffect(() => {
    setLoaded(false)
    getAgentSessionMessages(sessionId).then((msgs) => {
      setMessages(msgs as AgentMessage[])
      setLoaded(true)
    })
  }, [sessionId])

  // Poll while run is active so the transcript stays live.
  useEffect(() => {
    if (!isRunning) return
    const id = setInterval(() => {
      getAgentSessionMessages(sessionId).then((msgs) =>
        setMessages(msgs as AgentMessage[])
      )
    }, 2000)
    return () => clearInterval(id)
  }, [isRunning, sessionId])

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Breadcrumb */}
      <div className="flex items-center gap-1 px-3 py-2 border-b border-border/50 text-xs text-muted-foreground shrink-0">
        <button
          onClick={onBack}
          className="titlebar-no-drag text-primary hover:underline"
        >
          ← 动态
        </button>
        <span>/</span>
        <span>运行详情</span>
        {isRunning && (
          <span className="ml-auto flex items-center gap-1 text-primary">
            <span className="size-1.5 rounded-full bg-primary animate-pulse" />
            运行中
          </span>
        )}
      </div>

      {/* Report card (pinned above transcript) */}
      {activity && <ReportCard activity={activity} />}

      {/* Divider */}
      {activity && (activity.reportText || activity.status === 'running' || activity.status === 'queued') && (
        <div className="px-3 pt-2 pb-1 shrink-0">
          <p className="text-[10px] uppercase tracking-wider text-muted-foreground/50 font-semibold">
            对话过程
          </p>
        </div>
      )}

      {/* Transcript */}
      <div className="flex-1 overflow-hidden">
        <AgentMessages
          sessionId={sessionId}
          messages={messages}
          messagesLoaded={loaded}
          streaming={false}
        />
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run the tests**

```bash
cd ui && npm test -- --run RunSessionSubView 2>&1 | tail -10
```

Expected: all 5 pass.

- [ ] **Step 5: Run TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/automation/RunSessionSubView.tsx ui/src/components/automation/RunSessionSubView.test.tsx
git commit -m "feat(automation): collapsible report card in RunSessionSubView"
```

---

## Task 7: Fix FilePathChip dead-click in Kaleidoscope

**Files:**
- Modify: `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`

**Context:** `FilePathChip` dispatches `openPreviewAction` → sets `previewPanelOpenAtom`. `PreviewPanel` subscribes to that atom but is only mounted in `WorkspaceShell`. In Kaleidoscope nothing responds. Fix: mount `<PreviewPanel />` once in `KaleidoscopeShell`. Only one shell is visible at a time so there is no double-mount conflict.

- [ ] **Step 1: Add `PreviewPanel` import and render**

In `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx`, add the import after the existing imports:

```typescript
import { PreviewPanel } from '@/components/preview/PreviewPanel'
```

Then add `<PreviewPanel />` just before the closing `</div>` of the outer flex container (after the main-area div):

```typescript
  return (
    <div className="flex flex-1 min-w-0 min-h-0">
      <div data-tauri-drag-region className="titlebar-drag-region p-2 pr-0 shrink-0">
        <KaleidoscopeRail />
      </div>
      <div data-tauri-drag-region className="titlebar-drag-region relative flex-1 min-w-0 min-h-0 p-2">
        <div className="h-full rounded-2xl shadow-xl bg-content-area overflow-hidden relative">
          <AnimatePresence mode="wait">
            <motion.div
              key={moduleId}
              initial={{ opacity: 0, x: 12 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.08, ease: [0.32, 0.72, 0, 1] }}
              className="absolute inset-0"
            >
              {moduleId === 'humans' ? (
                <HumansModule />
              ) : moduleId === 'store' ? (
                <StoreModule />
              ) : moduleId === 'apps' ? (
                <AppsModule />
              ) : moduleId === 'memory' ? (
                <MemoryModule />
              ) : moduleId === 'skills' ? (
                <SkillsModule />
              ) : moduleId === 'integrations' ? (
                <IntegrationsModule />
              ) : (
                <ComingSoonModule moduleId={moduleId} />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </div>
      {/* PreviewPanel: responds to openPreviewAction dispatched by FilePathChip
          inside RunSessionSubView > AgentMessages. Not present in WorkspaceShell
          when Kaleidoscope is active, so no double-mount. */}
      <PreviewPanel />
    </div>
  )
```

- [ ] **Step 2: Run TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 3: Run the full test suite**

```bash
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: 119 test files pass, 0 new failures.

- [ ] **Step 4: Commit**

```bash
git add ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx
git commit -m "fix(kaleidoscope): mount PreviewPanel so FilePathChip clicks open file preview"
```

---

## Final verification checklist

After all 7 tasks are committed:

```bash
# Full TS check
cd ui && npx tsc --noEmit

# Full test suite
cd ui && npm test -- --run 2>&1 | tail -5

# Rust build
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5

# Rust tests
cd src-tauri && cargo test --lib activity 2>&1 | tail -10
```

Manual smoke test (requires app running via `cargo tauri dev`):
1. Open AutomationHub → select smoke-2a spec → 动态 tab → verify timeline nodes with dot colors.
2. Click "运行" → new node appears with blue pulsing dot and "运行中，暂无报告…" placeholder.
3. After run completes → green dot, "有效" badge, `reportText` rendered as Markdown.
4. If artifacts present → chips appear; file chip opens system app, url chip opens browser.
5. Click "查看进程 ›" → RunSessionSubView shows report card collapsed/expanded correctly.
6. In RunSessionSubView, click a `FilePathChip` in AgentMessages → file preview panel opens (no longer dead).
