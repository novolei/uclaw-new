import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import * as personaApi from '@/lib/persona'
import type { PersonaRelationshipTimeline } from '@/lib/persona-types'
import { PersonaBondTimeline } from './PersonaBondTimeline'

vi.mock('@/lib/persona', () => ({
  getPersonaRelationshipTimeline: vi.fn(),
  updatePersonaKeepsakeStatus: vi.fn(),
}))

const timeline: PersonaRelationshipTimeline = {
  affinity: {
    score: 18,
    explanation: ['+2 accepted keepsakes'],
  },
  factors: {
    successfulMinutes: 120,
    acceptedKeepsakes: 2,
    positiveFeedback: 0,
    stableStyleFragments: 0,
    recoveredFailures: 0,
    inactivityDays: 0,
    rejectedCandidates: 0,
    unresolvedFailures: 0,
    correctionCount: 0,
  },
  keepsakes: [
    {
      id: 'ks_1',
      title: '第一次顺利合并',
      narrative: '我们一起把人格 MVP 合并进 main。',
      learnedText: '用户偏好先收敛 MVP，再拆后续增强。',
      evidence: ['pr:545'],
      status: 'proposed',
    },
  ],
  recentEvents: [],
}

describe('PersonaBondTimeline', () => {
  it('loads relationship timeline and accepts a proposed keepsake', async () => {
    vi.mocked(personaApi.getPersonaRelationshipTimeline).mockResolvedValueOnce(timeline)
    vi.mocked(personaApi.updatePersonaKeepsakeStatus).mockResolvedValueOnce({
      ...timeline,
      affinity: { score: 24, explanation: ['+3 accepted keepsakes'] },
      factors: { ...timeline.factors, acceptedKeepsakes: 3 },
      keepsakes: [{ ...timeline.keepsakes[0], status: 'accepted' }],
    })

    const { user } = renderWithProviders(<PersonaBondTimeline />)

    expect(screen.getByText('关系时间线')).toBeInTheDocument()
    expect(screen.getByText(/不改变 Agent 能力/)).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('第一次顺利合并')).toBeInTheDocument())
    expect(screen.getByText('18')).toBeInTheDocument()
    expect(screen.getByText('勋章')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /接受/ }))

    expect(personaApi.updatePersonaKeepsakeStatus).toHaveBeenCalledWith({
      id: 'ks_1',
      status: 'accepted',
    })
    await waitFor(() => expect(screen.getByText('24')).toBeInTheDocument())
  })
})
