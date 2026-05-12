/**
 * Fixture-driven regression suite for the assistant markdown renderer.
 *
 * Each `.md` file under __fixtures__/markdown-samples is loaded via Vite's
 * raw import and fed to <MessageResponse>. We assert structural invariants
 * that have been violated in recent regressions:
 *
 * - No KaTeX output (remark-math was removed; nothing should produce
 *   `<span class="katex">`).
 * - <strong> uses font-medium (not browser-default bold/700) and
 *   inherits color — guarded by checking for the className applied
 *   by MarkdownStrong.
 * - <em> is rendered non-italic — same rationale.
 * - No literal `\$` escape markers leaked through (regression for
 *   the math-removal era).
 */
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { MessageResponse } from './message'

// useChipCacheInvalidator calls listen() from @tauri-apps/api/event which
// is not available in jsdom. Stub it with a no-op that returns an unlisten fn.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}))
// useFileChipResolver calls invoke() from @tauri-apps/api/core.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => []),
}))

// Vite glob — bundles every .md fixture at build time as raw text.
const fixtures = import.meta.glob<string>(
  './__fixtures__/markdown-samples/*.md',
  { eager: true, import: 'default', query: '?raw' },
)

describe('MessageResponse — markdown rendering regressions', () => {
  for (const [path, content] of Object.entries(fixtures)) {
    const name = path.split('/').pop()!.replace('.md', '')

    it(`${name}: no KaTeX output (math removed)`, () => {
      const { container } = renderWithProviders(<MessageResponse>{content}</MessageResponse>)
      expect(container.querySelectorAll('.katex')).toHaveLength(0)
      expect(container.querySelectorAll('[class*="katex"]')).toHaveLength(0)
    })

    it(`${name}: <strong> uses font-medium (no bold/700)`, () => {
      const { container } = renderWithProviders(<MessageResponse>{content}</MessageResponse>)
      const strongs = container.querySelectorAll('strong')
      strongs.forEach((el) => {
        // MarkdownStrong applies "font-medium text-inherit"
        expect(el.className).toContain('font-medium')
        expect(el.className).toContain('text-inherit')
      })
    })

    it(`${name}: <em> rendered non-italic`, () => {
      const { container } = renderWithProviders(<MessageResponse>{content}</MessageResponse>)
      const ems = container.querySelectorAll('em')
      ems.forEach((el) => {
        expect(el.className).toContain('not-italic')
        expect(el.className).toContain('font-medium')
      })
    })

    it(`${name}: no literal escape markers leaked`, () => {
      const { container } = renderWithProviders(<MessageResponse>{content}</MessageResponse>)
      // remark-math removal regression: no literal `\$` should appear
      // in the rendered DOM.
      expect(container.textContent ?? '').not.toContain('\\$')
    })
  }

  it('table fixture renders inside not-prose card wrapper', () => {
    const tableFixture = fixtures['./__fixtures__/markdown-samples/04-table-with-status-cells.md']
    expect(tableFixture).toBeDefined()
    const { container } = renderWithProviders(<MessageResponse>{tableFixture}</MessageResponse>)
    const tables = container.querySelectorAll('table')
    expect(tables.length).toBeGreaterThan(0)
    // MarkdownTable wraps the table in not-prose + bg-card.
    const wrapper = tables[0].closest('.not-prose')
    expect(wrapper).not.toBeNull()
  })

  it('blockquote fixture renders with dimmed text-foreground/75', () => {
    const bqFixture = fixtures['./__fixtures__/markdown-samples/05-blockquote-with-bold.md']
    expect(bqFixture).toBeDefined()
    const { container } = renderWithProviders(<MessageResponse>{bqFixture}</MessageResponse>)
    const bq = container.querySelector('blockquote')
    expect(bq).not.toBeNull()
    expect(bq!.className).toContain('text-foreground/75')
    // <strong> inside the blockquote inherits color via text-inherit.
    const strongInBq = bq!.querySelector('strong')
    expect(strongInBq).not.toBeNull()
    expect(strongInBq!.className).toContain('text-inherit')
  })

  it('snapshot: mixed-cjk-latin fixture', () => {
    const f = fixtures['./__fixtures__/markdown-samples/01-mixed-cjk-latin.md']
    const { container } = renderWithProviders(<MessageResponse>{f}</MessageResponse>)
    expect(container.innerHTML).toMatchSnapshot()
  })

  it('snapshot: nested-lists fixture', () => {
    const f = fixtures['./__fixtures__/markdown-samples/06-nested-lists.md']
    const { container } = renderWithProviders(<MessageResponse>{f}</MessageResponse>)
    expect(container.innerHTML).toMatchSnapshot()
  })
})
