import { describe, it, expect } from 'vitest'
import { buildRenderHunks, buildAllAddedLines } from './useDiffHunks'

describe('buildRenderHunks', () => {
  it('returns empty array for identical content', () => {
    const hunks = buildRenderHunks('a\nb\nc', 'a\nb\nc', 3)
    expect(hunks).toEqual([])
  })

  it('detects changes and includes add lines', () => {
    const hunks = buildRenderHunks('a\nb', 'a\nb\nc', 1)
    expect(hunks.length).toBeGreaterThan(0)
    const allAdds = hunks.flatMap((h) => h.lines.filter((l) => l.kind === 'add'))
    expect(allAdds.length).toBeGreaterThan(0)
  })

  it('emits del lines when content is removed', () => {
    const hunks = buildRenderHunks('a\nb\nc', 'a\nc', 3)
    expect(hunks).toHaveLength(1)
    const dels = hunks[0]!.lines.filter((l) => l.kind === 'del')
    expect(dels.map((l) => l.text)).toEqual(['b'])
  })

  it('handles mixed add+del+ctx', () => {
    const hunks = buildRenderHunks('a\nb\nc', 'a\nB\nc', 3)
    expect(hunks).toHaveLength(1)
    const kinds = hunks[0]!.lines.map((l) => l.kind).sort()
    expect(kinds).toContain('add')
    expect(kinds).toContain('del')
    expect(kinds).toContain('ctx')
  })
})

describe('buildAllAddedLines', () => {
  it('emits one add line per row for fresh file', () => {
    const lines = buildAllAddedLines('foo\nbar\nbaz')
    expect(lines.map((l) => l.text)).toEqual(['foo', 'bar', 'baz'])
    expect(lines.every((l) => l.kind === 'add')).toBe(true)
  })

  it('returns empty for empty input', () => {
    expect(buildAllAddedLines('')).toEqual([])
  })
})
