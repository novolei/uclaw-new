# Slice E — Floating Desktop Pet Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A transparent, always-on-top floating pet window (form factor A): idle shows only the sprite; click expands a compact chat panel that talks to the **local** MiniCPM engine (`:7337`, local-only — never cloud); a restrained proactive bubble appears on key cross-window events. Reuses the existing `PetWidget` sprites + `pet-atoms` state machine.

**Architecture:** A second Tauri `WebviewWindow` created from JS (mirroring `ui/src/lib/automation-login-window.ts` — the established uclaw pattern; **not** Rust commands, per CLAUDE.md "match the codebase shape"). It loads the same bundle with `?uclawWindow=pet`; `main.tsx`'s root switch renders `<PetWindow/>` instead of `<App/>`. The pet webview is its own JS/jotai context: it drives its own pet state locally and calls `:7337/v1/chat/completions` directly via `fetch` (SSE). Cross-window coordination uses Tauri events (`pet://nudge`, `pet://open-wizard`). **No Rust changes**; one `tauri.conf.json` CSP edit so the pet webview may reach `localhost:7337`.

**Tech Stack:** React + jotai + `@tauri-apps/api/webviewWindow` (`WebviewWindow`) + `@tauri-apps/api/event` (`emit`/`listen`) + `fetch` SSE; Vitest/jsdom with mocked Tauri APIs. Reuses Slice B's `:7337` endpoint (no-think default from #659) + Slice D's wizard.

---

## Boundary / key facts (recon)

- **Window creation pattern:** `ui/src/lib/automation-login-window.ts` → `new WebviewWindow(label, opts)` from `@tauri-apps/api/webviewWindow`. Secondary windows load `index.html?uclawWindow=<name>`; `main.tsx:138` switches on `new URLSearchParams(window.location.search).get('uclawWindow')`.
- **Permissions (already granted, `src-tauri/capabilities/default.json`):** `core:webview:allow-create-webview-window`, `core:window:allow-{close,show,hide,set-position,set-focus,set-size,start-dragging}`. No capability change needed.
- **`macOSPrivateApi: true`** already set (`tauri.conf.json:13`) → transparent windows work on macOS.
- **CSP blocker:** `tauri.conf.json` `connect-src` lacks `localhost:7337` → the pet webview's `fetch` to the local engine would be blocked. **Must add** `http://localhost:7337 http://127.0.0.1:7337`.
- **Pet state:** `ui/src/atoms/pet-atoms.ts` — `petEnabledAtom`/`petCharacterAtom` (`atomWithStorage`, localStorage), `petPrimaryStateAtom` (idle/thinking/typing/success/error), `petDisplayStateAtom` (derived). `PetWidget` (`ui/src/components/agent/PetWidget.tsx`) renders `/pet/<char>-<state>.webp` crossfade; returns null if `!petEnabledAtom`.
- **Each WebviewWindow is a separate jotai store.** The pet window reads `petCharacterAtom`/`petEnabledAtom` from localStorage (initial) and drives `petPrimaryStateAtom` **locally** from its own chat generation (NOT `usePetStateSync`, which is the main-agent sync). Window open/close is driven imperatively by the PetSettings toggle calling `pet-window.ts`, not by cross-window atom reactivity.
- **Local endpoint:** `POST http://127.0.0.1:7337/v1/chat/completions`, `{model:"local/minicpm5-1b", messages, stream:true}`. 503 body = model not ready (Slice B). No-think is the server default (#659), so responses are clean.
- **Wizard:** Slice D's `MiniCPMWizard` + `minicpmWizardAtom` live in the **main** window. The pet (separate window) signals "open wizard" via a Tauri event the main window listens for.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/tauri.conf.json` | CSP `connect-src` += `http://localhost:7337 http://127.0.0.1:7337` |
| `ui/src/lib/pet-window.ts` | open/close/toggle/setPosition the pet `WebviewWindow` (mirrors automation-login-window.ts) |
| `ui/src/components/settings/PetSettings.tsx` | toggle also opens/closes the pet window |
| `ui/src/main.tsx` | root switch: `uclawWindow==='pet'` → `<PetWindow/>` |
| `ui/src/components/pet/PetWindow.tsx` | transparent shell: drag region, idle sprite (PetWidget), click→expand chat panel, proactive bubble |
| `ui/src/components/pet/PetChat.tsx` | compact chat panel: message list + input, SSE streaming from :7337, local state machine, 503→wizard handoff |
| `ui/src/lib/pet-chat.ts` | `streamPetChat(messages, onDelta)` — fetch SSE to :7337; throws `PetModelNotReady` on 503 |
| `ui/src/atoms/pet-chat-atoms.ts` | in-memory pet chat history + proactive bubble atom |
| `ui/src/components/agent/AgentView.tsx` (or wherever main emits) | emit `pet://nudge` on model-ready / long-task-done (restrained) |

All new `.ts`/`.tsx` files: no SPDX needed (frontend convention — match existing UI files, which have none).

---

## Task 1: CSP + pet-window.ts opener + PetSettings toggle wiring

**Files:** `src-tauri/tauri.conf.json`, `ui/src/lib/pet-window.ts` (+ test), `ui/src/components/settings/PetSettings.tsx`

- [ ] **Step 1 (TDD): write `ui/src/lib/pet-window.test.ts`** mocking `@tauri-apps/api/webviewWindow`:
  - `openPetWindow()` calls `WebviewWindow.getByLabel('pet')`; if absent, constructs `new WebviewWindow('pet', opts)` with `transparent:true, decorations:false, alwaysOnTop:true, skipTaskbar:true, resizable:false, shadow:false` and url containing `uclawWindow=pet`; if present, `.show()`+`.focus()`.
  - `closePetWindow()` → `getByLabel('pet')?.close()`.
  - `togglePetWindow(enabled)` → open when true, close when false.
  Assert the constructor opts + the getByLabel/close calls (mock returns a fake window with `show/focus/close/setPosition` spies).

- [ ] **Step 2: implement `ui/src/lib/pet-window.ts`** (mirror `automation-login-window.ts`):
```typescript
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'

export const PET_WINDOW_LABEL = 'pet'

const PET_WINDOW_OPTS = {
  url: 'index.html?uclawWindow=pet',
  width: 360,
  height: 480,
  transparent: true,
  decorations: false,
  alwaysOnTop: true,
  skipTaskbar: true,
  resizable: false,
  shadow: false,
  focus: false,
  // bottom-right default; position persistence is handled by the window itself
  // via setPosition (Task 2). x/y omitted → OS default placement.
} as const

export async function openPetWindow(): Promise<void> {
  const existing = await WebviewWindow.getByLabel(PET_WINDOW_LABEL)
  if (existing) {
    await existing.show()
    await existing.setFocus().catch(() => {})
    return
  }
  // eslint-disable-next-line no-new
  new WebviewWindow(PET_WINDOW_LABEL, PET_WINDOW_OPTS)
}

export async function closePetWindow(): Promise<void> {
  const existing = await WebviewWindow.getByLabel(PET_WINDOW_LABEL)
  await existing?.close()
}

export async function togglePetWindow(enabled: boolean): Promise<void> {
  if (enabled) await openPetWindow()
  else await closePetWindow()
}
```

- [ ] **Step 3: CSP** — in `src-tauri/tauri.conf.json`, append to the `connect-src` directive (after `'self'`): ` http://localhost:7337 http://127.0.0.1:7337`. (Add to the existing `connect-src` list; do not remove the existing cloud hosts.)

- [ ] **Step 4: PetSettings toggle** — in `PetSettings.tsx`, change `onCheckedChange={setEnabled}` to also drive the window:
```tsx
import { togglePetWindow } from '@/lib/pet-window'
// ...
onCheckedChange={(v) => { setEnabled(v); void togglePetWindow(v) }}
```

- [ ] **Step 5: verify** — `cd ui && npx tsc --noEmit 2>&1 | head` (no new errors); `npm test -- --run pet-window 2>&1 | tail` (opener tests pass).

- [ ] **Step 6: commit**
```bash
git add src-tauri/tauri.conf.json ui/src/lib/pet-window.ts ui/src/lib/pet-window.test.ts ui/src/components/settings/PetSettings.tsx
git commit -m "feat(pet): pet WebviewWindow opener + CSP for :7337 + settings toggle

Slice E Task 1. pet-window.ts opens a transparent/always-on-top/skip-taskbar
WebviewWindow loading ?uclawWindow=pet (mirrors automation-login-window.ts);
PetSettings toggle opens/closes it; CSP connect-src += localhost:7337 so the pet
webview can reach the local engine. No Rust (JS window control, per codebase)."
```

---

## Task 2: main.tsx root switch + PetWindow shell (idle sprite + click-expand + drag)

**Files:** `ui/src/main.tsx`, `ui/src/components/pet/PetWindow.tsx` (+ test)

- [ ] **Step 1: main.tsx root switch** — after the `isAutomationLoginBrowserWindow` block, add:
```tsx
const isPetWindow =
  new URLSearchParams(window.location.search).get('uclawWindow') === 'pet'
```
and a branch in the render: `isPetWindow ? <PetWindow /> : ...` (place before the automation/main branches; the pet window needs neither GlobalShortcuts nor the full App). Import `PetWindow` from `@/components/pet/PetWindow`. The pet branch renders ONLY `<PetWindow />` + `<Toaster />` (no `<App/>`).

- [ ] **Step 2: PetWindow.tsx shell** (TDD the expand/collapse + drag region + sprite):
```tsx
import * as React from 'react'
import { PetWidget } from '@/components/agent/PetWidget'
import { PetChat } from './PetChat'
import './PetWindow.css'

export function PetWindow(): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  return (
    <div className="pet-window-root">
      {/* drag region: the sprite area is draggable; click (no drag) toggles chat */}
      <div
        className="pet-sprite"
        data-tauri-drag-region
        onClick={() => setExpanded((e) => !e)}
        data-testid="pet-sprite"
      >
        <PetWidget />
      </div>
      {expanded && (
        <div className="pet-panel" data-testid="pet-panel">
          <PetChat onClose={() => setExpanded(false)} />
        </div>
      )}
    </div>
  )
}
```
Plus `PetWindow.css`: `html,body,#root{background:transparent!important}` and `.pet-window-root{...}` sizing the sprite (small) + panel; the transparent areas must not capture clicks (size the root to content). NOTE: `PetWidget` returns null if `petEnabledAtom` is false — in the pet window the persisted `pet.enabled` is true (the window only opens when enabled), so it renders. If a test mounts it with enabled=false, wrap PetWidget rendering with a forced-enabled fallback OR seed the atom; for the shell test, seed `petEnabledAtom=true` in a jotai store.

- [ ] **Step 3: PetWindow.test.tsx** — render with a jotai store seeding `petEnabledAtom=true` + `petCharacterAtom='astro'`; mock `./PetChat` (`vi.mock('./PetChat', () => ({ PetChat: () => <div data-testid="petchat-mock"/> }))`); assert: sprite present, panel absent initially; click sprite → panel appears; click again → panel gone.

- [ ] **Step 4: verify** — `npx tsc --noEmit` clean; `npm test -- --run PetWindow` passes.

- [ ] **Step 5: commit**
```bash
git add ui/src/main.tsx ui/src/components/pet/PetWindow.tsx ui/src/components/pet/PetWindow.css ui/src/components/pet/PetWindow.test.tsx
git commit -m "feat(pet): PetWindow shell — transparent root, drag region, click-expand

Slice E Task 2. main.tsx renders <PetWindow/> for ?uclawWindow=pet; idle shows
the PetWidget sprite (draggable via data-tauri-drag-region), click toggles a
compact chat panel. Transparent CSS. Vitest covers expand/collapse."
```

---

## Task 3: pet local chat (SSE from :7337, local state machine, 503→wizard handoff)

**Files:** `ui/src/lib/pet-chat.ts` (+ test), `ui/src/atoms/pet-chat-atoms.ts`, `ui/src/components/pet/PetChat.tsx` (+ test)

- [ ] **Step 1: `pet-chat-atoms.ts`** — in-memory history:
```typescript
import { atom } from 'jotai'
export interface PetMsg { role: 'user' | 'assistant'; content: string }
export const petHistoryAtom = atom<PetMsg[]>([])
export const petBubbleAtom = atom<string | null>(null)   // proactive bubble text
```

- [ ] **Step 2: `pet-chat.ts`** — SSE streaming + not-ready signal:
```typescript
export class PetModelNotReady extends Error {}

export interface PetChatMsg { role: 'user' | 'assistant' | 'system'; content: string }

/** Stream a completion from the LOCAL engine. Calls onDelta(text) per chunk.
 *  Throws PetModelNotReady on 503 (model not downloaded/loaded). */
export async function streamPetChat(
  messages: PetChatMsg[],
  onDelta: (t: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  const resp = await fetch('http://127.0.0.1:7337/v1/chat/completions', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ model: 'local/minicpm5-1b', messages, stream: true }),
    signal,
  })
  if (resp.status === 503) throw new PetModelNotReady('model not ready')
  if (!resp.ok || !resp.body) throw new Error(`pet chat HTTP ${resp.status}`)
  const reader = resp.body.getReader()
  const decoder = new TextDecoder()
  let buf = ''
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    buf += decoder.decode(value, { stream: true })
    const lines = buf.split('\n')
    buf = lines.pop() ?? ''
    for (const line of lines) {
      const s = line.trim()
      if (!s.startsWith('data:')) continue
      const data = s.slice(5).trim()
      if (data === '[DONE]') return
      try {
        const json = JSON.parse(data)
        const delta = json?.choices?.[0]?.delta?.content
        if (typeof delta === 'string' && delta) onDelta(delta)
      } catch { /* ignore keep-alive / partial */ }
    }
  }
}
```

- [ ] **Step 3: `pet-chat.test.ts`** — mock `global.fetch`:
  - streaming: fetch resolves a `ReadableStream` emitting two `data: {"choices":[{"delta":{"content":"你"}}]}` / `data: {...好}` lines + `data: [DONE]`; assert `onDelta` called with "你" then "好".
  - 503: fetch resolves `{status:503}` → `streamPetChat` rejects with `PetModelNotReady`.

- [ ] **Step 4: `PetChat.tsx`** — panel UI + state machine + wizard handoff:
```tsx
import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { emit } from '@tauri-apps/api/event'
import { useSetAtom as _ } from 'jotai' // (remove if unused)
import { petPrimaryStateAtom } from '@/atoms/pet-atoms'
import { petHistoryAtom, petBubbleAtom } from '@/atoms/pet-chat-atoms'
import { streamPetChat, PetModelNotReady, type PetChatMsg } from '@/lib/pet-chat'

export function PetChat({ onClose }: { onClose: () => void }): React.ReactElement {
  const [history, setHistory] = useAtom(petHistoryAtom)
  const setPrimary = useSetAtom(petPrimaryStateAtom)
  const setBubble = useSetAtom(petBubbleAtom)
  const [input, setInput] = React.useState('')
  const [streaming, setStreaming] = React.useState(false)

  const send = async () => {
    const text = input.trim()
    if (!text || streaming) return
    setInput('')
    const next = [...history, { role: 'user' as const, content: text }]
    setHistory(next)
    setStreaming(true)
    setPrimary('thinking')
    // assistant placeholder we stream into
    let acc = ''
    setHistory([...next, { role: 'assistant', content: '' }])
    const msgs: PetChatMsg[] = next.map((m) => ({ role: m.role, content: m.content }))
    try {
      await streamPetChat(msgs, (d) => {
        acc += d
        setPrimary('typing')
        setHistory([...next, { role: 'assistant', content: acc }])
      })
      setPrimary('success')
      setTimeout(() => setPrimary('idle'), 1200)
    } catch (e) {
      if (e instanceof PetModelNotReady) {
        setBubble('我还在热身,去把模型装好呀~')
        // ask the MAIN window to open the onboarding wizard
        void emit('pet://open-wizard')
        // roll back the empty assistant placeholder
        setHistory(next)
      } else {
        setPrimary('error')
        setTimeout(() => setPrimary('idle'), 1500)
      }
    } finally {
      setStreaming(false)
    }
  }

  return (
    <div className="pet-chat" data-testid="pet-chat">
      <div className="pet-chat-msgs">
        {history.map((m, i) => (
          <div key={i} className={`pet-msg pet-msg-${m.role}`}>{m.content}</div>
        ))}
      </div>
      <div className="pet-chat-input">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void send() }}
          placeholder="和我聊聊…"
          disabled={streaming}
        />
        <button type="button" onClick={() => void send()} disabled={streaming}>发送</button>
        <button type="button" onClick={onClose} aria-label="收起">×</button>
      </div>
    </div>
  )
}
```
(Clean up the stray `useSetAtom as _` import — that line is a placeholder; remove it. Keep imports minimal/correct.)

- [ ] **Step 5: `PetChat.test.tsx`** — mock `@tauri-apps/api/event` (`emit`) + `@/lib/pet-chat`:
  - happy path: `streamPetChat` mock invokes `onDelta('你好')` then resolves; type + send → history shows user msg + streamed assistant "你好".
  - not-ready: `streamPetChat` mock rejects `new PetModelNotReady()` → asserts `emit` called with `'pet://open-wizard'` and the bubble atom is set (render petBubbleAtom via a probe or assert emit).

- [ ] **Step 6: verify** — `npx tsc --noEmit` clean; `npm test -- --run pet-chat PetChat` pass.

- [ ] **Step 7: commit**
```bash
git add ui/src/lib/pet-chat.ts ui/src/lib/pet-chat.test.ts ui/src/atoms/pet-chat-atoms.ts ui/src/components/pet/PetChat.tsx ui/src/components/pet/PetChat.test.tsx
git commit -m "feat(pet): local-only pet chat (SSE from :7337) + wizard handoff

Slice E Task 3. streamPetChat fetches the local engine's SSE, drives the pet
state machine (thinking/typing/success), keeps in-memory history. Local-only:
on 503 model-not-ready it shows a warm bubble + emits pet://open-wizard (never
silently falls back to cloud). Vitest covers streaming + the not-ready path."
```

---

## Task 4: proactive bubble + cross-window wiring (pet://nudge, pet://open-wizard)

**Files:** `ui/src/components/pet/PetWindow.tsx` (bubble render + nudge listener), `ui/src/App.tsx` (main listens pet://open-wizard → open wizard), main-side emit site for `pet://nudge`

- [ ] **Step 1: PetWindow listens `pet://nudge` + renders the bubble** — add to PetWindow:
```tsx
import { listen } from '@tauri-apps/api/event'
import { useAtom } from 'jotai'
import { petBubbleAtom } from '@/atoms/pet-chat-atoms'
// inside PetWindow:
const [bubble, setBubble] = useAtom(petBubbleAtom)
React.useEffect(() => {
  let un: (() => void) | undefined
  let cancelled = false
  listen<{ text: string }>('pet://nudge', (e) => setBubble(e.payload?.text ?? null))
    .then((fn) => { if (cancelled) fn(); else un = fn })
  return () => { cancelled = true; un?.() }
}, [setBubble])
// auto-dismiss the bubble after a few seconds
React.useEffect(() => {
  if (!bubble) return
  const t = setTimeout(() => setBubble(null), 6000)
  return () => clearTimeout(t)
}, [bubble, setBubble])
```
Render the bubble above the sprite when `bubble` is non-null (`{bubble && <div className="pet-bubble" data-testid="pet-bubble">{bubble}</div>}`).

- [ ] **Step 2: main window listens `pet://open-wizard`** — in `App.tsx` (root), add an effect that listens and opens the wizard via `minicpmWizardAtom`:
```tsx
React.useEffect(() => {
  let un: (() => void) | undefined; let cancelled = false
  listen('pet://open-wizard', () => setWizard((s) => ({ ...s, step: 'intro' })))
    .then((fn) => { if (cancelled) fn(); else un = fn })
  return () => { cancelled = true; un?.() }
}, [setWizard])
```
(import `minicpmWizardAtom` + `useSetAtom`; place near `useOnboardingGate()`.)

- [ ] **Step 3: main emits `pet://nudge` (restrained v1 triggers).** Wire TWO emits only:
  - model-ready: when onboarding completes / model becomes ready — emit `pet://nudge {text:'本地模型准备好啦,点我聊两句~'}`. Hook into the wizard's `done`/`finish` (Slice D `MiniCPMWizard.finish()`): after `setOnboardingState('completed')`, `void emit('pet://nudge', { text: '本地模型准备好啦,点我聊两句~' })`.
  - long-task-done: when a long agent task finishes — find the existing agent stream-complete signal (`chat:stream-complete` event already emitted in tauri_commands.rs, or the frontend agent-complete handler) and, IF the task was long (heuristic: had ≥1 tool call OR ran > N seconds), `emit('pet://nudge', {text:'刚忙完一件大事!'})`. KEEP RESTRAINED — if wiring a clean "long task" signal is non-trivial, ship ONLY the model-ready nudge in v1 and leave a `// TODO Slice E+: long-task-done nudge` with a one-line note; do not over-build.

- [ ] **Step 4: tests** — `PetWindow.test.tsx`: add a case mocking `@tauri-apps/api/event` `listen` so that invoking the registered `pet://nudge` callback sets the bubble (assert `data-testid="pet-bubble"` appears). `App` wizard-open-on-event: a focused test that the `pet://open-wizard` listener sets the wizard step (can reuse the gate-test harness style; mock `listen` to capture+invoke the callback). If an App-level test is heavy, test the listener logic via a small extracted hook `usePetWizardBridge()` instead and unit-test that.

- [ ] **Step 5: verify** — `npx tsc --noEmit` clean; `npm test -- --run PetWindow pet 2>&1 | tail`; full `npm test -- --run 2>&1 | tail -12` (no NEW failures vs baseline).

- [ ] **Step 6: commit**
```bash
git add ui/src/components/pet/PetWindow.tsx ui/src/App.tsx ui/src/components/onboarding/MiniCPMWizard.tsx [+ any long-task emit site]
git commit -m "feat(pet): proactive bubble + cross-window nudge/open-wizard wiring

Slice E Task 4. Pet listens pet://nudge → auto-dismissing bubble; main listens
pet://open-wizard → opens the Slice D wizard. Restrained v1 nudge: model-ready
(from wizard finish). Vitest covers bubble-on-nudge + wizard-open-on-event."
```

---

## Final verification (before PR)
- [ ] `cd ui && npx tsc --noEmit` clean; `npm test -- --run 2>&1 | tail -12` — all new pet tests pass, no NEW regressions (note the 2 pre-existing KaleidoscopeShell/MemoryModule failures).
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` empty (only tauri.conf.json changed backend-side — confirm it still parses; `cargo build` validates the config at build via tauri macros, or run `cargo tauri info`/a build).
- [ ] **Manual E2E (the real proof — needs the desktop app):** enable the pet in Settings → a transparent always-on-top sprite appears bottom-right → drag it → click → chat panel → type "你好" → (model present from the earlier validation) streams a local reply, sprite animates thinking→typing→success. With the model absent: bubble "我还在热身…" + the main window's wizard opens.

## PR body must call out
- **No Rust changes** (JS window control per codebase pattern; the spec's `pet_window_*` Rust commands intentionally not used — matches `automation-login-window.ts`).
- **CSP `connect-src` += localhost:7337** so the pet webview reaches the local engine (the one config edit).
- **Local-only**: pet chat never falls back to cloud; 503 → wizard handoff.
- **Known gaps:** multi-window behavior (transparency, always-on-top, drag, position persistence) is only truly verifiable by running the desktop app — Vitest covers component logic with mocked Tauri APIs, not real window behavior; position persistence is OS-default in v1 (no explicit save/restore — `// TODO`); long-task-done nudge may be deferred (model-ready nudge ships); cross-window atom sync relies on imperative open/close + localStorage, not reactive atoms.
- **Slice F dependency:** the pet's system prompt = persona is Slice F; v1 uses a default (no system prompt or a simple built-in).
- Commits (bisectable): one row per Task 1–4.

## Self-review notes
- Spec coverage: transparent always-on-top window ✓ (T1 opts), idle sprite + click-expand ✓ (T2), local-only chat + 503 handoff ✓ (T3), proactive bubble + cross-window ✓ (T4), reuse PetWidget/pet-atoms ✓. Form factor A (sprite + click-expand + absorbed bubble) ✓.
- Deviations (noted): JS window control instead of Rust `pet_window_*` commands (codebase pattern); position persistence deferred to OS default; long-task nudge may defer.
- Type consistency: `PetMsg`/`PetChatMsg`, `petHistoryAtom`/`petBubbleAtom`, `streamPetChat`/`PetModelNotReady`, `pet://nudge`/`pet://open-wizard` consistent across files.
- Implementer confirmations flagged inline: the `useSetAtom as _` placeholder line must be removed (T3); the long-task signal source (T4 — defer if non-trivial); App-level wizard-open test may extract a hook (T4).
</content>
</invoke>
