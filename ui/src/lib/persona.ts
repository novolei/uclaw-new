import { invoke } from '@tauri-apps/api/core'

import type { PersonaConfig, VoiceProfile } from './persona-types'

export async function getPersonaConfig(): Promise<PersonaConfig> {
  return invoke<PersonaConfig>('get_persona_config')
}

export async function updatePersonaVoiceProfile(input: VoiceProfile): Promise<PersonaConfig> {
  return invoke<PersonaConfig>('update_persona_voice_profile', { input })
}
