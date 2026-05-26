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

export interface PersonaRelationshipTimeline {
  affinity: RelationshipAffinity
  factors: AffinityFactors
  keepsakes: PersonaKeepsake[]
  recentEvents: PersonaEvent[]
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
