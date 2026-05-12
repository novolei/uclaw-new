/**
 * composer-serialize — walk a TipTap doc and produce the plain string the
 * backend's `send_agent_message` IPC expects.
 *
 * Why we don't use `editor.getText()` directly: it works (chips render their
 * wire-format via `renderText` in MentionChipNode), but doing it inline here
 * lets us:
 *   - Pin the contract in tests (which we'd otherwise have to mount TipTap
 *     to exercise — slow + brittle).
 *   - Choose paragraph separators ourselves; TipTap's default is "\n\n" which
 *     would change the wire format compared to the textarea era.
 *
 * The chip rule is duplicated with `MentionChipNode.renderText` intentionally —
 * the schema-level renderer powers TipTap's own getText, this top-level
 * walker powers our serialization tests + the controlled-component bridge.
 * Both produce identical output; if you change one, change the other.
 */
import type { JSONContent } from '@tiptap/core'
import { chipToWireText, type MentionChipAttrs } from './MentionChipNode'

/** Serialize a TipTap JSON doc to the wire-format string the backend
 *  expects. Round-trip with `parseWireFormat` is best-effort — chips
 *  embedded by the user via popup are preserved, but plain text in
 *  hydrated drafts is NOT auto-chipified (see spec §"Migration to draft
 *  strings"). */
export function serializeDocToWireText(doc: JSONContent | null | undefined): string {
  if (!doc || !doc.content) return ''
  return doc.content.map(serializeBlock).join('\n').replace(/\n+$/, '')
}

function serializeBlock(block: JSONContent): string {
  if (block.type !== 'paragraph') {
    // Forward-compat: any non-paragraph block (only `hardBreak` should
    // appear at this level with our extension config, and hard breaks
    // are inline so they'd be inside paragraphs) gets joined naively.
    return block.content?.map(serializeInline).join('') ?? ''
  }
  return (block.content ?? []).map(serializeInline).join('')
}

function serializeInline(node: JSONContent): string {
  switch (node.type) {
    case 'text':
      return node.text ?? ''
    case 'hardBreak':
      return '\n'
    case 'mentionChip': {
      const attrs = node.attrs as MentionChipAttrs | undefined
      if (!attrs) return ''
      return chipToWireText(attrs)
    }
    default:
      // Unknown inline node — emit nothing rather than throwing. Forward-
      // compatible if the extension set ever grows.
      return ''
  }
}
