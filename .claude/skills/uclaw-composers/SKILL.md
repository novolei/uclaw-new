---
name: uclaw-composers
description: Use whenever you change paste, drop, attachment, send, or any input behavior in the chat or agent UI. Trigger phrases include "chat input", "agent input", "composer", "paste files", "drag drop", "submit handler", "RichTextInput", "ChatInput.tsx", "AgentView.tsx", "attachment", "@-mention", "slash command", "/", "@". Catches the most common UI regression — applying a fix to only one of the two parallel composers, which hides a bug in the more-used mode.
---

# uClaw — Two Parallel Composers

uClaw has **two parallel composer components** that wrap the same
underlying input element. They look like the same feature but they are
*separate React components* with *separate handlers*. Any behavior change
must be applied to both.

| Component | Mode | File |
|---|---|---|
| `ChatInput.tsx` | Chat mode (lighter, conversation-focused) | `ui/src/components/chat/ChatInput.tsx` |
| `AgentView.tsx` | Agent mode (more common in real use) | `ui/src/components/agent/AgentView.tsx` |

Both wrap `RichTextInput` (a `[PLACEHOLDER]` textarea today; real TipTap
port scheduled for W4 of the Proma preview port). **Until the TipTap port
lands, prop wiring lives in the composers, NOT in RichTextInput.**

## The trap

When a bug is reported against one mode, the fix is applied there, tested,
shipped — and the bug stays in the other mode. Worse, Agent mode is the
more-used surface, so a Chat-mode-only fix leaves the worse bug live.

A specific class of handler this matters for:

- `handlePasteFiles` (Cmd-V on an image / file)
- `handleDrop` (drag-drop attachment)
- `onSubmit` / send wiring (Enter, Cmd+Enter, send button click)
- `@`-mention and `/`-slash command handling
- Attachment chip rendering and removal
- Multi-line vs single-line behavior

## The rule — apply to both, mention both

For any composer behavior change:

1. **Make the edit in both files in the same commit.** Don't split.
2. **Verify the prop wiring matches.** If you add a new prop to
   `RichTextInput`, both composers must pass it.
3. **Call out in the commit body** that this touches both, so reviewers
   know it's not a copy-paste mistake. Example:
   ```
   ui(chat,agent): handle pasted PNG via createObjectURL

   Applies to both composers (per CLAUDE.md §"Chat-composer behavior change"):
   - ui/src/components/chat/ChatInput.tsx → handlePasteFiles
   - ui/src/components/agent/AgentView.tsx → handlePasteFiles
   ```
4. **Test both modes** in dev. Switching modes is one click in the UI;
   it's faster than the diff review.

## Procedure — implementing a composer change

1. **Read both files first.** Don't assume they're symmetric — they often
   diverge in subtle ways (different prop sets, different validation).
2. **Decide whether the change is shared or mode-specific.** Most paste/
   drop / submit changes are shared; some UI affordances (e.g. mode
   indicator) are intentionally one-sided.
3. **Write the change as identical code in both files** unless mode-
   specific. Resist the urge to "extract to a hook" mid-task — that's a
   refactor, and the codebase prefers flat shape (CLAUDE.md
   §"Match the codebase shape").
4. **If the change demands a new shared piece**, the right move is to add
   it to `RichTextInput` (the shared wrapper) — but only when the TipTap
   port lands. Pre-port, keep it duplicated in the composers.
5. **Vitest both**: there are tests like `ChatInput.test.tsx` and
   `AgentView.test.tsx` next to the components. Run:
   ```bash
   cd ui && npm test -- --run components/chat components/agent 2>&1 | tail -20
   ```

## What lives where

| Lives in `ChatInput.tsx` | Lives in `AgentView.tsx` |
|---|---|
| `handlePasteFiles` | `handlePasteFiles` |
| `handleDrop` | `handleDrop` |
| Send → `chatService` IPC | Send → `agentService` IPC |
| Renders chat-mode toolbar (lighter) | Renders agent-mode toolbar (more controls) |
| Conversation atom (jotai) | Session atom (jotai) |

The IPC targets and atoms differ — that's intentional, don't unify.

## See also

- `ui/src/components/chat/ChatInput.tsx`
- `ui/src/components/agent/AgentView.tsx`
- `ui/src/components/shared/RichTextInput.tsx` (placeholder; TipTap port pending)
- CLAUDE.md Part 1 *Adjacent edits that look like scope creep* → "Chat-composer behavior change"
