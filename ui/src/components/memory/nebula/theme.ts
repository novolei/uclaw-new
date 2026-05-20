import type { ThemeStyle } from '@/lib/chat-types'

// ─── 主题自适应配置 ──────────────────────────────────────────────────────

export interface NebulaThemeConfig {
  fogColor: string
  ambientColor: string
  ambientIntensity: number
  pointLightColor: string
  pointLightIntensity: number
  starsFactor: number
  starsFade: boolean
  edgeOpacity: number
  edgeHighlightOpacity: number
  emissiveScale: number
  showNebulaDust: boolean
}

export function getNebulaThemeConfig(resolved: 'light' | 'dark', style: ThemeStyle): NebulaThemeConfig {
  const darkDefault: NebulaThemeConfig = {
    fogColor: '#0a0a1a',
    ambientColor: '#1a1a3a',
    ambientIntensity: 0.3,
    pointLightColor: '#ffffff',
    pointLightIntensity: 0.6,
    starsFactor: 5,
    starsFade: true,
    edgeOpacity: 0.12,
    edgeHighlightOpacity: 0.45,
    emissiveScale: 1.0,
    showNebulaDust: true,
  }
  const lightDefault: NebulaThemeConfig = {
    fogColor: '#e8eaf0',
    ambientColor: '#f0f4ff',
    ambientIntensity: 0.7,
    pointLightColor: '#ffffff',
    pointLightIntensity: 0.4,
    starsFactor: 2,
    starsFade: false,
    edgeOpacity: 0.08,
    edgeHighlightOpacity: 0.30,
    emissiveScale: 0.5,
    showNebulaDust: false,
  }

  if (resolved === 'dark') {
    switch (style) {
      case 'ocean-dark':
        return { ...darkDefault, fogColor: '#0a1628', ambientColor: '#1a3050' }
      case 'forest-dark':
        return { ...darkDefault, fogColor: '#0a1a0f', ambientColor: '#1a3a20', ambientIntensity: 0.35, starsFactor: 4, emissiveScale: 0.9 }
      case 'qingye':
        return { ...darkDefault, fogColor: '#0f1a12', ambientColor: '#1a3522', ambientIntensity: 0.35, starsFactor: 4, emissiveScale: 0.9 }
      case 'black':
        return { ...darkDefault, fogColor: '#000000', ambientColor: '#0a0a0a', ambientIntensity: 0.2, starsFactor: 7, emissiveScale: 1.2 }
      case 'the-finals':
        return { ...darkDefault, fogColor: '#1a0a1e', ambientColor: '#2a1030', emissiveScale: 1.1 }
      case 'slate-dark':
        return { ...darkDefault, fogColor: '#0f1419', ambientColor: '#1a2530' }
      default:
        return darkDefault
    }
  } else {
    switch (style) {
      case 'ocean-light':
        return { ...lightDefault, fogColor: '#e0f0ff', ambientColor: '#c8e8ff' }
      case 'forest-light':
        return { ...lightDefault, fogColor: '#e8f5e8', ambientColor: '#d0f0d0' }
      case 'warm-paper':
        return { ...lightDefault, fogColor: '#f5f0e8', ambientColor: '#faf5e8', ambientIntensity: 0.8, starsFactor: 1.5, emissiveScale: 0.4 }
      case 'slate-light':
        return { ...lightDefault, fogColor: '#e8ecf0', ambientColor: '#d0d8e0' }
      default:
        return lightDefault
    }
  }
}
