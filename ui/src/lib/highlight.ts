/**
 * highlight — Shiki 语法高亮工具
 *
 * 本地化 @proma/core 的 highlight 模块。
 * 使用 Shiki 提供代码语法高亮，支持懒加载。
 */

import { type Highlighter, createHighlighter, type BundledLanguage, type BundledTheme } from 'shiki'

/** 支持的常用语言列表（按需加载） */
const COMMON_LANGUAGES: BundledLanguage[] = [
  'javascript',
  'typescript',
  'jsx',
  'tsx',
  'python',
  'rust',
  'go',
  'java',
  'c',
  'cpp',
  'csharp',
  'ruby',
  'php',
  'swift',
  'kotlin',
  'html',
  'css',
  'json',
  'yaml',
  'toml',
  'markdown',
  'bash',
  'shell',
  'sql',
  'xml',
  'dockerfile',
  'diff',
]

/** 高亮主题 — 选择高对比度且色彩鲜明的主题，避免标识符颜色过暗 */
const LIGHT_THEME: BundledTheme = 'github-light'
const DARK_THEME: BundledTheme = 'one-dark-pro'

/** 全局 highlighter 单例（懒初始化） */
let highlighterPromise: Promise<Highlighter> | null = null

/**
 * 获取或初始化 Shiki highlighter 单例
 */
export function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [LIGHT_THEME, DARK_THEME],
      langs: COMMON_LANGUAGES,
    })
  }
  return highlighterPromise
}

/**
 * 将代码高亮为 HTML
 */
export async function highlightCode(
  code: string,
  language: string,
  theme?: 'light' | 'dark',
): Promise<string> {
  try {
    const highlighter = await getHighlighter()

    // 检查语言是否已加载
    const loadedLangs = highlighter.getLoadedLanguages()
    const lang = language.toLowerCase() as BundledLanguage
    if (!loadedLangs.includes(lang)) {
      try {
        await highlighter.loadLanguage(lang)
      } catch {
        // 语言不支持时使用纯文本
        return escapeHtml(code)
      }
    }

    const shikiTheme = theme === 'light' ? LIGHT_THEME : DARK_THEME

    return highlighter.codeToHtml(code, {
      lang,
      theme: shikiTheme,
    })
  } catch (error) {
    console.warn('[highlight] 高亮失败:', error)
    return `<pre><code>${escapeHtml(code)}</code></pre>`
  }
}

/**
 * 同步高亮（如果 highlighter 已初始化）
 * 返回 null 表示需要异步加载
 */
export function highlightCodeSync(
  code: string,
  language: string,
  theme?: 'light' | 'dark',
): string | null {
  if (!highlighterPromise) return null

  // 尝试同步获取
  let highlighter: Highlighter | null = null
  highlighterPromise.then((h) => { highlighter = h }).catch(() => {})

  if (!highlighter) return null

  try {
    const lang = language.toLowerCase() as BundledLanguage
    const loadedLangs = (highlighter as Highlighter).getLoadedLanguages()
    if (!loadedLangs.includes(lang)) return null

    const shikiTheme = theme === 'light' ? LIGHT_THEME : DARK_THEME
    return (highlighter as Highlighter).codeToHtml(code, { lang, theme: shikiTheme })
  } catch {
    return null
  }
}

/**
 * HTML 转义
 */
export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}

/**
 * 预热 highlighter（可在 App 启动时调用）
 */
export function preloadHighlighter(): void {
  getHighlighter().catch((err) => {
    console.warn('[highlight] 预加载失败:', err)
  })
}
