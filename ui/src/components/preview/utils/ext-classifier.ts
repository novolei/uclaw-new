/**
 * ext-classifier — Route a filename to a renderer kind.
 *
 * `kind: 'image' | 'markdown' | 'code' | 'binary'` drives `usePreviewRouter`.
 * W4b will introduce `'pdf' | 'docx' | 'xlsx' | 'pptx' | 'legacyOffice'`.
 * W4c will introduce `'diff'`.
 */

export type RendererKind =
  | 'image'
  | 'markdown'
  | 'code'
  | 'pdf'
  | 'docx'
  | 'xlsx'
  | 'pptx'
  | 'legacyOffice'
  | 'diff'
  | 'binary'

export interface ClassificationResult {
  kind: RendererKind
  /** Lowercased file extension without the dot. Empty string for no-ext files. */
  ext: string
  /** For `kind === 'code'`, the language hint passed to shiki. */
  language?: string
}

/** Image file extensions handled by `<ImageRenderer>` via the asset protocol. */
export const IMAGE_EXTS: ReadonlySet<string> = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico',
])

/** Markdown file extensions handled by `<MarkdownRenderer>`. */
export const MD_EXTS: ReadonlySet<string> = new Set(['md', 'markdown'])

/**
 * Code-renderable extensions, mapping to the shiki language id.
 * Plain-text files (.txt, .log, .csv, etc) intentionally map to `'text'`
 * so the renderer shows them in a monospace pane without syntax highlight.
 */
export const CODE_EXTS: ReadonlyMap<string, string> = new Map([
  // typescript / javascript
  ['ts', 'ts'], ['tsx', 'tsx'], ['js', 'js'], ['jsx', 'jsx'],
  ['mjs', 'js'], ['cjs', 'js'],
  // systems / native
  ['rs', 'rs'], ['go', 'go'], ['c', 'c'], ['h', 'c'],
  ['cpp', 'cpp'], ['hpp', 'cpp'], ['cs', 'cs'],
  ['swift', 'swift'], ['kt', 'kotlin'], ['java', 'java'],
  // scripting
  ['py', 'py'], ['rb', 'rb'], ['php', 'php'],
  ['sh', 'bash'], ['bash', 'bash'], ['zsh', 'bash'], ['fish', 'fish'],
  // web
  ['html', 'html'], ['htm', 'html'],
  ['css', 'css'], ['scss', 'scss'], ['less', 'less'],
  // data / config
  ['json', 'json'], ['jsonc', 'jsonc'], ['json5', 'json5'],
  ['yaml', 'yaml'], ['yml', 'yaml'],
  ['toml', 'toml'], ['ini', 'ini'], ['env', 'dotenv'],
  ['xml', 'xml'],
  ['sql', 'sql'], ['graphql', 'graphql'], ['gql', 'graphql'],
  ['lock', 'yaml'],
  // plain text fallthrough
  ['txt', 'text'], ['log', 'text'], ['csv', 'text'],
  ['cfg', 'text'], ['conf', 'text'],
  ['gitignore', 'text'], ['dockerfile', 'docker'],
])

/**
 * Every extension that produces something other than `kind: 'binary'`
 * from `classifyExtension`. Used by the chip plugin to gate ambiguous
 * tokens like `arr.map` (where the suffix isn't a real ext).
 */
export const ALL_PREVIEWABLE_EXTS: ReadonlySet<string> = new Set<string>([
  ...IMAGE_EXTS,
  ...MD_EXTS,
  'pdf',
  'docx', 'xlsx', 'pptx',
  'doc', 'xls', 'ppt',
  ...Array.from(CODE_EXTS.keys()),
])

/** True if the extension would route to a non-binary renderer. */
export function isPreviewableExt(ext: string): boolean {
  if (!ext) return false
  return ALL_PREVIEWABLE_EXTS.has(ext.toLowerCase())
}

/**
 * Extract a normalized extension (lowercase, no dot) from a filename.
 * Returns the empty string for filenames with no extension (`Makefile`).
 * For dotfiles (`.gitignore` / `.env`), treats the name-after-dot as the extension.
 */
export function getExtension(filename: string): string {
  const dot = filename.lastIndexOf('.')
  if (dot === -1) return ''
  // For dotfiles like `.gitignore`, dot is at index 0 — extension is the
  // rest of the name.
  if (dot === 0) return filename.slice(1).toLowerCase()
  return filename.slice(dot + 1).toLowerCase()
}

/**
 * Classify a filename into a renderer kind + optional shiki language hint.
 * Priority: image → markdown → pdf/office → code → binary fallback.
 */
export function classifyExtension(filename: string): ClassificationResult {
  const ext = getExtension(filename)
  if (ext && IMAGE_EXTS.has(ext)) return { kind: 'image', ext }
  if (ext && MD_EXTS.has(ext)) return { kind: 'markdown', ext }
  if (ext === 'pdf') return { kind: 'pdf', ext }
  if (ext === 'docx') return { kind: 'docx', ext }
  if (ext === 'xlsx') return { kind: 'xlsx', ext }
  if (ext === 'pptx') return { kind: 'pptx', ext }
  if (ext === 'doc' || ext === 'xls' || ext === 'ppt') {
    return { kind: 'legacyOffice', ext }
  }
  if (ext === 'diff' || ext === 'patch') return { kind: 'diff', ext }
  if (ext && CODE_EXTS.has(ext)) {
    return { kind: 'code', ext, language: CODE_EXTS.get(ext) }
  }
  return { kind: 'binary', ext }
}
