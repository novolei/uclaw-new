import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { openZoneAtom, stickyNotesAtom } from '@/atoms/home-office-atoms'
import { HomeOfficeView } from './HomeOfficeView'

// Mock the PIXI scene — jsdom can't render WebGL
vi.mock('./scene/HomeOfficeScene', () => ({
  HomeOfficeScene: () => <div data-testid="pixi-stage" />,
}))

// Mock Tauri event listener
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}))

function renderWith(store: ReturnType<typeof createStore>) {
  return render(
    <Provider store={store}>
      <HomeOfficeView />
    </Provider>,
  )
}

describe('HomeOfficeView', () => {
  it('renders the pixi scene stub', () => {
    const store = createStore()
    renderWith(store)
    expect(screen.getByTestId('pixi-stage')).toBeInTheDocument()
  })

  it('shows StickyNoteModal when openZone=sticky', () => {
    const store = createStore()
    store.set(openZoneAtom, 'sticky')
    renderWith(store)
    expect(screen.getByText('📌 Sticky Notes')).toBeInTheDocument()
  })

  it('can add a sticky note via the modal', () => {
    const store = createStore()
    store.set(openZoneAtom, 'sticky')
    renderWith(store)
    const input = screen.getByPlaceholderText('写一条便签…')
    fireEvent.change(input, { target: { value: 'hello' } })
    fireEvent.click(screen.getByText('Add'))
    expect(store.get(stickyNotesAtom)).toHaveLength(1)
    expect(store.get(stickyNotesAtom)[0].text).toBe('hello')
  })

  it('shows DiaryDeskModal when openZone=diary', () => {
    const store = createStore()
    store.set(openZoneAtom, 'diary')
    renderWith(store)
    expect(screen.getByText('✍️ Agent Diary')).toBeInTheDocument()
  })

  it('shows MusicGazeboModal when openZone=music', () => {
    const store = createStore()
    store.set(openZoneAtom, 'music')
    renderWith(store)
    expect(screen.getByText('🎵 Music Gazebo')).toBeInTheDocument()
  })
})
