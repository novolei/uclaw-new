# Bottom Dock + Connection Indicator + Color Scales Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port three OpenHuman UI elements to uClaw: a macOS-Dock-style bottom navigation bar with L1/L2 animations, a 3-channel connection indicator, and sage/coral/amber Tailwind color scales.

**Architecture:** The Dock is a `position: fixed` overlay that never affects layout; it is hidden by default and slides out when the mouse enters a 16px hot-zone at the bottom of the window. It reads/writes existing navigation atoms (appModeAtom, topLevelViewAtom, kaleidoscopeModuleAtom) and adds 4 new atoms in `dock-atoms.ts` for enable-toggle and connection state. Two new Tauri commands (get_app_health, get_memu_status) back the 3-channel connection indicator.

**Tech Stack:** React 18 + TypeScript + `motion/react` (already a dep) + Jotai `atomWithStorage` + Radix UI Tooltip (`@/components/ui/tooltip`) + Lucide icons + Tailwind CSS + Vitest + Rust/Tauri

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `ui/tailwind.config.js` | Add sage/coral/amber fixed color scales |
| Modify | `src-tauri/src/tauri_commands.rs` | Add `get_app_health` + `get_memu_status` commands |
| Modify | `src-tauri/src/main.rs` | Register both commands in `invoke_handler!` |
| Create | `ui/src/atoms/dock-atoms.ts` | 4 atoms: enabled toggle + 3 connection state atoms |
| Create | `ui/src/components/dock/useConnectionStatus.ts` | Polling hook that writes to the 3 connection atoms |
| Create | `ui/src/components/dock/DockItem.tsx` | Single icon with L1 label + L2 spring scale animation |
| Create | `ui/src/components/dock/ConnectionIndicator.tsx` | 3 colored dots with Radix Tooltip per channel |
| Create | `ui/src/components/dock/BottomDock.tsx` | Main Dock component (spring reveal/hide, nav items, connection indicator) |
| Modify | `ui/src/components/app-shell/AppShell.tsx` | Mount DockHotZone + BottomDock |
| Modify | `ui/src/components/settings/GeneralSettings.tsx` | Add 外观 section with bottomDockEnabledAtom toggle |

---

## Task 1: Tailwind Color Scales (sage / coral / amber)

**Files:**
- Modify: `ui/tailwind.config.js:66-68` (after the `danger` block, before `fontFamily`)

- [ ] **Step 1: Add the three color scales**

  In `ui/tailwind.config.js`, replace the closing `},` of the `danger` block (line ~68) with the following — the new colors go inside `theme.extend.colors`:

  ```js
        danger: {
          DEFAULT: 'hsl(var(--danger))',
          bg: 'hsl(var(--danger-bg))',
        },
        // Fixed cross-theme scales — used by Dock/ConnectionIndicator; do not
        // change with theme switches unlike the CSS-variable tokens above.
        sage: {
          '50':  '#f0fdf4',
          '100': '#dcfce7',
          '200': '#bbf7d0',
          '300': '#86efac',
          '400': '#4ade80',
          '500': '#34c759',
          '600': '#16a34a',
          '700': '#15803d',
          '800': '#166534',
          '900': '#14532d',
        },
        coral: {
          '50':  '#fff1f2',
          '100': '#ffe4e6',
          '200': '#fecdd3',
          '300': '#fda4af',
          '400': '#fb7185',
          '500': '#ef4444',
          '600': '#dc2626',
          '700': '#b91c1c',
          '800': '#991b1b',
          '900': '#7f1d1d',
        },
        amber: {
          '50':  '#fffbeb',
          '100': '#fef3c7',
          '200': '#fde68a',
          '300': '#fcd34d',
          '400': '#fbbf24',
          '500': '#f59e0b',
          '600': '#d97706',
          '700': '#b45309',
          '800': '#92400e',
          '900': '#78350f',
        },
  ```

- [ ] **Step 2: Verify TypeScript still compiles**

  ```bash
  cd ui && npx tsc --noEmit 2>&1 | head -10
  ```

  Expected: no output (zero errors).

- [ ] **Step 3: Commit**

  ```bash
  git add ui/tailwind.config.js
  git commit -m "$(cat <<'EOF'
  feat(dock): add sage/coral/amber fixed color scales to Tailwind

  Cross-theme fixed scales for the bottom Dock and ConnectionIndicator.
  Unlike the existing CSS-variable tokens (which shift per theme), these
  values stay constant so the connection indicator dots always read at the
  same saturation regardless of the active theme.
  EOF
  )"
  ```

---

## Task 2: Rust Commands (get_app_health + get_memu_status)

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (append at end of file)
- Modify: `src-tauri/src/main.rs` (add 2 entries to `invoke_handler!`)

- [ ] **Step 1: Append the two commands to tauri_commands.rs**

  Add at the very end of `src-tauri/src/tauri_commands.rs` (after the last `}` of `respond_plan_mode_suggest`):

  ```rust
  /// Minimal liveness probe — frontend receiving Ok proves the Tauri backend is up.
  #[tauri::command]
  pub async fn get_app_health() -> Result<serde_json::Value, String> {
      Ok(serde_json::json!({ "backend": true }))
  }

  /// Check whether the memU Python bridge is healthy.
  /// Returns { "online": true/false }. Best-effort — always returns Ok so the
  /// agent loop is never affected by a failed health check.
  #[tauri::command]
  pub async fn get_memu_status(
      state: State<'_, AppState>,
  ) -> Result<serde_json::Value, String> {
      let client = state.memu_client.clone();
      match client {
          None => Ok(serde_json::json!({ "online": false, "reason": "not_initialized" })),
          Some(c) => match c.health_check().await {
              Ok(true)  => Ok(serde_json::json!({ "online": true })),
              Ok(false) | Err(_) => Ok(serde_json::json!({ "online": false })),
          },
      }
  }
  ```

- [ ] **Step 2: Register both commands in main.rs invoke_handler!**

  In `src-tauri/src/main.rs`, locate the last two entries of the `invoke_handler!` macro:

  ```rust
              // STT (SenseVoice ONNX, local)
              uclaw_core::stt::commands::stt_save_settings,
              uclaw_core::stt::commands::stt_list_microphones,
              // Global Shortcut
              update_global_shortcut,
  ```

  Replace with:

  ```rust
              // STT (SenseVoice ONNX, local)
              uclaw_core::stt::commands::stt_save_settings,
              uclaw_core::stt::commands::stt_list_microphones,
              // Connection health (Bottom Dock)
              uclaw_core::tauri_commands::get_app_health,
              uclaw_core::tauri_commands::get_memu_status,
              // Global Shortcut
              update_global_shortcut,
  ```

- [ ] **Step 3: Build Rust to verify**

  ```bash
  cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
  ```

  Expected: no output (zero errors).

- [ ] **Step 4: Commit**

  ```bash
  git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
  git commit -m "$(cat <<'EOF'
  feat(dock): add get_app_health + get_memu_status Tauri commands

  Two best-effort liveness probes backing the 3-channel ConnectionIndicator.
  get_app_health returns {backend:true} — frontend receiving Ok proves the
  Tauri backend is alive. get_memu_status delegates to MemUClient::health_check
  and always returns Ok so a memU outage never propagates errors to the agent.
  EOF
  )"
  ```

---

## Task 3: dock-atoms.ts + useConnectionStatus hook

**Files:**
- Create: `ui/src/atoms/dock-atoms.ts`
- Create: `ui/src/components/dock/useConnectionStatus.ts`
- Create (test): `ui/src/atoms/dock-atoms.test.ts`

- [ ] **Step 1: Write the failing test**

  Create `ui/src/atoms/dock-atoms.test.ts`:

  ```ts
  import { createStore } from 'jotai'
  import { describe, it, expect } from 'vitest'
  import {
    bottomDockEnabledAtom,
    internetOnlineAtom,
    backendOnlineAtom,
    memuOnlineAtom,
  } from './dock-atoms'

  describe('dock-atoms', () => {
    it('bottomDockEnabledAtom defaults to false', () => {
      const store = createStore()
      expect(store.get(bottomDockEnabledAtom)).toBe(false)
    })

    it('internetOnlineAtom defaults to true', () => {
      const store = createStore()
      expect(store.get(internetOnlineAtom)).toBe(true)
    })

    it('backendOnlineAtom defaults to true', () => {
      const store = createStore()
      expect(store.get(backendOnlineAtom)).toBe(true)
    })

    it('memuOnlineAtom defaults to null', () => {
      const store = createStore()
      expect(store.get(memuOnlineAtom)).toBeNull()
    })

    it('bottomDockEnabledAtom can be toggled', () => {
      const store = createStore()
      store.set(bottomDockEnabledAtom, true)
      expect(store.get(bottomDockEnabledAtom)).toBe(true)
    })
  })
  ```

- [ ] **Step 2: Run test — verify it fails**

  ```bash
  cd ui && npm test -- --run dock-atoms 2>&1 | tail -15
  ```

  Expected: FAIL — `Cannot find module './dock-atoms'`.

- [ ] **Step 3: Create dock-atoms.ts**

  Create `ui/src/atoms/dock-atoms.ts`:

  ```ts
  import { atomWithStorage } from 'jotai/utils'
  import { atom } from 'jotai'

  /** Persisted to localStorage; default off so Dock only shows when user opts in. */
  export const bottomDockEnabledAtom = atomWithStorage('dock:enabled', false)

  /** Mirrors navigator.onLine + online/offline events. */
  export const internetOnlineAtom = atom(true)

  /** True when get_app_health Tauri invoke succeeds. */
  export const backendOnlineAtom = atom(true)

  /**
   * null = not yet polled (initializing)
   * true = memU bridge alive
   * false = bridge offline or not initialized
   */
  export const memuOnlineAtom = atom<boolean | null>(null)
  ```

- [ ] **Step 4: Run test — verify it passes**

  ```bash
  cd ui && npm test -- --run dock-atoms 2>&1 | tail -10
  ```

  Expected: `5 passed`.

- [ ] **Step 5: Create useConnectionStatus.ts**

  Create `ui/src/components/dock/useConnectionStatus.ts`:

  ```ts
  import { useEffect, useRef } from 'react'
  import { useSetAtom } from 'jotai'
  import { invoke } from '@tauri-apps/api/core'
  import {
    internetOnlineAtom,
    backendOnlineAtom,
    memuOnlineAtom,
  } from '@/atoms/dock-atoms'

  const POLL_INTERVAL_MS = 30_000

  export function useConnectionStatus() {
    const setInternet = useSetAtom(internetOnlineAtom)
    const setBackend = useSetAtom(backendOnlineAtom)
    const setMemu = useSetAtom(memuOnlineAtom)
    const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)

    useEffect(() => {
      setInternet(navigator.onLine)

      const onOnline = () => setInternet(true)
      const onOffline = () => setInternet(false)
      window.addEventListener('online', onOnline)
      window.addEventListener('offline', onOffline)

      async function poll() {
        if (!navigator.onLine) return
        try {
          await invoke('get_app_health')
          setBackend(true)
        } catch {
          setBackend(false)
        }
        try {
          const result = await invoke<{ online: boolean }>('get_memu_status')
          setMemu(result.online)
        } catch {
          setMemu(false)
        }
      }

      void poll()
      timerRef.current = setInterval(poll, POLL_INTERVAL_MS)

      return () => {
        window.removeEventListener('online', onOnline)
        window.removeEventListener('offline', onOffline)
        if (timerRef.current !== null) clearInterval(timerRef.current)
      }
    }, [setInternet, setBackend, setMemu])
  }
  ```

- [ ] **Step 6: TypeScript check**

  ```bash
  cd ui && npx tsc --noEmit 2>&1 | head -10
  ```

  Expected: no output.

- [ ] **Step 7: Commit**

  ```bash
  git add ui/src/atoms/dock-atoms.ts ui/src/atoms/dock-atoms.test.ts ui/src/components/dock/useConnectionStatus.ts
  git commit -m "$(cat <<'EOF'
  feat(dock): dock-atoms + useConnectionStatus polling hook

  Four atoms: bottomDockEnabledAtom (persisted) + three transient connection
  atoms (internet/backend/memU). useConnectionStatus syncs internet from
  navigator.onLine events and polls the two Tauri commands every 30s,
  pausing automatically when the browser reports offline.
  EOF
  )"
  ```

---

## Task 4: DockItem + ConnectionIndicator Components

**Files:**
- Create: `ui/src/components/dock/DockItem.tsx`
- Create: `ui/src/components/dock/ConnectionIndicator.tsx`
- Create (test): `ui/src/components/dock/DockItem.test.tsx`
- Create (test): `ui/src/components/dock/ConnectionIndicator.test.tsx`

- [ ] **Step 1: Write failing tests**

  Create `ui/src/components/dock/DockItem.test.tsx`:

  ```tsx
  import { describe, it, expect, vi } from 'vitest'
  import { render, screen, fireEvent } from '@testing-library/react'
  import { DockItem } from './DockItem'
  import { Bot } from 'lucide-react'

  describe('DockItem', () => {
    it('renders the label when active', () => {
      render(
        <DockItem
          icon={<Bot size={18} />}
          label="Agent"
          isActive={true}
          index={0}
          hoveredIndex={null}
          onHoverIndexChange={vi.fn()}
          onClick={vi.fn()}
        />
      )
      const label = screen.getByText('Agent')
      expect(label).toBeInTheDocument()
    })

    it('calls onClick when clicked', () => {
      const onClick = vi.fn()
      render(
        <DockItem
          icon={<Bot size={18} />}
          label="Agent"
          isActive={false}
          index={0}
          hoveredIndex={null}
          onHoverIndexChange={vi.fn()}
          onClick={onClick}
        />
      )
      fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
      expect(onClick).toHaveBeenCalledOnce()
    })
  })
  ```

  Create `ui/src/components/dock/ConnectionIndicator.test.tsx`:

  ```tsx
  import { describe, it, expect } from 'vitest'
  import { render, screen } from '@testing-library/react'
  import { createStore } from 'jotai'
  import { JotaiProvider } from '@/test-utils/render'
  import { ConnectionIndicator } from './ConnectionIndicator'
  import { internetOnlineAtom, backendOnlineAtom, memuOnlineAtom } from '@/atoms/dock-atoms'

  function renderWithStore(overrides: { internet?: boolean; backend?: boolean; memu?: boolean | null } = {}) {
    const store = createStore()
    if (overrides.internet !== undefined) store.set(internetOnlineAtom, overrides.internet)
    if (overrides.backend !== undefined) store.set(backendOnlineAtom, overrides.backend)
    if (overrides.memu !== undefined) store.set(memuOnlineAtom, overrides.memu)
    return render(
      <JotaiProvider store={store}>
        <ConnectionIndicator />
      </JotaiProvider>
    )
  }

  describe('ConnectionIndicator', () => {
    it('renders the status container', () => {
      renderWithStore({ internet: true, backend: true, memu: true })
      expect(screen.getByLabelText('连接状态')).toBeInTheDocument()
    })

    it('renders three dots', () => {
      const { container } = renderWithStore({ internet: true, backend: true, memu: true })
      const dots = container.querySelectorAll('[class*="rounded-full"]')
      expect(dots.length).toBeGreaterThanOrEqual(3)
    })
  })
  ```

- [ ] **Step 2: Run tests — verify they fail**

  ```bash
  cd ui && npm test -- --run "DockItem|ConnectionIndicator" 2>&1 | tail -10
  ```

  Expected: FAIL — module not found.

- [ ] **Step 3: Create DockItem.tsx**

  Create `ui/src/components/dock/DockItem.tsx`:

  ```tsx
  import * as React from 'react'
  import { motion, useSpring } from 'motion/react'
  import { cn } from '@/lib/utils'

  interface DockItemProps {
    icon: React.ReactNode
    label: string
    isActive: boolean
    index: number
    hoveredIndex: number | null
    onHoverIndexChange: (index: number | null) => void
    onClick: () => void
  }

  export function DockItem({
    icon,
    label,
    isActive,
    index,
    hoveredIndex,
    onHoverIndexChange,
    onClick,
  }: DockItemProps) {
    const distance = hoveredIndex === null ? Infinity : Math.abs(index - hoveredIndex)

    const scaleSpring = useSpring(1, { stiffness: 320, damping: 22 })
    const ySpring = useSpring(0, { stiffness: 320, damping: 22 })

    React.useEffect(() => {
      if (distance === 0) {
        scaleSpring.set(1.38)
        ySpring.set(-5)
      } else if (distance === 1) {
        scaleSpring.set(1.15)
        ySpring.set(-2)
      } else {
        scaleSpring.set(1)
        ySpring.set(0)
      }
    }, [distance, scaleSpring, ySpring])

    // L1: label expands on hover or when active
    const showLabel = distance === 0 || isActive

    return (
      <motion.button
        className="relative flex flex-col items-center gap-0.5 select-none outline-none focus-visible:ring-2 focus-visible:ring-indigo-500/50 rounded-[11px]"
        style={{ scale: scaleSpring, y: ySpring }}
        onMouseEnter={() => onHoverIndexChange(index)}
        onMouseLeave={() => onHoverIndexChange(null)}
        onClick={onClick}
        aria-label={label}
        aria-pressed={isActive}
      >
        <div
          className={cn(
            'w-10 h-10 rounded-[11px] flex items-center justify-center transition-colors duration-150',
            isActive
              ? 'bg-gradient-to-b from-indigo-500/40 to-indigo-600/30 ring-1 ring-indigo-500/50 shadow-[0_0_12px_rgba(99,102,241,0.4)]'
              : 'bg-white/[0.08] hover:bg-white/[0.12]'
          )}
        >
          {icon}
        </div>
        {/* Active dot */}
        {isActive && (
          <span className="absolute -bottom-1.5 w-1 h-1 rounded-full bg-indigo-400" />
        )}
        {/* L1 label — max-width transition */}
        <span
          className="text-[10px] text-white/60 font-medium overflow-hidden whitespace-nowrap"
          style={{
            maxWidth: showLabel ? '60px' : '0px',
            opacity: showLabel ? 1 : 0,
            transition: 'max-width 500ms cubic-bezier(0.22, 1, 0.36, 1), opacity 500ms cubic-bezier(0.22, 1, 0.36, 1)',
          }}
        >
          {label}
        </span>
      </motion.button>
    )
  }
  ```

- [ ] **Step 4: Create ConnectionIndicator.tsx**

  Create `ui/src/components/dock/ConnectionIndicator.tsx`:

  ```tsx
  import { useAtomValue } from 'jotai'
  import { cn } from '@/lib/utils'
  import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
  } from '@/components/ui/tooltip'
  import { internetOnlineAtom, backendOnlineAtom, memuOnlineAtom } from '@/atoms/dock-atoms'

  type DotState = 'online' | 'warning' | 'offline'

  function StatusDot({ tooltipText, state }: { tooltipText: string; state: DotState }) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            className={cn(
              'block w-1.5 h-1.5 rounded-full cursor-default',
              state === 'online' && 'bg-sage-500 shadow-[0_0_4px_theme(colors.sage.500)]',
              state === 'warning' && 'bg-amber-500',
              state === 'offline' && 'bg-coral-500',
            )}
          />
        </TooltipTrigger>
        <TooltipContent side="top">
          {tooltipText}
        </TooltipContent>
      </Tooltip>
    )
  }

  export function ConnectionIndicator() {
    const internet = useAtomValue(internetOnlineAtom)
    const backend = useAtomValue(backendOnlineAtom)
    const memu = useAtomValue(memuOnlineAtom)

    const netState: DotState = internet ? 'online' : 'offline'
    const backendState: DotState = !internet ? 'offline' : backend ? 'online' : 'offline'
    const memuState: DotState = !internet ? 'offline' : memu === null ? 'warning' : memu ? 'online' : 'offline'

    return (
      <TooltipProvider delayDuration={300}>
        <div className="flex items-center gap-1.5" aria-label="连接状态">
          <StatusDot
            state={netState}
            tooltipText={`网络：${internet ? '在线' : '离线'}`}
          />
          <StatusDot
            state={backendState}
            tooltipText={`后端：${!internet ? '离线' : backend ? '在线' : '离线'}`}
          />
          <StatusDot
            state={memuState}
            tooltipText={`memU：${!internet ? '离线' : memu === null ? '初始化中' : memu ? '在线' : '离线'}`}
          />
        </div>
      </TooltipProvider>
    )
  }
  ```

- [ ] **Step 5: Run tests — verify they pass**

  ```bash
  cd ui && npm test -- --run "DockItem|ConnectionIndicator" 2>&1 | tail -10
  ```

  Expected: `4 passed`.

- [ ] **Step 6: TypeScript check**

  ```bash
  cd ui && npx tsc --noEmit 2>&1 | head -10
  ```

  Expected: no output.

- [ ] **Step 7: Commit**

  ```bash
  git add ui/src/components/dock/DockItem.tsx ui/src/components/dock/DockItem.test.tsx ui/src/components/dock/ConnectionIndicator.tsx ui/src/components/dock/ConnectionIndicator.test.tsx
  git commit -m "$(cat <<'EOF'
  feat(dock): DockItem (L1+L2 animation) + ConnectionIndicator (3-channel dots)

  DockItem: framer-motion spring scale (1→1.38 self, 1.15 neighbor) driven by
  hoveredIndex from parent; L1 label expands via max-width CSS transition on
  active or hovered. ConnectionIndicator: three 6px dots (sage/amber/coral)
  reading from dock-atoms, with Radix Tooltip showing channel detail on hover.
  EOF
  )"
  ```

---

## Task 5: BottomDock Component + AppShell Mount

**Files:**
- Create: `ui/src/components/dock/BottomDock.tsx`
- Modify: `ui/src/components/app-shell/AppShell.tsx`

- [ ] **Step 1: Create BottomDock.tsx**

  Create `ui/src/components/dock/BottomDock.tsx`:

  ```tsx
  import * as React from 'react'
  import { motion } from 'motion/react'
  import { useAtomValue, useSetAtom } from 'jotai'
  import { MessageSquare, Bot, Brain, Sparkles } from 'lucide-react'
  import { DockItem } from './DockItem'
  import { ConnectionIndicator } from './ConnectionIndicator'
  import { useConnectionStatus } from './useConnectionStatus'
  import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'
  import { appModeAtom, type AppMode } from '@/atoms/app-mode'
  import { topLevelViewAtom, type TopLevelView } from '@/atoms/top-level-view'
  import { kaleidoscopeModuleAtom, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'

  interface BottomDockProps {
    revealed: boolean
    onRevealChange: (revealed: boolean) => void
  }

  interface NavCtx {
    appMode: AppMode
    topLevelView: TopLevelView
    kaleidoscopeModule: KaleidoscopeModuleId
  }

  interface ActionCtx {
    setAppMode: (m: AppMode) => void
    setTopLevelView: (v: TopLevelView) => void
    setKaleidoscopeModule: (m: KaleidoscopeModuleId) => void
  }

  interface NavItem {
    id: string
    icon: React.ReactNode
    label: string
    isActive: (ctx: NavCtx) => boolean
    onClick: (ctx: ActionCtx) => void
  }

  const NAV_ITEMS: NavItem[] = [
    {
      id: 'chat',
      icon: <MessageSquare size={18} className="text-white/80" />,
      label: '聊天',
      isActive: ({ appMode, topLevelView }) =>
        appMode === 'chat' && topLevelView === 'workspace',
      onClick: ({ setAppMode, setTopLevelView }) => {
        setAppMode('chat')
        setTopLevelView('workspace')
      },
    },
    {
      id: 'agent',
      icon: <Bot size={18} className="text-white/80" />,
      label: 'Agent',
      isActive: ({ appMode, topLevelView }) =>
        appMode === 'agent' && topLevelView === 'workspace',
      onClick: ({ setAppMode, setTopLevelView }) => {
        setAppMode('agent')
        setTopLevelView('workspace')
      },
    },
    {
      id: 'memory',
      icon: <Brain size={18} className="text-white/80" />,
      label: '记忆',
      isActive: ({ topLevelView, kaleidoscopeModule }) =>
        topLevelView === 'kaleidoscope' && kaleidoscopeModule === 'memory',
      onClick: ({ setKaleidoscopeModule, setTopLevelView }) => {
        setKaleidoscopeModule('memory')
        setTopLevelView('kaleidoscope')
      },
    },
    {
      id: 'kaleidoscope',
      icon: <Sparkles size={18} className="text-white/80" />,
      label: '万花筒',
      isActive: ({ topLevelView, kaleidoscopeModule }) =>
        topLevelView === 'kaleidoscope' && kaleidoscopeModule !== 'memory',
      onClick: ({ setTopLevelView }) => {
        setTopLevelView('kaleidoscope')
      },
    },
  ]

  export function BottomDock({ revealed, onRevealChange }: BottomDockProps) {
    const isDockEnabled = useAtomValue(bottomDockEnabledAtom)
    const appMode = useAtomValue(appModeAtom)
    const topLevelView = useAtomValue(topLevelViewAtom)
    const kaleidoscopeModule = useAtomValue(kaleidoscopeModuleAtom)
    const setAppMode = useSetAtom(appModeAtom)
    const setTopLevelView = useSetAtom(topLevelViewAtom)
    const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)

    const [hoveredIndex, setHoveredIndex] = React.useState<number | null>(null)
    const hideTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

    useConnectionStatus()

    if (!isDockEnabled) return null

    const navCtx: NavCtx = { appMode, topLevelView, kaleidoscopeModule }
    const actionCtx: ActionCtx = { setAppMode, setTopLevelView, setKaleidoscopeModule }

    const handleDockMouseEnter = () => {
      if (hideTimerRef.current !== null) clearTimeout(hideTimerRef.current)
    }

    const handleDockMouseLeave = () => {
      setHoveredIndex(null)
      hideTimerRef.current = setTimeout(() => onRevealChange(false), 200)
    }

    return (
      <motion.div
        className="fixed bottom-0 left-1/2 -translate-x-1/2 z-[70] pointer-events-auto"
        animate={{ y: revealed ? 0 : 'calc(100% + 8px)' }}
        initial={{ y: 'calc(100% + 8px)' }}
        transition={{ type: 'spring', stiffness: 300, damping: 28 }}
        onMouseEnter={handleDockMouseEnter}
        onMouseLeave={handleDockMouseLeave}
      >
        <div className="flex items-end gap-2 px-4 pt-3 pb-2 rounded-t-2xl bg-black/70 backdrop-blur-xl border-t border-x border-white/[0.08] shadow-[0_-4px_24px_rgba(0,0,0,0.4)]">
          {NAV_ITEMS.map((item, i) => (
            <DockItem
              key={item.id}
              icon={item.icon}
              label={item.label}
              isActive={item.isActive(navCtx)}
              index={i}
              hoveredIndex={hoveredIndex}
              onHoverIndexChange={(idx) => setHoveredIndex(idx)}
              onClick={() => item.onClick(actionCtx)}
            />
          ))}
          <div className="ml-3 mb-1 flex items-center self-center">
            <ConnectionIndicator />
          </div>
        </div>
      </motion.div>
    )
  }
  ```

- [ ] **Step 2: Mount BottomDock + DockHotZone in AppShell.tsx**

  In `ui/src/components/app-shell/AppShell.tsx`:

  **2a.** Add two imports after the existing `QuickCaptureDialog` import (line 55):

  ```tsx
  import { BottomDock } from '@/components/dock/BottomDock'
  import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'
  ```

  **2b.** Inside the `AppShell` function body, after `const focusMode = useAtomValue(focusModeAtom)` (line 69), add:

  ```tsx
    const isDockEnabled = useAtomValue(bottomDockEnabledAtom)
    const [dockRevealed, setDockRevealed] = React.useState(false)
  ```

  **2c.** After `<QuickCaptureDialog />` (line 398) and before the closing `</div>` (line 399), add:

  ```tsx
        {/* 底部 Dock 感应区：16px 不可见热区，鼠标触底时唤出 Dock */}
        {isDockEnabled && (
          <div
            className="fixed bottom-0 inset-x-0 h-4 pointer-events-auto z-[70]"
            onMouseEnter={() => setDockRevealed(true)}
          />
        )}
        {/* macOS Dock 风格底部导航栏 — 触底滑出，离开后自动收回 */}
        <BottomDock revealed={dockRevealed} onRevealChange={setDockRevealed} />
  ```

- [ ] **Step 3: TypeScript check**

  ```bash
  cd ui && npx tsc --noEmit 2>&1 | head -10
  ```

  Expected: no output.

- [ ] **Step 4: Manual smoke test**

  Run `cargo tauri dev` from `src-tauri/`. Then:
  1. Open Settings → General; the new 外观 section is not yet there (that's Task 6).
  2. Manually set `localStorage.setItem('dock:enabled', 'true')` in the browser console and reload.
  3. Move the mouse to the bottom edge of the window — the Dock should slide up.
  4. Move the mouse away — the Dock should slide back down after ~200ms.
  5. Hover over each icon — L2 scale animation on icon + neighbors.
  6. Click `Agent` icon — `appModeAtom` switches to `agent`.
  7. Verify the three connection dots appear in the right section of the Dock.

- [ ] **Step 5: Commit**

  ```bash
  git add ui/src/components/dock/BottomDock.tsx ui/src/components/app-shell/AppShell.tsx
  git commit -m "$(cat <<'EOF'
  feat(dock): BottomDock main component + AppShell hot-zone mount

  BottomDock: framer-motion spring reveal (stiffness:300, damping:28),
  4 nav items synced to existing atoms, ConnectionIndicator on right.
  AppShell: 16px fixed hot-zone triggers reveal; BottomDock renders null
  when bottomDockEnabledAtom is false (zero cost when disabled).
  EOF
  )"
  ```

---

## Task 6: GeneralSettings 外观 Section + Toggle

**Files:**
- Modify: `ui/src/components/settings/GeneralSettings.tsx`

- [ ] **Step 1: Add the Jotai import + atom read to GeneralSettings.tsx**

  In `ui/src/components/settings/GeneralSettings.tsx`, add after the last existing import (line 7):

  ```tsx
  import { useAtom } from 'jotai'
  import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'
  ```

- [ ] **Step 2: Add the atom read inside the component function**

  Inside `GeneralSettings()`, after the `showTimestamp` state line (line 19), add:

  ```tsx
    const [bottomDockEnabled, setBottomDockEnabled] = useAtom(bottomDockEnabledAtom)
  ```

- [ ] **Step 3: Add the 外观 section in JSX**

  In `GeneralSettings.tsx`, after the closing `</SettingsSection>` of the `消息` section (line 59), add a new section:

  ```tsx
      <SettingsSection title="外观">
        <SettingsCard>
          <SettingsToggle
            label="底部 Dock 导航栏"
            description="触底滑出，macOS Dock 风格快速导航。开启后鼠标移至窗口底边缘时 Dock 自动滑出，移开后自动收回。"
            checked={bottomDockEnabled}
            onCheckedChange={setBottomDockEnabled}
          />
        </SettingsCard>
      </SettingsSection>
  ```

- [ ] **Step 4: TypeScript check**

  ```bash
  cd ui && npx tsc --noEmit 2>&1 | head -10
  ```

  Expected: no output.

- [ ] **Step 5: Manual verification**

  With `cargo tauri dev` running:
  1. Open Settings → General — the 外观 section appears at the bottom.
  2. Toggle "底部 Dock 导航栏" ON → Dock hot-zone activates immediately (no reload needed).
  3. Hover the bottom edge → Dock slides up.
  4. Toggle OFF → Dock disappears.
  5. Reload app → preference persists (stored in `localStorage` key `dock:enabled`).

- [ ] **Step 6: Run full test suite**

  ```bash
  cd ui && npm test -- --run 2>&1 | tail -10
  ```

  Expected: all existing tests still pass + the 7 new dock tests pass.

- [ ] **Step 7: Commit**

  ```bash
  git add ui/src/components/settings/GeneralSettings.tsx
  git commit -m "$(cat <<'EOF'
  feat(dock): GeneralSettings 外观 section with Dock enable toggle

  Adds a new 外观 section to General settings with a single toggle bound
  to bottomDockEnabledAtom (persisted via atomWithStorage). Change takes
  effect immediately — no reload required.
  EOF
  )"
  ```

---

## Self-Review Checklist

**Spec coverage:**

| Spec requirement | Task |
|---|---|
| sage/coral/amber Tailwind scales | Task 1 |
| get_app_health Tauri command | Task 2 |
| get_memu_status Tauri command | Task 2 |
| main.rs invoke_handler registration | Task 2 |
| bottomDockEnabledAtom (atomWithStorage, default false) | Task 3 |
| internetOnlineAtom / backendOnlineAtom / memuOnlineAtom | Task 3 |
| useConnectionStatus 30s polling + navigator.onLine | Task 3 |
| DockItem L1 label expand (max-width CSS transition) | Task 4 |
| DockItem L2 spring scale (hoveredIndex-driven) | Task 4 |
| ConnectionIndicator 3 dots + Tooltip per channel | Task 4 |
| BottomDock spring reveal (stiffness 300, damping 28) | Task 5 |
| DockHotZone 16px fixed strip | Task 5 |
| AppShell mount: isDockEnabled + dockRevealed state | Task 5 |
| 200ms auto-hide on mouse leave | Task 5 |
| 4 nav items (chat/agent/memory/kaleidoscope) | Task 5 |
| GeneralSettings 外观 section + toggle | Task 6 |

**Type consistency check:**

- `DockItem` props: `index`, `hoveredIndex: number | null`, `onHoverIndexChange: (index: number | null) => void` — used consistently in Task 4 (definition) and Task 5 (BottomDock usage). ✓
- `BottomDock` props: `revealed: boolean`, `onRevealChange: (revealed: boolean) => void` — defined in Task 5, called with `dockRevealed` / `setDockRevealed` in AppShell. ✓
- `bottomDockEnabledAtom` — exported from `dock-atoms.ts` (Task 3), imported in `BottomDock.tsx` (Task 5), `AppShell.tsx` (Task 5), `GeneralSettings.tsx` (Task 6). ✓
- `useConnectionStatus` — no return value, called once inside `BottomDock` (Task 5). ✓
- Tailwind classes `bg-sage-500`, `bg-coral-500`, `bg-amber-500` — scales registered in Task 1, used in `ConnectionIndicator` (Task 4). ✓
