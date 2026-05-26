pub mod affinity;
pub mod presets;
pub mod render;
pub mod store;
pub mod types;

pub use affinity::calculate_affinity;
pub use presets::built_in_presets;
pub use render::{render_persona_prompt_block, PERSONA_STYLE_ONLY_BOUNDARY};
pub use types::{
    AffinityFactors, BondProfile, PersonaBadge, PersonaEvent, PersonaEventKind, PersonaKeepsake,
    PersonaKeepsakeStatus, PersonaPreset, PersonaPresetId, PersonaPromptContext,
    PersonaRelationshipTimeline, PersonaScope, ProposePersonaKeepsakeInput,
    RecordPersonaEventInput, RelationshipAffinity, UpdatePersonaKeepsakeStatusInput, VoiceProfile,
};
