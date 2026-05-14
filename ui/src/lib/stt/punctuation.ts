/**
 * punctuation — 段定稿文本的标点规整 + 增量拼接（纯函数）。
 *
 * SenseVoice 解码器只拼 token、不保证标点；这里补句末标点 + 规整空白，
 * 让转写文本「自动带标点」（对齐 Proma 的 enable_punc 体验）。
 */
import type { Language } from '@/atoms/stt-atoms'

// 句末标点（中英都算）。已以这些结尾就不再补。
const TERMINAL_PUNCT = /[。．.！!？?；;…]$/
// 陈述句末标点——疑问句被识别出来时，这些会被换成 ？/?。
const STATEMENT_TERMINAL = /[。．.]$/
// 判断一段文本是否 CJK 主导（用于 auto 语言选全角还是半角句号）。
const CJK = /[一-鿿぀-ヿ가-힯]/g
// ASCII 词字符（用于 smartJoin 判断是否补空格）。
const ASCII_WORD = /[A-Za-z0-9]/

// 中文疑问信号：句末语气词，或句中疑问词。
const CJK_QUESTION_TAIL = /[吗呢吧麽么]$/
const CJK_QUESTION_WORD =
  /什么|为什么|为何|怎么|怎样|怎么样|哪里|哪儿|哪个|哪些|哪一|是不是|是否|有没有|能不能|可不可以|要不要|对不对|难道|多少|几个|几点|几时/
// 英文疑问信号：以疑问/助动词开头。
const EN_QUESTION_LEAD =
  /^(what|why|how|who|whom|whose|where|when|which|is|are|am|was|were|do|does|did|can|could|will|would|should|shall|may|might|have|has|had)\b/i

function isCjkDominant(text: string): boolean {
  const cjk = (text.match(CJK) ?? []).length
  return cjk > 0 && cjk >= text.replace(/\s/g, '').length / 2
}

/** 启发式判断一段文本是否疑问句。不求完美，只覆盖明显的中英文问句。 */
function isQuestion(text: string, useCjk: boolean): boolean {
  // 去掉可能已有的句末标点再判断。
  const core = text.replace(/[。．.！!？?；;…]+$/, '').trim()
  if (core === '') return false
  if (useCjk) {
    return CJK_QUESTION_TAIL.test(core) || CJK_QUESTION_WORD.test(core)
  }
  return EN_QUESTION_LEAD.test(core)
}

/**
 * 规整一段转写文本：trim、折叠内部连续空白、按语言补句末标点。
 * 识别疑问句 → 补 ？/?；SenseVoice 给问句误加的 。/. 也纠正成 ？/?。
 * 已有 ？/?/！/! 则保持不动；空白输入返回空串。
 */
export function regularizePunctuation(text: string, language: Language): string {
  const cleaned = text.trim().replace(/\s+/g, ' ')
  if (cleaned === '') return ''
  const useCjk =
    language === 'zh' ||
    language === 'yue' ||
    language === 'ja' ||
    language === 'ko' ||
    (language === 'auto' && isCjkDominant(cleaned))
  const question = isQuestion(cleaned, useCjk)

  if (TERMINAL_PUNCT.test(cleaned)) {
    // 已有句末标点。若是陈述句号但内容明显是问句，纠正成问号。
    if (question && STATEMENT_TERMINAL.test(cleaned)) {
      return cleaned.slice(0, -1) + (useCjk ? '？' : '?')
    }
    return cleaned
  }
  // 没有句末标点：问句补 ？/?，否则补 。/.
  if (question) return cleaned + (useCjk ? '？' : '?')
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
