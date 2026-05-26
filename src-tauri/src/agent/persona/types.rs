use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersonaScope {
    Global,
    Workspace,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersonaPresetId {
    Clarity,
    Muse,
    Anchor,
    Critic,
    Operator,
    Companion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaPreset {
    pub id: PersonaPresetId,
    pub label: String,
    pub role: String,
    pub voice: String,
    pub profile: VoiceProfile,
    pub example_user_prompt: String,
    pub example_reply: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceProfile {
    pub preset_id: PersonaPresetId,
    pub warmth: u8,
    pub directness: u8,
    pub challenge: u8,
    pub playfulness: u8,
    pub detail: u8,
    pub initiative: u8,
    pub structure: u8,
    pub restraint: u8,
    pub neutral_mode: bool,
}

impl VoiceProfile {
    pub fn clamp(mut self) -> Self {
        self.warmth = self.warmth.min(5);
        self.directness = self.directness.min(5);
        self.challenge = self.challenge.min(5);
        self.playfulness = self.playfulness.min(5);
        self.detail = self.detail.min(5);
        self.initiative = self.initiative.min(5);
        self.structure = self.structure.min(5);
        self.restraint = self.restraint.min(5);
        self
    }
}

impl Default for VoiceProfile {
    fn default() -> Self {
        Self {
            preset_id: PersonaPresetId::Clarity,
            warmth: 2,
            directness: 4,
            challenge: 3,
            playfulness: 1,
            detail: 3,
            initiative: 3,
            structure: 4,
            restraint: 4,
            neutral_mode: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BondProfile {
    pub collaboration_rhythm: Vec<String>,
    pub challenge_contract: Vec<String>,
    pub support_style: Vec<String>,
    pub communication_dislikes: Vec<String>,
}

impl Default for BondProfile {
    fn default() -> Self {
        Self {
            collaboration_rhythm: vec!["Lead with the next useful action.".into()],
            challenge_contract: vec![],
            support_style: vec![],
            communication_dislikes: vec!["Avoid hollow praise.".into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaPromptContext {
    pub voice: VoiceProfile,
    pub bond: BondProfile,
    pub relationship_gamification_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityFactors {
    pub successful_minutes: i64,
    pub accepted_keepsakes: i64,
    pub positive_feedback: i64,
    pub stable_style_fragments: i64,
    pub recovered_failures: i64,
    pub inactivity_days: i64,
    pub rejected_candidates: i64,
    pub unresolved_failures: i64,
    pub correction_count: i64,
}

impl Default for AffinityFactors {
    fn default() -> Self {
        Self {
            successful_minutes: 0,
            accepted_keepsakes: 0,
            positive_feedback: 0,
            stable_style_fragments: 0,
            recovered_failures: 0,
            inactivity_days: 0,
            rejected_candidates: 0,
            unresolved_failures: 0,
            correction_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipAffinity {
    pub score: i64,
    pub explanation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaRelationshipTimeline {
    pub affinity: RelationshipAffinity,
    pub factors: AffinityFactors,
    pub keepsakes: Vec<PersonaKeepsake>,
    pub recent_events: Vec<PersonaEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaBadge {
    pub badge_key: String,
    pub label: String,
    pub unlock_reason: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaKeepsakeStatus {
    Proposed,
    Accepted,
    Hidden,
    Discarded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaKeepsake {
    pub id: String,
    pub title: String,
    pub narrative: String,
    pub learned_text: Option<String>,
    pub evidence: Vec<String>,
    pub status: PersonaKeepsakeStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposePersonaKeepsakeInput {
    pub title: String,
    pub narrative: String,
    pub learned_text: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePersonaKeepsakeStatusInput {
    pub id: String,
    pub status: PersonaKeepsakeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaEventKind {
    CollaborationMinutes,
    TaskSucceeded,
    PositiveFeedback,
    StylePreferenceAccepted,
    FailureRecovered,
    InactivityDecay,
    KeepsakeAccepted,
    CandidateRejected,
    FailureUnresolved,
    Correction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaEvent {
    pub id: String,
    pub kind: PersonaEventKind,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub minutes: i64,
    pub weight: i64,
    pub note: Option<String>,
    pub evidence: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPersonaEventInput {
    pub kind: PersonaEventKind,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub minutes: i64,
    pub weight: i64,
    pub note: Option<String>,
    pub evidence: Vec<String>,
}
