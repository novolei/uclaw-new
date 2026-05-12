import { describe, it, expect } from 'vitest'
import { parseBranchList, uncommittedFromStatus } from './api'

describe('parseBranchList', () => {
  it('returns one entry per non-empty line with current detected by *', () => {
    const raw = '  main         abcdef1 init\n* feat/foo     1234567 wip\n  bug/x        ffeeddc fix'
    const result = parseBranchList(raw)
    expect(result).toEqual([
      { name: 'main', isCurrent: false },
      { name: 'feat/foo', isCurrent: true },
      { name: 'bug/x', isCurrent: false },
    ])
  })

  it('treats worktree-locked branches (+) as non-current', () => {
    const raw = '+ shared/x      abcdef1 used elsewhere\n* main         1234567 init'
    const result = parseBranchList(raw)
    expect(result).toEqual([
      { name: 'shared/x', isCurrent: false },
      { name: 'main', isCurrent: true },
    ])
  })

  it('skips the (HEAD detached at sha) pseudo-entry', () => {
    const raw = '* (HEAD detached at abc123)\n  main         abcdef1 init'
    const result = parseBranchList(raw)
    expect(result).toEqual([{ name: 'main', isCurrent: false }])
  })

  it('returns empty array for empty input', () => {
    expect(parseBranchList('')).toEqual([])
    expect(parseBranchList('   \n   ')).toEqual([])
  })
})

describe('uncommittedFromStatus', () => {
  it('returns 0 for null (clean tree)', () => {
    expect(uncommittedFromStatus(null)).toBe(0)
  })

  it('returns 0 when only the branch header is present', () => {
    expect(uncommittedFromStatus('## main')).toBe(0)
  })

  it('counts non-empty file lines after the branch header', () => {
    const raw = '## main\n M src/foo.ts\n?? src/bar.tsx\n A docs/note.md'
    expect(uncommittedFromStatus(raw)).toBe(3)
  })
})
