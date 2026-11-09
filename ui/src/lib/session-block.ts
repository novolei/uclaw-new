/**
 * parseSessionBlock — reads the post-compaction session block (context
 * continuity P2, backend `agent/session_block.rs`) back into display data:
 *
 *   <fold markdown>
 *
 *   <file_working_set>…</file_working_set>          (optional)
 *
 *   <current_plan path="…">body</current_plan>      (optional, or self-closing with omitted="…")
 *
 *   <recent_files>
 *   <file path="…">\ncontent\n</file>              (body NOT escaped)
 *   <file path="…" omitted="reason"/>
 *   </recent_files>                                 (optional)
 *
 * Path attributes escape `&` and `"`. Bodies are raw file text and may
 * contain tag-like text, so sections are located structurally (in order,
 * anchored at line starts) and bodies are split by backtracking against the
 * remaining structure. Never throws; a pre-P2 block without tags parses to
 * its full text as `summary` and empty lists.
 */

export interface ParsedSessionBlock {
  /** Fold markdown with every session-block tag stripped. */
  summary: string
  /** Paths listed under 已修改的文件 (labels and rewound notes removed). */
  modifiedFiles: string[]
  /** Modified files elided as `…另有 N 个文件` when the list was cut to budget. */
  modifiedOmittedCount: number
  /** Number of files listed under 读过的文件 (plus any `…另有 N 个文件`). */
  readCount: number
  /** Path of the current plan, when one is attached. */
  plan?: string
  /** Paths of files whose current contents were re-inserted (omitted ones excluded). */
  reloadedFiles: string[]
}

const WS_OPEN = '<file_working_set>'
const WS_CLOSE = '</file_working_set>'
const PLAN_OPEN = '<current_plan path="'
const PLAN_CLOSE = '</current_plan>'
const RF_OPEN = '<recent_files>\n'
const RF_CLOSE = '</recent_files>'

const ATTR = '([^"]*)'
const SELF_CLOSING_FILE = new RegExp(`^<file path="${ATTR}" omitted="${ATTR}"/>\\n`)
const OPEN_FILE = new RegExp(`^<file path="${ATTR}">\\n`)
const MODIFIED_LINE = /^- (.+)（(?:新建|已删除|修改 \d+ 次)）(?: \[来自已回退的轮次，磁盘上仍保留\])?$/
const MORE_LINE = /^…另有 (\d+) 个文件$/

function unescapeAttr(s: string): string {
  return s.replace(/&quot;/g, '"').replace(/&amp;/g, '&')
}

/** Index of `needle` at a line start (index 0 or right after `\n`), from `from`. */
function indexAtLineStart(text: string, needle: string, from = 0): number {
  if (from === 0 && text.startsWith(needle)) return 0
  const i = text.indexOf(`\n${needle}`, Math.max(0, from - 1))
  return i < 0 ? -1 : i + 1
}

/** Parse `<recent_files>` entries from `rest` (just after its opener) to the
 *  closing tag at the end of the block. Returns null when the structure does
 *  not hold. Bodies end at the earliest `\n</file>\n` after which the rest
 *  still parses (backtracking), so tag-like text inside a body is tolerated. */
function parseRecentEntries(rest: string, depth = 0): string[] | null {
  if (depth > 64) return null
  if (rest.trimEnd() === RF_CLOSE) return []
  const omitted = SELF_CLOSING_FILE.exec(rest)
  if (omitted) {
    return parseRecentEntries(rest.slice(omitted[0].length), depth + 1)
  }
  const open = OPEN_FILE.exec(rest)
  if (!open) return null
  const path = unescapeAttr(open[1] ?? '')
  const bodyStart = open[0].length
  const END = '\n</file>\n'
  // The body always ends with `\n` (backend push_body), so the closing tag
  // sits right after a newline — search from bodyStart - 1 for empty bodies.
  let at = rest.indexOf(END, bodyStart - 1)
  while (at >= 0) {
    const tail = parseRecentEntries(rest.slice(at + END.length), depth + 1)
    if (tail) return [path, ...tail]
    at = rest.indexOf(END, at + 1)
  }
  return null
}

function parseWorkingSet(inner: string): Pick<ParsedSessionBlock, 'modifiedFiles' | 'modifiedOmittedCount' | 'readCount'> {
  const modifiedFiles: string[] = []
  let modifiedOmittedCount = 0
  let readCount = 0
  let section: 'modified' | 'read' | null = null
  for (const line of inner.split('\n')) {
    if (line === '已修改的文件：') {
      section = 'modified'
      continue
    }
    if (line === '读过的文件：') {
      section = 'read'
      continue
    }
    const more = MORE_LINE.exec(line)
    if (more) {
      const n = Number(more[1])
      if (section === 'modified') modifiedOmittedCount += n
      else if (section === 'read') readCount += n
      continue
    }
    if (!line.startsWith('- ')) continue
    if (section === 'modified') {
      const m = MODIFIED_LINE.exec(line)
      modifiedFiles.push(m ? m[1]! : line.slice(2))
    } else if (section === 'read') {
      readCount += 1
    }
  }
  return { modifiedFiles, modifiedOmittedCount, readCount }
}

/** Try to read `<recent_files>` starting exactly at `text[pos]` through the end. */
function recentFilesAt(text: string, pos: number): string[] | null {
  if (!text.startsWith(RF_OPEN, pos)) return null
  return parseRecentEntries(text.slice(pos + RF_OPEN.length))
}

/** End-of-plan-section candidates: `</current_plan>` followed by the end of
 *  the block or a parseable `<recent_files>` section. */
function parsePlanTail(text: string, bodyFrom: number): { reloaded: string[] } | null {
  let at = text.indexOf(PLAN_CLOSE, bodyFrom)
  while (at >= 0) {
    const after = at + PLAN_CLOSE.length
    const rest = text.slice(after)
    if (rest.trim() === '') return { reloaded: [] }
    const lead = rest.length - rest.trimStart().length
    const rf = recentFilesAt(text, after + lead)
    if (rf) return { reloaded: rf }
    at = text.indexOf(PLAN_CLOSE, at + 1)
  }
  return null
}

function parseInner(markdown: string): ParsedSessionBlock {
  const text = markdown
  const empty: ParsedSessionBlock = {
    summary: text,
    modifiedFiles: [],
    modifiedOmittedCount: 0,
    readCount: 0,
    plan: undefined,
    reloadedFiles: [],
  }

  // Each section can only start where it is the first tag at a line start;
  // the fold markdown always precedes them.
  const starts = [
    indexAtLineStart(text, WS_OPEN),
    indexAtLineStart(text, PLAN_OPEN),
    indexAtLineStart(text, RF_OPEN.trimEnd()),
  ].filter((i) => i >= 0)
  if (starts.length === 0) return empty
  const tagStart = Math.min(...starts)

  const result: ParsedSessionBlock = { ...empty, summary: text.slice(0, tagStart).trimEnd() }
  let pos = tagStart

  // <file_working_set> — line-oriented, no arbitrary bodies.
  if (text.startsWith(WS_OPEN, pos)) {
    const close = text.indexOf(`\n${WS_CLOSE}`, pos)
    if (close < 0) return result
    Object.assign(result, parseWorkingSet(text.slice(pos + WS_OPEN.length, close)))
    pos = close + 1 + WS_CLOSE.length
    while (pos < text.length && /\s/.test(text[pos]!)) pos += 1
  }

  // <current_plan …>
  if (text.startsWith(PLAN_OPEN, pos)) {
    const attrEnd = text.indexOf('"', pos + PLAN_OPEN.length)
    if (attrEnd < 0) return result
    result.plan = unescapeAttr(text.slice(pos + PLAN_OPEN.length, attrEnd))
    const afterAttr = text.slice(attrEnd + 1)
    const selfClosing = /^ omitted="[^"]*"\/>/.exec(afterAttr)
    if (selfClosing) {
      pos = attrEnd + 1 + selfClosing[0].length
      while (pos < text.length && /\s/.test(text[pos]!)) pos += 1
    } else if (afterAttr.startsWith('>')) {
      const tail = parsePlanTail(text, attrEnd + 2)
      if (tail) result.reloadedFiles = tail.reloaded
      return result
    } else {
      return result
    }
  }

  // <recent_files>
  const rf = recentFilesAt(text, pos)
  if (rf) result.reloadedFiles = rf
  return result
}

export function parseSessionBlock(markdown: string): ParsedSessionBlock {
  try {
    return parseInner(markdown ?? '')
  } catch {
    return {
      summary: markdown ?? '',
      modifiedFiles: [],
      modifiedOmittedCount: 0,
      readCount: 0,
      plan: undefined,
      reloadedFiles: [],
    }
  }
}
