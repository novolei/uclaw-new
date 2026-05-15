# Kaleidoscope Automation Run Surface Design

## Overview

Embed automation run sessions directly inside the Kaleidoscope (万花筒) page. Clicking a run or starting a run surfaces a full chat window within Kaleidoscope, with on-demand reuse of the Agent window's file preview panel and right-side panel tabs. This fixes the Phase 2a bug where `useOpenSession` opened a workspace tab invisibly while the user remained on `topLevelView = 'kaleidoscope'`.

---

## Decisions Made

| Question | Decision |
|----------|----------|
| Interaction model | Full bidirectional chat (Phase 2b pulled forward) |
| Conversation/run model | M3 Hybrid — persistent home thread per spec + Phase 2a run-sessions preserved |
| Kaleidoscope layout | Three-column persistent (rail \| spec list \| run surface) |
| Run surface internal layout | Three tabs: 聊天 / 动态 / 设置 |
| "查看进程 >" interaction | D2 — push sub-view within 动态 tab, breadcrumb "← 动态" |
| State persistence | S1 — Jotai atoms remember specId + tab + run-sessionId across module switches |
| Workspace relationship | W2 — Kaleidoscope is canonical; useOpenSession routes automation sessions to Kaleidoscope |
| Liveness | UI designed for streaming; backend StreamSink wiring is a separate Phase 2b spec; this phase uses activity polling |

---

## Architecture

### Three-column layout

```
KaleidoscopeShell
├── KaleidoscopeRail          (120px, fixed, titlebar-drag-region on top spacer)
└── AutomationModule
    ├── SpecList               (~240px, persistent sidebar, like a messaging app conversation list)
    │   └── SpecListItem       (spec name / status dot / hover ▶ Run button)
    └── SpecRunSurface         (flex-1)
        ├── SpecRunHeader      (spec name, ▶ 运行 button, titlebar-drag-region)
        ├── TabBar             [聊天] [动态] [设置]
        └── TabContent
            ├── 聊天 tab  →  HomeThreadView
            │   ├── AgentMessages (home-thread session)
            │   ├── Composer
            │   ├── PreviewPanel  (horizontal split, on-demand)
            │   └── RightPanel    (files / trajectory, on-demand, ~380px)
            ├── 动态 tab  →  ActivityHistoryView
            │   ├── ActivityList  (timeline: timestamp / status / summary / 查看进程 >)
            │   └── [D2] RunSessionSubView  ← pushed on "查看进程 >"
            │       ├── breadcrumb "← 动态"
            │       ├── AgentMessages (run session)
            │       ├── PreviewPanel  (on-demand)
            │       └── RightPanel    (files / trajectory, on-demand)
            └── 设置 tab  →  SpecSettingsView
                (计划任务 / 模型 / 工具权限 / 通知渠道 / YAML view)
```

### Key design principles

1. `AgentMessages` is reused in three places (home-thread / D2 run-session) with no internal changes — only `sessionId` differs.
2. `RightSidePanel` gains a `visibleTabs?: TabId[]` prop; when omitted, existing workspace behavior is unchanged. For automation surfaces, pass `['files', 'trajectory']` to hide teams/browser tabs.
3. The 设置 tab is the sole entry point for spec configuration; the right panel carries no settings responsibility.
4. `PreviewPanel` uses a locally-scoped atom pair so automation preview state does not bleed into the workspace surface.

---

## Data Model

### No new migration required for the core feature

Phase 2a stores `spec_id` and `origin` inside `agent_sessions.metadata_json` (not as dedicated columns). Run sessions are created with `"origin": "automation:<trigger>"` (e.g. `"automation:cron"`, `"automation:manual"`). The home-thread session follows the same pattern using `"automation:home_thread"`:

```sql
-- Find the persistent home-thread session for a spec
SELECT * FROM agent_sessions
WHERE json_extract(metadata_json, '$.spec_id') = ?
  AND json_extract(metadata_json, '$.origin') = 'automation:home_thread'
LIMIT 1;
-- If no row: INSERT a new agent_session with metadata_json containing
-- {"spec_id": "<id>", "origin": "automation:home_thread"}
```

The activity history timeline queries existing tables (V24 added `session_id` to `automation_activities`):

```sql
SELECT aa.*, ag.id as session_id, ag.status
FROM automation_activities aa
LEFT JOIN agent_sessions ag ON ag.id = aa.session_id
WHERE aa.spec_id = ?
ORDER BY aa.created_at DESC;
```

### Possible V26 migration (verify against actual schema before committing)

If `automation_specs` is missing any of the following columns used by the 设置 tab, add them in V26:
- `model_override TEXT` — per-spec model (NULL = follow global)
- `notification_level TEXT` — `'important' | 'all' | 'none'`
- `tool_permissions_json TEXT` — JSON blob of enabled tool flags (browser, email, im_push)

Implementor must check actual V20 schema columns before writing this migration.

### New Tauri command

```rust
// Returns the home-thread AgentSession for a spec, creating one if absent.
get_or_create_spec_home_thread(spec_id: String) -> Result<AgentSession>
```

---

## Frontend State (S1 persistence)

New file: `ui/src/atoms/automation.ts`

```ts
// Which spec is selected in SpecList (persists across module switches)
export const automationSelectedSpecIdAtom = atom<string | null>(null);

// Which tab is active in SpecRunSurface
export const automationActiveTabAtom = atom<'chat' | 'activity' | 'settings'>('activity');

// D2 sub-view: which run-session is being viewed inside 动态 tab
// null = showing ActivityList; non-null = showing RunSessionSubView
export const automationActivityRunSessionIdAtom = atom<string | null>(null);
```

All three atoms are module-level (not workspace-scoped) and survive Kaleidoscope ↔ other-module switches.

---

## W2: useOpenSession Routing

File: `ui/src/hooks/useOpenSession.ts` (or wherever the hook lives)

```ts
// Add at the top of the open-session logic, before existing workspace routing:
const meta = JSON.parse(session.metadata_json ?? '{}');
const origin: string = meta.origin ?? '';
if (origin.startsWith('automation:')) {
  setTopLevelView('kaleidoscope');
  setKaleidoscopeModule('humans');               // kaleidoscopeModuleAtom value for 数字人
  setAutomationSelectedSpecId(meta.spec_id);
  setAutomationActiveTab(
    origin === 'automation:home_thread' ? 'chat' : 'activity'
  );
  if (origin !== 'automation:home_thread') {
    setAutomationActivityRunSessionId(session.id);  // opens D2 sub-view
  }
  return;
}
// else: existing workspace routing unchanged
```

This fixes the Phase 2a bug where automation sessions opened invisibly in the workspace while `topLevelView` remained `'kaleidoscope'`.

---

## Component Inventory

### New components (`ui/src/components/automation/`)

| Component | Responsibility |
|-----------|---------------|
| `SpecList.tsx` | 240px persistent sidebar; spec list with status dots and quick ▶ Run |
| `SpecListItem.tsx` | Single spec row: name / status indicator / hover Run button |
| `SpecRunSurface.tsx` | Right-area container: header + TabBar + TabContent routing |
| `SpecRunHeader.tsx` | Spec name + ▶ 运行 button; carries `titlebar-drag-region` |
| `HomeThreadView.tsx` | 聊天 tab: AgentMessages + Composer + on-demand PreviewPanel/RightPanel |
| `ActivityHistoryView.tsx` | 动态 tab: ActivityList + D2 RunSessionSubView router |
| `ActivityListItem.tsx` | Single run entry: timestamp / status badge / one-line summary / 查看进程 > |
| `RunSessionSubView.tsx` | D2 sub-view: breadcrumb + AgentMessages + on-demand panels |
| `SpecSettingsView.tsx` | 设置 tab: form/YAML toggle, schedule, model, permissions, notifications |

### Zero-change reuse

| Component | Source |
|-----------|--------|
| `AgentMessages` | `components/agent/` — pass `sessionId`, no workspace atoms needed |
| `Composer / AgentInput` | `components/agent/` — same |

### Light-touch changes (additive props only)

| Component | Change | Workspace impact |
|-----------|--------|-----------------|
| `RightSidePanel` | Add `visibleTabs?: TabId[]` prop | None — omitting prop = current behavior |
| `PreviewPanel` | Extract preview atom pair to prop or local atom scope | Scoped; workspace path unchanged |
| `useOpenSession` | Add automation origin branch (W2); else path untouched | One if-else addition |
| `AutomationHub.tsx` | Refactor into SpecList + SpecRunSurface composition; Hub becomes thin shell | Internal restructure; external interface unchanged |

### Untouched

`AgentHeader`, `WorkspaceShell`, `topLevelViewAtom` switch logic, all workspace atoms and workspace-surface components.

---

## Activity List Item Design

Each row in the 动态 tab corresponds to one `automation_activity` record:

```
05/15 01:41  ✅ 已完成               查看进程 >
B站评论自动回复第98轮：本次没有需要回复的新评论。
通知总数 20 条，12 条已回复，7 条无实质内容，1 条负面跳过。

05/14 23:42  ✅ 已完成               查看进程 >
...

05/14 21:42  ⏭ 已跳过               查看进程 >
...
```

Status badges: `completed` → green ✅, `skipped` → gray ⏭, `failed` → red ❌, `escalation` → orange ⚠️ (highlighted, requires attention).

---

## Settings Tab Layout

Mirrors the reference design with two view modes toggled by [设置] / [YAML] buttons:

**Form view sections:**
1. 计划任务 — toggle + interval picker (1m / 5m / 15m / 30m / 1h / 2h / 6h / 12h / 1d) or cron expression
2. 运行时 — model selector (follows global / override)
3. 工具权限 — AI浏览器 / 电子邮件 / IM推送 toggles with configuration links
4. 所需登录 — list of required service logins (e.g., 哔哩哔哩) with Halo browser open action
5. 系统通知 — 重要 / 全部 / 无 pill selector
6. 消息通道 — notification channel picker

**YAML view:** read/edit raw spec YAML (existing functionality, surfaced here).

---

## Out of Scope (separate specs)

| Item | Reason |
|------|--------|
| AutomationDelegate → StreamSink → IPC live-streaming wiring | Pure backend; Phase 2b spec. UI slot pre-reserved — connecting it requires zero UI changes. |
| IM push / message channel actual send logic | Phase 2b. Settings tab has the toggle; send logic is separate. |
| Spec create / edit wizard | Existing YAML editor preserved; new-spec wizard is independent feature. |
| Marketplace / install flow | Phase 3. |
| Mobile / responsive layout | Desktop Tauri fixed layout; not in scope. |

---

## Success Criteria

- Clicking any run record in AutomationHub navigates to Kaleidoscope 动态 tab with D2 sub-view open showing that run's transcript.
- Starting a run from ▶ 运行 button shows 动态 tab with the new run entry appearing (status: running → completed).
- Switching away from 数字人 module and back returns to the exact specId + tab + run-session that was open.
- `RightSidePanel` in automation surfaces shows only files and trajectory tabs; teams and browser tabs are hidden.
- Workspace surface is completely unaffected: all existing agent sessions open in workspace as before.
- TypeScript and Cargo compile clean; existing Vitest suite passes.
