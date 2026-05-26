import { invoke } from '@tauri-apps/api/core'

import type {
  PersonaConfig,
  PersonaRelationshipTimeline,
  ProposePersonaKeepsakeInput,
  RecordPersonaEventInput,
  UpdatePersonaKeepsakeStatusInput,
  VoiceProfile,
} from './persona-types'

export async function getPersonaConfig(): Promise<PersonaConfig> {
  return invoke<PersonaConfig>('get_persona_config')
}

export async function updatePersonaVoiceProfile(input: VoiceProfile): Promise<PersonaConfig> {
  return invoke<PersonaConfig>('update_persona_voice_profile', { input })
}

export async function getPersonaRelationshipTimeline(): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('get_persona_relationship_timeline')
}

export async function recordPersonaEvent(
  input: RecordPersonaEventInput,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('record_persona_event', { input })
}

export async function proposePersonaKeepsake(
  input: ProposePersonaKeepsakeInput,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('propose_persona_keepsake', { input })
}

export async function updatePersonaKeepsakeStatus(
  input: UpdatePersonaKeepsakeStatusInput,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('update_persona_keepsake_status', { input })
}
