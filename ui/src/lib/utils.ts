import { type ClassValue, clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs))
}

/**
 * 安全解析各种格式的日期值。
 * 支持：RFC3339 字符串、ISO 字符串、Unix 时间戳（秒或毫秒）、数字字符串。
 */
export function safeParseDate(value: unknown): Date | null {
  if (!value) return null

  // 如果是数字或数字字符串，判断是秒还是毫秒
  if (typeof value === 'number' || (typeof value === 'string' && /^\d+$/.test(value))) {
    const num = typeof value === 'number' ? value : parseInt(value, 10)
    // Unix 时间戳（秒）通常小于 1e12，毫秒大于 1e12
    const ms = num < 1e12 ? num * 1000 : num
    const date = new Date(ms)
    return isNaN(date.getTime()) ? null : date
  }

  // 字符串日期（RFC3339、ISO 等）
  if (typeof value === 'string') {
    const date = new Date(value)
    return isNaN(date.getTime()) ? null : date
  }

  return null
}

export function formatDate(value: unknown, fallback = '—'): string {
  const date = safeParseDate(value)
  return date ? date.toLocaleDateString() : fallback
}

export function formatDateTime(value: unknown, fallback = '—'): string {
  const date = safeParseDate(value)
  return date ? date.toLocaleString() : fallback
}
