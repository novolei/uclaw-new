/**
 * clipboard-attachment — Wave 1 of the renderer quick-wins port.
 *
 * Pure helpers that convert pasted long text into a File ready for the
 * existing attachment pipeline. Mirrors Proma's AgentView logic:
 *  - LONG_TEXT_ATTACHMENT_THRESHOLD = 500
 *  - markdown-looking text → clipboard-YYYYMMDD-HHMMSS.md + text/markdown
 *  - otherwise → .txt + text/plain
 */

export const LONG_TEXT_ATTACHMENT_THRESHOLD = 500

const MARKDOWN_PATTERNS: readonly RegExp[] = [
  /^#{1,6}\s+\S/m,
  /```[\s\S]*?```/,
  /^\s*\|.+\|\s*\n\s*\|[\s:-]+\|/m,
  /^---\n[\s\S]*?\n---\n/,
  /^\s*> .+/m,
  /^\s*[-*+]\s+\S/m,
  /^\s*\d+\.\s+\S/m,
  /\[[^\]]+\]\([^)]+\)/,
]

export function looksLikeMarkdown(text: string): boolean {
  // Normalize CRLF → LF so the YAML-frontmatter pattern (anchored at `---\n`)
  // and other newline-sensitive patterns work on Windows-origin clipboard text.
  const normalized = text.includes('\r') ? text.replace(/\r\n/g, '\n') : text
  return MARKDOWN_PATTERNS.some((p) => p.test(normalized))
}

export function formatClipboardTimestamp(date: Date = new Date()): string {
  const pad = (n: number): string => String(n).padStart(2, '0')
  return (
    `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}` +
    `-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`
  )
}

export function createClipboardTextFile(text: string): File {
  const isMd = looksLikeMarkdown(text)
  const ext = isMd ? 'md' : 'txt'
  const mediaType = isMd ? 'text/markdown' : 'text/plain'
  const filename = `clipboard-${formatClipboardTimestamp()}.${ext}`
  return new File([text], filename, { type: mediaType })
}
