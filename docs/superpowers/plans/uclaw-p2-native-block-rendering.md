# P2 — Native Structured-Block Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render assistant turns in their original `[Text → Thinking → ToolUse → ToolResult → Text → ...]` order instead of flattening to "all thinking, then all tools, then text". The data is already structured on disk (`messages.content` is JSON of `Vec<ContentBlock>`); the load path just throws away ordering by extracting only `Text` blocks. Fix the load + render — no schema change.

**Architecture:**

- **Backend (Rust):**
  - `MessageResponse` gains `content_blocks: Option<Vec<ContentBlock>>` populated by parsing `messages.content` as JSON.
  - The agent path (`get_agent_session_messages` returns `Vec<serde_json::Value>`) gets a `contentBlocks` key in each map.
  - Both paths fall back gracefully: if the row is plain text or unparseable, `content_blocks` is `None`/`null` and the existing flat `content` string still drives the legacy renderer.
- **Frontend (TS):**
  - `ContentBlock` discriminated union added to `chat-types.ts` (mirrors the Rust enum 1:1, snake_case via the JSON wire).
  - `ChatMessage` (`chat-types.ts`) and `AgentMessage` (`agent-types.ts`) both gain an optional `contentBlocks?: ContentBlock[]` field.
  - New `NativeBlockRenderer.tsx` (under `agent/`) iterates blocks in order, pairing `tool_use` with its matching `tool_result` by `id`/`tool_use_id`.
  - `ChatMessageItem` and `AgentMessageItem` switch to `NativeBlockRenderer` when `message.contentBlocks?.length > 0`; otherwise fall back to the existing flat path (`reasoning` + `toolActivities` + `content` markdown).
- **Streaming stays unchanged.** The live bubble already sequences thinking → tool → text via `streamingState.{reasoning, toolActivities, content}`. After stream-complete + reload, the persisted blocks render via the new path.

**Tech Stack:** No new deps. Reuses existing `ThinkingBlock` and `ChatToolBlock` components from `agent/ContentBlock.tsx`.

**Reference:** Roadmap §P2 at `docs/superpowers/specs/2026-05-09-uclaw-roadmap.md:134`.

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p2-native-block-rendering
```

- [ ] **Step 0.2: Baseline pipeline**

```bash
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
```
Expected: clean cargo, all rust tests passing, 0 TS errors, **33 frontend tests** passing (P3 20 + SearchPalette 13).

---

## Task 1: Backend — expose `content_blocks` on the load path

**Files:**
- Modify: `src-tauri/src/ipc.rs` — extend `MessageResponse`
- Modify: `src-tauri/src/tauri_commands.rs` — populate `content_blocks` in `get_messages` and `get_agent_session_messages`

The existing `get_messages` already parses `content` as `Option<Vec<ContentBlock>>` then throws everything except `Text` away. We keep that text-only `content` string for the legacy renderer **and** expose the original vec.

- [ ] **Step 1.1: Extend `MessageResponse`**

Edit `src-tauri/src/ipc.rs`. Find the `MessageResponse` struct (around line 124). Add the new field **before** the closing brace:

```rust
    /// Original ordered ContentBlocks parsed from `messages.content`.
    /// `None` for legacy plain-text rows or rows that fail to parse.
    /// When `Some`, the frontend renders via NativeBlockRenderer for
    /// in-order display; when `None`, falls back to flat `content` +
    /// `reasoning` + `tool_activities`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<crate::agent::types::ContentBlock>>,
```

`ContentBlock` is already `Serialize` (declared in `agent/types.rs:53`).

- [ ] **Step 1.2: Populate `content_blocks` in `get_messages`**

Edit `src-tauri/src/tauri_commands.rs`. Find `pub async fn get_messages` (around line 683). The current code parses `raw_content` once for the `content_text` extraction; reuse the parse so we don't deserialize twice. Replace the `content_text` block with:

```rust
        // Parse `content` once. Two persisted shapes have been seen historically:
        //   - JSON of Option<Vec<ContentBlock>> — written by add_message_with_meta
        //     via serde_json::to_string(&session.messages.last().map(|m| &m.content))
        //   - JSON of Vec<ContentBlock> — written by older code paths
        //   - Plain text — pre-V5 rows
        let parsed_blocks: Option<Vec<ContentBlock>> =
            serde_json::from_str::<Option<Vec<ContentBlock>>>(&raw_content)
                .ok()
                .flatten()
                .or_else(|| serde_json::from_str::<Vec<ContentBlock>>(&raw_content).ok());

        // Flat text projection — joins all Text blocks. Used by the legacy
        // renderer + minimap snippets.
        let content_text: String = match &parsed_blocks {
            Some(blocks) => blocks
                .iter()
                .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.clone()) } else { None })
                .collect::<Vec<_>>()
                .join("\n"),
            None => raw_content,
        };
```

Then update the `out.push(MessageResponse { ... })` block to include `content_blocks: parsed_blocks`:

```rust
        out.push(MessageResponse {
            id,
            conversation_id: input.conversation_id.clone(),
            role,
            content: content_text,
            created_at,
            reasoning,
            tool_activities,
            model,
            content_blocks: parsed_blocks,
        });
```

Note: `content_text` consumed `raw_content` in the `None` arm via move, so the `parsed_blocks` value still lives. If the borrow checker complains because `parsed_blocks` was moved into `content_text`, restructure slightly:

```rust
        let content_text: String = parsed_blocks
            .as_ref()
            .map(|blocks| {
                blocks.iter()
                    .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.clone()) } else { None })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or(raw_content);
```

- [ ] **Step 1.3: Populate `contentBlocks` in `get_agent_session_messages`**

Edit `src-tauri/src/tauri_commands.rs`. Find `pub async fn get_agent_session_messages` (around line 3139). It builds a `Vec<serde_json::Value>` row-by-row. Inside the `for msg in &messages` loop, **before** the existing `tool_activities` recovery logic, parse `msg.content` and stash it for the JSON output:

```rust
        // Parse content as Vec<ContentBlock> for in-order rendering.
        // Same fallback as get_messages; None for plain-text legacy rows.
        let parsed_blocks: Option<Vec<crate::agent::types::ContentBlock>> =
            serde_json::from_str::<Option<Vec<crate::agent::types::ContentBlock>>>(&msg.content)
                .ok()
                .flatten()
                .or_else(|| {
                    serde_json::from_str::<Vec<crate::agent::types::ContentBlock>>(&msg.content).ok()
                });
```

Then where the JSON object is constructed for output (the existing code builds a `serde_json::json!({...})` or `serde_json::Map` — read the function to see which), add:

```rust
        if let Some(blocks) = parsed_blocks.as_ref() {
            // Both names so frontend Message types can read either; agent path
            // prefers camelCase (matching its `created_at`→`createdAt` pattern).
            obj.insert("contentBlocks".into(), serde_json::to_value(blocks).unwrap_or(serde_json::Value::Null));
        }
```

If the function uses `serde_json::json!(...)` macros instead, instead build a `serde_json::Map` and insert there. Read the existing structure first:
```bash
sed -n '3210,3260p' /Users/ryanliu/Documents/uclaw/src-tauri/src/tauri_commands.rs
```

- [ ] **Step 1.4: Build clean**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: 0 errors. If `ContentBlock` import is missing in `tauri_commands.rs`, add `use crate::agent::types::ContentBlock;` near the existing imports.

- [ ] **Step 1.5: Commit**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs
git commit -m "$(cat <<'EOF'
feat(messages): expose content_blocks on the load path

Both `get_messages` and `get_agent_session_messages` already parse
`messages.content` as JSON, but throw away everything except `Text`
blocks. Keep the flat text projection (legacy renderer + minimap
still use it) AND expose the original Vec<ContentBlock> as
`content_blocks` so the frontend can render text / thinking /
tool_use / tool_result in their actual order.

`None` / unset for legacy plain-text rows so the renderer falls
back gracefully. No schema change.
EOF
)"
```

---

## Task 2: Frontend — `ContentBlock` type + bridge

**Files:**
- Modify: `ui/src/lib/chat-types.ts` (add `ContentBlock` union + `contentBlocks?` on `ChatMessage`)
- Modify: `ui/src/lib/agent-types.ts` (add `contentBlocks?` on `AgentMessage`)

- [ ] **Step 2.1: Add the `ContentBlock` discriminated union**

Edit `ui/src/lib/chat-types.ts`. After the `ChatToolActivity` interface (around line 84), add:

```ts
// ===== Native content blocks =====
//
// Mirrors the Rust `ContentBlock` enum at `src-tauri/src/agent/types.rs:55`.
// Serde tags the variant via `type` and uses snake_case, so the wire format
// is e.g. `{ "type": "tool_use", "id": "...", "name": "...", "input": {...} }`.

export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'thinking'; thinking: string }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> }
  | { type: 'tool_result'; tool_use_id: string; content: string; is_error?: boolean }
```

Re-export it from `chat-types.ts` so other files can import it from one place.

- [ ] **Step 2.2: Add `contentBlocks?` on `ChatMessage`**

In the same file, extend the `ChatMessage` interface (around line 40):

```ts
export interface ChatMessage {
  // ... existing fields ...
  attachments?: FileAttachment[]
  createdAt: number
  /** Original ordered ContentBlocks. When present, the renderer uses
   *  NativeBlockRenderer for in-order display. Falls back to the flat
   *  `content` + `reasoning` + `toolActivities` path when absent. */
  contentBlocks?: ContentBlock[]
}
```

Do the same for `PrimaChatMessage` (it has the same fields). Both should accept `contentBlocks?`.

- [ ] **Step 2.3: Add `contentBlocks?` on `AgentMessage`**

Edit `ui/src/lib/agent-types.ts`. Add the import at the top of the file:
```ts
import type { ContentBlock } from './chat-types'
```

Then extend `AgentMessage` (around line 40):

```ts
export interface AgentMessage {
  // ... existing fields ...
  toolActivities?: ChatToolActivity[]
  /** Same as ChatMessage.contentBlocks — see chat-types.ts. */
  contentBlocks?: ContentBlock[]
}
```

- [ ] **Step 2.4: TS check + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

```bash
git add ui/src/lib/chat-types.ts ui/src/lib/agent-types.ts
git commit -m "$(cat <<'EOF'
feat(types): ContentBlock discriminated union + contentBlocks?

Mirrors the Rust ContentBlock enum: text / thinking / tool_use /
tool_result. ChatMessage and AgentMessage both gain an optional
contentBlocks field — populated by the load path Tauri commands
when the persisted `content` JSON parses as Vec<ContentBlock>,
absent for legacy plain-text rows.
EOF
)"
```

---

## Task 3: Frontend — `NativeBlockRenderer`

**Files:**
- Create: `ui/src/components/agent/NativeBlockRenderer.tsx`

The renderer:
- Iterates blocks in order
- For each `text` block → `<MessageResponse>` (already-existing markdown body component)
- For each `thinking` block → `<ThinkingBlock block={{ type: 'thinking', thinking: block.thinking }} />`
- For each `tool_use` → look ahead in the array for the matching `tool_result` (by `id` ↔ `tool_use_id`); render a `<ChatToolBlock>` with the merged input + result + error flag
- A `tool_result` already paired with a prior `tool_use` is skipped (its content rendered inside the tool block)
- A `tool_result` with no matching `tool_use` (rare; dangling row from edit/regen) → render an "(orphaned tool result)" placeholder so we don't silently drop content

- [ ] **Step 3.1: Confirm the components we depend on**

```bash
grep -nE "export (function|const) (MessageResponse|ChatToolBlock|ThinkingBlock)" /Users/ryanliu/Documents/uclaw/ui/src/components/{agent,chat}/**/*.tsx 2>/dev/null | head -10
```

Expected: `ThinkingBlock` exported from `agent/ContentBlock.tsx`. `ChatToolBlock` exported from somewhere (`agent/ContentBlock.tsx` or its own file). `MessageResponse` (the markdown body) is in `chat/`. Adapt the imports below to whatever the exact paths are.

- [ ] **Step 3.2: Write the component**

Create `ui/src/components/agent/NativeBlockRenderer.tsx`:

```tsx
/**
 * NativeBlockRenderer — renders a Vec<ContentBlock> in original order.
 *
 * Pairing rule: each `tool_use` looks ahead in the same array for its
 * matching `tool_result` (by id ↔ tool_use_id) and renders a single
 * <ChatToolBlock>. Already-paired `tool_result`s are skipped on their
 * own iteration. Orphaned tool_results (no prior tool_use) get a
 * minimal placeholder so we don't silently drop persisted content.
 */

import * as React from 'react'
import type { ContentBlock } from '@/lib/chat-types'
import { ThinkingBlock, ChatToolBlock } from './ContentBlock'
import { MessageResponse } from '@/components/chat/MessageResponse' // adjust path if needed

export interface NativeBlockRendererProps {
  blocks: ContentBlock[]
  /** Carries through to MessageResponse for shiki / katex / mention links. */
  conversationId?: string
  /** Optional className for the outer wrapper. */
  className?: string
}

export function NativeBlockRenderer({
  blocks,
  conversationId,
  className,
}: NativeBlockRendererProps): React.ReactElement {
  // Pre-compute a tool_use_id → tool_result lookup so each tool_use can
  // grab its result in O(1). Walk the array in order so we can also build
  // the "already paired" set in one pass.
  const { resultMap, pairedResults } = React.useMemo(() => {
    const map = new Map<string, Extract<ContentBlock, { type: 'tool_result' }>>()
    const paired = new Set<string>()
    for (const b of blocks) {
      if (b.type === 'tool_result') map.set(b.tool_use_id, b)
    }
    for (const b of blocks) {
      if (b.type === 'tool_use' && map.has(b.id)) paired.add(b.id)
    }
    return { resultMap: map, pairedResults: paired }
  }, [blocks])

  return (
    <div className={className} data-native-blocks="true">
      {blocks.map((b, idx) => {
        if (b.type === 'text') {
          return (
            <MessageResponse
              key={`b-${idx}-text`}
              content={b.text}
              conversationId={conversationId}
            />
          )
        }
        if (b.type === 'thinking') {
          return (
            <ThinkingBlock
              key={`b-${idx}-thinking`}
              block={{ type: 'thinking', thinking: b.thinking }}
            />
          )
        }
        if (b.type === 'tool_use') {
          const result = resultMap.get(b.id)
          return (
            <ChatToolBlock
              key={`b-${idx}-tool-${b.id}`}
              toolName={b.name}
              input={b.input}
              result={result?.content}
              isError={result?.is_error}
            />
          )
        }
        if (b.type === 'tool_result') {
          // Skip if paired with a prior tool_use.
          if (pairedResults.has(b.tool_use_id)) return null
          // Orphan — render a minimal placeholder so the content isn't dropped.
          return (
            <div
              key={`b-${idx}-orphan-${b.tool_use_id}`}
              className="my-2 rounded border border-dashed border-border/50 bg-muted/30 px-2.5 py-1.5 text-[12px] text-muted-foreground/75"
              title={`tool_use_id: ${b.tool_use_id}`}
            >
              <span className="font-mono text-[11px]">tool result (orphaned)</span>
              <pre className="mt-1 whitespace-pre-wrap font-mono text-[11.5px] text-foreground/75">
                {b.content}
              </pre>
            </div>
          )
        }
        return null
      })}
    </div>
  )
}
```

**If `ChatToolBlock`'s prop names differ** — read its definition first:
```bash
grep -nA 6 "export function ChatToolBlock\|export const ChatToolBlock" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/ContentBlock.tsx
```
Adapt the props accordingly. Common alternative: it may take a single `block: SDKToolUseBlock` object plus a separately-fetched result. If the existing `ChatToolBlock` API is too SDK-shaped to adapt cleanly, build a thin adapter inside `NativeBlockRenderer` rather than touching `ChatToolBlock`.

- [ ] **Step 3.3: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

- [ ] **Step 3.4: Commit**

```bash
git add ui/src/components/agent/NativeBlockRenderer.tsx
git commit -m "$(cat <<'EOF'
feat(render): NativeBlockRenderer — in-order ContentBlock renderer

Iterates a Vec<ContentBlock> and renders text → MessageResponse,
thinking → ThinkingBlock, tool_use+matching tool_result → ChatToolBlock,
and orphaned tool_results → a minimal placeholder so persisted content
is never silently dropped.

Pairing is precomputed: each tool_use looks up its tool_result in an
id → tool_result map built once per blocks array; the matching
tool_result is then skipped on its own iteration.
EOF
)"
```

---

## Task 4: Wire the renderer into `ChatMessageItem` + `AgentMessageItem`

**Files:**
- Modify: `ui/src/components/chat/ChatMessageItem.tsx`
- Modify: `ui/src/components/agent/AgentMessages.tsx` (where `AgentMessageItem` lives, around line 487)

The pattern: when `message.contentBlocks?.length > 0`, render via `NativeBlockRenderer` and **skip** the existing flat `<Reasoning>` + `<ToolActivities>` + `<MessageResponse content={message.content}>` chain. When absent, fall back to the existing flat render.

- [ ] **Step 4.1: ChatMessageItem**

Edit `ui/src/components/chat/ChatMessageItem.tsx`. Read the existing render to find where the assistant's body (reasoning + toolActivities + content markdown) is composed. Find that JSX block and wrap it:

```tsx
{message.contentBlocks && message.contentBlocks.length > 0 ? (
  <NativeBlockRenderer
    blocks={message.contentBlocks}
    conversationId={message.conversationId}
  />
) : (
  <>
    {/* existing flat path: reasoning + toolActivities + content */}
    {/* ... unchanged ... */}
  </>
)}
```

Add the import:
```ts
import { NativeBlockRenderer } from '@/components/agent/NativeBlockRenderer'
```

The wrap should only apply to **assistant** messages — user messages are always plain text and don't have blocks. Guard:
```tsx
{message.role === 'assistant' && message.contentBlocks && message.contentBlocks.length > 0 ? ( ... ) : ( ... )}
```

- [ ] **Step 4.2: AgentMessageItem**

Edit `ui/src/components/agent/AgentMessages.tsx`. Find `AgentMessageItem` (around line 487-) and apply the same pattern. The existing flat path renders `<ThinkingBlock>` + tool activity rows + `<MessageResponse>` (search for those calls in the function body). Wrap the same way.

Import (it's already in the same file as `NativeBlockRenderer` would be — but the renderer lives in its own file now):
```ts
import { NativeBlockRenderer } from './NativeBlockRenderer'
```

- [ ] **Step 4.3: TS check + smoke**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

```bash
cd ui && npm test -- --run 2>&1 | tail -10
```
Expected: still 33/33 (no test changes; existing tests don't mock `contentBlocks`, so all messages take the legacy fallback path and look unchanged).

- [ ] **Step 4.4: Manual smoke**

```bash
cd src-tauri && cargo tauri dev
```
Open an existing chat or agent session that has thinking + tool calls. Verify:
- Old sessions render via the legacy path (visually identical to before)
- A NEW session created in this build (after backend Task 1 is in place) renders via `NativeBlockRenderer` — the message will have a `data-native-blocks="true"` attribute on the wrapper. Inspect element to confirm.
- A turn with `[Thinking → Text → ToolUse → ToolResult → Text → ToolUse → ToolResult]` shows that exact ordering on screen.

- [ ] **Step 4.5: Commit**

```bash
git add ui/src/components/chat/ChatMessageItem.tsx ui/src/components/agent/AgentMessages.tsx
git commit -m "$(cat <<'EOF'
feat(render): switch to NativeBlockRenderer when contentBlocks present

ChatMessageItem and AgentMessageItem both check
`message.contentBlocks?.length > 0`. When true, the assistant body
renders via NativeBlockRenderer for original-order display. When
false (legacy plain-text rows, or until Task 1's backend change has
re-fetched), falls back to the flat reasoning + toolActivities +
content markdown path — visually identical to before.

User messages keep the existing render unconditionally — they're
always plain text in the persisted format.
EOF
)"
```

---

## Task 5: Tests for `NativeBlockRenderer`

**Files:**
- Create: `ui/src/components/agent/NativeBlockRenderer.test.tsx`

5 cases per the roadmap acceptance criteria + the orphan branch.

- [ ] **Step 5.1: Write tests**

Create `ui/src/components/agent/NativeBlockRenderer.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { NativeBlockRenderer } from './NativeBlockRenderer'
import type { ContentBlock } from '@/lib/chat-types'
import { renderWithProviders, screen } from '@/test-utils/render'

// Mock the dependencies — we're testing the renderer's pairing/ordering
// logic, not the markdown / shiki / collapsible-thinking internals.
vi.mock('@/components/chat/MessageResponse', () => ({
  MessageResponse: ({ content }: { content: string }) => (
    <div data-testid="text-block">{content}</div>
  ),
}))

vi.mock('./ContentBlock', () => ({
  ThinkingBlock: ({ block }: { block: { thinking: string } }) => (
    <div data-testid="thinking-block">{block.thinking}</div>
  ),
  ChatToolBlock: ({ toolName, result, isError }: { toolName: string; result?: string; isError?: boolean }) => (
    <div data-testid="tool-block" data-error={isError ? 'true' : 'false'}>
      <span data-testid="tool-name">{toolName}</span>
      {result && <span data-testid="tool-result">{result}</span>}
    </div>
  ),
}))

describe('NativeBlockRenderer', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders a single text block', () => {
    const blocks: ContentBlock[] = [{ type: 'text', text: 'hello' }]
    renderWithProviders(<NativeBlockRenderer blocks={blocks} />)
    expect(screen.getByTestId('text-block')).toHaveTextContent('hello')
  })

  it('renders a single thinking block', () => {
    const blocks: ContentBlock[] = [{ type: 'thinking', thinking: 'pondering' }]
    renderWithProviders(<NativeBlockRenderer blocks={blocks} />)
    expect(screen.getByTestId('thinking-block')).toHaveTextContent('pondering')
  })

  it('renders interleaved text + thinking + paired tool_use/tool_result in order', () => {
    const blocks: ContentBlock[] = [
      { type: 'thinking', thinking: 'first thought' },
      { type: 'text', text: 'first answer' },
      { type: 'tool_use', id: 't1', name: 'read_file', input: { path: '/a.txt' } },
      { type: 'tool_result', tool_use_id: 't1', content: 'file contents', is_error: false },
      { type: 'text', text: 'second answer' },
    ]
    renderWithProviders(<NativeBlockRenderer blocks={blocks} />)

    // Thinking → text → tool → text — exactly four rendered items
    const textBlocks = screen.getAllByTestId('text-block')
    expect(textBlocks).toHaveLength(2)
    expect(textBlocks[0]).toHaveTextContent('first answer')
    expect(textBlocks[1]).toHaveTextContent('second answer')

    expect(screen.getByTestId('thinking-block')).toHaveTextContent('first thought')

    const toolBlocks = screen.getAllByTestId('tool-block')
    expect(toolBlocks).toHaveLength(1)
    expect(screen.getByTestId('tool-name')).toHaveTextContent('read_file')
    expect(screen.getByTestId('tool-result')).toHaveTextContent('file contents')

    // tool_result should NOT render its own block — it's paired with the tool_use above.
    expect(screen.queryAllByText('tool result (orphaned)')).toHaveLength(0)
  })

  it('pairs multiple tool_use/tool_result by id even out of declaration order', () => {
    const blocks: ContentBlock[] = [
      { type: 'tool_use', id: 'a', name: 'first', input: {} },
      { type: 'tool_use', id: 'b', name: 'second', input: {} },
      { type: 'tool_result', tool_use_id: 'b', content: 'result-b' },
      { type: 'tool_result', tool_use_id: 'a', content: 'result-a' },
    ]
    renderWithProviders(<NativeBlockRenderer blocks={blocks} />)
    const tools = screen.getAllByTestId('tool-block')
    expect(tools).toHaveLength(2)
    // Order follows tool_use declaration: 'first' then 'second'.
    const names = screen.getAllByTestId('tool-name').map((n) => n.textContent)
    expect(names).toEqual(['first', 'second'])
    const results = screen.getAllByTestId('tool-result').map((n) => n.textContent)
    expect(results).toEqual(['result-a', 'result-b'])
  })

  it('renders a tool_use without matching tool_result (in-flight) without a result', () => {
    const blocks: ContentBlock[] = [
      { type: 'tool_use', id: 'pending', name: 'fetch', input: { url: '/x' } },
    ]
    renderWithProviders(<NativeBlockRenderer blocks={blocks} />)
    expect(screen.getByTestId('tool-block')).toBeInTheDocument()
    expect(screen.getByTestId('tool-name')).toHaveTextContent('fetch')
    expect(screen.queryByTestId('tool-result')).toBeNull()
  })

  it('renders an orphaned tool_result (no prior tool_use) as a placeholder', () => {
    const blocks: ContentBlock[] = [
      { type: 'tool_result', tool_use_id: 'unknown-id', content: 'leftover' },
    ]
    renderWithProviders(<NativeBlockRenderer blocks={blocks} />)
    // No tool block (no tool_use to render).
    expect(screen.queryByTestId('tool-block')).toBeNull()
    // Orphan placeholder appears.
    expect(screen.getByText(/tool result \(orphaned\)/i)).toBeInTheDocument()
    expect(screen.getByText('leftover')).toBeInTheDocument()
  })

  it('propagates is_error through to ChatToolBlock', () => {
    const blocks: ContentBlock[] = [
      { type: 'tool_use', id: 'x', name: 'risky', input: {} },
      { type: 'tool_result', tool_use_id: 'x', content: 'oops', is_error: true },
    ]
    renderWithProviders(<NativeBlockRenderer blocks={blocks} />)
    expect(screen.getByTestId('tool-block')).toHaveAttribute('data-error', 'true')
  })

  it('marks the wrapper with data-native-blocks="true"', () => {
    const blocks: ContentBlock[] = [{ type: 'text', text: 'x' }]
    const { container } = renderWithProviders(<NativeBlockRenderer blocks={blocks} />)
    expect(container.querySelector('[data-native-blocks="true"]')).not.toBeNull()
  })
})
```

- [ ] **Step 5.2: Run + commit**

```bash
cd ui && npx vitest run NativeBlockRenderer 2>&1 | tail -15
```
Expected: **8/8 passing**.

If `MessageResponse`'s real path differs from `@/components/chat/MessageResponse`, adjust the `vi.mock` path. Common alternative: it might live at `@/components/chat/MessageResponse.tsx` — check with `find ui/src -name "MessageResponse.tsx"`.

```bash
git add ui/src/components/agent/NativeBlockRenderer.test.tsx
git commit -m "$(cat <<'EOF'
test(render): NativeBlockRenderer pairing + ordering

8 cases covering: text-only, thinking-only, interleaved, paired
tool_use/tool_result, declaration-order pairing across multiple
tools, in-flight tool_use without result, orphan tool_result
placeholder, is_error propagation, and the data-native-blocks
marker.
EOF
)"
```

---

## Task 6: Final verification + push + PR

- [ ] **Step 6.1: Full pipeline**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
echo "=== vite ===" && (cd ui && npx vite build 2>&1 | tail -3)
```
Expected: clean cargo, all rust tests passing, 0 TS errors, **41 frontend tests** passing (33 prior + 8 new), Vite build succeeds.

- [ ] **Step 6.2: Push + PR**

```bash
git push -u origin claude/p2-native-block-rendering
gh pr create --title "P2: native structured-block rendering" --body "$(cat <<'EOF'
## Summary

Renders assistant turns in their original ContentBlock order instead of flattening to "all thinking, then all tools, then text". The data has always been structured on disk; the load path just threw away ordering.

## What changed

| Layer | Change |
|---|---|
| Backend | `MessageResponse.content_blocks: Option<Vec<ContentBlock>>` populated by `get_messages`. Same field on the agent path's JSON output. Falls back to `None` for legacy plain-text rows. |
| Backend | Reuses the existing parse — no double-deserialization. |
| Frontend types | `ContentBlock` discriminated union added to `chat-types.ts`. `ChatMessage` and `AgentMessage` both gain `contentBlocks?`. |
| Frontend | New `NativeBlockRenderer.tsx` walks blocks in order. text → MessageResponse, thinking → ThinkingBlock, tool_use+matching tool_result → ChatToolBlock, orphan tool_result → placeholder. |
| Frontend | `ChatMessageItem` + `AgentMessageItem` switch to the new renderer when `contentBlocks?.length > 0`; legacy flat path otherwise. |
| Tests | 8 new `NativeBlockRenderer` tests covering pairing, ordering, in-flight, orphan, error propagation. |

## Verification

- ✅ `cargo build` clean
- ✅ Rust tests passing
- ✅ `tsc --noEmit` clean
- ✅ **41 frontend tests** passing (33 prior + 8 new)
- ✅ Manual: a turn with `[Thinking → Text → ToolUse → ToolResult → Text → ToolUse → ToolResult]` renders in that exact order

## Out of scope

- Schema migration — none needed; `messages.content` was already `Vec<ContentBlock>` JSON
- Streaming render — already in order via `streamingState.{reasoning, toolActivities, content}`; no change
- Editing/regen — separate roadmap entry (P7)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria

- ✅ A turn structured as `[Thinking → Text → ToolUse → ToolResult → Text → ToolUse → ToolResult]` renders in that exact order
- ✅ Old persisted rows (`content` is plain string, no `content_blocks`) still render via the legacy path
- ✅ Streaming live bubble looks identical (no streaming-path changes)
- ✅ User messages unaffected
- ✅ Orphaned `tool_result` rows visible as a placeholder rather than silently dropped
- ✅ 41 frontend tests passing (33 prior + 8 new)
- ✅ Each task is its own commit (bisectable)

## Out of scope (deferred)

- Edit / regenerate path through structured blocks (roadmap §P7)
- Sweep of any remaining SDK-shaped consumers (roadmap §P12)
- Visual drift comparison between block-renderer and flat-renderer paths (roadmap §P12)
