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
