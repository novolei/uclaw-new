import { describe, expect, it } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { PersonaBondTimeline } from './PersonaBondTimeline'

describe('PersonaBondTimeline', () => {
  it('states that relationship rewards do not change capability', () => {
    renderWithProviders(<PersonaBondTimeline />)
    expect(screen.getByText('关系时间线')).toBeInTheDocument()
    expect(screen.getByText(/不改变 Agent 能力/)).toBeInTheDocument()
    expect(screen.getByText('纪念物')).toBeInTheDocument()
    expect(screen.getByText('勋章')).toBeInTheDocument()
  })
})
