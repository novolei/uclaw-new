/**
 * punctuation — 段定稿文本的标点规整 + 增量拼接（纯函数）。
 *
 * SenseVoice 解码器只拼 token、不保证标点；这里补句末标点 + 规整空白，
 * 让转写文本「自动带标点」（对齐 Proma 的 enable_punc 体验）。
 */
import type { Language } from '@/atoms/stt-atoms'

// 句末标点（中英都算）。已以这些结尾就不再补。
const TERMINAL_PUNCT = /[。．.！!？?；;…]$/
// 判断一段文本是否 CJK 主导（用于 auto 语言选全角还是半角句号）。
const CJK = /[一-鿿぀-ヿ가-힯]/g
// ASCII 词字符（用于 smartJoin 判断是否补空格）。
const ASCII_WORD = /[A-Za-z0-9]/

function isCjkDominant(text: string): boolean {
  const cjk = (text.match(CJK) ?? []).length
  return cjk > 0 && cjk >= text.replace(/\s/g, '').length / 2
}

/**
 * 规整一段转写文本：trim、折叠内部连续空白、按语言补句末标点。
 * 已有句末标点则不动；空白输入返回空串。
 */
export function regularizePunctuation(text: string, language: Language): string {
  const cleaned = text.trim().replace(/\s+/g, ' ')
  if (cleaned === '') return ''
  if (TERMINAL_PUNCT.test(cleaned)) return cleaned
  const useCjk =
    language === 'zh' ||
    language === 'yue' ||
    language === 'ja' ||
    language === 'ko' ||
    (language === 'auto' && isCjkDominant(cleaned))
  return cleaned + (useCjk ? '。' : '.')
}

/**
 * 把新段文本拼到已有文本后。两侧都是 ASCII 词字符时补一个空格；
 * 左侧为 ASCII 标点且右侧为 ASCII 词时补空格；否则直接拼接。
 * 任一侧为空返回另一侧。
 */
export function smartJoin(left: string, right: string): string {
  if (left === '') return right
  if (right === '') return left
  const lastL = left[left.length - 1]!
  const firstR = right[0]!
  // 两侧都是 ASCII 词字符：补空格
  if (ASCII_WORD.test(lastL) && ASCII_WORD.test(firstR)) {
    return left + ' ' + right
  }
  // 左侧是 ASCII 标点，右侧是 ASCII 词字符：补空格（易读）
  if (/[.!?;:]/.test(lastL) && ASCII_WORD.test(firstR)) {
    return left + ' ' + right
  }
  // 其他情况直接拼接（CJK 场景、标点后接 CJK 等）
  return left + right
}
