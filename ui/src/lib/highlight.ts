/**
 * highlight — Shiki 语法高亮工具
 *
 * 本地化 @proma/core 的 highlight 模块。
 * 使用 Shiki 提供代码语法高亮，支持懒加载。
 */

import { type Highlighter, createHighlighter, type BundledLanguage, type BundledTheme } from 'shiki'

/** 高亮主题 — 预加载两套基础主题，其余主题按需加载 */
const LIGHT_THEME: BundledTheme = 'github-light'
const DARK_THEME: BundledTheme = 'one-dark-pro'

/**
 * 根据当前应用主题（document.documentElement 的 class）选择 Shiki 主题
 *
 * 映射关系：
 * - 默认 light → github-light
 * - 默认 dark → one-dark-pro
 * - warm-paper → vitesse-light（暖色调）
 * - ocean-light / forest-light / slate-light → solarized-light
 * - ocean-dark / forest-dark → vitesse-dark
 * - slate-dark / qingye → vitesse-dark
 * - black → min-dark
 * - the-finals → dracula
 */
export function getShikiThemeForCurrentApp(): BundledTheme {
  if (typeof document === 'undefined') return DARK_THEME
  const html = document.documentElement
  const isDark = html.classList.contains('dark')

  if (html.classList.contains('theme-the-finals')) return 'dracula'
  if (html.classList.contains('theme-black')) return 'min-dark'
  if (html.classList.contains('theme-warm-paper')) return 'vitesse-light'
  if (html.classList.contains('theme-qingye')) return 'vitesse-dark'
  if (html.classList.contains('theme-ocean-dark')) return 'vitesse-dark'
  if (html.classList.contains('theme-forest-dark')) return 'vitesse-dark'
  if (html.classList.contains('theme-slate-dark')) return 'vitesse-dark'
  if (html.classList.contains('theme-ocean-light')) return 'solarized-light'
  if (html.classList.contains('theme-forest-light')) return 'solarized-light'
  if (html.classList.contains('theme-slate-light')) return 'solarized-light'

  return isDark ? DARK_THEME : LIGHT_THEME
}

/** 全局 highlighter 单例（懒初始化） */
let highlighterPromise: Promise<Highlighter> | null = null

/** 已加载主题和语言的缓存集合，避免重复 loadTheme/loadLanguage 调用 */
const loadedThemes = new Set<BundledTheme>([LIGHT_THEME, DARK_THEME])
const loadedLangs = new Set<BundledLanguage>(['plaintext' as BundledLanguage])

/**
 * 获取或初始化 Shiki highlighter 单例
 * 仅预加载 2 套基础主题 + plaintext，其余主题/语言按需加载
 */
export function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [LIGHT_THEME, DARK_THEME],
      langs: ['plaintext'],
    })
  }
  return highlighterPromise
}

async function ensureTheme(highlighter: Highlighter, theme: BundledTheme): Promise<void> {
  if (loadedThemes.has(theme)) return
  try {
    await highlighter.loadTheme(theme)
    loadedThemes.add(theme)
  } catch (err) {
    console.warn('[highlight] loadTheme failed:', theme, err)
    // theme will fall back to whichever is already loaded; don't crash
  }
}

async function ensureLanguage(highlighter: Highlighter, lang: BundledLanguage): Promise<void> {
  if (loadedLangs.has(lang)) return
  try {
    await highlighter.loadLanguage(lang)
    loadedLangs.add(lang)
  } catch {
    // language doesn't exist — caller will fall back to plaintext silently
  }
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
    const lang = language.toLowerCase() as BundledLanguage
    await ensureLanguage(highlighter, lang)

    const shikiTheme: BundledTheme = theme === 'light'
      ? LIGHT_THEME
      : theme === 'dark'
        ? DARK_THEME
        : getShikiThemeForCurrentApp()
    await ensureTheme(highlighter, shikiTheme)

    // If the language failed to load (still missing from getLoadedLanguages), fall back to plaintext
    const currentLoadedLangs = highlighter.getLoadedLanguages()
    const finalLang = currentLoadedLangs.includes(lang) ? lang : ('plaintext' as BundledLanguage)

    return highlighter.codeToHtml(code, { lang: finalLang, theme: shikiTheme })
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
    const currentLoadedLangs = (highlighter as Highlighter).getLoadedLanguages()
    if (!currentLoadedLangs.includes(lang)) return null

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
