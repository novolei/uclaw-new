import { describe, it, expect } from 'vitest'
import { parseSessionBlock } from './session-block'

const FOLD = '## Earlier conversation (compacted)\n\n### 目标\n修 bug\n\n### 进度\n完成一半'

const FULL = [
  FOLD,
  '',
  '<file_working_set>\n已修改的文件：\n- src/a.rs（修改 3 次）\n  · 改了 foo\n- src/new（1）.ts（新建）\n- old.txt（已删除） [来自已回退的轮次，磁盘上仍保留]\n…另有 2 个文件\n读过的文件：\n- README.md（1–40）\n- docs/x.md\n…另有 4 个文件\n</file_working_set>',
  '',
  '<current_plan path=".uclaw/plans/p &amp; q.md">\n- [ ] step one\n</current_plan>',
  '',
  '<recent_files>\n<file path="src/a.rs">\nfn main() {}\n</file>\n<file path="src/b &quot;q&quot;.rs" omitted="二进制文件"/>\n<file path="src/c.rs">\nlet s = "</file>";\n</file>\n</recent_files>',
].join('\n')

describe('parseSessionBlock', () => {
  it('parses working set, plan and recent files out of a session block', () => {
    const p = parseSessionBlock(FULL)
    expect(p.summary).toBe(FOLD)
    expect(p.modifiedFiles).toEqual(['src/a.rs', 'src/new（1）.ts', 'old.txt'])
    expect(p.modifiedOmittedCount).toBe(2)
    expect(p.readCount).toBe(6)
    expect(p.plan).toBe('.uclaw/plans/p & q.md')
    expect(p.reloadedFiles).toEqual(['src/a.rs', 'src/c.rs'])
  })

  it('returns empty lists for an old block without tags', () => {
    const p = parseSessionBlock(FOLD)
    expect(p).toEqual({
      summary: FOLD,
      modifiedFiles: [],
      modifiedOmittedCount: 0,
      readCount: 0,
      plan: undefined,
      reloadedFiles: [],
    })
  })

  it('handles an omitted plan and a body that contains tag-like text', () => {
    const block = [
      FOLD,
      '',
      '<current_plan path="p.md" omitted="超出会话块预算"/>',
      '',
      '<recent_files>\n<file path="x.ts">\nconst t = `\n</file>\n<file path="fake">`\n</file>\n<file path="y.ts" omitted="超出会话块预算"/>\n</recent_files>',
    ].join('\n')
    const p = parseSessionBlock(block)
    expect(p.summary).toBe(FOLD)
    expect(p.plan).toBe('p.md')
    expect(p.modifiedFiles).toEqual([])
    expect(p.reloadedFiles[0]).toBe('x.ts')
    expect(p.reloadedFiles).toEqual(['x.ts'])
  })

  it('keeps a plan body that contains </current_plan> from leaking into the summary', () => {
    const block = `${FOLD}\n\n<current_plan path="p.md">\nsee </current_plan> tag\n</current_plan>`
    const p = parseSessionBlock(block)
    expect(p.plan).toBe('p.md')
    expect(p.summary).toBe(FOLD)
    expect(p.reloadedFiles).toEqual([])
  })

  it('never throws on malformed input', () => {
    for (const s of ['', '<file_working_set>', '<recent_files>\n<file path="a">\nunterminated', '<current_plan path="x']) {
      expect(() => parseSessionBlock(s)).not.toThrow()
    }
    expect(parseSessionBlock('<recent_files>\n<file path="a">\nunterminated').reloadedFiles).toEqual([])
  })
})
