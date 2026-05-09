# P1 — Cleanup Batch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land every Priority A cleanup item plus the dead-SDK-renderer removal in one tight pass — clear cargo warnings, rename `proma-types.ts`, split the JS bundle, lazy-load Shiki, and delete the vestigial Claude Code SDK plumbing the frontend never actually used.

**Architecture:** Pure cleanup — no schema changes, no new features, no behavior changes the user can see. Eight independent tasks, each its own commit. Bisectable: every commit keeps `cargo build`, `tsc --noEmit`, and `vite build` green.

**Tech Stack:** Rust (Tauri v2), TypeScript, Vite, react-shiki, Jotai. No new dependencies.

---

## Pre-flight

- [ ] **Step 0.1: Create a fresh feature branch off main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p1-cleanup-batch
```

- [ ] **Step 0.2: Capture baseline metrics (so we can prove improvement)**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "warning:" | wc -l > /tmp/p1-baseline-warnings.txt
cd ../ui && npx vite build 2>&1 | grep "index-" > /tmp/p1-baseline-bundle.txt
cd ..
cat /tmp/p1-baseline-warnings.txt /tmp/p1-baseline-bundle.txt
```

Expected: warnings count is **3**, `index-*.js` is roughly **1.16 MB** pre-gzip.

---

## Task 1: Remove the unused `browser_tool!` macro

**Files:**
- Modify: `src-tauri/src/browser/tools.rs` (delete lines around the `macro_rules!` block)

- [ ] **Step 1.1: Confirm the macro is truly unused**

```bash
cd /Users/ryanliu/Documents/uclaw
grep -rn "browser_tool!" src-tauri/src/
```

Expected output: only the definition site (`src-tauri/src/browser/tools.rs:7`). If anything else surfaces, **stop and re-evaluate** — invocation sites should be migrated, not deleted.

- [ ] **Step 1.2: Read the macro definition to know exact bounds**

```bash
sed -n '1,40p' src-tauri/src/browser/tools.rs
```

Note the exact line range covered by `macro_rules! browser_tool { ... }` (typically opens with `macro_rules! browser_tool {` and closes with the matching outer `}`).

- [ ] **Step 1.3: Delete the macro definition**

Edit `src-tauri/src/browser/tools.rs` and remove the entire `macro_rules! browser_tool { ... }` block including the leading doc comment if it exclusively documents this macro. Leave the file's other contents untouched.

- [ ] **Step 1.4: Verify the build still passes and the warning is gone**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "browser_tool|warning: unused macro"
```

Expected: empty output (no `unused macro` warning, no compile error referencing the removed macro).

- [ ] **Step 1.5: Commit**

```bash
git add src-tauri/src/browser/tools.rs
git commit -m "chore(rust): remove unused browser_tool! macro

Defined in src-tauri/src/browser/tools.rs but never invoked.
Verified via repo-wide grep — sole reference was the definition.
Drops one cargo warning."
```

---

## Task 2: Remove the dead `thinking_started = false` resets

**Files:**
- Modify: `src-tauri/src/agent/dispatcher.rs:391, 419` (or thereabouts — confirm in step 2.1)

These two resets land in branches the code exits immediately after, so the assignment is genuinely dead. Removing them silences `value assigned to thinking_started is never read` without changing behavior.

- [ ] **Step 2.1: Locate the exact lines**

```bash
cd /Users/ryanliu/Documents/uclaw
grep -n "thinking_started = false" src-tauri/src/agent/dispatcher.rs
```

Expected: 4 matches. The keepers are inside `TextDelta` and `ToolCallDelta` arms (where the loop continues afterward and the value IS re-read). The deletables are inside the `Done` arm (loop exits). Verify by reading 5 lines of context around each match:

```bash
grep -n -B 2 -A 2 "thinking_started = false" src-tauri/src/agent/dispatcher.rs
```

The `Done` arm is the one whose surrounding context references `finish_reason` or `usage`.

- [ ] **Step 2.2: Delete the two resets inside the `Done` arm**

Open `src-tauri/src/agent/dispatcher.rs` and within the `Ok(StreamDelta::Done { ... })` match arm, change:

```rust
if thinking_started {
    thinking_started = false;
    let duration = thinking_start_time
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0);
    self.emit_thinking_done(duration);
}
```

to:

```rust
if thinking_started {
    let duration = thinking_start_time
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0);
    self.emit_thinking_done(duration);
}
```

(remove only the `thinking_started = false;` line; `emit_thinking_done` stays, the value just isn't reset because we exit the stream loop after `Done`.)

There should be **two** such occurrences inside `Done`-related arms — apply the same edit to both.

- [ ] **Step 2.3: Verify**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "thinking_started|unused_assignments"
```

Expected: empty output.

Also smoke-test that streaming still works correctly by running:

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: tests pass.

- [ ] **Step 2.4: Commit**

```bash
git add src-tauri/src/agent/dispatcher.rs
git commit -m "chore(rust): drop dead thinking_started resets in stream Done arm

The two 'thinking_started = false' writes inside the Done match arm
fire just before the stream loop terminates, so the value is never
read again. The compiler was rightly warning unused_assignments.

Behavior is unchanged — emit_thinking_done still fires, the resets
were defensive but pointless."
```

---

## Task 3: Remove the unused `TitleGenerated` struct

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs:3203-3208`

- [ ] **Step 3.1: Confirm no usage**

```bash
cd /Users/ryanliu/Documents/uclaw
grep -rn "TitleGenerated\b" src-tauri/src/
```

Expected: only the definition at `src-tauri/src/tauri_commands.rs:3205`. If anything else surfaces, do not delete — investigate.

- [ ] **Step 3.2: Delete the struct definition**

Open `src-tauri/src/tauri_commands.rs` and delete:

```rust
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TitleGenerated {
    title: String,
    emoji: String,
}
```

(Leave the `SessionTitleUpdatePayload` directly below it — that one IS used.)

- [ ] **Step 3.3: Verify**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "TitleGenerated|never constructed|warning:"
```

Expected: empty output (or the count of warnings is 0).

- [ ] **Step 3.4: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "chore(rust): remove unused TitleGenerated struct

Vestige from an earlier title-generation flow that switched to
extract_json_object_slice + manual parsing. Never constructed."
```

---

## Task 4: Remove the dead Claude Code SDK renderer plumbing (frontend)

uClaw runs its own pure-Rust agent loop — the frontend's `useSDKRenderer` / `persistedSDKMessages` / `getAgentSessionSDKMessages` plumbing is a Proma-era leftover that never activates because no Rust side ever produces SDK messages. Delete it cleanly. The legacy renderer (which we've been polishing in PRs #5–#18) handles all current sessions correctly and PR P2 will further improve its block ordering.

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts` (delete `getAgentSessionSDKMessages`)
- Modify: `ui/src/components/agent/AgentView.tsx` (drop `persistedSDKMessages` state + `Promise.all` load)
- Modify: `ui/src/components/agent/AgentMessages.tsx` (drop `useSDKRenderer` branch + memos + props)
- Test: `ui/src/components/agent/AgentMessages.test.tsx` (will be added in P3 — for now we rely on `tsc` + smoke test)

- [ ] **Step 4.1: Snapshot what we're about to delete**

```bash
cd /Users/ryanliu/Documents/uclaw
grep -rn "getAgentSessionSDKMessages\|persistedSDKMessages\|useSDKRenderer\|SDKMessage\b" ui/src/ | tee /tmp/p1-sdk-plumbing.txt | wc -l
```

Expected: roughly 30–50 lines across 4–6 files. Save the list — it's the deletion checklist.

- [ ] **Step 4.2: Remove `getAgentSessionSDKMessages` from the bridge**

Open `ui/src/lib/tauri-bridge.ts`, find lines 700–702:

```ts
export const getAgentSessionSDKMessages = (sessionId: string): Promise<any[]> =>
  invoke<any[]>('get_agent_session_sdk_messages', { sessionId }).catch(() => [])
```

Delete those lines.

- [ ] **Step 4.3: Drop `persistedSDKMessages` from AgentView**

Open `ui/src/components/agent/AgentView.tsx`. Make these changes:

a) Remove the import:
```ts
import { ..., getAgentSessionSDKMessages, ... } from '@/lib/tauri-bridge'
// ↓ becomes
import { ..., ... } from '@/lib/tauri-bridge'
```

b) Remove the state declaration (around line 198):
```ts
const [persistedSDKMessages, setPersistedSDKMessages] = React.useState<SDKMessage[]>([])
```
…and the `SDKMessage` type import that goes with it.

c) Replace the `Promise.all([loadOldMessages, loadSDKMessages])` (around line 431-438) with a single call:
```ts
// Before:
const loadOldMessages = getAgentSessionMessages(sessionId)
const loadSDKMessages = getAgentSessionSDKMessages(sessionId)
Promise.all([loadOldMessages, loadSDKMessages])
  .then(([msgs, sdkMsgs]) => {
    setMessages(msgs)
    setPersistedSDKMessages(sdkMsgs)
    setMessagesLoaded(true)
    ...
  })

// After:
getAgentSessionMessages(sessionId)
  .then((msgs) => {
    setMessages(msgs)
    setMessagesLoaded(true)
    ...
  })
```

d) Remove `persistedSDKMessages={persistedSDKMessages}` from the `<AgentMessages ... />` props (around line 1318 area).

- [ ] **Step 4.4: Drop the `useSDKRenderer` branch from AgentMessages**

Open `ui/src/components/agent/AgentMessages.tsx`. Remove:

a) The prop from `AgentMessagesProps`:
```ts
persistedSDKMessages?: SDKMessage[]
```

b) The destructured prop from the function signature.

c) The memo derivations:
```ts
const useSDKRenderer = persistedSDKMessages && persistedSDKMessages.length > 0
const hasContent = useSDKRenderer ? persistedSDKMessages.length > 0 : messages.length > 0
const allSDKMessages = React.useMemo(() => { ... }, [persistedSDKMessages, liveMessages])
const allGroups = React.useMemo(() => {
  if (!useSDKRenderer) return []
  ...
}, [useSDKRenderer, allSDKMessages, sessionModelId])
```

d) The conditional render branches that reference `useSDKRenderer`. Search for `useSDKRenderer` in the file — every occurrence either becomes the falsy branch (`messages.map(...)`) or gets simplified.

e) Adjust `hasContent` to just `messages.length > 0`.

- [ ] **Step 4.5: Strip SDK-only types from `proma-types.ts`**

In `ui/src/lib/proma-types.ts`, delete these types (they have no other uses after step 4.4):
- `SDKMessage`
- `SDKAssistantMessage`
- `SDKUserMessage`
- `SDKSystemMessage`
- `SDKResultMessage`
- `SDKMessageContent`
- `SDKThinkingBlock`
- `SDKTextBlock`
- `SDKToolUseBlock`

(Search for `^export.*SDK` to enumerate.) The `AgentEvent` types stay — those are uClaw's own.

- [ ] **Step 4.6: Verify**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected: zero errors. If TS complains about a missing import, check that `ContentBlock.tsx` and other files in `agent/` don't still reference `SDKThinkingBlock` etc. — replace those references with the corresponding native types from `proma-types.ts` (uClaw's own `ContentBlock` enum union).

```bash
grep -rn "useSDKRenderer\|persistedSDKMessages\|getAgentSessionSDKMessages" ui/src/
```

Expected: empty output.

```bash
npx vite build 2>&1 | tail -3
```

Expected: build succeeds.

- [ ] **Step 4.7: Commit**

```bash
git add ui/src/lib/tauri-bridge.ts ui/src/lib/proma-types.ts \
        ui/src/components/agent/AgentView.tsx \
        ui/src/components/agent/AgentMessages.tsx \
        ui/src/components/agent/ContentBlock.tsx
git commit -m "chore(ui): remove dead Claude Code SDK renderer plumbing

uClaw runs its own pure-Rust agent loop (agent/agentic_loop.rs +
agent/dispatcher.rs); there is no external SDK in the loop. The
frontend SDK renderer plumbing — getAgentSessionSDKMessages,
persistedSDKMessages, useSDKRenderer, SDK*Message types — is a
Proma-era leftover that never activates because no Rust side ever
produces SDK messages.

Removed:
  - getAgentSessionSDKMessages from tauri-bridge (silent .catch)
  - persistedSDKMessages state + Promise.all wiring in AgentView
  - useSDKRenderer / allSDKMessages / allGroups derivations
  - SDK*Message type exports from proma-types

The legacy renderer path handles all current sessions correctly.
P2 (native structured-block rendering) will further improve block
ordering for the same path."
```

---

## Task 5: Split `proma-types.ts` into `chat-types.ts` + `agent-types.ts`

**Files:**
- Create: `ui/src/lib/chat-types.ts`
- Create: `ui/src/lib/agent-types.ts`
- Modify: `ui/src/lib/proma-types.ts` (re-export only, for back-compat — to remove in a follow-up)
- Modify: ~80 import sites (codemod)

- [ ] **Step 5.1: Read the current `proma-types.ts` to understand what's there**

```bash
cd /Users/ryanliu/Documents/uclaw
wc -l ui/src/lib/proma-types.ts
grep -E "^export (interface|type)" ui/src/lib/proma-types.ts
```

This gives the export inventory. Group mentally into:
- **Chat group:** `PrimaChatMessage`, `ChatMessage` (the type-alias one), `ChatToolActivity`, related `FileAttachment`, `ChannelModel`, etc. — anything used by `components/chat/*`.
- **Agent group:** `AgentMessage`, `AgentEvent`, `AgentEventUsage`, `AgentQueueMessageInput`, etc. — anything used by `components/agent/*`.
- **Shared:** Some types are imported by both; put them in `chat-types.ts` and re-export from `agent-types.ts`.

- [ ] **Step 5.2: Create `chat-types.ts`**

Create `ui/src/lib/chat-types.ts`. Move (cut + paste) the chat group exports from `proma-types.ts` into it. Add at the top:

```ts
/**
 * Chat-layer types — used by components/chat/* and shared with components/agent/*.
 * Split from the legacy proma-types.ts as part of P1 cleanup (Roadmap A4).
 */
```

- [ ] **Step 5.3: Create `agent-types.ts`**

Create `ui/src/lib/agent-types.ts`. Move the agent group exports. Add at top:

```ts
/**
 * Agent-specific types — used by components/agent/*.
 * Split from the legacy proma-types.ts as part of P1 cleanup (Roadmap A4).
 */

import type { ChatToolActivity } from './chat-types'
// re-export the cross-domain types so agent consumers don't need 2 imports
export type { ChatToolActivity }
```

- [ ] **Step 5.4: Reduce `proma-types.ts` to a deprecated re-export shim**

After moving everything, replace `ui/src/lib/proma-types.ts`'s body with:

```ts
/**
 * @deprecated Import from chat-types or agent-types directly.
 * This file is a transitional re-export shim — remove in P1 follow-up.
 */
export * from './chat-types'
export * from './agent-types'
```

- [ ] **Step 5.5: Verify TS still compiles**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected: zero errors. (The shim means existing imports keep working.)

- [ ] **Step 5.6: Codemod: rewrite all imports of `@/lib/proma-types` to point at the right new module**

For each TypeScript/TSX file under `ui/src/`, replace `from '@/lib/proma-types'` with the appropriate target. Run this as a one-shot:

```bash
cd /Users/ryanliu/Documents/uclaw/ui

# Build a list of (file, type-name) pairs to figure out which target each import line needs.
# The simplest strategy: rewrite each import to pull from BOTH new modules with explicit
# type names, and let `tsc` tell us which ones are wrong (re-export from the other module
# resolves those automatically because chat-types re-exports nothing from agent-types and
# vice versa — TypeScript will narrow it).

# Actually simpler: do a literal text substitution. Both new modules re-export each other
# transparently via the shim staying alive at proma-types, so the strategy is:
#   1. Keep the deprecation shim in proma-types.ts (already done in 5.4)
#   2. Run a targeted sed to rewrite imports from proma-types → chat-types (since chat-
#      types now re-exports the agent ones via star export)
# Wait — the shim re-exports. The new modules don't re-export each other. Easier path:

# Grep imports first
grep -rn "from '@/lib/proma-types'" src/ | head -20
```

Decide the codemod strategy based on what `grep` shows. The recommended approach:

a) Write a small script `scripts/codemod-types.ts`:

```ts
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs'
import { join, extname } from 'node:path'

const CHAT_TYPES = new Set([
  'PrimaChatMessage', 'ChatMessage', 'ChatToolActivity', 'FileAttachment',
  'ChannelModel', 'ConversationMeta', /* …enumerate by reading chat-types.ts… */
])
const AGENT_TYPES = new Set([
  'AgentMessage', 'AgentEvent', 'AgentEventUsage', 'AgentQueueMessageInput',
  /* …enumerate by reading agent-types.ts… */
])

function rewriteFile(path: string): void {
  const src = readFileSync(path, 'utf8')
  const re = /import\s+(?:type\s+)?\{([^}]+)\}\s+from\s+['"]@\/lib\/proma-types['"]/g
  const out = src.replace(re, (_full, names: string) => {
    const ids = names.split(',').map((s) => s.trim().replace(/^type\s+/, ''))
    const chat = ids.filter((id) => CHAT_TYPES.has(id))
    const agent = ids.filter((id) => AGENT_TYPES.has(id))
    const lines: string[] = []
    if (chat.length) lines.push(`import type { ${chat.join(', ')} } from '@/lib/chat-types'`)
    if (agent.length) lines.push(`import type { ${agent.join(', ')} } from '@/lib/agent-types'`)
    return lines.join('\n')
  })
  if (out !== src) {
    writeFileSync(path, out, 'utf8')
    console.log('rewrote:', path)
  }
}

function walk(dir: string): void {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e)
    if (statSync(p).isDirectory()) walk(p)
    else if (['.ts', '.tsx'].includes(extname(p))) rewriteFile(p)
  }
}

walk('./src')
```

b) Populate `CHAT_TYPES` and `AGENT_TYPES` Sets by reading the actual exports from the two new files:

```bash
grep -E "^export (interface|type)" src/lib/chat-types.ts | awk '{print $3}' | tr -d '<' | head
grep -E "^export (interface|type)" src/lib/agent-types.ts | awk '{print $3}' | tr -d '<' | head
```

Paste the names into the script's two Sets.

c) Run:
```bash
npx tsx scripts/codemod-types.ts
```

d) Type-check:
```bash
npx tsc --noEmit 2>&1 | head -20
```

Expected: zero errors. If any file still imports a type that's in the wrong Set, manually fix it (the script's grep-based enumeration may have missed compound types).

- [ ] **Step 5.7: Verify shim is now unused; delete the codemod script**

```bash
grep -rn "from '@/lib/proma-types'" src/
```

Expected: empty. Delete the codemod script and the shim:

```bash
rm scripts/codemod-types.ts
# Keep proma-types.ts for one more PR as deprecation safety net — final removal in
# follow-up. For now, leave the shim file in place but verify it's truly unused:
grep -rn "proma-types" src/
```

Expected: only matches inside `proma-types.ts` itself.

- [ ] **Step 5.8: Final TS + build check**

```bash
npx tsc --noEmit && npx vite build 2>&1 | tail -3
```

Expected: zero TS errors, vite build success.

- [ ] **Step 5.9: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/lib/chat-types.ts ui/src/lib/agent-types.ts ui/src/lib/proma-types.ts ui/src/
git commit -m "refactor(ui): split proma-types.ts into chat-types + agent-types

The old proma-types.ts ballooned to ~1000 lines mixing chat-domain
and agent-domain types. Split by responsibility:

  - chat-types.ts: PrimaChatMessage, ChatMessage, ChatToolActivity,
    FileAttachment, ChannelModel, ConversationMeta (+ shared)
  - agent-types.ts: AgentMessage, AgentEvent, AgentEventUsage,
    AgentQueueMessageInput

proma-types.ts becomes a deprecated re-export shim (delete in
follow-up after one or two more PRs settle the import sites).

~80 import sites rewritten via codemod. Zero behavior change."
```

---

## Task 6: Configure Vite manualChunks to split the bundle

**Files:**
- Modify: `ui/vite.config.ts`

- [ ] **Step 6.1: Read the current Vite config**

```bash
cat /Users/ryanliu/Documents/uclaw/ui/vite.config.ts
```

Locate the existing `build.rollupOptions.output.manualChunks` (per CLAUDE.md it splits `react`, `tauri`, `vendor`).

- [ ] **Step 6.2: Add route-level chunks**

Edit `ui/vite.config.ts` and extend `manualChunks`:

```ts
build: {
  rollupOptions: {
    output: {
      manualChunks(id: string) {
        // Existing splits — preserve.
        if (id.includes('node_modules/react/') || id.includes('node_modules/react-dom/')) {
          return 'react'
        }
        if (id.includes('node_modules/@tauri-apps/')) {
          return 'tauri'
        }
        if (id.includes('node_modules/jotai') || id.includes('node_modules/clsx') || id.includes('node_modules/tailwind-merge')) {
          return 'vendor'
        }
        // NEW: route-level splits — heaviest views become their own async chunks
        if (id.includes('/components/settings/')) return 'view-settings'
        if (id.includes('/components/memory/')) return 'view-memory'
        if (id.includes('/components/automation/')) return 'view-automation'
        if (id.includes('/components/agent/')) return 'view-agent'
        // NEW: shiki + its languages/themes — biggest single-source contributor
        if (id.includes('node_modules/shiki') || id.includes('node_modules/@shikijs')) {
          return 'shiki'
        }
        return undefined
      },
    },
  },
},
```

(Adapt the exact path matchers if your repo lays out files differently.)

- [ ] **Step 6.3: Build and check chunk sizes**

```bash
cd ui && npx vite build 2>&1 | grep "static/assets" | sort -k1
```

Expected:
- `index-*.js` drops from ~1.1 MB to **<700 KB pre-gzip**
- New chunks: `view-settings`, `view-memory`, `view-automation`, `view-agent`, `shiki` each appear with reasonable sizes
- Total transferred bytes (sum of gzipped) should not increase by more than ~5%

If `index-*.js` is still over 700 KB, inspect with `npx vite build --mode analyze` and add another path matcher.

- [ ] **Step 6.4: Smoke-test that lazy chunks load correctly**

Cannot run dev server in this environment, but verify the production build serves correctly by checking the manifest:

```bash
cat ../static/.vite/manifest.json 2>/dev/null | head -20 || ls ../static/assets/ | sort
```

Expected: chunks listed with hashed filenames; entry chunk references the others as `imports`.

- [ ] **Step 6.5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/vite.config.ts
git commit -m "build(vite): split bundle by route + isolate Shiki

Adds manualChunks rules for the heaviest view directories
(Settings/Memory/Automation/Agent) and pulls Shiki + its language/
theme imports into their own 'shiki' chunk.

Initial bundle (index-*.js) drops from ~1.1MB → <700KB pre-gzip.
Chunks load on demand when the user navigates to that view, so
first paint is faster while total transfer is roughly unchanged."
```

---

## Task 7: Switch Shiki to lazy theme + language loading

**Files:**
- Modify: `ui/src/lib/highlight.ts`

The current implementation eagerly preloads 7 themes + 18 languages at startup (~150–200 KB of JSON). Move that to lazy loading inside `highlightCode`.

- [ ] **Step 7.1: Read the current highlight.ts**

```bash
cat /Users/ryanliu/Documents/uclaw/ui/src/lib/highlight.ts
```

Note the `createHighlighter({ themes, langs })` call and the `EXTRA_THEMES` / `COMMON_LANGUAGES` arrays.

- [ ] **Step 7.2: Convert eager preload to lazy load**

Edit `ui/src/lib/highlight.ts`. Find:

```ts
export function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [LIGHT_THEME, DARK_THEME, ...EXTRA_THEMES],
      langs: COMMON_LANGUAGES,
    })
  }
  return highlighterPromise
}
```

Replace with a minimal preload + per-request lazy load:

```ts
const loadedThemes = new Set<BundledTheme>([LIGHT_THEME, DARK_THEME])
const loadedLangs = new Set<BundledLanguage>(['plaintext'])

export function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [LIGHT_THEME, DARK_THEME],   // preload only 2; others load on demand
      langs: ['plaintext'],                // preload only 1; others load on demand
    })
  }
  return highlighterPromise
}

async function ensureTheme(highlighter: Highlighter, theme: BundledTheme): Promise<void> {
  if (loadedThemes.has(theme)) return
  await highlighter.loadTheme(theme)
  loadedThemes.add(theme)
}

async function ensureLanguage(highlighter: Highlighter, lang: BundledLanguage): Promise<void> {
  if (loadedLangs.has(lang)) return
  try {
    await highlighter.loadLanguage(lang)
    loadedLangs.add(lang)
  } catch {
    // language doesn't exist — fall back to plaintext silently
  }
}
```

Then update `highlightCode` to call `ensureTheme` + `ensureLanguage` before `codeToHtml`:

```ts
export async function highlightCode(
  code: string,
  language: string,
  theme?: 'light' | 'dark',
): Promise<string> {
  try {
    const highlighter = await getHighlighter()
    const lang = language.toLowerCase() as BundledLanguage
    await ensureLanguage(highlighter, lang)

    const shikiTheme: BundledTheme = theme === 'light'
      ? LIGHT_THEME
      : theme === 'dark'
        ? DARK_THEME
        : getShikiThemeForCurrentApp()
    await ensureTheme(highlighter, shikiTheme)

    return highlighter.codeToHtml(code, { lang, theme: shikiTheme })
  } catch (error) {
    console.warn('[highlight] 高亮失败:', error)
    return `<pre><code>${escapeHtml(code)}</code></pre>`
  }
}
```

Delete the now-unused `EXTRA_THEMES` and `COMMON_LANGUAGES` constants.

- [ ] **Step 7.3: Verify**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10 && npx vite build 2>&1 | grep "shiki\|index-"
```

Expected: zero TS errors. The `shiki` chunk size should be slightly smaller (no eager loads in the initial bundle), and `index-*.js` further reduced. Net effect: first paint is faster; first code block render takes ~100–300 ms longer (acceptable — invisible during streaming).

- [ ] **Step 7.4: Commit**

```bash
git add ui/src/lib/highlight.ts
git commit -m "perf(highlight): lazy-load Shiki themes + languages

Was eagerly preloading 7 themes + 18 languages at startup,
~150-200KB of JSON the user might never need. Switch to per-call
ensureTheme/ensureLanguage with a Set cache so repeated highlights
in the same theme/lang only pay the load cost once.

Trade-off: first code block render takes ~100-300ms longer (invisible
during streaming because Shiki is called after content lands). Initial
bundle shrinks proportionally."
```

---

## Task 8: Final verification + smoke test

- [ ] **Step 8.1: Cargo build clean (0 warnings)**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "warning:" | wc -l
```

Expected: **0**.

- [ ] **Step 8.2: TS clean**

```bash
cd ../ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: empty output.

- [ ] **Step 8.3: Vite build with chunk metrics**

```bash
npx vite build 2>&1 | grep "static/assets/" | sort -k1
```

Expected:
- `index-*.js` < 700 KB pre-gzip (was 1.16 MB)
- Multiple `view-*` chunks present
- Separate `shiki` chunk
- No single chunk warning above 500 KB

- [ ] **Step 8.4: Verify all the cleanup goals are met**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== A2 (cargo warnings) ==="
cd src-tauri && cargo build 2>&1 | grep -c "warning:"
echo "=== A3 (bundle size) ==="
cd ../ui && npx vite build 2>&1 | grep "index-" | head -1
echo "=== A4 (proma-types usage) ==="
grep -rn "from '@/lib/proma-types'" src/ | wc -l
echo "=== A5 (eager Shiki) ==="
grep -E "EXTRA_THEMES|COMMON_LANGUAGES" src/lib/highlight.ts | wc -l
echo "=== Dead-SDK removal ==="
grep -rn "useSDKRenderer\|persistedSDKMessages\|getAgentSessionSDKMessages" src/ | wc -l
```

Expected output:
```
=== A2 (cargo warnings) ===
0
=== A3 (bundle size) ===
[a line showing index-*.js under ~700 KB]
=== A4 (proma-types usage) ===
0
=== A5 (eager Shiki) ===
0
=== Dead-SDK removal ===
0
```

If any number isn't 0 (or for A3, isn't under target), go back to the relevant task and fix.

- [ ] **Step 8.5: Push the branch and open a PR**

```bash
cd /Users/ryanliu/Documents/uclaw
git push -u origin claude/p1-cleanup-batch
gh pr create --title "P1 cleanup batch — A2 + A3 + A4 + A5 + dead-SDK-renderer removal" --body "$(cat <<'EOF'
## Summary

Implements P1 from the roadmap (`docs/superpowers/specs/2026-05-09-uclaw-roadmap.md`). Pure cleanup — no behavior changes the user can see; every commit keeps the repo green.

## What's in this batch

- **A2** Cleared 3 cargo warnings:
  - Removed unused `browser_tool!` macro from `src-tauri/src/browser/tools.rs`
  - Removed dead `thinking_started = false` resets in the stream `Done` arm of `agent/dispatcher.rs`
  - Removed unused `TitleGenerated` struct from `tauri_commands.rs`
- **A3** Vite manualChunks split — `index-*.js` shrinks from ~1.1MB → <700KB pre-gzip; new `view-settings`, `view-memory`, `view-automation`, `view-agent`, `shiki` lazy chunks
- **A4** `proma-types.ts` split into `chat-types.ts` + `agent-types.ts` (deprecated shim kept temporarily for back-compat)
- **A5** Shiki themes/languages switched from eager preload to lazy `ensureTheme`/`ensureLanguage`; saves ~150–200KB from the initial bundle
- **Dead-SDK-renderer removal** (per roadmap rescope of P2): deleted `useSDKRenderer`, `persistedSDKMessages`, `getAgentSessionSDKMessages`, and the `SDK*Message` types from `proma-types`. uClaw's pure-Rust agent loop never produced SDK messages; the frontend plumbing was a Proma-era leftover that never activated.

## Verification

- [x] `cargo build` shows 0 warnings (was 3)
- [x] `tsc --noEmit` clean
- [x] `vite build` succeeds; `index-*.js` < 700KB pre-gzip
- [x] No remaining grep hits for: `useSDKRenderer`, `persistedSDKMessages`, `getAgentSessionSDKMessages`, `EXTRA_THEMES` (Shiki preload), `from '@/lib/proma-types'` (after rename)

## Test plan (manual smoke)

- [ ] Open agent view → thinking + tools render correctly via legacy renderer
- [ ] Open chat view → markdown renders, font-size/serif popover works
- [ ] Switch themes — code blocks re-highlight (now lazy-loaded)
- [ ] Settings / Memory / Automation views still load (just lazy now)
EOF
)"
```

- [ ] **Step 8.6: Self-review**

After CI passes, before requesting human review, run a final read-through of the diff:

```bash
git log main..HEAD --oneline
git diff main...HEAD --stat
```

Verify each commit has a focused diff and a clear message. Each task's commit should appear as one line in the log.

---

## Acceptance criteria (rolls up from each task)

- ✅ `cargo build` shows 0 warnings (was 3)
- ✅ `vite build` reports `index-*.js` < 700 KB pre-gzip (was ~1.1 MB)
- ✅ `tsc --noEmit` clean after rename + dead-code removal
- ✅ App still loads — agent + chat views render thinking + tools correctly
- ✅ `grep -rn "useSDKRenderer\|persistedSDKMessages\|getAgentSessionSDKMessages\|SDKMessage" ui/src/` returns empty
- ✅ `grep -rn "from '@/lib/proma-types'" ui/src/` returns empty (only the shim file itself remains, used by no one)
- ✅ Each task committed separately so the PR is bisectable

## Out of scope (future plans)

- Final removal of `proma-types.ts` shim — wait one or two PRs after merge to confirm no external code depends on it.
- Native structured-block rendering (`NativeBlockRenderer`) — that's P2, not P1.
- Chat-side `Reasoning` component update to match Agent's `ThinkingBlock` style — captured in P12 sweep if visual drift is found.
