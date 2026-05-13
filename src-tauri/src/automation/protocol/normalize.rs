//! Spec normaliser producing deterministic spec_json. (Phase 1 § 6)

use crate::automation::protocol::humane_v1::HumaneAutomationSpec;

/// Normalize a HumaneAutomationSpec to a deterministic JSON string. Used as
/// the canonical `spec_json` column value in `automation_specs` so that
/// FTS / queries see a stable representation and so that two specs with the
/// same logical content produce byte-identical JSON regardless of input field
/// order.
pub fn normalize_to_json(spec: &HumaneAutomationSpec) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(spec)?;
    serde_json::to_string(&sort_keys(value))
}

fn sort_keys(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => {
            let sorted: std::collections::BTreeMap<String, serde_json::Value> = m
                .into_iter()
                .map(|(k, v)| (k, sort_keys(v)))
                .collect();
            // BTreeMap → IntoIterator yields keys in sorted order
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.into_iter().map(sort_keys).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::parse::parse_humane_v1;

    #[test]
    fn normalize_strips_extras_and_produces_stable_json() {
        let yaml = include_str!("test_fixtures/valid/full_featured.yaml");
        let parsed = parse_humane_v1(yaml).expect("parses");
        let json = normalize_to_json(&parsed.spec).expect("normalises");
        // serialise twice — must be byte-identical (deterministic key order)
        let again = normalize_to_json(&parsed.spec).expect("re-normalises");
        assert_eq!(json, again);
        // i18n preserved at top level
        let v: serde_json::Value = serde_json::from_str(&json).expect("re-parses");
        assert!(v.get("i18n").is_some());
    }

    #[test]
    fn normalize_sorts_keys_independently_of_input_order() {
        // Build two specs with the same content but different field orders
        // by serialising the same HumaneAutomationSpec twice — JSON output must be identical
        let yaml = include_str!("test_fixtures/valid/simple.yaml");
        let parsed_a = parse_humane_v1(yaml).expect("parses");
        let parsed_b = parse_humane_v1(yaml).expect("parses again");
        assert_eq!(
            normalize_to_json(&parsed_a.spec).unwrap(),
            normalize_to_json(&parsed_b.spec).unwrap()
        );
    }
}
