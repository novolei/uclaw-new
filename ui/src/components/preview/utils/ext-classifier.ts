/**
 * ext-classifier — Route a filename to a renderer kind.
 *
 * `kind: 'image' | 'markdown' | 'code' | 'binary'` drives `usePreviewRouter`.
 * W4b will introduce `'pdf' | 'docx' | 'xlsx' | 'pptx' | 'legacyOffice'`.
 * W4c will introduce `'diff'`.
 */

export type RendererKind = 'image' | 'markdown' | 'code' | 'binary'

export interface ClassificationResult {
  kind: RendererKind
  /** Lowercased file extension without the dot. Empty string for no-ext files. */
  ext: string
  /** For `kind === 'code'`, the language hint passed to shiki. */
  language?: string
}

export const IMAGE_EXTS: ReadonlySet<string> = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico',
])

export const MD_EXTS: ReadonlySet<string> = new Set(['md', 'markdown'])

/**
 * Code-rendererable extensions, mapping to the shiki language id.
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
  // diff / patch
  ['diff', 'diff'], ['patch', 'diff'],
  // plain text fallthrough
  ['txt', 'text'], ['log', 'text'], ['csv', 'text'],
  ['cfg', 'text'], ['conf', 'text'],
  ['gitignore', 'text'], ['dockerfile', 'docker'],
])

export function getExtension(filename: string): string {
  const dot = filename.lastIndexOf('.')
  if (dot === -1) return ''
  // For dotfiles like `.gitignore`, dot is at index 0 — extension is the
  // rest of the name.
  if (dot === 0) return filename.slice(1).toLowerCase()
  return filename.slice(dot + 1).toLowerCase()
}

export function classifyExtension(filename: string): ClassificationResult {
  const ext = getExtension(filename)
  if (ext && IMAGE_EXTS.has(ext)) return { kind: 'image', ext }
  if (ext && MD_EXTS.has(ext)) return { kind: 'markdown', ext }
  if (ext && CODE_EXTS.has(ext)) {
    return { kind: 'code', ext, language: CODE_EXTS.get(ext) }
  }
  return { kind: 'binary', ext }
}
