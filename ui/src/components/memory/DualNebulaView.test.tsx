import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'

vi.mock('@react-three/fiber', () => ({ Canvas: ({ children }: any) => <div data-testid="r3f-canvas">{children}</div> }))
vi.mock('./nebula/primitives', () => ({
  StarNode: ({ id, onClick }: any) => <button type="button" data-testid={`star-${id}`} onClick={() => onClick(id)} />,
  EdgeLines: () => null,
  NebulaDust: () => null,
  AutoRotateControls: () => null,
}))

import { DualNebulaView } from './DualNebulaView'

const know = { nodes: [{ slug: 'k1', title: 'K', type: 'entity' }], edges: [] }
const mem = { nodes: [{ id: 'm1', spaceId: 'default', kind: 'boot', title: 'B', createdAt: '', updatedAt: '' }], edges: [], routes: [] } as any

describe('DualNebulaView', () => {
  it('empty state when both null', () => {
    renderWithProviders(<DualNebulaView memory={null} knowledge={null} />)
    expect(screen.getByTestId('dual-nebula-empty')).toBeInTheDocument()
  })

  it('renders stars for both layers and routes clicks by layer', async () => {
    const onSelect = vi.fn()
    const { user } = renderWithProviders(<DualNebulaView memory={mem} knowledge={know} onSelect={onSelect} />)
    await user.click(screen.getByTestId('star-m1'))
    expect(onSelect).toHaveBeenCalledWith('m1', 'memory')
    await user.click(screen.getByTestId('star-k1'))
    expect(onSelect).toHaveBeenCalledWith('k1', 'knowledge')
  })
})
