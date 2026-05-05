/**
 * 主题状态原子
 *
 * 管理应用主题模式（浅色/深色/跟随系统/特殊风格）和特殊风格。
 * 从 Proma 迁移，IPC 使用 tauri-bridge 适配层。
 */

import { atom } from 'jotai'
import type { ThemeMode, ThemeStyle } from '@/lib/proma-types'
import * as bridge from '@/lib/tauri-bridge'

/** localStorage 缓存键 */
const THEME_CACHE_KEY = 'uclaw-theme-mode'
const THEME_STYLE_CACHE_KEY = 'uclaw-theme-style'

function getCachedThemeMode(): ThemeMode {
  try {
    const cached = localStorage.getItem(THEME_CACHE_KEY)
    if (cached === 'light' || cached === 'dark' || cached === 'system' || cached === 'special') {
      return cached
    }
  } catch {
    // localStorage 不可用时忽略
  }
  return 'dark'
}

function getCachedThemeStyle(): ThemeStyle {
  try {
    const cached = localStorage.getItem(THEME_STYLE_CACHE_KEY)
    if (cached === 'default' || cached === 'ocean-light' || cached === 'ocean-dark' || cached === 'forest-light' || cached === 'forest-dark' || cached === 'slate-light' || cached === 'slate-dark') {
      return cached
    }
  } catch {
    // localStorage 不可用时忽略
  }
  return 'default'
}

function cacheThemeMode(mode: ThemeMode): void {
  try {
    localStorage.setItem(THEME_CACHE_KEY, mode)
  } catch {
    // localStorage 不可用时忽略
  }
}

function cacheThemeStyle(style: ThemeStyle): void {
  try {
    localStorage.setItem(THEME_STYLE_CACHE_KEY, style)
  } catch {
    // localStorage 不可用时忽略
  }
}

/** 用户选择的主题模式 */
export const themeModeAtom = atom<ThemeMode>(getCachedThemeMode())

/** 用户选择的特殊风格 */
export const themeStyleAtom = atom<ThemeStyle>(getCachedThemeStyle())

/** 系统当前是否为深色模式 */
export const systemIsDarkAtom = atom<boolean>(true)

/** 派生：最终解析的主题（light | dark） */
export const resolvedThemeAtom = atom<'light' | 'dark'>((get) => {
  const mode = get(themeModeAtom)
  if (mode === 'system') {
    return get(systemIsDarkAtom) ? 'dark' : 'light'
  }
  if (mode === 'special') {
    const style = get(themeStyleAtom)
    return style.endsWith('-light') ? 'light' : 'dark'
  }
  return mode
})

const ALL_THEME_STYLE_CLASSES = [
  'theme-ocean-light',
  'theme-ocean-dark',
  'theme-forest-light',
  'theme-forest-dark',
  'theme-slate-light',
  'theme-slate-dark',
] as const

/**
 * 应用主题到 DOM
 */
export function applyThemeToDOM(themeMode: ThemeMode, themeStyle: ThemeStyle = 'default', systemIsDark: boolean = true): void {
  const html = document.documentElement
  let targetStyleClass: string | null = null
  let targetIsDark: boolean

  if (themeMode === 'special' && themeStyle !== 'default') {
    targetStyleClass = `theme-${themeStyle}`
    targetIsDark = themeStyle.endsWith('-dark')
  } else if (themeMode === 'system') {
    targetIsDark = systemIsDark
  } else {
    targetIsDark = themeMode === 'dark'
  }

  const currentIsDark = html.classList.contains('dark')
  const currentStyleClass = ALL_THEME_STYLE_CLASSES.find((c) => html.classList.contains(c)) ?? null

  if (currentIsDark === targetIsDark && currentStyleClass === targetStyleClass) {
    return
  }

  if (currentStyleClass !== targetStyleClass) {
    if (currentStyleClass) html.classList.remove(currentStyleClass)
    if (targetStyleClass) html.classList.add(targetStyleClass)
  }
  if (currentIsDark !== targetIsDark) {
    html.classList.toggle('dark', targetIsDark)
  }
}

/**
 * 初始化主题系统（使用 Tauri bridge）
 */
export async function initializeTheme(
  setThemeMode: (mode: ThemeMode) => void,
  setSystemIsDark: (isDark: boolean) => void,
  setThemeStyle?: (style: ThemeStyle) => void,
): Promise<() => void> {
  // 从 Tauri 后端加载设置
  try {
    const settings = await bridge.getSettings()
    const themeMode = (settings.theme === 'light' || settings.theme === 'dark' || settings.theme === 'system' || settings.theme === 'special')
      ? settings.theme as ThemeMode
      : 'dark'
    setThemeMode(themeMode)
    cacheThemeMode(themeMode)
  } catch {
    // Tauri API 不可用时使用缓存值
    console.warn('[Theme] 无法从后端加载主题设置，使用缓存值')
  }

  // 系统主题检测（Tauri 环境中通过 CSS media query 或后端获取）
  const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  setSystemIsDark(isDark)

  // 监听系统主题变化（使用 Web API）
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
  const handleChange = (e: MediaQueryListEvent): void => {
    setSystemIsDark(e.matches)
  }
  mediaQuery.addEventListener('change', handleChange)

  return () => {
    mediaQuery.removeEventListener('change', handleChange)
  }
}

/**
 * 更新主题模式并持久化（使用 Tauri bridge）
 */
export async function updateThemeMode(mode: ThemeMode): Promise<void> {
  cacheThemeMode(mode)
  await bridge.patchSettings({ theme: mode })
}

/**
 * 更新特殊风格并持久化
 */
export async function updateThemeStyle(style: ThemeStyle): Promise<void> {
  cacheThemeStyle(style)
}

// Re-export types for convenience
export type { ThemeMode, ThemeStyle }
