import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor, within } from '@/test-utils/render'
import userEvent from '@testing-library/user-event'
import { SkillsModule } from './SkillsModule'

const learnedFixture = [
  {
    id: 'L1', name: 'systematic-debugging', context: '修复流式 bug 的场景',
    principles: '先复现', steps: '1. 复现', pitfalls: '别猜',
    enabled: true, usageCount: 3, createdAt: '2026-05-12T10:00:00Z',
  },
]
const builtinFixture = [
  {
    name: 'brainstorming', version: '1.0.0', description: '把想法变成设计',
    author: 'uclaw', enabled: true, category: 'design', provenance: 'bundled' as const,
  },
]

const listLearnedSkills = vi.fn()
const listSkills = vi.fn()
const deleteLearnedSkill = vi.fn()

vi.mock('@/lib/tauri-bridge', () => ({
  listLearnedSkills: (...a: unknown[]) => listLearnedSkills(...a),
  toggleLearnedSkill: vi.fn().mockResolvedValue(undefined),
  deleteLearnedSkill: (...a: unknown[]) => deleteLearnedSkill(...a),
  proposeSkillConsolidation: vi.fn().mockResolvedValue({ clusters: [] }),
  backfillSkillKeywords: vi.fn().mockResolvedValue({ backfilledSkills: 0, totalLearnedSkills: 0, keywordsInserted: 0 }),
  listSkills: (...a: unknown[]) => listSkills(...a),
  toggleSkill: vi.fn().mockResolvedValue(true),
  forkSkillToUser: vi.fn().mockResolvedValue('~/.uclaw/skills/x'),
  reloadSkills: vi.fn().mockResolvedValue([]),
}))

// 重子树 —— stub 掉,本测试只关心 SkillsModule 的 merge / 分组 / 选中。
vi.mock('@/components/settings/SkillEvolutionTab', () => ({
  SkillEvolutionTab: () => <div data-testid="skill-evolution-tab" />,
}))
vi.mock('@/components/settings/SkillConsolidationDialog', () => ({
  SkillConsolidationDialog: () => null,
}))
vi.mock('react-markdown', () => ({ default: ({ children }: { children: string }) => <span>{children}</span> }))

describe('SkillsModule', () => {
  beforeEach(() => {
    listLearnedSkills.mockReset().mockResolvedValue(learnedFixture)
    listSkills.mockReset().mockResolvedValue(builtinFixture)
    deleteLearnedSkill.mockReset().mockResolvedValue(undefined)
  })

  it('merges learned + builtin into the two groups with counts', async () => {
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getByText('systematic-debugging')).toBeInTheDocument())
    expect(screen.getByText('brainstorming')).toBeInTheDocument()
    expect(screen.getByText(/学得 · 1/)).toBeInTheDocument()
    expect(screen.getByText(/内置 · 1/)).toBeInTheDocument()
  })

  it('collapses a group when its header is clicked', async () => {
    const user = userEvent.setup()
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getByText('systematic-debugging')).toBeInTheDocument())
    await user.click(screen.getByText(/学得 · 1/))
    expect(screen.queryByText('systematic-debugging')).not.toBeInTheDocument()
  })

  it('shows the detail pane when a skill is selected', async () => {
    const user = userEvent.setup()
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getByText('systematic-debugging')).toBeInTheDocument())
    await user.click(screen.getByText('systematic-debugging'))
    // 详情面板渲染了「场景」段(此 label 只在详情面板出现,列表行没有)
    expect(screen.getByText('场景')).toBeInTheDocument()
  })

  it('renders the empty state when both sources are empty', async () => {
    listLearnedSkills.mockResolvedValue([])
    listSkills.mockResolvedValue([])
    renderWithProviders(<SkillsModule />)
    await waitFor(() =>
      expect(screen.getByText(/还没学到技能/)).toBeInTheDocument(),
    )
  })

  it('refetches and restores the skill when delete fails', async () => {
    const user = userEvent.setup()
    deleteLearnedSkill.mockRejectedValueOnce(new Error('backend offline'))
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getByText('systematic-debugging')).toBeInTheDocument())
    // 选中 → 删除 → 确认
    await user.click(screen.getByText('systematic-debugging'))
    await user.click(screen.getByRole('button', { name: '删除' }))
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: '删除' }))
    // 删除失败 → onConfirmDelete 走 refetch,fixture 仍返回该技能 → 它重新出现
    await waitFor(() =>
      expect(screen.getByText('systematic-debugging')).toBeInTheDocument(),
    )
  })
})
