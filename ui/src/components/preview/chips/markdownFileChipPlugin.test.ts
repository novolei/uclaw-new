import { describe, it, expect } from 'vitest'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import { markdownFileChipPlugin } from './markdownFileChipPlugin'

function findChipNodes(tree: any): any[] {
  const out: any[] = []
  function walk(node: any) {
    if (node?.data?.hName === 'file-path-chip') out.push(node)
    if (Array.isArray(node?.children)) node.children.forEach(walk)
  }
  walk(tree)
  return out
}

function run(md: string) {
  const tree = unified().use(remarkParse).use(remarkGfm).use(markdownFileChipPlugin).parse(md)
  return unified().use(remarkParse).use(remarkGfm).use(markdownFileChipPlugin).runSync(tree)
}

describe('markdownFileChipPlugin', () => {
  it('converts a markdown link to a chip', () => {
    const tree = run('See [the entry](src/main.rs) for details.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties).toMatchObject({
      rawPath: 'src/main.rs',
      label: 'the entry',
    })
  })

  it('converts inline-code single filename to a chip', () => {
    const tree = run('Check `style.css` for the rules.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties).toMatchObject({ rawPath: 'style.css', label: 'style.css' })
  })

  it('does NOT convert inline code that is not a filename', () => {
    const tree = run('Call `arr.map((x) => x + 1)` here.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(0)
  })

  it('converts slash-bearing path tokens in text', () => {
    const tree = run('Open src/main.rs to see the entry.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties.rawPath).toBe('src/main.rs')
  })

  it('strips :line:col from path tokens', () => {
    const tree = run('Bug at ui/src/atoms.ts:42:15 in the reducer.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties).toMatchObject({
      rawPath: 'ui/src/atoms.ts',
      line: 42,
      col: 15,
    })
  })

  it('does NOT match http/https URLs', () => {
    const tree = run('See https://example.com/foo.ts for context.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(0)
  })

  it('does NOT descend into fenced code blocks', () => {
    const tree = run('Outer src/a.ts here.\n\n```\ninner src/b.ts inside\n```\n')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(1)
    expect(chips[0].data.hProperties.rawPath).toBe('src/a.ts')
  })

  it('rejects extensions not in ALL_PREVIEWABLE_EXTS', () => {
    const tree = run('Run foo.exe then bar.map there.')
    const chips = findChipNodes(tree)
    expect(chips).toHaveLength(0)
  })
})
