/**
 * Tests for the TipTap-doc → wire-format-string serializer.
 *
 * Why test the serializer directly rather than mounting TipTap: this is
 * the load-bearing contract — `agent_messages.content TEXT` and
 * `send_agent_message`'s `user_message` param BOTH consume what this
 * function produces. If chips drop or re-emit differently, the backend
 * silently sees different input. Tests pin the contract.
 */
import { describe, it, expect } from 'vitest'
import type { JSONContent } from '@tiptap/core'
import { serializeDocToWireText } from './composer-serialize'

function doc(...content: JSONContent[]): JSONContent {
  return { type: 'doc', content }
}

function p(...content: JSONContent[]): JSONContent {
  return { type: 'paragraph', content }
}

function text(s: string): JSONContent {
  return { type: 'text', text: s }
}

function skillChip(value: string): JSONContent {
  return { type: 'mentionChip', attrs: { kind: 'skill', display: value, value } }
}

function fileChip(absPath: string, display = absPath): JSONContent {
  return { type: 'mentionChip', attrs: { kind: 'file', display, value: absPath } }
}

describe('serializeDocToWireText', () => {
  it('empty doc returns empty string', () => {
    expect(serializeDocToWireText(doc())).toBe('')
    expect(serializeDocToWireText(doc(p()))).toBe('')
    expect(serializeDocToWireText(null)).toBe('')
  })

  it('plain text round-trips', () => {
    expect(serializeDocToWireText(doc(p(text('hello world'))))).toBe('hello world')
  })

  it('skill chip emits /<value>', () => {
    expect(serializeDocToWireText(doc(p(skillChip('tdd'))))).toBe('/tdd')
  })

  it('file chip emits @<absolutePath>', () => {
    expect(
      serializeDocToWireText(doc(p(fileChip('/Users/foo/bar.tsx')))),
    ).toBe('@/Users/foo/bar.tsx')
  })

  it('mixed prose + chip inline', () => {
    expect(
      serializeDocToWireText(
        doc(p(
          text('help me '),
          skillChip('tdd'),
          text(' refactor this'),
        )),
      ),
    ).toBe('help me /tdd refactor this')
  })

  it('multiple chips in one paragraph', () => {
    expect(
      serializeDocToWireText(
        doc(p(
          text('compare '),
          fileChip('/src/a.ts'),
          text(' with '),
          fileChip('/src/b.ts'),
          text(' for me'),
        )),
      ),
    ).toBe('compare @/src/a.ts with @/src/b.ts for me')
  })

  it('hardBreak becomes newline', () => {
    expect(
      serializeDocToWireText(
        doc(p(text('line 1'), { type: 'hardBreak' }, text('line 2'))),
      ),
    ).toBe('line 1\nline 2')
  })

  it('multiple paragraphs joined with single newline (NOT TipTap default \\n\\n)', () => {
    // Pinned: the textarea-era wire format used `\n` between lines.
    // Doubling to `\n\n` would change what the backend's content
    // column looks like and break diff comparisons with old messages.
    expect(
      serializeDocToWireText(doc(p(text('p1')), p(text('p2')))),
    ).toBe('p1\np2')
  })

  it('trailing newlines stripped (matches send_agent_message expectation)', () => {
    // The textarea era's value never had trailing newlines because the
    // submit flow trimmed. We do the same so backend cost / token
    // counts match.
    expect(
      serializeDocToWireText(doc(p(text('hi')), p(), p())),
    ).toBe('hi')
  })

  it('unknown inline node types are dropped silently (forward-compat)', () => {
    // If a future extension adds a new inline node type, the serializer
    // shouldn't throw — just emit nothing for it. The doc stays
    // editable; only the wire-format omits the unknown bit.
    expect(
      serializeDocToWireText(
        doc(p(text('a'), { type: 'mysteryNode' }, text('b'))),
      ),
    ).toBe('ab')
  })

  it('skill chip value with hyphens preserved', () => {
    expect(
      serializeDocToWireText(doc(p(skillChip('write-a-skill')))),
    ).toBe('/write-a-skill')
  })

  it('file chip with absolute path containing spaces preserved', () => {
    // Absolute paths can contain spaces (macOS "My Documents").
    // Wire format keeps them verbatim — the agent loop's path-policy
    // walker can dequote / quote as needed.
    expect(
      serializeDocToWireText(
        doc(p(text('open '), fileChip('/Users/foo/My Folder/file.tsx'))),
      ),
    ).toBe('open @/Users/foo/My Folder/file.tsx')
  })
})
