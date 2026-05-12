# Composer TipTap Chip Container — Design

**Date:** 2026-05-13
**Status:** Approved (option #2 from earlier ROI discussion)
**Implements:** Replacement of the placeholder textarea in `RichTextInput` with a TipTap-based editor scoped narrowly to: paragraphs + mention chips + paste hooks. Bold/italic/markdown/code-blocks are **out of scope** — this is a chip container, not a rich text editor.

## Why not the full TipTap port

Earlier discussion ROI-evaluated three options:

1. Keep textarea + popover (status quo from PR #130)
2. Full TipTap port (~1500 LOC, blast radius across 6 composer props + IME risk)
3. **Middle ground (this PR)** — TipTap as a chip container only

Option 3 is the only one that delivers the *real prize* (atomic mention chips that can't be corrupted character-by-character) without paying for things uClaw doesn't need (bold/italic in chat messages, markdown formatting, structured content).

## Atomic mention chip — the load-bearing feature

Today (post-PR #130), selecting `tdd` from the `/` popup inserts `/tdd ` as plain text. The user can:
- Place the cursor inside `tdd` and delete `t`, producing `/dd` — silently corrupted
- Backspace at the end deletes one char at a time
- Drag-select half the path of a file mention and lose the `@` prefix

With chip nodes:
- The chip is an atomic ProseMirror node — cursor can be **before** or **after** but never **inside** it
- Backspace deletes the whole chip in one keystroke
- Visual styling distinguishes mention from prose
- Selection includes the whole chip or nothing

## What we keep from PR #130

The popup itself stays:
- `ComposerMentionPopup` — unchanged
- `ComposerMentionController` — same React component, only the `commitReplacement` path changes (TipTap chip insert command instead of string splice)
- Data fetching (`listInvocableSkills`, `searchWorkspaceFilesForMention`) — unchanged
- Both composers' wiring shape — unchanged

What changes:
- `useComposerMentionTrigger` (textarea selectionStart-based) → replaced by a TipTap-aware trigger hook that reads `editor.state.selection.from` from the ProseMirror document
- `RichTextInput` internal — TipTap editor instead of `<textarea>`

## Wire format compatibility (critical decision)

**Decision: chips serialize back to the same inline form they replaced.**

| Chip kind | Display | Serialized to backend |
|---|---|---|
| skill | `/<name>` (styled pill) | `/<name>` (string) |
| file | `<name>` (with full path tooltip) | `@<absolutePath>` (string) |

This means:
- `agent_messages.content` stays TEXT
- `send_agent_message`'s `user_message` param stays a plain string
- The PR #120 slash-command resolver in `tauri_commands.rs::resolve_slash_skill` works unchanged
- Backend has zero migration

The chip is **a UI sugar layer**. The wire format is the source of truth. Draft persistence (`agentSessionDraftsAtom`, `conversationDraftsAtom`) stores strings — when hydrating a saved draft, we DON'T auto-convert `@/path/foo` text into chips; chips appear only on fresh popup selection. This is fine because mentions in old drafts are already fully-qualified strings the backend handles.

## TipTap extension set (lean)

Disable everything in StarterKit except the structural minimum:

```ts
StarterKit.configure({
  heading: false,
  bold: false,
  italic: false,
  strike: false,
  blockquote: false,
  code: false,
  codeBlock: false,
  horizontalRule: false,
  bulletList: false,
  orderedList: false,
  listItem: false,
  // Keep: document, paragraph, text, hardBreak, history, dropcursor, gapcursor
})
```

Plus:
- `Placeholder` extension (custom placeholder text)
- Custom `MentionChipNode` (inline atom node with `kind`, `value`, `display` attrs)

Total bundle increment vs PR #130: ~70 KB gzip. StarterKit is already pulled in by the preview editor's `MarkdownRichEditor` chunk, but the composer's TipTap will be in the main bundle (the composer mounts immediately when the user opens the app, no lazy load).

## Trigger detection rewrite

PR #130's `useComposerMentionTrigger` reads `textareaRef.current.selectionStart` — a DOM offset that doesn't exist in TipTap's contenteditable model.

New approach: `useEditorMentionTrigger(editor, options)` reads:
- `editor.state.doc` — the current document
- `editor.state.selection.from` — the cursor position as a ProseMirror integer offset
- The text content between the trigger char and cursor — the active query

The trigger detection logic itself stays the same shape (walk back from cursor to find `/` or `@` at a word boundary), just operating on ProseMirror positions instead of DOM offsets.

## Keymap (load-bearing)

| Key | Behavior |
|---|---|
| `Enter` | Submit (preserves textarea behavior) UNLESS popup is open → popup consumes |
| `Shift+Enter` | Insert hard break (paragraph stays single-block) |
| `⌘/Ctrl+Enter` | Alternative submit when `sendWithCmdEnter` |
| `Backspace` after chip | Delete chip atomically (built-in atom node behavior) |
| Arrow nav around chip | Cursor jumps over the chip as one unit |

## Out of scope

- **Bold/italic/headings** — chat messages don't need them
- **Markdown rendering of input** — `**bold**` stays as literal text
- **Drag-from-FilesRail-to-composer** — possible with TipTap's drop handler but deferred until users ask
- **`#` MCP autocomplete** — same as PR #130, deferred
- **Chip serialization to structured JSON for backend** — explicitly not doing this; wire format stays plain string. The chip's structure is UI-only.

## Risk: IME (Chinese / Japanese input)

ProseMirror has historically had IME quirks (candidate-character cursor jumps). TipTap v3 includes the upstream ProseMirror IME fixes. Plan:

- Verify with a basic Chinese-IME smoke test (type pinyin → select candidate → confirm chip insertion still works)
- If broken: fall back is to keep PR #130's textarea path behind a feature flag

## Migration to draft strings

`AgentView` and `ChatInput` both keep their current `draftsMap` atoms (string-typed). On editor update:

```ts
editor.on('update', () => {
  const text = serializeToWireFormat(editor.state.doc)
  setValue(text)  // unchanged callback
})
```

`serializeToWireFormat` walks the doc and:
- For text nodes: returns the text
- For MentionChipNode: returns `chip.attrs.kind === 'skill' ? `/${chip.attrs.value}` : `@${chip.attrs.value}` `
- For HardBreak: returns `\n`
- For Paragraph: joins children + appends `\n`

## File structure

| File | Status |
|---|---|
| `ui/src/components/composer/MentionChipNode.tsx` | NEW — TipTap Node definition + React render |
| `ui/src/components/composer/composer-serialize.ts` | NEW — wire format serializer + tests |
| `ui/src/hooks/useEditorMentionTrigger.ts` | NEW — TipTap-aware trigger detection |
| `ui/src/hooks/useComposerMentionTrigger.ts` | **DELETED** — replaced |
| `ui/src/hooks/useComposerMentionTrigger.test.tsx` | **DELETED** — replaced by new tests |
| `ui/src/components/ai-elements/rich-text-input.tsx` | REWRITE — TipTap-based |
| `ui/src/components/composer/ComposerMentionController.tsx` | EDIT — replace string-splice commit with TipTap chip-insert command |
| `ui/src/components/agent/AgentView.tsx` | EDIT — `composerTextareaRef` → `composerEditorRef`; same wiring shape |
| `ui/src/components/chat/ChatInput.tsx` | EDIT — same |
| `ui/src/types/tiptap.d.ts` | **DELETED** — was a stub blocking real TipTap types (PR #127 deviation #1) |

## Success criteria

1. All PR #130 tests + manual checks still pass (no behavior regression)
2. Backspacing through a chip deletes it as one keystroke
3. Cursor cannot land inside a chip
4. Backend receives identical string content (verified via integration test on `send_agent_message`)
5. `npx tsc --noEmit` clean with real TipTap types (no stub blocking)
6. IME smoke test passes with at least one CJK input source
