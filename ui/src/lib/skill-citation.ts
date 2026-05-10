/**
 * Parse Learned-Skill citations out of an assistant message.
 *
 * Backend contract (set by `format_recall_for_prompt` in
 * `src-tauri/src/memory_graph/recall.rs`): when the LLM applies a
 * learned skill it must prefix its response with one or more lines
 * shaped like:
 *
 *   > 应用技能：<技能名> — <一句话理由>
 *
 * We grep those out so:
 *   1. The chip UI can render them as standalone affordances below
 *      the message body (instead of as raw quote blocks)
 *   2. The cleaned message body keeps the rest of the response intact
 *      for normal markdown rendering
 *   3. Downstream code can call `record_skill_cited` to bump the
 *      cited_count metric (separate from recalled_count)
 *
 * Conservative grammar:
 *   - Match anywhere in the content (LLMs sometimes put the citation
 *     after a brief greeting), not just at the start
 *   - Title is everything up to the first separator (— / – / -)
 *   - Reason is everything after the separator on the same line
 *   - Multiple citations on consecutive lines are all picked up
 */

export interface SkillCitation {
  /** Skill title as written by the LLM (may differ from canonical title by spacing/punctuation). */
  title: string
  /** One-line justification the LLM provided. */
  reason: string
  /** Original matched line — kept for debugging. */
  raw: string
}

export interface ParsedAssistantContent {
  /** Message body with citation lines stripped. Safe to feed to MessageResponse. */
  cleanedContent: string
  /** Citations found, in order of appearance. Empty if none. */
  citations: SkillCitation[]
}

// Per-line: optional `>` quote marker, the literal "应用技能", flexible
// punctuation (full-width or half-width colon), title, separator (— / – / -),
// reason. Multiline flag so it scans the whole body.
const CITATION_RE = /^[ \t]*>?[ \t]*应用技能[：:]\s*([^——–\n]+?)\s*[——–\-]+\s*(.+?)\s*$/gm

export function parseSkillCitations(content: string): ParsedAssistantContent {
  if (!content) {
    return { cleanedContent: content, citations: [] }
  }
  const citations: SkillCitation[] = []
  // Reset regex state — gm flag leaves lastIndex stateful between calls.
  CITATION_RE.lastIndex = 0
  const cleanedContent = content
    .replace(CITATION_RE, (raw, title: string, reason: string) => {
      citations.push({
        title: title.trim(),
        reason: reason.trim(),
        raw: raw.trim(),
      })
      return ''
    })
    // After removal we may have stranded blank lines at the start or
    // multiple consecutive blank lines mid-body — collapse them so the
    // markdown render doesn't show big gaps where citations used to be.
    .replace(/^\s*\n+/, '')
    .replace(/\n{3,}/g, '\n\n')

  return { cleanedContent, citations }
}
