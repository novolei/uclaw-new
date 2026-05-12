# Sidebar Git-actions relocation

**Date:** 2026-05-13
**Status:** Spec
**Branch (target):** new branch off `main`

---

## 1. Background

The chat composer footer currently hosts the full `GitChipsRow` — `BranchPicker` (`⎇ main`) + `GitActionsPicker` (`提交 ▾`) + `GitWorkbenchDialog`. After several rounds of agent-mode iteration the composer has grown crowded (model selector, permissions, brain, attach, target, branch, submit-actions, send button). The "提交" affordance, while logically related to git, is a destination action — not a per-message switch — so it doesn't earn its place inline with each message.

The MCP·Skills capability indicator row in the left sidebar already establishes a "this workspace's capabilities" zone. Moving `GitActionsPicker` there:

- Frees ~80px of composer footer width per message turn.
- Groups git actions with their natural siblings (MCP, Skills, Automations) under the "workspace context" frame.
- Keeps `BranchPicker` in the composer because branch is per-conversation routing context, not a workspace-level fact.

## 2. Scope

In:
- Move **only** `GitActionsPicker` (the `提交 ▾` popover) + its companion `GitWorkbenchDialog` from `GitChipsRow` to the left sidebar.
- Add a sidebar variant trigger style for `GitActionsPicker` that matches the MCP·Skills row aesthetic.
- Display MCP·Skills section and 提交 section on the **same row**, separated by a 1px hairline divider.

Out:
- `BranchPicker` stays in the composer (both `ChatInput` and `AgentView`, per CLAUDE.md dual-composer rule).
- No changes to popover content, state machine, or git dispatchers (`runCommit`, `runCreateBranch`, `runCreatePr`, `runInitRepo`).
- No changes to `BranchPicker` trigger, popover, or `useBranchPicker.ts` logic.
- No new migration; no Rust changes.

## 3. Architecture

### 3.1 Component split

`GitChipsRow` ([ui/src/components/chat/git/GitChipsRow.tsx](ui/src/components/chat/git/GitChipsRow.tsx)) is reduced to **just the `BranchPicker`** — it still owns the `cwd → isGitRepo + currentBranch` probe and passes those into `BranchPicker`. `GitActionsPicker` and `GitWorkbenchDialog` are removed from this file. The file stays at its current path so `ChatInput.tsx` and `AgentView.tsx` keep importing the same name.

A new file `ui/src/components/app-shell/SidebarGitActions.tsx` mirrors the probe pattern: reads `activeWorkspaceCwdAtom`, probes `gitIsRepo` + `gitCurrentBranch`, and renders `GitActionsPicker` (sidebar variant) + `GitWorkbenchDialog`. The `currentBranch` value here is used **only** for the workbench dialog header — the sidebar trigger itself never shows branch text (BranchPicker already does that).

> **Cost of duplicate probes:** Both `GitChipsRow` and `SidebarGitActions` will probe `gitIsRepo` + `gitCurrentBranch` on `cwd` change. These are fast filesystem operations (single `git` invocation each), the result is per-cwd, and React batches the resulting renders. We accept the duplicate over introducing a new "git status" Jotai atom for one extra consumer.

### 3.2 `GitActionsPicker` variant prop

Add `variant?: 'composer' | 'sidebar'` to `GitActionsPicker` ([ui/src/components/chat/git/GitActionsPicker.tsx:47-64](ui/src/components/chat/git/GitActionsPicker.tsx#L47-L64)). Default = `'composer'` so existing call sites remain byte-identical.

Variant differences (everything else identical):

| Aspect | `composer` (default) | `sidebar` |
|---|---|---|
| Trigger root class | `inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-[12px] font-medium` | `flex items-center gap-1.5 rounded-[10px] px-3 py-2 text-[12px]` |
| Text color (repo) | `border-border/70 text-muted-foreground hover:border-border hover:bg-accent hover:text-accent-foreground` | `text-foreground/50 hover:bg-foreground/[0.04] hover:text-foreground/70` |
| Text color (no repo) | `border-amber-200 bg-amber-50 text-amber-800 hover:...` | `text-amber-600 hover:bg-amber-500/12 hover:text-amber-500` (mirrors `BranchPicker` amber state) |
| Icon size | `h-3.5 w-3.5` | `h-[13px] w-[13px]` |
| ChevronDown | `h-3 w-3` muted | `h-[11px] w-[11px] text-foreground/30` |
| `PopoverContent` `side` | `top` (current default) | `right` |
| `PopoverContent` `align` | `center` | `start` |
| `PopoverContent` `sideOffset` | `12` (current) | `8` |

The popover content itself (`renderContent()`) is unchanged across variants.

### 3.3 `LeftSidebar.tsx` row structure

Replace the standalone MCP·Skills block at [LeftSidebar.tsx:874-890](ui/src/components/app-shell/LeftSidebar.tsx#L874-L890) with a two-section container:

```tsx
{mode === 'agent' && capabilities && (
  <div className="px-3 pb-1">
    <div className="flex items-stretch gap-2">
      {/* Section A: MCP · Skills (existing button, unchanged inner markup) */}
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={() => { setSettingsTab('agent'); setSettingsOpen(true) }}
            className="flex-1 min-w-0 flex items-center gap-3 px-3 py-2 rounded-[10px] text-[12px] text-foreground/50 hover:bg-foreground/[0.04] hover:text-foreground/70 transition-colors titlebar-no-drag"
          >
            <div className="flex items-center gap-2.5 flex-1 min-w-0">
              <span className="flex items-center gap-1">
                <Plug size={13} className="text-foreground/40" />
                <span className="tabular-nums">{capabilities.mcpServers.filter((s) => s.enabled).length}</span>
                <span className="text-foreground/30">MCP</span>
              </span>
              <span className="text-foreground/20">·</span>
              <span className="flex items-center gap-1">
                <Zap size={13} className="text-foreground/40" />
                <span className="tabular-nums">{capabilities.skills.length}</span>
                <span className="text-foreground/30">Skills</span>
              </span>
            </div>
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">点击配置 MCP 与 Skills</TooltipContent>
      </Tooltip>

      {/* Hairline divider — only shown when SidebarGitActions renders something */}
      <SidebarGitActions />
    </div>
  </div>
)}
```

The hairline divider lives **inside** `SidebarGitActions.tsx` so it disappears together with the 提交 button when `cwd` is missing — the row degrades to MCP·Skills alone with no orphan divider.

`SidebarGitActions.tsx` render shape:

```tsx
if (!cwd) return null  // no workspace dir → no git surface at all

return (
  <>
    <div className="self-center w-px h-4 bg-foreground/10" aria-hidden="true" />
    <GitActionsPicker
      variant="sidebar"
      cwd={cwd}
      isGitRepo={isGitRepo}
      onBranchChange={() => { /* no-op — sidebar trigger has no branch label */ }}
      onOpenWorkbench={() => setWorkbenchOpen(true)}
    />
    <GitWorkbenchDialog
      open={workbenchOpen}
      onOpenChange={setWorkbenchOpen}
      cwd={cwd}
      currentBranch={currentBranch}
    />
  </>
)
```

### 3.4 Cross-surface branch sync

Today `GitChipsRow` passes its `setCurrentBranch` as `onBranchChange` to `GitActionsPicker`, so a "create branch" flow inside the popover immediately updates the BranchPicker label sitting next to it. With the popover moving to the sidebar, the composer's BranchPicker would otherwise stay stale until the user reopens it.

Fix: add a single tick atom `branchSyncTickAtom` in [ui/src/atoms/workspace.ts](ui/src/atoms/workspace.ts) (default `0`). Two consumers:

- `GitChipsRow`: include `branchSyncTick` in its probe `useEffect` deps — bumping the tick triggers a re-probe of `gitCurrentBranch(cwd)`.
- `SidebarGitActions`: its `onBranchChange` handler (1) updates local `currentBranch` state for the workbench dialog header, and (2) bumps the tick. Same wiring on the composer side is unnecessary because composer's BranchPicker already controls branch checkout itself via its own popover.

This is a one-line atom + one extra dep — minimal blast radius, no observable behavior change for users with one git surface (composer-only or sidebar-only) because the bump-then-re-probe is idempotent.

### 3.5 Final layout

```
┌─ LeftSidebar (Agent mode) ──────────────┐
│  … session list …                       │
├─────────────────────────────────────────┤
│ 🔌 0 MCP · ⚡ 8 Skills │ ⎇ 提交 ▾        │  ← two sections, hairline divider
│ 🤖 Automations                          │
├─────────────────────────────────────────┤
│  WorkspaceSwitcherBar                   │
│  👤 User                  ⚙              │
└─────────────────────────────────────────┘
```

Composer footer after change (BranchPicker stays, 提交 gone):

```
[deepseek ▾] [Ask permissions ▾] [🧠] [📎] [🎯 平衡] [⎇ main]    [submit ▾]
```

## 4. Behavior

- **No cwd / no workspace:** Entire `SidebarGitActions` (including divider) renders `null`. MCP·Skills row occupies full width as today.
- **cwd exists, not a git repo:** `GitActionsPicker` shows amber "初始化 Git" trigger (mirrors `BranchPicker`'s amber init state). Click → existing init flow (now also from sidebar).
- **cwd is a git repo:** Default "⎇ 提交 ▾" trigger. Click → popover opens to the **right** of the sidebar, anchored to the trigger's `start` edge (top edge), so it expands into the chat-area whitespace rather than overlapping sidebar items.
- **Workbench dialog:** Lives in `SidebarGitActions`, opens via `onOpenWorkbench` callback, identical content as today.

## 5. Visual polish details

- **Divider:** `w-px h-4 bg-foreground/10` — 1px wide, 16px tall, sits flush with text baseline because parent uses `items-stretch` and divider uses `self-center`. Color is theme-aware (foreground-alpha), so it survives across all 11 themes (warm-paper, qingye, forest-*, etc.) per CLAUDE.md theming rule.
- **Section heights:** Both sections share the same `py-2` vertical padding so the row visually reads as a single 32px-tall band.
- **Hover bounds:** Each section is its own button — hover background stops at its section edge. The divider does not get a hover state.
- **Focus rings:** Inherit default `focus-visible:ring-2 ring-ring/40` from the project's button reset; verify visually that focus ring on the right section does not bleed across the divider.
- **Popover side:** `right` keeps the popover from covering its own trigger or the MCP·Skills section. Radix collision detection will flip to `left` automatically if there's no room (small windows).

## 6. Files touched

- `ui/src/components/chat/git/GitChipsRow.tsx` — drop `GitActionsPicker` + `GitWorkbenchDialog` import & render; keep `BranchPicker` only. Update top-of-file docstring to reflect new scope.
- `ui/src/components/chat/git/GitActionsPicker.tsx` — add `variant` prop; branch trigger className + popover side/align/sideOffset on variant.
- `ui/src/components/app-shell/SidebarGitActions.tsx` — new file; cwd probe + `GitActionsPicker variant="sidebar"` + `GitWorkbenchDialog`.
- `ui/src/components/app-shell/LeftSidebar.tsx` — wrap MCP·Skills row + new `<SidebarGitActions />` in a `flex items-stretch gap-2` container. Reduce MCP·Skills button to `flex-1 min-w-0`.
- `ui/src/atoms/workspace.ts` — add `branchSyncTickAtom` (§3.4).

No Rust changes. No new migration. No CSP changes.

## 7. Testing

Vitest (jsdom):
- `SidebarGitActions.test.tsx` — renders `null` when `activeWorkspaceCwdAtom` is empty; renders trigger + divider when cwd is set and `gitIsRepo` resolves `true`; clicking the trigger opens the popover.
- Update `GitChipsRow.test.tsx` (if it exists) — verify GitActionsPicker is no longer rendered from this surface.
- Update `GitActionsPicker.test.tsx` — add a case asserting `variant="sidebar"` applies the sidebar className and `side="right"` on the popover.

Manual:
- Theme parity: switch through warm-paper, qingye, forest-night → divider visible but subtle; section hover backgrounds clean.
- Composer parity (CLAUDE.md rule): `BranchPicker` still works identically in both Chat mode (`ChatInput`) and Agent mode (`AgentView`); 提交 button is gone from both.
- Popover anchoring: open 提交 popover with sidebar at default width → opens to the right; resize window so sidebar is near the right edge → Radix flips popover to left.
- Init-repo path: open a non-git workspace dir → sidebar shows amber "初始化 Git"; click → flow runs and chip flips to "提交".

## 8. Risks & mitigations

- **Popover collision in narrow windows.** Mitigated by Radix's built-in collision detection (`collisionPadding` already set on the existing PopoverContent).
- **Theme regression on divider.** Mitigated by using `bg-foreground/10` (theme token) instead of a hardcoded color.
- **Loss of "提交" discoverability in composer.** This is the intentional move — flagged in commit message + PR description.
- **Duplicate `gitIsRepo` probe** (composer + sidebar). Accepted; see §3.1.

## 9. Out of scope follow-ups

- Consider moving `BranchPicker` to the sidebar too once we have data showing branch-switching frequency. Not in this PR.
- Consider extracting a shared `useGitProbe(cwd)` hook if a third consumer appears. Not justified by current N=2.
