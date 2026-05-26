export type PersonaPresetId =
  | 'clarity'
  | 'muse'
  | 'anchor'
  | 'critic'
  | 'operator'
  | 'companion'

export interface VoiceProfile {
  presetId: PersonaPresetId
  warmth: number
  directness: number
  challenge: number
  playfulness: number
  detail: number
  initiative: number
  structure: number
  restraint: number
  neutralMode: boolean
}

export interface PersonaPreset {
  id: PersonaPresetId
  label: string
  role: string
  voice: string
  profile: VoiceProfile
  exampleUserPrompt: string
  exampleReply: string
}

export interface PersonaConfig {
  presets: PersonaPreset[]
  voice: VoiceProfile
  renderedPrompt: string
}
