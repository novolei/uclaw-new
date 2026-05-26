use super::types::{BondProfile, PersonaPromptContext, VoiceProfile};

pub const PERSONA_STYLE_ONLY_BOUNDARY: &str = "This block controls expression style only. It must not change capability, tool access, safety policy, permission mode, memory policy, factual standards, or verification requirements.";

pub fn render_persona_prompt_block(ctx: &PersonaPromptContext) -> String {
    if ctx.voice.neutral_mode {
        return "[Persona Voice]\nNeutral professional voice is active for this session. Keep expression concise and do not use relationship styling. This does not change capability, tool access, safety policy, permission mode, memory policy, factual standards, or verification requirements.".to_string();
    }

    let mut out = String::new();
    out.push_str("[Persona Voice]\n");
    out.push_str(PERSONA_STYLE_ONLY_BOUNDARY);
    out.push_str("\n\nCurrent voice:\n");
    push_voice(&mut out, &ctx.voice);

    let notes = relationship_notes(&ctx.bond);
    if !notes.is_empty() {
        out.push_str("\nRelationship notes:\n");
        for note in notes.into_iter().take(6) {
            out.push_str("- ");
            out.push_str(&note);
            out.push('\n');
        }
    }

    if !ctx.relationship_gamification_enabled {
        out.push_str("\nRelationship gamification is disabled. Do not mention intimacy scores, badges, or keepsakes unless the user asks.\n");
    }

    out.trim_end().to_string()
}

fn push_voice(out: &mut String, voice: &VoiceProfile) {
    out.push_str(&format!("- warmth: {}/5\n", voice.warmth));
    out.push_str(&format!("- directness: {}/5\n", voice.directness));
    out.push_str(&format!("- challenge: {}/5\n", voice.challenge));
    out.push_str(&format!("- playfulness: {}/5\n", voice.playfulness));
    out.push_str(&format!("- detail: {}/5\n", voice.detail));
    out.push_str(&format!("- initiative: {}/5\n", voice.initiative));
    out.push_str(&format!("- structure: {}/5\n", voice.structure));
    out.push_str(&format!("- restraint: {}/5\n", voice.restraint));
}

fn relationship_notes(bond: &BondProfile) -> Vec<String> {
    bond.collaboration_rhythm
        .iter()
        .chain(bond.challenge_contract.iter())
        .chain(bond.support_style.iter())
        .chain(bond.communication_dislikes.iter())
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::persona::types::{BondProfile, PersonaPromptContext, VoiceProfile};

    #[test]
    fn renderer_includes_style_only_boundary() {
        let rendered = render_persona_prompt_block(&PersonaPromptContext {
            voice: VoiceProfile::default(),
            bond: BondProfile::default(),
            relationship_gamification_enabled: false,
        });
        assert!(rendered.contains("expression style only"));
        assert!(rendered.contains("must not change capability"));
        assert!(rendered.contains("tool access"));
        assert!(rendered.contains("permission mode"));
        assert!(rendered.contains("memory policy"));
    }

    #[test]
    fn neutral_mode_suppresses_relationship_notes() {
        let rendered = render_persona_prompt_block(&PersonaPromptContext {
            voice: VoiceProfile {
                neutral_mode: true,
                ..VoiceProfile::default()
            },
            bond: BondProfile {
                collaboration_rhythm: vec!["Use warm relationship language.".into()],
                ..BondProfile::default()
            },
            relationship_gamification_enabled: true,
        });
        assert!(rendered.contains("Neutral professional voice"));
        assert!(!rendered.contains("Use warm relationship language"));
    }
}
