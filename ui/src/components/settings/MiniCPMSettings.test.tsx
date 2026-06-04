import { describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'

vi.mock('@/lib/tauri-bridge', () => ({
  localModelList: vi.fn().mockResolvedValue([
    { model_id: 'minicpm5-1b', installed: false, files: [], total_bytes: 0 },
  ]),
  localModelDelete: vi.fn().mockResolvedValue(undefined),
  petPersonaList: vi.fn().mockResolvedValue([
    { id: 'astro', name: '小宇 Astro', system_prompt: 'p', sprite_set: 'astro', greeting: '', source: 'builtin' },
    { id: 'neko', name: '猫娘', system_prompt: 'p', sprite_set: 'astro', greeting: '', source: 'imported', lora_adapter: '/x.gguf' },
  ]),
  petPersonaGetActive: vi.fn().mockResolvedValue({
    id: 'astro', name: '小宇 Astro', system_prompt: 'p', sprite_set: 'astro', greeting: '', source: 'builtin',
  }),
  petPersonaSetActive: vi.fn().mockResolvedValue(undefined),
  petPersonaImport: vi.fn().mockResolvedValue({
    id: 'x', name: 'x', system_prompt: 'p', sprite_set: 'astro', greeting: '', source: 'imported',
  }),
  petPersonaDelete: vi.fn().mockResolvedValue(undefined),
  openPersonaFileDialog: vi.fn().mockResolvedValue(null),
}))

// Use real atoms so the store-based assertions work correctly.
// minicpm-wizard atom is a real primitive atom imported via the real module.
import { MiniCPMSettings } from './MiniCPMSettings'
import { minicpmWizardAtom, INITIAL_WIZARD } from '@/atoms/minicpm-wizard'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import {
  petPersonaSetActive,
  petPersonaDelete,
} from '@/lib/tauri-bridge'

describe('MiniCPMSettings', () => {
  it('renders the not-installed state and a start button', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByText(/未安装/)).toBeInTheDocument())
    expect(screen.getByText(/开始设置/)).toBeInTheDocument()
  })

  it('renders persona list with both personas', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => {
      expect(screen.getByText('小宇 Astro')).toBeInTheDocument()
      expect(screen.getByText('猫娘')).toBeInTheDocument()
    })
  })

  it('shows 使用中 badge for active persona (astro) and no 删除 for builtin', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByText('使用中')).toBeInTheDocument())
    // astro is active → no select button, but neko has one
    expect(screen.getByTestId('pet-persona-select-neko')).toBeInTheDocument()
    // astro is builtin → no delete button
    expect(screen.queryByTestId('pet-persona-delete-astro')).not.toBeInTheDocument()
  })

  it('clicking 猫娘\'s 选择 calls petPersonaSetActive("neko")', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByTestId('pet-persona-select-neko')).toBeInTheDocument())
    fireEvent.click(screen.getByTestId('pet-persona-select-neko'))
    await waitFor(() => expect(petPersonaSetActive).toHaveBeenCalledWith('neko'))
  })

  it('imported 猫娘 has a 删除 button that calls petPersonaDelete("neko")', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByTestId('pet-persona-delete-neko')).toBeInTheDocument())
    fireEvent.click(screen.getByTestId('pet-persona-delete-neko'))
    await waitFor(() => expect(petPersonaDelete).toHaveBeenCalledWith('neko'))
  })

  it('import path input + button are rendered', async () => {
    render(<Provider><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByTestId('pet-persona-section')).toBeInTheDocument())
    expect(screen.getByTestId('pet-persona-import-path-input')).toBeInTheDocument()
    expect(screen.getByTestId('pet-persona-import-path-btn')).toBeInTheDocument()
    expect(screen.getByTestId('pet-persona-import-dialog-btn')).toBeInTheDocument()
  })

  it('clicking 开始设置 sets wizard step to intro AND closes the settings panel', async () => {
    const store = createStore()
    // Start with settings open
    store.set(settingsOpenAtom, true)
    // Ensure wizard is in idle state
    store.set(minicpmWizardAtom, INITIAL_WIZARD)

    render(<Provider store={store}><MiniCPMSettings /></Provider>)
    await waitFor(() => expect(screen.getByText(/开始设置/)).toBeInTheDocument())

    fireEvent.click(screen.getByText(/开始设置/))

    // Settings panel should be closed
    expect(store.get(settingsOpenAtom)).toBe(false)
    // Wizard step should be 'intro'
    expect(store.get(minicpmWizardAtom).step).toBe('intro')
  })
})
