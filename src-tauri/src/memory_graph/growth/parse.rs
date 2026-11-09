//! Parse LLM output for memory growth. Tolerates markdown fences and
//! surrounding prose by extracting the first `{` … last `}`.

#[derive(Debug, Clone, PartialEq)]
pub struct Consolidation {
    pub reflections: Vec<(String, f64)>,
    pub user_model: Option<String>,
}

/// The first `{` … last `}` of `s`, if any (tolerates fences and prose).
fn json_object(s: &str) -> Option<serde_json::Value> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&s[start..=end]).ok()
}

fn confidence(v: Option<&serde_json::Value>) -> f64 {
    v.and_then(|c| c.as_f64()).unwrap_or(0.5).clamp(0.0, 1.0)
}

/// `{insight, confidence}` → `(insight, confidence ∈ [0,1], default 0.5)`.
/// Non-JSON prose is kept as the insight at 0.5; empty → `None`.
pub fn parse_reflection(s: &str) -> Option<(String, f64)> {
    if let Some(obj) = json_object(s) {
        if let Some(insight) = obj.get("insight").and_then(|v| v.as_str()) {
            let insight = insight.trim();
            return (!insight.is_empty()).then(|| (insight.to_string(), confidence(obj.get("confidence"))));
        }
    }
    let prose = s.trim();
    (!prose.is_empty()).then(|| (prose.to_string(), 0.5))
}

/// `{reflections:[{insight,confidence}], user_model}`; `None` on prose or
/// when no non-blank reflection survives.
pub fn parse_consolidation(s: &str) -> Option<Consolidation> {
    let obj = json_object(s)?;
    let reflections: Vec<(String, f64)> = obj
        .get("reflections")?
        .as_array()?
        .iter()
        .filter_map(|r| {
            let insight = r.get("insight")?.as_str()?.trim();
            (!insight.is_empty()).then(|| (insight.to_string(), confidence(r.get("confidence"))))
        })
        .collect();
    if reflections.is_empty() {
        return None;
    }
    let user_model = obj
        .get("user_model")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some(Consolidation { reflections, user_model })
}

/// Apply only when the result is non-empty and not larger than its input.
pub fn consolidation_applies(input_count: usize, c: &Consolidation) -> bool {
    !c.reflections.is_empty() && c.reflections.len() <= input_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reflection_json_fenced_prose_and_empty() {
        assert_eq!(
            parse_reflection(r#"{"insight":"Prefers Chinese replies","confidence":0.9}"#),
            Some(("Prefers Chinese replies".to_string(), 0.9))
        );
        assert_eq!(
            parse_reflection("```json\n{\"insight\": \" Uses TDD \", \"confidence\": 7}\n```"),
            Some(("Uses TDD".to_string(), 1.0)),
            "fences stripped, insight trimmed, confidence clamped"
        );
        assert_eq!(
            parse_reflection(r#"{"insight":"no conf"}"#),
            Some(("no conf".to_string(), 0.5))
        );
        assert_eq!(parse_reflection("  The user ships small PRs.  "), Some(("The user ships small PRs.".to_string(), 0.5)));
        assert_eq!(parse_reflection("   "), None);
        assert_eq!(parse_reflection(r#"{"insight":"  ","confidence":0.9}"#), None, "blank insight in JSON");
    }

    #[test]
    fn parse_consolidation_reads_both_and_rejects_prose_or_empty() {
        let c = parse_consolidation(
            "```json\n{\"reflections\":[{\"insight\":\"a\",\"confidence\":0.7},{\"insight\":\" \",\"confidence\":0.2}],\"user_model\":\"um\"}\n```",
        )
        .unwrap();
        assert_eq!(c.reflections, vec![("a".to_string(), 0.7)]);
        assert_eq!(c.user_model.as_deref(), Some("um"));
        let no_um = parse_consolidation(r#"{"reflections":[{"insight":"a"}],"user_model":"  "}"#).unwrap();
        assert_eq!(no_um.user_model, None);
        assert_eq!(no_um.reflections, vec![("a".to_string(), 0.5)]);
        assert_eq!(parse_consolidation("just prose"), None);
        assert_eq!(parse_consolidation(r#"{"reflections":[]}"#), None);
    }

    #[test]
    fn consolidation_applies_guards_growth_and_empty() {
        let one = Consolidation { reflections: vec![("a".into(), 0.5)], user_model: None };
        assert!(consolidation_applies(3, &one));
        assert!(consolidation_applies(1, &one));
        assert!(!consolidation_applies(0, &one), "output larger than input");
        let empty = Consolidation { reflections: vec![], user_model: None };
        assert!(!consolidation_applies(3, &empty));
    }
}
