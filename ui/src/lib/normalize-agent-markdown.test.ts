import { describe, it, expect } from 'vitest'
import { normalizeAgentMarkdown } from './normalize-agent-markdown'

describe('normalizeAgentMarkdown', () => {
  describe('the bug-fix that motivated this util', () => {
    it('does NOT split valid `## heading` lines into `#` + `# heading`', () => {
      // Regression: PR #68's regex `([^\n])(#{1,6} )` matched the first
      // `#` as the prefix capture and ate the second `#` into the marker,
      // turning `## 📊 Waytoon` into `#\n# 📊 Waytoon` (stray `#` + H1
      // instead of intended H2). User saw empty `|` decoration rows in
      // the rendered output.
      const input = '## 📊 Waytoon 全部视频播放量统计'
      expect(normalizeAgentMarkdown(input)).toBe(input)
    })

    it('does not split `### heading` either', () => {
      const input = '### 🏆 总播放量（已获取数据）'
      expect(normalizeAgentMarkdown(input)).toBe(input)
    })

    it('preserves multi-line markdown with mixed heading levels', () => {
      const input = [
        '正文段落。',
        '',
        '---',
        '',
        '## 📊 H2 heading',
        '',
        '### 🏆 H3 heading',
        '',
        '| col | col |',
        '|:---|:---|',
        '| a | b |',
      ].join('\n')
      expect(normalizeAgentMarkdown(input)).toBe(input)
    })
  })

  describe('intended use — fixing inline heading markers', () => {
    it('inserts newline when content runs into a heading marker', () => {
      const input = 'prose### Heading'
      const out = normalizeAgentMarkdown(input)
      expect(out).toBe('prose\n### Heading')
    })

    it('handles multiple heading levels inline', () => {
      const input = 'foo# H1\nbar## H2\nbaz### H3'
      const out = normalizeAgentMarkdown(input)
      expect(out).toBe('foo\n# H1\nbar\n## H2\nbaz\n### H3')
    })
  })

  describe('table preservation', () => {
    it('leaves table separator rows alone', () => {
      // Earlier list-marker rule rewrote `--- ` substrings inside
      // separator rows, breaking the whole table. Skipping `|`-prefixed
      // lines is the belt-and-suspenders guarantee.
      const sep = '|:----|:-----:|:--------:|'
      expect(normalizeAgentMarkdown(sep)).toBe(sep)
    })

    it('leaves regular table rows alone', () => {
      const row = '| ✅ **已获取数据** | **40 个** | **13,813 次** |'
      expect(normalizeAgentMarkdown(row)).toBe(row)
    })
  })

  describe('edge cases', () => {
    it('returns empty string unchanged', () => {
      expect(normalizeAgentMarkdown('')).toBe('')
    })

    it('returns content with no heading markers unchanged', () => {
      const input = '这是一段没有标题的纯文本。\n第二行。'
      expect(normalizeAgentMarkdown(input)).toBe(input)
    })

    it('does not match # without trailing space (e.g. anchors)', () => {
      const input = 'See section #1 for details'
      expect(normalizeAgentMarkdown(input)).toBe(input)
    })
  })
})
