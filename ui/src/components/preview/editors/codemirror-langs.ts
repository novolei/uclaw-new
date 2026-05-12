/**
 * codemirror-langs — Lazy language pack loader for the TextEditor.
 *
 * Each loader is a function that returns a Promise resolving to a
 * `LanguageSupport` instance. Languages load on demand from the
 * `editors` Vite chunk (manualChunks set in vite.config.ts), keeping
 * cold-start small.
 *
 * Keyed on shiki language id (from ext-classifier's CODE_EXTS map)
 * so callers don't need to know CM6 package names.
 *
 * NOTE: TypeScript is handled by lang-javascript with { typescript: true }
 * (no separate @codemirror/lang-typescript package).
 */

import type { LanguageSupport } from '@codemirror/language'

type LangLoader = () => Promise<LanguageSupport>

const LOADERS: Record<string, LangLoader> = {
  ts: () =>
    import('@codemirror/lang-javascript').then((m) =>
      m.javascript({ typescript: true, jsx: false }),
    ),
  tsx: () =>
    import('@codemirror/lang-javascript').then((m) =>
      m.javascript({ typescript: true, jsx: true }),
    ),
  js: () => import('@codemirror/lang-javascript').then((m) => m.javascript({ jsx: false })),
  jsx: () => import('@codemirror/lang-javascript').then((m) => m.javascript({ jsx: true })),
  py: () => import('@codemirror/lang-python').then((m) => m.python()),
  rs: () => import('@codemirror/lang-rust').then((m) => m.rust()),
  go: () => import('@codemirror/lang-go').then((m) => m.go()),
  html: () => import('@codemirror/lang-html').then((m) => m.html()),
  css: () => import('@codemirror/lang-css').then((m) => m.css()),
  scss: () => import('@codemirror/lang-css').then((m) => m.css()),
  json: () => import('@codemirror/lang-json').then((m) => m.json()),
  jsonc: () => import('@codemirror/lang-json').then((m) => m.json()),
  markdown: () => import('@codemirror/lang-markdown').then((m) => m.markdown()),
}

/**
 * Resolve a CM6 LanguageSupport for the given shiki language id.
 * Returns null if no loader is registered (falls back to plain text).
 */
export async function loadLanguage(language: string): Promise<LanguageSupport | null> {
  const loader = LOADERS[language]
  if (!loader) return null
  try {
    return await loader()
  } catch (e) {
    console.warn(`[codemirror-langs] failed to load '${language}':`, e)
    return null
  }
}
