import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { themeModeAtom } from '@/atoms/theme'
import { ReadResultRenderer } from './read-result'

vi.mock('@pierre/diffs/react', () => ({
  File: (props: Record<string, unknown>) => (
    <div data-testid="pierre-file" data-props={JSON.stringify(props)}>
      pierre stub
    </div>
  ),
}))

function renderWithTheme(theme: 'light' | 'dark', el: React.ReactElement) {
  const store = createStore()
  store.set(themeModeAtom, theme)
  return render(<Provider store={store}>{el}</Provider>)
}

describe('ReadResultRenderer', () => {
  it('renders Pierre File with path, content, detected language, theme', () => {
    renderWithTheme('light',
      <ReadResultRenderer
        input={{ path: 'src/foo.ts' }}
        result='console.log("hello")'
        isError={false}
      />,
    )
    const f = screen.getByTestId('pierre-file')
    expect(f).toBeInTheDocument()
    const props = JSON.parse(f.getAttribute('data-props') ?? '{}')
    expect(JSON.stringify(props)).toContain('src/foo.ts')
    expect(JSON.stringify(props)).toContain('console.log')
    expect(JSON.stringify(props)).toContain('typescript')
    expect(JSON.stringify(props)).toContain('one-light')
  })

  it('uses dark theme when resolved theme is dark', () => {
    renderWithTheme('dark',
      <ReadResultRenderer
        input={{ path: 'a.md' }}
        result="# Hello"
        isError={false}
      />,
    )
    const f = screen.getByTestId('pierre-file')
    expect(JSON.stringify(JSON.parse(f.getAttribute('data-props') ?? '{}'))).toContain('one-dark-pro')
  })

  it('renders error state when isError', () => {
    renderWithTheme('light',
      <ReadResultRenderer
        input={{ path: 'missing.ts' }}
        result="ENOENT: no such file"
        isError={true}
      />,
    )
    expect(screen.getByText(/ENOENT/)).toBeInTheDocument()
    expect(screen.queryByTestId('pierre-file')).not.toBeInTheDocument()
  })

  it('strips numbered line prefixes if the result has them', () => {
    // Some SDK outputs prefix lines with "    1\tcontent". Pierre wants raw.
    renderWithTheme('light',
      <ReadResultRenderer
        input={{ path: 'a.txt' }}
        result="     1\tline-one\n     2\tline-two"
        isError={false}
      />,
    )
    const f = screen.getByTestId('pierre-file')
    const props = JSON.parse(f.getAttribute('data-props') ?? '{}')
    // Whatever the contents prop is called (contents / code / text), it should
    // have the prefixes stripped:
    expect(JSON.stringify(props)).toContain('line-one\\nline-two')
    expect(JSON.stringify(props)).not.toMatch(/\\s+\d+\\t/)
  })
})
