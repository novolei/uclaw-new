/**
 * 聊天外观状态原子
 *
 * 管理聊天内容的视觉调整：字体大小、衬线/无衬线、行距。
 * 通过 localStorage 持久化（无需后端）。
 * 应用方式：在 <html> 上设置 data-chat-* 属性，由 globals.css 中
 * 对应的选择器读取并设置 CSS 变量 / 类。
 */

import { atom } from 'jotai'

export type ChatFontSize = 'sm' | 'md' | 'lg'

const FONT_SIZE_KEY = 'uclaw-chat-font-size'
const SERIF_KEY = 'uclaw-chat-serif'

function getCachedFontSize(): ChatFontSize {
  try {
    const v = localStorage.getItem(FONT_SIZE_KEY)
    if (v === 'sm' || v === 'md' || v === 'lg') return v
  } catch {
    /* ignore */
  }
  return 'md'
}

function getCachedSerif(): boolean {
  try {
    return localStorage.getItem(SERIF_KEY) === 'true'
  } catch {
    return false
  }
}

function cacheFontSize(v: ChatFontSize): void {
  try {
    localStorage.setItem(FONT_SIZE_KEY, v)
  } catch {
    /* ignore */
  }
}

function cacheSerif(v: boolean): void {
  try {
    localStorage.setItem(SERIF_KEY, v ? 'true' : 'false')
  } catch {
    /* ignore */
  }
}

/** 字体大小（小 / 中 / 大） */
export const chatFontSizeAtom = atom<ChatFontSize>(getCachedFontSize())

/** 是否使用衬线字体 */
export const chatSerifAtom = atom<boolean>(getCachedSerif())

/** 应用聊天外观到 DOM（在 <html> 上设置 data-chat-* 属性） */
export function applyChatAppearanceToDOM(fontSize: ChatFontSize, serif: boolean): void {
  const html = document.documentElement
  if (html.getAttribute('data-chat-font-size') !== fontSize) {
    html.setAttribute('data-chat-font-size', fontSize)
  }
  const serifValue = serif ? 'true' : 'false'
  if (html.getAttribute('data-chat-serif') !== serifValue) {
    html.setAttribute('data-chat-serif', serifValue)
  }
}

/** 更新字体大小并持久化 */
export function updateChatFontSize(v: ChatFontSize): void {
  cacheFontSize(v)
}

/** 更新衬线设置并持久化 */
export function updateChatSerif(v: boolean): void {
  cacheSerif(v)
}
