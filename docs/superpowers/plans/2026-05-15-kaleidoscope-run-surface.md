# Kaleidoscope Automation Run Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed automation run sessions directly in the Kaleidoscope page — three-column layout (rail | spec list | run surface) with three-tab run surface (聊天 / 动态 / 设置), D2 sub-view for run-session inspection, S1 state persistence, and W2 useOpenSession routing that fixes the Phase 2a invisible-tab bug.

**Architecture:** The existing `AutomationHub` is refactored into a SpecList sidebar + SpecRunSurface main area. Run surface has three tabs rendered by new focused components; `AgentMessages` is reused read-only for both the home-thread chat and the D2 run-session sub-view. Three Jotai atoms track selected spec, active tab, and active D2 session — all module-level, persisting across Kaleidoscope ↔ other-module switches.

**Tech Stack:** React 18, TypeScript, Jotai, Tauri IPC (`invoke`), Vitest + React Testing Library, Rust/rusqlite (one new Tauri command), Tailwind + existing theme tokens.

**Spec:** `docs/superpowers/specs/2026-05-15-kaleidoscope-run-surface-design.md`

---

## File Map

### New files
| Path | Responsibility |
|------|---------------|
| `ui/src/atoms/automation-ui.ts` | S1 navigation atoms: selectedSpecId, activeTab, D2 runSessionId |
| `ui/src/atoms/automation-ui.test.ts` | Atom default-value tests |
| `ui/src/components/automation/SpecListItem.tsx` | Single spec row: name / status dot / hover Run button |
| `ui/src/components/automation/SpecList.tsx` | 240px sidebar, lists specs from humaneSpecsAtom |
| `ui/src/components/automation/ActivityListItem.tsx` | One run record: timestamp / status badge / summary / 查看进程 > |
| `ui/src/components/automation/ActivityHistoryView.tsx` | 动态 tab: ActivityList + D2 RunSessionSubView router |
| `ui/src/components/automation/RunSessionSubView.tsx` | D2 sub-view: breadcrumb + AgentMessages (read-only) |
| `ui/src/components/automation/AutomationRightPanel.tsx` | Lightweight files + trajectory tabs (no appMode dependency) |
| `ui/src/components/automation/HomeThreadView.tsx` | 聊天 tab: home-thread messages + composer |
| `ui/src/components/automation/SpecSettingsView.tsx` | 设置 tab: spec info + enabled toggle + permissions + YAML |
| `ui/src/components/automation/SpecRunHeader.tsx` | Header bar: spec name + ▶ 运行 + titlebar-drag-region |
| `ui/src/components/automation/SpecRunSurface.tsx` | Container: SpecRunHeader + TabBar + TabContent |
| `ui/src/components/automation/ActivityHistoryView.test.tsx` | Tests for ActivityHistoryView |
| `ui/src/components/automation/SpecList.test.tsx` | Tests for SpecList |

### Modified files
| Path | Change |
|------|--------|
| `ui/src/components/automation/AutomationHub.tsx` | Thin shell: renders SpecList + SpecRunSurface side-by-side |
| `ui/src/hooks/useOpenSession.ts` | W2: automation sessions route to Kaleidoscope, not workspace |
| `ui/src/lib/tauri-bridge.ts` | Add `getOrCreateSpecHomeThread` |
| `src-tauri/src/tauri_commands.rs` | Add `get_or_create_spec_home_thread` command |
| `src-tauri/src/main.rs` | Register new command in invoke_handler! |

---

## Task 1: Rust command — `get_or_create_spec_home_thread`

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write the failing Rust unit test**

Append this test module at the end of `src-tauri/src/tauri_commands.rs`:

```rust
#[cfg(test)]
mod home_thread_tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::migrations::run_migrations;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn home_thread_creates_session_and_is_idempotent() {
        use crate::automation::runtime::run_session::ensure_automations_space;
        let conn = test_conn();
        ensure_automations_space(&conn).unwrap();

        // Insert a minimal spec row so FK works
        conn.execute(
            "INSERT INTO automation_specs (id, name, version, author, description,
             system_prompt, spec_format, spec_yaml, spec_json, created_at, updated_at)
             VALUES ('spec1','Test','1.0','a','d','s','humane-yaml-v1','y','{}',0,0)",
            [],
        ).unwrap();

        // First call: creates session
        let id1 = create_home_thread_session(&conn, "spec1").unwrap();
        assert!(!id1.is_empty());

        // Second call: returns same session
        let id2 = create_home_thread_session(&conn, "spec1").unwrap();
        assert_eq!(id1, id2);
    }

    fn create_home_thread_session(conn: &Connection, spec_id: &str) -> rusqlite::Result<String> {
        use crate::automation::runtime::run_session::resolve_home_space;

        let space_id = resolve_home_space(conn, spec_id)?;

        let existing: Option<String> = conn.query_row(
            "SELECT id FROM agent_sessions
             WHERE json_extract(metadata_json, '$.spec_id') = ?1
               AND json_extract(metadata_json, '$.origin') = 'automation:home_thread'
             LIMIT 1",
            rusqlite::params![spec_id],
            |r| r.get(0),
        ).optional()?;

        if let Some(id) = existing {
            return Ok(id);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let meta = serde_json::json!({ "spec_id": spec_id, "origin": "automation:home_thread" });
        conn.execute(
            "INSERT INTO agent_sessions
             (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
             VALUES (?1,?2,?3,?4,0,0,0,?5,?5)",
            rusqlite::params![&id, &space_id, "Home thread", meta.to_string(), now],
        )?;
        Ok(id)
    }
}
```

- [ ] **Step 2: Run test to verify it fails (function not yet defined as a Tauri command)**

```bash
cd src-tauri && cargo test home_thread_tests 2>&1 | grep -E "^error|FAILED|ok"
```

Expected: compiles and PASSES (the helper function is defined in the test module). The Tauri command itself doesn't exist yet — that's in the next step.

- [ ] **Step 3: Add the Tauri command to `tauri_commands.rs`**

Find the block of automation commands (search for `pub async fn get_automation_activity`). Add this command after it:

```rust
#[tauri::command]
pub async fn get_or_create_spec_home_thread(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<serde_json::Value, Error> {
    use crate::automation::runtime::run_session::{ensure_automations_space, resolve_home_space};

    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;

    ensure_automations_space(&conn)
        .map_err(|e| Error::Internal(format!("ensure automations space: {e}")))?;

    let space_id = resolve_home_space(&conn, &spec_id)
        .map_err(|e| Error::Internal(format!("resolve home space: {e}")))?;

    // Try to find existing home-thread session
    let existing: Option<(String, String, i64, i64, i64)> = conn.query_row(
        "SELECT id, title, message_count, created_at, updated_at
         FROM agent_sessions
         WHERE json_extract(metadata_json, '$.spec_id') = ?1
           AND json_extract(metadata_json, '$.origin') = 'automation:home_thread'
         LIMIT 1",
        rusqlite::params![&spec_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    ).optional()
        .map_err(|e| Error::Database(e))?;

    if let Some((id, title, msg_count, created_at, updated_at)) = existing {
        return Ok(serde_json::json!({
            "id": id,
            "workspaceId": space_id,
            "title": title,
            "messageCount": msg_count,
            "pinned": false,
            "archived": false,
            "createdAt": created_at,
            "updatedAt": updated_at,
        }));
    }

    // Create new home-thread session
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let meta = serde_json::json!({
        "spec_id": &spec_id,
        "origin": "automation:home_thread"
    });

    conn.execute(
        "INSERT INTO agent_sessions
         (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES (?1,?2,'Home thread',?3,0,0,0,?4,?4)",
        rusqlite::params![&id, &space_id, meta.to_string(), now],
    ).map_err(|e| Error::Database(e))?;

    Ok(serde_json::json!({
        "id": id,
        "workspaceId": space_id,
        "title": "Home thread",
        "messageCount": 0,
        "pinned": false,
        "archived": false,
        "createdAt": now,
        "updatedAt": now,
    }))
}
```

- [ ] **Step 4: Register command in `main.rs`**

In `main.rs`, find the `invoke_handler!` macro call (search for `get_automation_activity`). Add `get_or_create_spec_home_thread` to the list:

```rust
// Before (find this pattern):
get_automation_activity,
// After (add the new command immediately after):
get_automation_activity,
get_or_create_spec_home_thread,
```

- [ ] **Step 5: Verify compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output (clean compile).

- [ ] **Step 6: Run the unit test**

```bash
cd src-tauri && cargo test home_thread_tests 2>&1 | tail -5
```

Expected: `test home_thread_tests::home_thread_creates_session_and_is_idempotent ... ok`

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(automation): add get_or_create_spec_home_thread Tauri command"
```

---

## Task 2: TypeScript bridge + UI navigation atoms

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts`
- Create: `ui/src/atoms/automation-ui.ts`
- Create: `ui/src/atoms/automation-ui.test.ts`

- [ ] **Step 1: Add bridge function to `ui/src/lib/tauri-bridge.ts`**

Find the `getAgentSessionMessages` function (around line 952). Add after it:

```ts
export interface HomeThreadSession {
  id: string
  workspaceId: string
  title: string
  messageCount: number
  pinned: boolean
  archived: boolean
  createdAt: number
  updatedAt: number
}

export const getOrCreateSpecHomeThread = (specId: string): Promise<HomeThreadSession> =>
  invoke<HomeThreadSession>('get_or_create_spec_home_thread', { specId })
```

- [ ] **Step 2: Write failing test for automation-ui atoms**

Create `ui/src/atoms/automation-ui.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  automationSelectedSpecIdAtom,
  automationActiveTabAtom,
  automationActivityRunSessionIdAtom,
} from './automation-ui'

describe('automation-ui atoms', () => {
  it('automationSelectedSpecIdAtom defaults to null', () => {
    const store = createStore()
    expect(store.get(automationSelectedSpecIdAtom)).toBeNull()
  })

  it('automationActiveTabAtom defaults to activity', () => {
    const store = createStore()
    expect(store.get(automationActiveTabAtom)).toBe('activity')
  })

  it('automationActivityRunSessionIdAtom defaults to null', () => {
    const store = createStore()
    expect(store.get(automationActivityRunSessionIdAtom)).toBeNull()
  })

  it('atoms are writable and independent', () => {
    const store = createStore()
    store.set(automationSelectedSpecIdAtom, 'spec-123')
    store.set(automationActiveTabAtom, 'chat')
    store.set(automationActivityRunSessionIdAtom, 'session-abc')

    expect(store.get(automationSelectedSpecIdAtom)).toBe('spec-123')
    expect(store.get(automationActiveTabAtom)).toBe('chat')
    expect(store.get(automationActivityRunSessionIdAtom)).toBe('session-abc')
  })
})
```

- [ ] **Step 3: Run test to verify it fails**

```bash
cd ui && npm test -- --run automation-ui 2>&1 | tail -5
```

Expected: FAIL — `Cannot find module './automation-ui'`

- [ ] **Step 4: Create `ui/src/atoms/automation-ui.ts`**

```ts
import { atom } from 'jotai'

export type AutomationTab = 'chat' | 'activity' | 'settings'

// Which spec is selected in SpecList (persists across module switches)
export const automationSelectedSpecIdAtom = atom<string | null>(null)

// Which tab is active in SpecRunSurface
export const automationActiveTabAtom = atom<AutomationTab>('activity')

// D2 sub-view: non-null while viewing a run-session inside the 动态 tab
export const automationActivityRunSessionIdAtom = atom<string | null>(null)
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd ui && npm test -- --run automation-ui 2>&1 | tail -5
```

Expected: `4 passed`

- [ ] **Step 6: TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no output.

- [ ] **Step 7: Commit**

```bash
git add ui/src/lib/tauri-bridge.ts ui/src/atoms/automation-ui.ts ui/src/atoms/automation-ui.test.ts
git commit -m "feat(automation): add home-thread bridge + navigation atoms"
```

---

## Task 3: SpecListItem + SpecList

**Files:**
- Create: `ui/src/components/automation/SpecListItem.tsx`
- Create: `ui/src/components/automation/SpecList.tsx`
- Create: `ui/src/components/automation/SpecList.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `ui/src/components/automation/SpecList.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderWithProviders } from '@/test-utils/render'
import { SpecList } from './SpecList'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'

const makeSpec = (overrides: Partial<HumaneSpecRow> = {}): HumaneSpecRow => ({
  id: 'spec-1',
  name: 'Daily Report',
  version: '1.0',
  author: 'test',
  description: 'desc',
  systemPrompt: '',
  specFormat: 'humane-yaml-v1',
  specYaml: '',
  specJson: '{}',
  userConfigValues: '{}',
  permissionsGranted: '[]',
  permissionsDenied: '[]',
  status: 'active',
  enabled: true,
  spaceId: null,
  source: 'local',
  sourceRef: null,
  sourceVersion: null,
  createdAt: 0,
  updatedAt: 0,
  lastRunAt: null,
  lastRunOutcome: null,
  ...overrides,
})

describe('SpecList', () => {
  it('renders spec names', () => {
    const specs = [makeSpec({ id: 's1', name: 'Daily Report' }), makeSpec({ id: 's2', name: 'Weekly Summary' })]
    renderWithProviders(<SpecList specs={specs} />)
    expect(screen.getByText('Daily Report')).toBeInTheDocument()
    expect(screen.getByText('Weekly Summary')).toBeInTheDocument()
  })

  it('calls onSelect when a spec is clicked', async () => {
    const onSelect = vi.fn()
    const specs = [makeSpec({ id: 'spec-1', name: 'Daily Report' })]
    renderWithProviders(<SpecList specs={specs} onSelect={onSelect} />)
    await userEvent.click(screen.getByText('Daily Report'))
    expect(onSelect).toHaveBeenCalledWith('spec-1')
  })

  it('highlights the selected spec', () => {
    const specs = [makeSpec({ id: 'spec-1', name: 'Daily Report' })]
    renderWithProviders(<SpecList specs={specs} selectedSpecId="spec-1" />)
    // The selected item should have a highlighted border
    const item = screen.getByRole('button', { name: /Daily Report/i })
    expect(item.className).toMatch(/border-primary|border-blue/)
  })

  it('shows empty state when no specs', () => {
    renderWithProviders(<SpecList specs={[]} />)
    expect(screen.getByText(/没有数字人/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run to verify failure**

```bash
cd ui && npm test -- --run SpecList 2>&1 | tail -5
```

Expected: FAIL — `Cannot find module './SpecList'`

- [ ] **Step 3: Create `ui/src/components/automation/SpecListItem.tsx`**

```tsx
import type { HumaneSpecRow } from '@/lib/tauri-bridge'

interface Props {
  spec: HumaneSpecRow
  isSelected: boolean
  onSelect: () => void
  onRun: () => void
}

const STATUS_DOT: Record<string, string> = {
  active: 'bg-green-500',
  paused: 'bg-yellow-500',
  error: 'bg-red-500',
}

export function SpecListItem({ spec, isSelected, onSelect, onRun }: Props) {
  return (
    <button
      onClick={onSelect}
      className={[
        'group w-full text-left px-3 py-2 rounded-lg border transition-colors',
        'hover:bg-accent/50',
        isSelected
          ? 'border-primary bg-primary/5'
          : 'border-transparent',
      ].join(' ')}
    >
      <div className="flex items-center gap-2">
        <span
          className={[
            'h-2 w-2 rounded-full shrink-0',
            STATUS_DOT[spec.status] ?? 'bg-muted-foreground',
          ].join(' ')}
        />
        <span className="flex-1 truncate text-sm font-medium">{spec.name}</span>
        <button
          onClick={(e) => { e.stopPropagation(); onRun() }}
          className="titlebar-no-drag hidden group-hover:flex items-center gap-1 px-2 py-0.5 rounded text-xs bg-primary text-primary-foreground"
        >
          ▶
        </button>
      </div>
    </button>
  )
}
```

- [ ] **Step 4: Create `ui/src/components/automation/SpecList.tsx`**

```tsx
import { useAtomValue } from 'jotai'
import { humaneSpecsAtom } from '@/atoms/automation'
import { SpecListItem } from './SpecListItem'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'

interface Props {
  specs?: HumaneSpecRow[]           // if omitted, reads from humaneSpecsAtom
  selectedSpecId?: string | null
  onSelect?: (specId: string) => void
  onRun?: (specId: string) => void
}

export function SpecList({ specs: propSpecs, selectedSpecId, onSelect, onRun }: Props) {
  const atomSpecs = useAtomValue(humaneSpecsAtom)
  const specs = propSpecs ?? atomSpecs

  if (specs.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center p-4 text-sm text-muted-foreground">
        没有数字人
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-1 p-2 overflow-y-auto">
      {specs.map((spec) => (
        <SpecListItem
          key={spec.id}
          spec={spec}
          isSelected={spec.id === selectedSpecId}
          onSelect={() => onSelect?.(spec.id)}
          onRun={() => onRun?.(spec.id)}
        />
      ))}
    </div>
  )
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd ui && npm test -- --run SpecList 2>&1 | tail -5
```

Expected: `4 passed`

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/automation/SpecListItem.tsx ui/src/components/automation/SpecList.tsx ui/src/components/automation/SpecList.test.tsx
git commit -m "feat(automation): SpecListItem + SpecList sidebar components"
```

---

## Task 4: ActivityListItem + ActivityHistoryView

**Files:**
- Create: `ui/src/components/automation/ActivityListItem.tsx`
- Create: `ui/src/components/automation/ActivityHistoryView.tsx`
- Create: `ui/src/components/automation/ActivityHistoryView.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `ui/src/components/automation/ActivityHistoryView.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderWithProviders } from '@/test-utils/render'
import { ActivityHistoryView } from './ActivityHistoryView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

const makeActivity = (overrides: Partial<AutomationActivity> = {}): AutomationActivity => ({
  id: 'act-1',
  specId: 'spec-1',
  subscriptionId: null,
  triggerSourceType: 'schedule',
  triggerPayloadJson: '{}',
  status: 'completed',
  errorText: null,
  queuedAt: 1715741400000,
  startedAt: 1715741400000,
  completedAt: 1715741412400,
  durationMs: 12400,
  llmIterations: 3,
  llmTokensIn: 0,
  llmTokensOut: 0,
  sessionId: 'session-run-1',
  reportArtifactsJson: '[]',
  reportText: '3 指标正常，1 待确认',
  reportOutcome: 'completed',
  escalationId: null,
  resumedFromActivityId: null,
  resumedFromEscalationId: null,
  ...overrides,
})

describe('ActivityHistoryView', () => {
  it('renders activity report text', () => {
    renderWithProviders(
      <ActivityHistoryView specId="spec-1" activities={[makeActivity()]} />
    )
    expect(screen.getByText('3 指标正常，1 待确认')).toBeInTheDocument()
  })

  it('renders 查看进程 button for activity with sessionId', () => {
    renderWithProviders(
      <ActivityHistoryView specId="spec-1" activities={[makeActivity({ sessionId: 'run-sess' })]} />
    )
    expect(screen.getByRole('button', { name: /查看进程/i })).toBeInTheDocument()
  })

  it('calls onOpenRunSession when 查看进程 is clicked', async () => {
    const onOpen = vi.fn()
    renderWithProviders(
      <ActivityHistoryView
        specId="spec-1"
        activities={[makeActivity({ sessionId: 'run-sess' })]}
        onOpenRunSession={onOpen}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /查看进程/i }))
    expect(onOpen).toHaveBeenCalledWith('run-sess')
  })

  it('shows empty state when no activities', () => {
    renderWithProviders(<ActivityHistoryView specId="spec-1" activities={[]} />)
    expect(screen.getByText(/还没有运行记录/i)).toBeInTheDocument()
  })

  it('highlights escalation status', () => {
    const act = makeActivity({ status: 'waiting_user', escalationId: 'esc-1' })
    renderWithProviders(<ActivityHistoryView specId="spec-1" activities={[act]} />)
    // Escalation row should have orange/warning color
    const row = screen.getByTestId('activity-row-act-1')
    expect(row.className).toMatch(/border-orange|border-amber|ring-orange/)
  })
})
```

- [ ] **Step 2: Run to verify failure**

```bash
cd ui && npm test -- --run ActivityHistoryView 2>&1 | tail -5
```

Expected: FAIL — `Cannot find module './ActivityHistoryView'`

- [ ] **Step 3: Create `ui/src/components/automation/ActivityListItem.tsx`**

```tsx
import type { AutomationActivity } from '@/lib/tauri-bridge'

interface Props {
  activity: AutomationActivity
  onOpenRunSession?: (sessionId: string) => void
}

const STATUS_CONFIG: Record<string, { label: string; className: string }> = {
  completed: { label: '已完成', className: 'text-green-600' },
  failed: { label: '失败', className: 'text-red-600' },
  cancelled: { label: '已取消', className: 'text-muted-foreground' },
  filtered_out: { label: '已跳过', className: 'text-muted-foreground' },
  waiting_user: { label: '待确认', className: 'text-orange-500' },
  running: { label: '运行中', className: 'text-blue-500' },
  queued: { label: '排队中', className: 'text-muted-foreground' },
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

export function ActivityListItem({ activity, onOpenRunSession }: Props) {
  const cfg = STATUS_CONFIG[activity.status] ?? { label: activity.status, className: 'text-muted-foreground' }
  const isEscalation = activity.status === 'waiting_user'

  return (
    <div
      data-testid={`activity-row-${activity.id}`}
      className={[
        'rounded-lg border p-3 bg-background',
        isEscalation ? 'border-orange-400 ring-1 ring-orange-200' : 'border-border/50',
      ].join(' ')}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>{formatTs(activity.startedAt ?? activity.queuedAt)}</span>
          <span className={cfg.className}>{cfg.label}</span>
          {activity.durationMs > 0 && (
            <span>{formatDuration(activity.durationMs)}</span>
          )}
        </div>
        {activity.sessionId && onOpenRunSession && (
          <button
            onClick={() => onOpenRunSession(activity.sessionId!)}
            className="titlebar-no-drag text-xs text-primary hover:underline shrink-0"
          >
            查看进程 &gt;
          </button>
        )}
      </div>
      {activity.reportText && (
        <p className="mt-1 text-sm text-foreground line-clamp-3">{activity.reportText}</p>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Create `ui/src/components/automation/ActivityHistoryView.tsx`**

```tsx
import { useState } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { ActivityListItem } from './ActivityListItem'
import { RunSessionSubView } from './RunSessionSubView'

interface Props {
  specId: string
  activities: AutomationActivity[]
  onOpenRunSession?: (sessionId: string) => void
  // activeRunSessionId + onCloseRunSession used when rendered inside SpecRunSurface
  activeRunSessionId?: string | null
  onCloseRunSession?: () => void
}

export function ActivityHistoryView({
  specId,
  activities,
  onOpenRunSession,
  activeRunSessionId,
  onCloseRunSession,
}: Props) {
  // D2: if activeRunSessionId is set, show RunSessionSubView instead of list
  if (activeRunSessionId) {
    return (
      <RunSessionSubView
        sessionId={activeRunSessionId}
        onBack={() => onCloseRunSession?.()}
      />
    )
  }

  if (activities.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        还没有运行记录
      </div>
    )
  }

  return (
    <div className="flex-1 flex flex-col gap-2 p-3 overflow-y-auto">
      {activities.map((act) => (
        <ActivityListItem
          key={act.id}
          activity={act}
          onOpenRunSession={onOpenRunSession}
        />
      ))}
    </div>
  )
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd ui && npm test -- --run ActivityHistoryView 2>&1 | tail -5
```

Expected: `5 passed`

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/automation/ActivityListItem.tsx ui/src/components/automation/ActivityHistoryView.tsx ui/src/components/automation/ActivityHistoryView.test.tsx
git commit -m "feat(automation): ActivityListItem + ActivityHistoryView (动态 tab)"
```

---

## Task 5: RunSessionSubView (D2)

**Files:**
- Create: `ui/src/components/automation/RunSessionSubView.tsx`

- [ ] **Step 1: Create `ui/src/components/automation/RunSessionSubView.tsx`**

```tsx
import { useEffect, useState } from 'react'
import { getAgentSessionMessages } from '@/lib/tauri-bridge'
import AgentMessages from '@/components/agent/AgentMessages'
import type { AgentMessage } from '@/lib/agent-types'

interface Props {
  sessionId: string
  onBack: () => void
}

export function RunSessionSubView({ sessionId, onBack }: Props) {
  const [messages, setMessages] = useState<AgentMessage[]>([])
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    setLoaded(false)
    getAgentSessionMessages(sessionId).then((msgs) => {
      setMessages(msgs as AgentMessage[])
      setLoaded(true)
    })
  }, [sessionId])

  return (
    <div className="flex flex-col h-full">
      {/* breadcrumb */}
      <div className="flex items-center gap-1 px-3 py-2 border-b border-border/50 text-xs text-muted-foreground shrink-0">
        <button
          onClick={onBack}
          className="titlebar-no-drag text-primary hover:underline"
        >
          ← 动态
        </button>
        <span>/</span>
        <span>运行详情</span>
      </div>

      {/* transcript */}
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

- [ ] **Step 2: Verify compile**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/automation/RunSessionSubView.tsx
git commit -m "feat(automation): RunSessionSubView — D2 breadcrumb + read-only transcript"
```

---

## Task 6: AutomationRightPanel

**Files:**
- Create: `ui/src/components/automation/AutomationRightPanel.tsx`

`RightSidePanel` returns null when `appMode !== 'agent'`, making it unusable from Kaleidoscope. This creates a lightweight standalone replacement with only files + trajectory.

- [ ] **Step 1: Create `ui/src/components/automation/AutomationRightPanel.tsx`**

```tsx
import { useState } from 'react'
import { WorkspaceFilesView } from '@/components/agent/WorkspaceFilesView'
import { TrajectoryReel } from '@/components/agent/TrajectoryReel'

type Tab = 'files' | 'trajectory'

interface Props {
  sessionId: string
  sessionPath: string | null
}

export function AutomationRightPanel({ sessionId, sessionPath }: Props) {
  const [tab, setTab] = useState<Tab>('files')

  return (
    <div className="w-[380px] shrink-0 flex flex-col h-full border-l border-border/50 bg-background">
      {/* tab bar */}
      <div className="flex gap-0 border-b border-border/50 px-2 pt-2 shrink-0">
        {(['files', 'trajectory'] as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={[
              'titlebar-no-drag px-3 py-1.5 text-xs rounded-t border-b-2 transition-colors',
              tab === t
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground',
            ].join(' ')}
          >
            {t === 'files' ? '文件' : '轨迹'}
          </button>
        ))}
      </div>

      {/* content */}
      <div className="flex-1 overflow-hidden">
        {tab === 'files' && (
          <WorkspaceFilesView sessionId={sessionId} sessionPath={sessionPath} />
        )}
        {tab === 'trajectory' && (
          <TrajectoryReel sessionId={sessionId} />
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Verify compile**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors. If `WorkspaceFilesView` or `TrajectoryReel` import paths differ, check `ui/src/components/agent/` with `ls` and adjust.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/automation/AutomationRightPanel.tsx
git commit -m "feat(automation): AutomationRightPanel — lightweight files + trajectory panel"
```

---

## Task 7: HomeThreadView (聊天 tab)

**Files:**
- Create: `ui/src/components/automation/HomeThreadView.tsx`

- [ ] **Step 1: Create `ui/src/components/automation/HomeThreadView.tsx`**

```tsx
import { useEffect, useRef, useState } from 'react'
import { getOrCreateSpecHomeThread, getAgentSessionMessages, sendAgentMessage } from '@/lib/tauri-bridge'
import AgentMessages from '@/components/agent/AgentMessages'
import type { AgentMessage } from '@/lib/agent-types'

interface Props {
  specId: string
  showRightPanel: boolean
  onToggleRightPanel: () => void
}

export function HomeThreadView({ specId, showRightPanel, onToggleRightPanel }: Props) {
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [messages, setMessages] = useState<AgentMessage[]>([])
  const [loaded, setLoaded] = useState(false)
  const [input, setInput] = useState('')
  const [sending, setSending] = useState(false)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    setLoaded(false)
    getOrCreateSpecHomeThread(specId).then((session) => {
      setSessionId(session.id)
      return getAgentSessionMessages(session.id)
    }).then((msgs) => {
      setMessages(msgs as AgentMessage[])
      setLoaded(true)
    })
  }, [specId])

  async function handleSend() {
    if (!sessionId || !input.trim() || sending) return
    const text = input.trim()
    setInput('')
    setSending(true)
    try {
      await sendAgentMessage({ sessionId, userMessage: text })
      // Reload messages after send
      const updated = await getAgentSessionMessages(sessionId)
      setMessages(updated as AgentMessage[])
    } finally {
      setSending(false)
      inputRef.current?.focus()
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  if (!sessionId) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        加载中…
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-hidden">
        <AgentMessages
          sessionId={sessionId}
          messages={messages}
          messagesLoaded={loaded}
          streaming={false}
        />
      </div>

      {/* composer */}
      <div className="shrink-0 border-t border-border/50 p-2">
        <div className="flex gap-2 items-end rounded-lg border border-border bg-background px-3 py-2">
          <textarea
            ref={inputRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="发消息…"
            rows={1}
            disabled={sending}
            className="flex-1 resize-none bg-transparent text-sm outline-none min-h-[24px] max-h-[120px] disabled:opacity-50"
          />
          <button
            onClick={handleSend}
            disabled={!input.trim() || sending}
            className="titlebar-no-drag shrink-0 text-primary disabled:opacity-40 text-sm"
          >
            ➤
          </button>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Verify compile**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/automation/HomeThreadView.tsx
git commit -m "feat(automation): HomeThreadView — home-thread chat + send composer"
```

---

## Task 8: SpecSettingsView (设置 tab)

**Files:**
- Create: `ui/src/components/automation/SpecSettingsView.tsx`

- [ ] **Step 1: Create `ui/src/components/automation/SpecSettingsView.tsx`**

```tsx
import { useState } from 'react'
import { setAutomationEnabled } from '@/lib/tauri-bridge'
import type { HumaneSpecRow } from '@/lib/tauri-bridge'

interface Props {
  spec: HumaneSpecRow
  onSpecChange: (updated: HumaneSpecRow) => void
}

export function SpecSettingsView({ spec, onSpecChange }: Props) {
  const [view, setView] = useState<'settings' | 'yaml'>('settings')
  const [saving, setSaving] = useState(false)

  async function handleToggleEnabled() {
    setSaving(true)
    try {
      await setAutomationEnabled(spec.id, !spec.enabled)
      onSpecChange({ ...spec, enabled: !spec.enabled })
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex flex-col h-full overflow-y-auto">
      {/* header */}
      <div className="flex items-center gap-2 p-4 border-b border-border/50">
        <div className="flex-1">
          <div className="font-semibold text-sm">{spec.name}</div>
          <div className="text-xs text-muted-foreground">
            v{spec.version} · {spec.author}
          </div>
        </div>
        {/* view toggle */}
        <div className="flex rounded-lg border border-border overflow-hidden text-xs">
          {(['settings', 'yaml'] as const).map((v) => (
            <button
              key={v}
              onClick={() => setView(v)}
              className={[
                'titlebar-no-drag px-3 py-1',
                view === v ? 'bg-muted text-foreground' : 'text-muted-foreground hover:bg-muted/50',
              ].join(' ')}
            >
              {v === 'settings' ? '⚙ 设置' : '<> YAML'}
            </button>
          ))}
        </div>
      </div>

      {view === 'yaml' ? (
        <pre className="flex-1 p-4 text-xs font-mono overflow-auto whitespace-pre-wrap text-muted-foreground">
          {spec.specYaml}
        </pre>
      ) : (
        <div className="flex flex-col gap-6 p-4">
          {/* enabled */}
          <Section title="状态">
            <Row label="启用" description="允许定时任务自动触发">
              <Toggle
                checked={spec.enabled}
                disabled={saving}
                onChange={handleToggleEnabled}
              />
            </Row>
          </Section>

          {/* permissions */}
          <Section title="权限">
            {(['AI 浏览器', '电子邮件', 'IM 推送'] as const).map((p) => (
              <Row key={p} label={p} description="">
                <span className="text-xs text-muted-foreground">
                  {spec.permissionsGranted.includes(p) ? '已授权' : '未授权'}
                </span>
              </Row>
            ))}
          </Section>

          {/* info */}
          <Section title="关于">
            <p className="text-xs text-muted-foreground">{spec.description}</p>
            <p className="text-xs text-muted-foreground mt-1">来源：{spec.source}</p>
          </Section>
        </div>
      )}
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">{title}</h3>
      <div className="flex flex-col gap-3">{children}</div>
    </div>
  )
}

function Row({ label, description, children }: { label: string; description: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="text-sm">{label}</div>
        {description && <div className="text-xs text-muted-foreground">{description}</div>}
      </div>
      {children}
    </div>
  )
}

function Toggle({ checked, disabled, onChange }: { checked: boolean; disabled: boolean; onChange: () => void }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={onChange}
      className={[
        'titlebar-no-drag relative w-10 h-5 rounded-full transition-colors',
        checked ? 'bg-primary' : 'bg-muted',
        disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer',
      ].join(' ')}
    >
      <span
        className={[
          'absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform',
          checked ? 'translate-x-5' : 'translate-x-0',
        ].join(' ')}
      />
    </button>
  )
}
```

- [ ] **Step 2: Verify compile**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/automation/SpecSettingsView.tsx
git commit -m "feat(automation): SpecSettingsView — spec info + enabled toggle + YAML view"
```

---

## Task 9: SpecRunHeader + SpecRunSurface (assembly)

**Files:**
- Create: `ui/src/components/automation/SpecRunHeader.tsx`
- Create: `ui/src/components/automation/SpecRunSurface.tsx`

- [ ] **Step 1: Create `ui/src/components/automation/SpecRunHeader.tsx`**

```tsx
interface Props {
  specName: string
  onRun: () => void
  isRunning: boolean
}

export function SpecRunHeader({ specName, onRun, isRunning }: Props) {
  return (
    // titlebar-drag-region on the header bar itself (does NOT cascade through the card)
    <div className="titlebar-drag-region flex items-center justify-between px-3 py-2 border-b border-border/50 shrink-0">
      <span className="font-semibold text-sm truncate">{specName}</span>
      <button
        onClick={onRun}
        disabled={isRunning}
        className="titlebar-no-drag flex items-center gap-1 px-3 py-1 rounded-md bg-primary text-primary-foreground text-xs disabled:opacity-60"
      >
        {isRunning ? '运行中…' : '▶ 运行'}
      </button>
    </div>
  )
}
```

- [ ] **Step 2: Create `ui/src/components/automation/SpecRunSurface.tsx`**

```tsx
import { useState } from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import {
  automationActiveTabAtom,
  automationActivityRunSessionIdAtom,
  type AutomationTab,
} from '@/atoms/automation-ui'
import { automationActivitiesAtom, humaneSpecsAtom } from '@/atoms/automation'
import { triggerAutomationManualHumane } from '@/lib/tauri-bridge'
import { SpecRunHeader } from './SpecRunHeader'
import { HomeThreadView } from './HomeThreadView'
import { ActivityHistoryView } from './ActivityHistoryView'
import { SpecSettingsView } from './SpecSettingsView'
import { AutomationRightPanel } from './AutomationRightPanel'

const TAB_LABELS: Record<AutomationTab, string> = {
  chat: '聊天',
  activity: '动态',
  settings: '设置',
}

interface Props {
  specId: string
}

export function SpecRunSurface({ specId }: Props) {
  const [activeTab, setActiveTab] = useAtom(automationActiveTabAtom)
  const [runSessionId, setRunSessionId] = useAtom(automationActivityRunSessionIdAtom)
  const [specs, setSpecs] = useAtom(humaneSpecsAtom)
  const activitiesMap = useAtomValue(automationActivitiesAtom)
  const [showRightPanel, setShowRightPanel] = useState(false)
  const [isRunning, setIsRunning] = useState(false)

  const spec = specs.find((s) => s.id === specId)
  const activities = activitiesMap[specId] ?? []

  if (!spec) return null

  async function handleRun() {
    setIsRunning(true)
    try {
      await triggerAutomationManualHumane(specId)
      setActiveTab('activity')
    } finally {
      setIsRunning(false)
    }
  }

  const rightPanelSessionId = activeTab === 'activity' && runSessionId ? runSessionId : null

  return (
    <div className="flex flex-col flex-1 h-full overflow-hidden">
      <SpecRunHeader specName={spec.name} onRun={handleRun} isRunning={isRunning} />

      {/* tab bar */}
      <div className="flex gap-0 border-b border-border/50 px-3 shrink-0">
        {(Object.keys(TAB_LABELS) as AutomationTab[]).map((t) => (
          <button
            key={t}
            onClick={() => { setActiveTab(t); if (t !== 'activity') setRunSessionId(null) }}
            className={[
              'titlebar-no-drag px-3 py-2 text-sm border-b-2 transition-colors',
              activeTab === t
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground',
            ].join(' ')}
          >
            {TAB_LABELS[t]}
          </button>
        ))}
      </div>

      {/* content + right panel */}
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-col flex-1 overflow-hidden">
          {activeTab === 'chat' && (
            <HomeThreadView
              specId={specId}
              showRightPanel={showRightPanel}
              onToggleRightPanel={() => setShowRightPanel((v) => !v)}
            />
          )}
          {activeTab === 'activity' && (
            <ActivityHistoryView
              specId={specId}
              activities={activities}
              onOpenRunSession={(sid) => setRunSessionId(sid)}
              activeRunSessionId={runSessionId}
              onCloseRunSession={() => setRunSessionId(null)}
            />
          )}
          {activeTab === 'settings' && (
            <SpecSettingsView
              spec={spec}
              onSpecChange={(updated) =>
                setSpecs((prev) => prev.map((s) => (s.id === updated.id ? updated : s)))
              }
            />
          )}
        </div>

        {/* Right panel: shown for chat tab or D2 run-session view */}
        {showRightPanel && (activeTab === 'chat' || (activeTab === 'activity' && runSessionId)) && (
          <AutomationRightPanel
            sessionId={rightPanelSessionId ?? ''}
            sessionPath={null}
          />
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Verify compile**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/automation/SpecRunHeader.tsx ui/src/components/automation/SpecRunSurface.tsx
git commit -m "feat(automation): SpecRunHeader + SpecRunSurface — three-tab assembly"
```

---

## Task 10: AutomationHub refactor (thin shell)

**Files:**
- Modify: `ui/src/components/automation/AutomationHub.tsx`

This replaces the existing monolithic Hub with a three-column layout: fixed rail gap | SpecList | SpecRunSurface.

- [ ] **Step 1: Read current AutomationHub.tsx**

```bash
head -60 ui/src/components/automation/AutomationHub.tsx
```

Note: the existing Hub loads specs via `humaneSpecsAtom` + `listAutomationsHumane()` and activities via `getAutomationActivity`. That loading logic is preserved but the render output is replaced.

- [ ] **Step 2: Replace the render/return section of `AutomationHub.tsx`**

Replace the entire file content with:

```tsx
import { useEffect } from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { humaneSpecsAtom, automationActivitiesAtom } from '@/atoms/automation'
import { automationSelectedSpecIdAtom, automationActiveTabAtom } from '@/atoms/automation-ui'
import { listAutomationsHumane, getAutomationActivity } from '@/lib/tauri-bridge'
import { SpecList } from './SpecList'
import { SpecRunSurface } from './SpecRunSurface'

export function AutomationHub() {
  const [specs, setSpecs] = useAtom(humaneSpecsAtom)
  const setActivities = useSetAtom(automationActivitiesAtom)
  const [selectedSpecId, setSelectedSpecId] = useAtom(automationSelectedSpecIdAtom)
  const setActiveTab = useSetAtom(automationActiveTabAtom)

  // Load specs on mount
  useEffect(() => {
    listAutomationsHumane().then(setSpecs)
  }, [setSpecs])

  // Load activities for selected spec
  useEffect(() => {
    if (!selectedSpecId) return
    getAutomationActivity(selectedSpecId, 50).then((acts) =>
      setActivities((prev) => ({ ...prev, [selectedSpecId]: acts }))
    )
  }, [selectedSpecId, setActivities])

  // Auto-select first spec if none selected
  useEffect(() => {
    if (!selectedSpecId && specs.length > 0) {
      setSelectedSpecId(specs[0].id)
    }
  }, [specs, selectedSpecId, setSelectedSpecId])

  return (
    <div className="flex h-full overflow-hidden">
      {/* spec list sidebar */}
      <div className="w-[240px] shrink-0 flex flex-col border-r border-border/50 overflow-hidden">
        <div className="titlebar-drag-region flex items-center px-3 py-2 border-b border-border/50 text-sm font-semibold shrink-0">
          数字人
        </div>
        <SpecList
          selectedSpecId={selectedSpecId}
          onSelect={(id) => { setSelectedSpecId(id); setActiveTab('activity') }}
          onRun={(id) => { setSelectedSpecId(id); setActiveTab('activity') }}
        />
      </div>

      {/* run surface */}
      <div className="flex-1 flex overflow-hidden">
        {selectedSpecId ? (
          <SpecRunSurface specId={selectedSpecId} />
        ) : (
          <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
            选择一个数字人
          </div>
        )}
      </div>
    </div>
  )
}

export default AutomationHub
```

- [ ] **Step 3: Verify compile + test suite**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10 && npm test -- --run 2>&1 | tail -10
```

Expected: no TS errors; existing tests pass.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/automation/AutomationHub.tsx
git commit -m "feat(automation): refactor AutomationHub — three-column shell with SpecList + SpecRunSurface"
```

---

## Task 11: useOpenSession W2 routing fix

**Files:**
- Modify: `ui/src/hooks/useOpenSession.ts`

This fixes the Phase 2a bug: clicking a run session while in Kaleidoscope opens a workspace tab invisibly. After this task, automation sessions route to Kaleidoscope.

- [ ] **Step 1: Read `ui/src/hooks/useOpenSession.ts`**

```bash
cat -n ui/src/hooks/useOpenSession.ts | head -80
```

Note the function signature and the existing atom setters used. The function takes `(type, sessionId, title)`.

- [ ] **Step 2: Add imports for the new atoms**

In `ui/src/hooks/useOpenSession.ts`, add these imports after the existing import block (after line 19):

```ts
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import {
  automationSelectedSpecIdAtom,
  automationActiveTabAtom,
  automationActivityRunSessionIdAtom,
} from '@/atoms/automation-ui'
```

- [ ] **Step 3: Add the new atom setters inside the hook, before `useCallback`**

After line 31 (`const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)`), add:

```ts
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const setAutomationSelectedSpecId = useSetAtom(automationSelectedSpecIdAtom)
  const setAutomationActiveTab = useSetAtom(automationActiveTabAtom)
  const setAutomationActivityRunSessionId = useSetAtom(automationActivityRunSessionIdAtom)
```

- [ ] **Step 4: Add W2 routing block inside the `useCallback` body**

In the `useCallback` body (currently line 34), the first line is:
```ts
      let displayTitle = title
```

Insert this block BEFORE that line:

```ts
      // W2: automation sessions route to Kaleidoscope, not workspace.
      // AgentSessionMeta.metadataJson carries origin + spec_id for automation runs.
      const session = agentSessions.find((s) => s.id === sessionId)
      const meta = (() => { try { return JSON.parse(session?.metadataJson ?? '{}') } catch { return {} } })()
      const origin: string = meta.origin ?? ''
      if (origin.startsWith('automation:')) {
        setTopLevelView('kaleidoscope')
        setKaleidoscopeModule('humans')
        setAutomationSelectedSpecId(meta.spec_id ?? null)
        if (origin === 'automation:home_thread') {
          setAutomationActiveTab('chat')
          setAutomationActivityRunSessionId(null)
        } else {
          setAutomationActiveTab('activity')
          setAutomationActivityRunSessionId(sessionId)
        }
        return
      }
```

- [ ] **Step 5: Update the `useCallback` dependency array**

The current deps array (line 82) ends with `activeWorkspaceId]`. Replace the entire deps array with:

```ts
    [tabs, setTabs, setActiveTabId, setAppMode, setCurrentConversationId, setCurrentAgentSessionId,
     agentSessions, setCurrentAgentWorkspaceId, setUnviewedCompleted, activeWorkspaceId,
     setTopLevelView, setKaleidoscopeModule, setAutomationSelectedSpecId,
     setAutomationActiveTab, setAutomationActivityRunSessionId],
```

- [ ] **Step 5: Verify compile + full test suite**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10 && npm test -- --run 2>&1 | tail -15
```

Expected: no errors, all existing tests pass.

- [ ] **Step 6: Commit**

```bash
git add ui/src/hooks/useOpenSession.ts
git commit -m "fix(automation): W2 routing — automation sessions open in Kaleidoscope (fixes Phase 2a invisible-tab bug)"
```

---

## Final verification

- [ ] **Full TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected: no output.

- [ ] **Full test suite**

```bash
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: all tests pass, new tests included.

- [ ] **Rust compile**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: no output.

- [ ] **Rust tests**

```bash
cd src-tauri && cargo test home_thread 2>&1 | tail -5
```

Expected: `1 passed`.

---

## Out of scope (follow-up specs)

- AutomationDelegate → StreamSink → IPC live token streaming (Phase 2b)
- IM push / message channel actual send logic (Phase 2b)
- PreviewPanel integration for automation surfaces (on-demand, no-blocker)
- Full schedule / model / notification settings in SpecSettingsView (requires schema additions)
- Spec create / edit wizard (independent feature)
