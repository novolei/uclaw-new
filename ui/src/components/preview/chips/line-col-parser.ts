/**
 * Parse a chip-candidate path that may carry a `:line` or `:line:col` suffix.
 *
 * Used by the markdown plugin (after extension matching) and by FilePathChip
 * (to preserve the suffix for future jump-to-line wiring).
 */

export interface ParsedLineCol {
  /** Bare path, never empty if input was non-empty. */
  path: string
  /** 1-indexed line number when present. */
  line?: number
  /** 1-indexed column when present. */
  col?: number
}

const LINE_COL_RE = /^(.*?):(\d+)(?::(\d+))?$/

export function parseLineCol(input: string): ParsedLineCol {
  const m = LINE_COL_RE.exec(input)
  if (!m) return { path: input }
  const path = m[1]!
  const line = Number(m[2])
  const colRaw = m[3]
  if (!Number.isInteger(line) || line < 1) return { path: input }
  if (colRaw !== undefined) {
    const col = Number(colRaw)
    if (!Number.isInteger(col) || col < 1) return { path: input }
    return { path, line, col }
  }
  return { path, line }
}
