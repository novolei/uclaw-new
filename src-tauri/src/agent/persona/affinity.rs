use super::types::{AffinityFactors, RelationshipAffinity};

pub fn calculate_affinity(f: &AffinityFactors) -> RelationshipAffinity {
    let positive = (f.successful_minutes / 60).min(25)
        + (f.accepted_keepsakes * 6).min(24)
        + (f.positive_feedback * 4).min(16)
        + (f.stable_style_fragments * 3).min(15)
        + (f.recovered_failures * 5).min(10);
    let negative = (f.inactivity_days / 14).min(10)
        + (f.rejected_candidates * 2).min(10)
        + (f.unresolved_failures * 5).min(15)
        + (f.correction_count * 3).min(15);
    let score = (positive - negative).clamp(0, 100);
    let mut explanation = Vec::new();
    if f.successful_minutes > 0 {
        explanation.push(format!(
            "+{} collaboration hours",
            f.successful_minutes / 60
        ));
    }
    if f.accepted_keepsakes > 0 {
        explanation.push(format!("+{} accepted keepsakes", f.accepted_keepsakes));
    }
    if f.inactivity_days > 0 {
        explanation.push(format!(
            "-{} days since recent collaboration",
            f.inactivity_days
        ));
    }
    if f.correction_count > 0 {
        explanation.push(format!(
            "-{} misunderstanding corrections",
            f.correction_count
        ));
    }
    RelationshipAffinity { score, explanation }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affinity_uses_positive_and_cooling_factors() {
        let affinity = calculate_affinity(&AffinityFactors {
            successful_minutes: 600,
            accepted_keepsakes: 2,
            positive_feedback: 1,
            stable_style_fragments: 2,
            recovered_failures: 1,
            inactivity_days: 30,
            rejected_candidates: 1,
            unresolved_failures: 0,
            correction_count: 1,
        });
        assert!(affinity.score > 0);
        assert!(affinity
            .explanation
            .iter()
            .any(|line| line.contains("accepted keepsakes")));
        assert!(affinity
            .explanation
            .iter()
            .any(|line| line.contains("days since")));
    }
}
