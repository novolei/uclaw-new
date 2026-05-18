import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import * as React from 'react'
import { BashResultRenderer } from './bash-result'

describe('BashResultRenderer', () => {
  it('renders command echo with $ prefix + stdout body', () => {
    render(
      <BashResultRenderer
        input={{ command: 'ls -la' }}
        result="total 8\ndrwxr-xr-x   2 root root 4096 May 18 10:00 ."
        isError={false}
      />,
    )
    expect(screen.getByText('$ ls -la')).toBeInTheDocument()
    expect(screen.getByText(/total 8/)).toBeInTheDocument()
  })

  it('highlights lines matching error patterns in red', () => {
    const { container } = render(
      <BashResultRenderer
        input={{ command: 'cargo build' }}
        result="compiling foo v0.1.0\nerror: cannot find module\nDone."
        isError={false}
      />,
    )
    const errorLine = container.querySelector('.text-red-400')
    expect(errorLine).toBeInTheDocument()
    expect(errorLine?.textContent).toContain('error: cannot find module')
  })

  it('marks all lines red when isError is true', () => {
    const { container } = render(
      <BashResultRenderer
        input={{ command: 'false' }}
        result="line 1\nline 2"
        isError={true}
      />,
    )
    const redLines = container.querySelectorAll('.text-red-400')
    expect(redLines.length).toBeGreaterThanOrEqual(2)
  })

  it('handles empty command gracefully', () => {
    render(<BashResultRenderer input={{}} result="output" isError={false} />)
    expect(screen.getByText('$')).toBeInTheDocument()
    expect(screen.getByText('output')).toBeInTheDocument()
  })
})
