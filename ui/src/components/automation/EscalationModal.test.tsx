import { describe, test, expect, vi } from 'vitest'
import * as React from 'react'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { EscalationModal } from './EscalationModal'
import type { EscalationRow } from '@/lib/tauri-bridge'

function makeEscalation(overrides: Partial<EscalationRow> = {}): EscalationRow {
  return {
    id: 'e1',
    specId: 'spec-12345678',
    activityId: 'a1',
    question: 'Which branch to release?',
    choicesJson: JSON.stringify([
      { id: 'main', label: 'main', description: 'production' },
      { id: 'staging', label: 'staging' },
    ]),
    status: 'waiting',
    userChoice: null,
    userNote: null,
    createdAt: 1700000000,
    respondedAt: null,
    ...overrides,
  }
}

describe('EscalationModal', () => {
  test('renders question and choices', () => {
    const { getByText } = renderWithProviders(
      <EscalationModal escalation={makeEscalation()} onResolve={() => {}} />
    )
    expect(getByText('Which branch to release?')).toBeInTheDocument()
    expect(getByText('main')).toBeInTheDocument()
    expect(getByText('staging')).toBeInTheDocument()
    expect(getByText(/production/)).toBeInTheDocument()
  })

  test('clicking a choice calls onResolve with id', async () => {
    const onResolve = vi.fn()
    const { getByText } = renderWithProviders(
      <EscalationModal escalation={makeEscalation()} onResolve={onResolve} />
    )
    fireEvent.click(getByText('main'))
    expect(onResolve).toHaveBeenCalledWith('main')
  })

  test('handles malformed choicesJson gracefully', () => {
    const { getByText } = renderWithProviders(
      <EscalationModal escalation={makeEscalation({ choicesJson: 'not json' })} onResolve={() => {}} />
    )
    expect(getByText(/选项缺失/)).toBeInTheDocument()
  })
})
