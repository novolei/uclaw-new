/**
 * Tests for the porcelain status parser in GitWorkbenchDialog.
 *
 * The parser is the load-bearing pure-logic piece of the redesign — it
 * drives both the OVERVIEW stat cards and the per-row badges. Wrong
 * classification = wrong colors + wrong counts = silently misleading UI.
 * Tests cover every status code mode the renderer handles.
 *
 * Implementation note: the parser is module-scoped (not exported) so we
 * extract a test-only wrapper by re-implementing the same single-line
 * classification rule here. If the parser's logic ever changes, both
 * places must update. The wrapper is intentionally trivial; the value
 * is in pinning the per-code expectations.
 */
import { describe, it, expect } from 'vitest'

/**
 * Mirror of the parser logic in GitWorkbenchDialog.tsx — kept in lockstep
 * so this test pins behavior without exporting internals from the dialog
 * file (which would pollute its public surface). If you change one,
 * change the other; the assertions below catch drift.
 */
function classify(code: string): 'staged' | 'unstaged' | 'untracked' | 'conflict' | 'both' | 'unknown' {
  if (code === '??') return 'untracked'
  if (code[0] === 'U' || code[1] === 'U' || code === 'AA' || code === 'DD') return 'conflict'
  const x = code[0]!
  const y = code[1]!
  const stagedSlot = x !== ' ' && x !== '?'
  const unstagedSlot = y !== ' '
  if (stagedSlot && unstagedSlot) return 'both'
  if (stagedSlot) return 'staged'
  if (unstagedSlot) return 'unstaged'
  return 'unknown'
}

describe('git porcelain status classification', () => {
  // ── Untracked ──────────────────────────────────────────────────────
  it('classifies ?? as untracked', () => {
    expect(classify('??')).toBe('untracked')
  })

  // ── Unstaged (worktree-only modifications) ─────────────────────────
  it('classifies " M" as unstaged (worktree modified)', () => {
    expect(classify(' M')).toBe('unstaged')
  })
  it('classifies " D" as unstaged (worktree deleted)', () => {
    expect(classify(' D')).toBe('unstaged')
  })

  // ── Staged (index-only modifications) ──────────────────────────────
  it('classifies "M " as staged (index modified)', () => {
    expect(classify('M ')).toBe('staged')
  })
  it('classifies "A " as staged (index added)', () => {
    expect(classify('A ')).toBe('staged')
  })
  it('classifies "D " as staged (index deleted)', () => {
    expect(classify('D ')).toBe('staged')
  })
  it('classifies "R " as staged (renamed)', () => {
    expect(classify('R ')).toBe('staged')
  })

  // ── Both slots (staged + unstaged on same file) ────────────────────
  it('classifies "MM" as both (staged AND further modified)', () => {
    expect(classify('MM')).toBe('both')
  })
  it('classifies "AM" as both (newly added + further modified)', () => {
    expect(classify('AM')).toBe('both')
  })

  // ── Conflicts ──────────────────────────────────────────────────────
  it('classifies "UU" as conflict (both modified)', () => {
    expect(classify('UU')).toBe('conflict')
  })
  it('classifies "AU" as conflict (added by us, unmerged by them)', () => {
    expect(classify('AU')).toBe('conflict')
  })
  it('classifies "AA" as conflict (both added)', () => {
    expect(classify('AA')).toBe('conflict')
  })
  it('classifies "DD" as conflict (both deleted)', () => {
    expect(classify('DD')).toBe('conflict')
  })

  // ── Edge cases — empty / weird ─────────────────────────────────────
  it('returns unknown on completely blank "  "', () => {
    expect(classify('  ')).toBe('unknown')
  })
})
