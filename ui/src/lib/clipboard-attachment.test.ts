import { describe, it, expect } from 'vitest'
import {
  LONG_TEXT_ATTACHMENT_THRESHOLD,
  looksLikeMarkdown,
  formatClipboardTimestamp,
  createClipboardTextFile,
} from './clipboard-attachment'

describe('clipboard-attachment', () => {
  describe('LONG_TEXT_ATTACHMENT_THRESHOLD', () => {
    it('is 500 (matches Proma)', () => {
      expect(LONG_TEXT_ATTACHMENT_THRESHOLD).toBe(500)
    })
  })

  describe('looksLikeMarkdown', () => {
    it('detects ATX header', () => {
      expect(looksLikeMarkdown('# Title\nbody')).toBe(true)
    })

    it('detects fenced code block', () => {
      expect(looksLikeMarkdown('hello\n```ts\nconst x = 1\n```')).toBe(true)
    })

    it('detects pipe table', () => {
      expect(looksLikeMarkdown('| a | b |\n|---|---|\n| 1 | 2 |')).toBe(true)
    })

    it('detects YAML frontmatter', () => {
      expect(looksLikeMarkdown('---\ntitle: x\n---\nbody')).toBe(true)
    })

    it('detects blockquote', () => {
      expect(looksLikeMarkdown('hello\n> quoted text')).toBe(true)
    })

    it('detects unordered list', () => {
      expect(looksLikeMarkdown('intro\n- one\n- two')).toBe(true)
    })

    it('detects ordered list', () => {
      expect(looksLikeMarkdown('intro\n1. one\n2. two')).toBe(true)
    })

    it('detects inline link', () => {
      expect(looksLikeMarkdown('see [docs](https://x)')).toBe(true)
    })

    it('rejects plain prose', () => {
      expect(looksLikeMarkdown('Just a plain paragraph with no markup at all.'))
        .toBe(false)
    })
  })

  describe('formatClipboardTimestamp', () => {
    it('zero-pads fields to YYYYMMDD-HHMMSS', () => {
      const ts = formatClipboardTimestamp(new Date(2026, 0, 7, 4, 5, 9))
      // month is 0-indexed so January === '01'
      expect(ts).toBe('20260107-040509')
    })

    it('handles December and 23:59:59', () => {
      const ts = formatClipboardTimestamp(new Date(2026, 11, 31, 23, 59, 59))
      expect(ts).toBe('20261231-235959')
    })
  })

  describe('createClipboardTextFile', () => {
    it('produces .md + text/markdown for markdown-looking text', () => {
      const f = createClipboardTextFile('# heading\nbody')
      expect(f.name).toMatch(/^clipboard-\d{8}-\d{6}\.md$/)
      expect(f.type).toBe('text/markdown')
    })

    it('produces .txt + text/plain for plain text', () => {
      const f = createClipboardTextFile('just some text')
      expect(f.name).toMatch(/^clipboard-\d{8}-\d{6}\.txt$/)
      expect(f.type).toBe('text/plain')
    })

    it('writes the input text into the File', () => {
      // ASCII-only payload: byte count equals character count, so f.size
      // matches the input length. (jsdom does not implement File.text().)
      const payload = 'payload here'
      const f = createClipboardTextFile(payload)
      expect(f instanceof File).toBe(true)
      expect(f.size).toBe(payload.length)
    })

    it('detects markdown when frontmatter uses CRLF line endings', () => {
      const crlfFrontmatter = '---\r\ntitle: x\r\n---\r\nbody'
      const f = createClipboardTextFile(crlfFrontmatter)
      expect(f.name).toMatch(/\.md$/)
      expect(f.type).toBe('text/markdown')
    })
  })
})
