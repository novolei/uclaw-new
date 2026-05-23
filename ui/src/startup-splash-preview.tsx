import * as React from 'react'
import ReactDOM from 'react-dom/client'
import { StartupSplash } from '@/components/startup/StartupSplash'
import {
  resolveStartupSplashPreviewOptions,
  type StartupSplashPreviewTheme,
} from '@/components/startup/startup-splash-scenarios'
import './styles/globals.css'

const { scenario, theme, reducedMotion } = resolveStartupSplashPreviewOptions(window.location.search)

applyPreviewTheme(theme)
if (reducedMotion) disablePreviewMotion()

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <StartupSplash viewModel={scenario.viewModel} detailsExpanded={scenario.detailsExpanded} />
  </React.StrictMode>,
)

function applyPreviewTheme(theme: StartupSplashPreviewTheme): void {
  const html = document.documentElement
  html.classList.remove(
    'dark',
    'theme-warm-paper',
    'theme-qingye',
    'theme-forest-light',
    'theme-forest-dark',
  )

  if (theme === 'dark') {
    html.classList.add('dark')
    return
  }

  if (theme === 'warm-paper') {
    html.classList.add('theme-warm-paper')
    return
  }

  if (theme === 'qingye') {
    html.classList.add('theme-qingye', 'dark')
    return
  }

  if (theme === 'forest-light') {
    html.classList.add('theme-forest-light')
    return
  }

  if (theme === 'forest-dark') {
    html.classList.add('theme-forest-dark', 'dark')
  }
}

function disablePreviewMotion(): void {
  const style = document.createElement('style')
  style.setAttribute('data-startup-preview-reduced-motion', 'true')
  style.textContent = `
    *, *::before, *::after {
      animation-duration: 0.001ms !important;
      animation-iteration-count: 1 !important;
      scroll-behavior: auto !important;
      transition-duration: 0.001ms !important;
    }
  `
  document.head.appendChild(style)
}
