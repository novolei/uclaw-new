import { describe, it, expect } from 'vitest'
import { regularizePunctuation, smartJoin } from './punctuation'

describe('regularizePunctuation', () => {
  it('appends 。 to a CJK sentence without terminal punctuation', () => {
    expect(regularizePunctuation('今天天气不错', 'zh')).toBe('今天天气不错。')
  })

  it('appends . to an English sentence without terminal punctuation', () => {
    expect(regularizePunctuation('hello world', 'en')).toBe('hello world.')
  })

  it('leaves text that already ends with terminal punctuation untouched', () => {
    expect(regularizePunctuation('已经有句号了。', 'zh')).toBe('已经有句号了。')
    expect(regularizePunctuation('done already!', 'en')).toBe('done already!')
    expect(regularizePunctuation('问号呢？', 'zh')).toBe('问号呢？')
  })

  it('collapses repeated internal whitespace and trims', () => {
    expect(regularizePunctuation('  hello   world  ', 'en')).toBe('hello world.')
  })

  it('returns empty string for empty/whitespace input', () => {
    expect(regularizePunctuation('', 'zh')).toBe('')
    expect(regularizePunctuation('   ', 'en')).toBe('')
  })

  it('for auto language, picks 。 when text is CJK-dominant, . otherwise', () => {
    expect(regularizePunctuation('这是中文', 'auto')).toBe('这是中文。')
    expect(regularizePunctuation('this is english', 'auto')).toBe('this is english.')
  })

  it('appends ？ to a CJK question (sentence-final particle or question word)', () => {
    expect(regularizePunctuation('这样对吗', 'zh')).toBe('这样对吗？')
    expect(regularizePunctuation('你今天去哪里', 'zh')).toBe('你今天去哪里？')
    expect(regularizePunctuation('为什么不去', 'zh')).toBe('为什么不去？')
  })

  it('appends ? to an English question (leading question/aux word)', () => {
    expect(regularizePunctuation('what time is it', 'en')).toBe('what time is it?')
    expect(regularizePunctuation('are you sure', 'en')).toBe('are you sure?')
  })

  it('corrects a statement period to ？/? when the sentence is clearly a question', () => {
    expect(regularizePunctuation('你今天去哪里。', 'zh')).toBe('你今天去哪里？')
    expect(regularizePunctuation('how does this work.', 'en')).toBe('how does this work?')
  })

  it('does not turn a plain statement into a question', () => {
    expect(regularizePunctuation('我明天去北京', 'zh')).toBe('我明天去北京。')
    expect(regularizePunctuation('this works fine', 'en')).toBe('this works fine.')
  })
})

describe('smartJoin', () => {
  it('joins two ASCII-word fragments with a space', () => {
    expect(smartJoin('hello', 'world')).toBe('hello world')
  })

  it('does not add a space between CJK fragments', () => {
    expect(smartJoin('你好', '世界')).toBe('你好世界')
  })

  it('does not add a space when the left side ends with punctuation', () => {
    expect(smartJoin('第一句。', '第二句')).toBe('第一句。第二句')
    expect(smartJoin('first.', 'second')).toBe('first. second')
  })

  it('returns the non-empty side when the other is empty', () => {
    expect(smartJoin('', 'world')).toBe('world')
    expect(smartJoin('hello', '')).toBe('hello')
    expect(smartJoin('', '')).toBe('')
  })
})
