/**
 * markdownFileChipPlugin — Remark plugin that rewrites file references
 * into custom HAST nodes the renderer maps to <FilePathChip>.
 *
 * Three detection patterns (in AST visitor order):
 *   1. mdast `link` whose URL ends in a previewable extension and is not
 *      a protocol URL.
 *   2. mdast `inlineCode` whose value is a single filename
 *      (`/^[\w.-]+\.([a-z0-9]+)$/` with previewable ext).
 *   3. mdast `text` containing slash-bearing path tokens with optional
 *      `:line:col` suffix.
 *
 * Fenced code (mdast `code`) is naturally skipped because the visitor only
 * matches `link` / `inlineCode` / `text` types and code-block text is NOT
 * exposed as a separate `text` node — it lives in the code node's `value`
 * string, so it's never visited.
 *
 * Custom node emission uses the unified ecosystem's `data.hName` /
 * `data.hProperties` convention: mdast-util-to-hast converts those into
 * a HAST element with the given tag name + properties. react-markdown then
 * renders the unknown tag via its `components` map (Task 10).
 */

import type { Plugin } from 'unified'
import type { Root, Link, InlineCode, Text, RootContent, PhrasingContent } from 'mdast'
import { visit, SKIP } from 'unist-util-visit'
import { isPreviewableExt, getExtension } from '@/components/preview/utils/ext-classifier'
import { parseLineCol } from './line-col-parser'

const PROTOCOL_RE = /^[a-z][a-z0-9+.-]*:\/\//i

const INLINE_CODE_FILENAME_RE = /^[\w.@-]+\.([a-z0-9]+)$/i

const PATH_TOKEN_RE =
  /(?:^|[\s\(])((?:[\w.@-]+\/)+[\w.-]+\.[a-z0-9]+(?::\d+(?::\d+)?)?)(?=[\s\)\.,;:!?]|$)/g

interface ChipHProperties {
  rawPath: string
  label: string
  line?: number
  col?: number
  [key: string]: unknown
}

function makeChipNode(rawPath: string, label: string): RootContent {
  const parsed = parseLineCol(rawPath)
  const hProperties: ChipHProperties = {
    rawPath: parsed.path,
    label,
  }
  if (parsed.line !== undefined) hProperties.line = parsed.line
  if (parsed.col !== undefined) hProperties.col = parsed.col
  return {
    type: 'text',
    value: '',
    data: { hName: 'file-path-chip', hProperties },
  } as RootContent
}

function urlIsChip(url: string): boolean {
  if (PROTOCOL_RE.test(url)) return false
  const parsed = parseLineCol(url)
  const ext = getExtension(parsed.path)
  return ext.length > 0 && isPreviewableExt(ext)
}

function inlineCodeIsChip(value: string): boolean {
  const m = INLINE_CODE_FILENAME_RE.exec(value)
  if (!m) return false
  return isPreviewableExt(m[1]!)
}

export const markdownFileChipPlugin: Plugin<[], Root> = function plugin() {
  return (tree: Root) => {
    // Pattern 1 + 2: link + inlineCode nodes — substitute in place via parent.
    visit(tree, (node, index, parent) => {
      if (!parent || typeof index !== 'number') return
      if (node.type === 'link') {
        const link = node as Link
        if (!urlIsChip(link.url)) return
        const labelNode = link.children?.[0]
        const label =
          labelNode && 'value' in labelNode && typeof labelNode.value === 'string'
            ? labelNode.value
            : link.url
        parent.children[index] = makeChipNode(link.url, label) as PhrasingContent
        return SKIP
      }
      if (node.type === 'inlineCode') {
        const inline = node as InlineCode
        if (!inlineCodeIsChip(inline.value)) return
        parent.children[index] = makeChipNode(inline.value, inline.value) as PhrasingContent
        return SKIP
      }
    })

    // Pattern 3: split text nodes around path-like tokens.
    visit(tree, 'text', (node: Text, index, parent) => {
      if (!parent || typeof index !== 'number') return
      const value = node.value
      const matches: { start: number; end: number; raw: string }[] = []
      PATH_TOKEN_RE.lastIndex = 0
      let m: RegExpExecArray | null
      while ((m = PATH_TOKEN_RE.exec(value)) !== null) {
        const raw = m[1]!
        const parsed = parseLineCol(raw)
        const ext = getExtension(parsed.path)
        if (!ext || !isPreviewableExt(ext)) continue
        const start = m.index + (m[0].length - raw.length)
        matches.push({ start, end: start + raw.length, raw })
      }
      if (matches.length === 0) return

      const replacement: PhrasingContent[] = []
      let cursor = 0
      for (const mh of matches) {
        if (mh.start > cursor) {
          replacement.push({ type: 'text', value: value.slice(cursor, mh.start) } as PhrasingContent)
        }
        const parsed = parseLineCol(mh.raw)
        const label = parsed.path.split('/').pop() ?? parsed.path
        replacement.push(makeChipNode(mh.raw, label) as PhrasingContent)
        cursor = mh.end
      }
      if (cursor < value.length) {
        replacement.push({ type: 'text', value: value.slice(cursor) } as PhrasingContent)
      }
      parent.children.splice(index, 1, ...replacement)
      return [SKIP, index + replacement.length]
    })
  }
}
