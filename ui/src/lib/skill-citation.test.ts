import { describe, it, expect } from 'vitest'
import { parseSkillCitations } from './skill-citation'

describe('parseSkillCitations', () => {
  it('extracts a single citation at message start', () => {
    const input = '> 应用技能：基于计划的增量式游戏开发工作流 — 本次任务匹配多步计划场景\n\n现在开始第一步...'
    const r = parseSkillCitations(input)
    expect(r.citations).toHaveLength(1)
    expect(r.citations[0].title).toBe('基于计划的增量式游戏开发工作流')
    expect(r.citations[0].reason).toBe('本次任务匹配多步计划场景')
    expect(r.cleanedContent.startsWith('现在开始第一步')).toBe(true)
  })

  it('extracts multiple citations on consecutive lines', () => {
    const input = '> 应用技能：A — 理由A\n> 应用技能：B — 理由B\n\n开始干活'
    const r = parseSkillCitations(input)
    expect(r.citations).toHaveLength(2)
    expect(r.citations[0].title).toBe('A')
    expect(r.citations[1].title).toBe('B')
    expect(r.cleanedContent.startsWith('开始干活')).toBe(true)
  })

  it('handles half-width colon and ASCII dash', () => {
    const input = '> 应用技能: edit 工具技巧 - 用户要改文件\n\nOK 走起'
    const r = parseSkillCitations(input)
    expect(r.citations).toHaveLength(1)
    expect(r.citations[0].title).toBe('edit 工具技巧')
    expect(r.citations[0].reason).toBe('用户要改文件')
  })

  it('handles citation without leading quote marker', () => {
    // E1 instructs LLM to use `>` but be tolerant if it omits.
    const input = '应用技能：A — B\n正文'
    const r = parseSkillCitations(input)
    expect(r.citations).toHaveLength(1)
    expect(r.citations[0].title).toBe('A')
  })

  it('returns empty citations for unrelated content', () => {
    const input = '这是一段普通的回复，没有任何技能引用。'
    const r = parseSkillCitations(input)
    expect(r.citations).toHaveLength(0)
    expect(r.cleanedContent).toBe(input)
  })

  it('handles empty / null-ish input gracefully', () => {
    expect(parseSkillCitations('').citations).toHaveLength(0)
    expect(parseSkillCitations('').cleanedContent).toBe('')
  })

  it('does not mistake regular blockquotes for citations', () => {
    const input = '> 这只是一个普通引用\n\n继续正文'
    const r = parseSkillCitations(input)
    expect(r.citations).toHaveLength(0)
    // Regular quote should stay in body
    expect(r.cleanedContent.includes('普通引用')).toBe(true)
  })

  it('strips citations mid-message (not just leading)', () => {
    const input = '让我先想一下。\n\n> 应用技能：X — Y\n\n好的，开始。'
    const r = parseSkillCitations(input)
    expect(r.citations).toHaveLength(1)
    expect(r.cleanedContent.includes('应用技能')).toBe(false)
    expect(r.cleanedContent.includes('让我先想一下')).toBe(true)
    expect(r.cleanedContent.includes('好的，开始')).toBe(true)
  })
})
