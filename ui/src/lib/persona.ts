import { invoke } from '@tauri-apps/api/core'

import type {
  BondProfile,
  CreatePersonaJournalEntryInput,
  PersonaConfig,
  PersonaRelationshipTimeline,
  PromotePersonaJournalEntryInput,
  ProposePersonaKeepsakeInput,
  RecordPersonaEventInput,
  UpdatePersonaBadgeVisibilityInput,
  UpdatePersonaKeepsakeStatusInput,
  UpdatePersonaRelationshipSettingsInput,
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

export async function createPersonaJournalEntry(
  input: CreatePersonaJournalEntryInput,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('create_persona_journal_entry', { input })
}

export async function deletePersonaJournalEntry(
  id: string,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('delete_persona_journal_entry', { id })
}

export async function promotePersonaJournalEntry(
  input: PromotePersonaJournalEntryInput,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('promote_persona_journal_entry', { input })
}

export async function updatePersonaBondProfile(
  input: BondProfile,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('update_persona_bond_profile', { input })
}

export async function updatePersonaRelationshipSettings(
  input: UpdatePersonaRelationshipSettingsInput,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('update_persona_relationship_settings', { input })
}

export async function updatePersonaBadgeVisibility(
  input: UpdatePersonaBadgeVisibilityInput,
): Promise<PersonaRelationshipTimeline> {
  return invoke<PersonaRelationshipTimeline>('update_persona_badge_visibility', { input })
}
