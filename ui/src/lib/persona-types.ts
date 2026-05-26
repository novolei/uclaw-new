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

export type PersonaEventKind =
  | 'collaboration_minutes'
  | 'task_succeeded'
  | 'positive_feedback'
  | 'style_preference_accepted'
  | 'failure_recovered'
  | 'inactivity_decay'
  | 'keepsake_accepted'
  | 'candidate_rejected'
  | 'failure_unresolved'
  | 'correction'

export type PersonaKeepsakeStatus = 'proposed' | 'accepted' | 'hidden' | 'discarded'

export interface AffinityFactors {
  successfulMinutes: number
  acceptedKeepsakes: number
  positiveFeedback: number
  stableStyleFragments: number
  recoveredFailures: number
  inactivityDays: number
  rejectedCandidates: number
  unresolvedFailures: number
  correctionCount: number
}

export interface RelationshipAffinity {
  score: number
  explanation: string[]
}

export interface BondProfile {
  collaborationRhythm: string[]
  challengeContract: string[]
  supportStyle: string[]
  communicationDislikes: string[]
}

export interface PersonaEvent {
  id: string
  kind: PersonaEventKind
  sessionId?: string | null
  taskId?: string | null
  minutes: number
  weight: number
  note?: string | null
  evidence: string[]
  createdAt: string
}

export interface PersonaKeepsake {
  id: string
  title: string
  narrative: string
  learnedText?: string | null
  evidence: string[]
  status: PersonaKeepsakeStatus
}

export interface PersonaBadge {
  id: string
  badgeKey: string
  label: string
  unlockReason: string
  evidence: string[]
  hidden: boolean
  awardedAt: string
}

export type PersonaJournalConfidence = 'low' | 'medium' | 'high'

export interface PersonaJournalEntry {
  id: string
  sessionId?: string | null
  taskId?: string | null
  observation: string
  interpretation?: string | null
  confidence: PersonaJournalConfidence
  promotedAt?: string | null
  createdAt: string
}

export type PersonaBondField =
  | 'collaboration_rhythm'
  | 'challenge_contract'
  | 'support_style'
  | 'communication_dislikes'

export interface PersonaRelationshipSettings {
  gamificationEnabled: boolean
}

export interface PersonaRelationshipTimeline {
  affinity: RelationshipAffinity
  factors: AffinityFactors
  bond: BondProfile
  journalEntries: PersonaJournalEntry[]
  keepsakes: PersonaKeepsake[]
  badges: PersonaBadge[]
  recentEvents: PersonaEvent[]
  settings: PersonaRelationshipSettings
}

export interface RecordPersonaEventInput {
  kind: PersonaEventKind
  sessionId?: string | null
  taskId?: string | null
  minutes: number
  weight: number
  note?: string | null
  evidence: string[]
}

export interface ProposePersonaKeepsakeInput {
  title: string
  narrative: string
  learnedText?: string | null
  evidence: string[]
}

export interface UpdatePersonaKeepsakeStatusInput {
  id: string
  status: PersonaKeepsakeStatus
}

export interface CreatePersonaJournalEntryInput {
  sessionId?: string | null
  taskId?: string | null
  observation: string
  interpretation?: string | null
  confidence: PersonaJournalConfidence
}

export interface PromotePersonaJournalEntryInput {
  id: string
  field: PersonaBondField
}

export interface UpdatePersonaRelationshipSettingsInput {
  gamificationEnabled: boolean
}

export interface UpdatePersonaBadgeVisibilityInput {
  badgeKey: string
  hidden: boolean
}
