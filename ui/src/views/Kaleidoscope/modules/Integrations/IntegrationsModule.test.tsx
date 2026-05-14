import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor, within } from '@/test-utils/render'
import userEvent from '@testing-library/user-event'
import { IntegrationsModule } from './IntegrationsModule'

const serversFixture = [
  {
    id: 'gh', name: 'github', description: 'GitHub 操作',
    transportType: 'stdio' as const, command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-github'], env: { GITHUB_TOKEN: 'x' },
    url: null, enabled: true, autoApprove: false, errorMessage: null, status: 'connected',
  },
  {
    id: 'sl', name: 'slack', description: '',
    transportType: 'stdio' as const, command: 'npx', args: [], env: {},
    url: null, enabled: true, autoApprove: false,
    errorMessage: 'spawn failed', status: 'error',
  },
]
const toolsFixture = [
  { serverId: 'gh', name: 'create_pull_request', description: '', parameters: {} },
  { serverId: 'gh', name: 'list_issues', description: '', parameters: {} },
]

const listMcpServers = vi.fn()
const listMcpTools = vi.fn()
const addMcpServer = vi.fn()
const updateMcpServer = vi.fn()
const connectMcpServer = vi.fn()

vi.mock('@/lib/tauri-bridge', () => ({
  listMcpServers: (...a: unknown[]) => listMcpServers(...a),
  listMcpTools: (...a: unknown[]) => listMcpTools(...a),
  addMcpServer: (...a: unknown[]) => addMcpServer(...a),
  updateMcpServer: (...a: unknown[]) => updateMcpServer(...a),
  removeMcpServer: vi.fn().mockResolvedValue(true),
  toggleMcpServer: vi.fn().mockResolvedValue(true),
  restartMcpServer: vi.fn().mockResolvedValue(true),
  connectMcpServer: (...a: unknown[]) => connectMcpServer(...a),
}))

describe('IntegrationsModule', () => {
  beforeEach(() => {
    listMcpServers.mockReset().mockResolvedValue(serversFixture)
    listMcpTools.mockReset().mockResolvedValue(toolsFixture)
    addMcpServer.mockReset().mockResolvedValue({ ...serversFixture[0], id: 'new-id', name: 'newsrv' })
    updateMcpServer.mockReset().mockResolvedValue({ ...serversFixture[0], id: 'new-id', name: 'newsrv' })
    connectMcpServer.mockReset().mockResolvedValue(true)
  })

  it('renders one card per MCP server', async () => {
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText('github')).toBeInTheDocument())
    expect(screen.getByText('slack')).toBeInTheDocument()
  })

  it('opens the detail drawer when a card is clicked', async () => {
    const user = userEvent.setup()
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText('github')).toBeInTheDocument())
    await user.click(screen.getByText('github'))
    // 抽屉(Sheet → role="dialog")打开后,工具列表里出现该工具 —— 用 within 限定到抽屉,
    // 不会被卡片上的 chip 预览(点击前就在 DOM 里)误判通过。
    const drawer = await screen.findByRole('dialog')
    expect(within(drawer).getByText('create_pull_request')).toBeInTheDocument()
  })

  it('opens the editor modal from the add button', async () => {
    const user = userEvent.setup()
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText('github')).toBeInTheDocument())
    await user.click(screen.getByRole('button', { name: /添加集成/ }))
    // 模板库出现
    expect(await screen.findByText('从模板新建')).toBeInTheDocument()
  })

  it('renders the empty state when there are no servers', async () => {
    listMcpServers.mockResolvedValue([])
    listMcpTools.mockResolvedValue([])
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText(/没有集成/)).toBeInTheDocument())
  })

  it('does not call addMcpServer twice when retrying after a connect failure', async () => {
    const user = userEvent.setup()
    connectMcpServer.mockRejectedValue(new Error('connect refused'))
    renderWithProviders(<IntegrationsModule />)
    await waitFor(() => expect(screen.getByText('github')).toBeInTheDocument())

    // 打开模板库 → 选 Custom(空表单)
    await user.click(screen.getByRole('button', { name: /添加集成/ }))
    await user.click(await screen.findByText('Custom'))

    // 填名称 + 命令(stdio 必填),保存
    const dialog = await screen.findByRole('dialog')
    const nameInput = within(dialog).getByPlaceholderText('github')
    await user.type(nameInput, 'mysrv')
    const cmdInput = within(dialog).getByPlaceholderText('npx')
    await user.type(cmdInput, 'node')
    await user.click(within(dialog).getByRole('button', { name: /测试连接并保存/ }))

    // connect 失败 → 内联错误,modal 不关
    await waitFor(() => expect(addMcpServer).toHaveBeenCalledTimes(1))
    // 重试 —— 不能再 add 一次,应走 update
    await user.click(within(dialog).getByRole('button', { name: /测试连接并保存/ }))
    await waitFor(() => expect(updateMcpServer).toHaveBeenCalledTimes(1))
    expect(addMcpServer).toHaveBeenCalledTimes(1)
  })
})
