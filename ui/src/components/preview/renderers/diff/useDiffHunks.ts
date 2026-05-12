/**
 * useDiffHunks — Compute render-ready hunks from old/new content.
 *
 * Borrowed from if2Ai's WriteToolDiffCard.tsx:40-100 — algorithms
 * (buildRenderHunks, gapLineCount, buildAllAddedLines) are ported
 * verbatim. uClaw's DiffRenderer adapts the output to a 2-column
 * side-by-side layout.
 */

import * as React from 'react'
import { structuredPatch } from 'diff'

export interface DiffLine {
  kind: 'ctx' | 'add' | 'del'
  text: string
  oldNo?: number
  newNo?: number
}

// Hunk interface from diff package (not exported publicly)
interface Hunk {
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  lines: string[]
}

export interface RenderHunk {
  hunk: Hunk
  lines: DiffLine[]
}

export function buildRenderHunks(oldText: string, newText: string, context: number): RenderHunk[] {
  const patch = structuredPatch('a', 'b', oldText, newText, '', '', { context })
  return patch.hunks.map((hunk) => {
    const lines: DiffLine[] = []
    let oldNo = hunk.oldStart
    let newNo = hunk.newStart
    for (const raw of hunk.lines) {
      const sign = raw.charAt(0)
      const text = raw.slice(1)
      if (sign === '+') {
        lines.push({ kind: 'add', text, newNo })
        newNo += 1
      } else if (sign === '-') {
        lines.push({ kind: 'del', text, oldNo })
        oldNo += 1
      } else {
        lines.push({ kind: 'ctx', text, oldNo, newNo })
        oldNo += 1
        newNo += 1
      }
    }
    return { hunk, lines }
  })
}

export function gapLineCount(prev: Hunk, next: Hunk): number {
  const prevEnd = prev.oldStart + prev.oldLines
  return Math.max(0, next.oldStart - prevEnd)
}

export function buildAllAddedLines(newText: string): DiffLine[] {
  if (newText === '') return []
  return newText.split('\n').map((text, i) => ({ kind: 'add' as const, text, newNo: i + 1 }))
}

export interface UseDiffHunksArgs {
  oldContent: string
  newContent: string
  contextLines?: number
  showFull?: boolean
}

export interface UseDiffHunksResult {
  hunks: RenderHunk[]
  totals: { add: number; del: number }
  isFreshFile: boolean
  fullLines: DiffLine[] | null
}

export function useDiffHunks(args: UseDiffHunksArgs): UseDiffHunksResult {
  const { oldContent, newContent, contextLines = 3, showFull = false } = args
  const isFreshFile = oldContent === ''
  const context = showFull ? Number.MAX_SAFE_INTEGER : contextLines

  const hunks = React.useMemo(
    () => (isFreshFile ? [] : buildRenderHunks(oldContent, newContent, context)),
    [isFreshFile, oldContent, newContent, context],
  )
  const fullLines = React.useMemo(
    () => (isFreshFile ? buildAllAddedLines(newContent) : null),
    [isFreshFile, newContent],
  )
  const totals = React.useMemo(() => {
    if (isFreshFile) return { add: fullLines?.length ?? 0, del: 0 }
    return hunks.reduce(
      (acc, h) => {
        for (const l of h.lines) {
          if (l.kind === 'add') acc.add += 1
          else if (l.kind === 'del') acc.del += 1
        }
        return acc
      },
      { add: 0, del: 0 },
    )
  }, [isFreshFile, fullLines, hunks])

  return { hunks, totals, isFreshFile, fullLines }
}
