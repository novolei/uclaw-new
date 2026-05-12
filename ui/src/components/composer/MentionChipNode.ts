/**
 * MentionChipNode — TipTap inline atom node for `/<skill>` and `@<file>`
 * mentions in the composer.
 *
 * Atomicity is the load-bearing property:
 *   - Cursor can be **before** or **after** the chip, never **inside**.
 *   - Backspace deletes the whole chip in one keystroke.
 *   - Selection includes the whole chip or nothing.
 *
 * Wire-format contract (intentional, see spec §"Wire format compatibility"):
 *   - `editor.getText({ blockSeparator: '\n' })` walks the doc and for each
 *     chip emits `renderText(node)` which produces `/<name>` or `@<absPath>`.
 *   - This keeps `agent_messages.content` as plain TEXT — backend doesn't
 *     need to know chips exist.
 *
 * Chip is a UI sugar layer on top of the same plain-string wire format the
 * pre-PR #130 textarea + popover path produced.
 */
import { Node, mergeAttributes } from '@tiptap/core'

/** Chip kinds — matches the two trigger characters they originate from. */
export type MentionChipKind = 'skill' | 'file'

export interface MentionChipAttrs {
  kind: MentionChipKind
  /** What to display in the chip body — for skills this is the slash name,
   *  for files this is the bare filename (the popup row title). */
  display: string
  /** What to emit in wire-format text — for skills `name` from
   *  list_invocable_skills, for files the `absolutePath` from
   *  search_workspace_files_for_mention. */
  value: string
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    mentionChip: {
      /** Insert a mention chip at the current selection, replacing any
       *  active query span (caller passes `from`/`to` to wipe). */
      insertMentionChip: (attrs: MentionChipAttrs & { from?: number; to?: number }) => ReturnType
    }
  }
}

/** Wire-format serialization for a single chip. Public so the doc walker
 *  in `composer-serialize.ts` can reuse the exact same rule. */
export function chipToWireText(attrs: MentionChipAttrs): string {
  return attrs.kind === 'skill' ? `/${attrs.value}` : `@${attrs.value}`
}

export const MentionChipNode = Node.create({
  name: 'mentionChip',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      kind: {
        default: 'skill' as MentionChipKind,
        parseHTML: (el) => (el.getAttribute('data-kind') as MentionChipKind) ?? 'skill',
        renderHTML: (attrs) => ({ 'data-kind': attrs.kind }),
      },
      display: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-display') ?? '',
        renderHTML: (attrs) => ({ 'data-display': attrs.display }),
      },
      value: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-value') ?? '',
        renderHTML: (attrs) => ({ 'data-value': attrs.value }),
      },
    }
  },

  parseHTML() {
    return [{ tag: 'span[data-mention-chip]' }]
  },

  renderHTML({ node, HTMLAttributes }) {
    const attrs = node.attrs as MentionChipAttrs
    const sigil = attrs.kind === 'skill' ? '/' : '@'
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        'data-mention-chip': '',
        // Tailwind classes keep theme tokens — `bg-primary/10` adapts to
        // every theme palette already used by the badge in PR #124's
        // Settings → 内置技能.
        class: [
          'inline-flex items-center gap-0.5 px-1.5 py-0 rounded',
          'text-[12px] leading-[1.5] align-baseline',
          attrs.kind === 'skill'
            ? 'bg-violet-500/10 text-violet-700 dark:text-violet-300 border border-violet-500/20'
            : 'bg-blue-500/10 text-blue-700 dark:text-blue-300 border border-blue-500/20',
          // contenteditable=false makes the chip a true atom in DOM too,
          // matching ProseMirror's atom: true at the schema level. Without
          // this the user can sometimes click inside and start typing.
        ].join(' '),
        contenteditable: 'false',
      }),
      `${sigil}${attrs.display}`,
    ]
  },

  /** TipTap calls this when computing `editor.getText()`. Emits the chip's
   *  wire-format inline form so the resulting plain string matches what a
   *  pre-PR #130 textarea would have contained. */
  renderText({ node }) {
    return chipToWireText(node.attrs as MentionChipAttrs)
  },

  addCommands() {
    return {
      insertMentionChip:
        (attrs) =>
        ({ chain }) => {
          const { from, to, ...nodeAttrs } = attrs
          let c = chain()
          // If the caller provided a span to wipe (the trigger char + query),
          // delete it first so the chip lands where the `/` or `@` was.
          if (from != null && to != null) {
            c = c.deleteRange({ from, to })
          }
          return c
            .insertContent({ type: 'mentionChip', attrs: nodeAttrs })
            // Trailing space so the user can immediately keep typing without
            // accidentally re-triggering on the next character. Mirrors the
            // PR #130 popover commit behavior.
            .insertContent(' ')
            .run()
        },
    }
  },
})
