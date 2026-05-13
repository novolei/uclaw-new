//! Strict + permissive parser for Humane v1 YAML. (Filled per spec § 4.5)

use crate::automation::protocol::humane_v1::HumaneAutomationSpec;
use garde::Validate;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("yaml syntax: {0}")]
    Yaml(String),
    #[error("validation: {0}")]
    Validate(String),
}

#[derive(Debug, Clone)]
pub struct ParsedSpec {
    pub spec: HumaneAutomationSpec,
    pub extra_fields: HashMap<String, Value>,
}

impl ParsedSpec {
    pub fn has_unknown_fields(&self) -> bool {
        !self.extra_fields.is_empty()
    }
}

const KNOWN_TOP_LEVEL_FIELDS: &[&str] = &[
    "type",
    "name",
    "version",
    "author",
    "description",
    "system_prompt",
    "subscriptions",
    "config_schema",
    "requires",
    "filters",
    "memory_schema",
    "output",
    "escalation",
    "permissions",
    "browser_login",
    "i18n",
];

pub fn parse_humane_v1(yaml: &str) -> Result<ParsedSpec, ParseError> {
    // Strict pass first — uses deny_unknown_fields
    match serde_yml::from_str::<HumaneAutomationSpec>(yaml) {
        Ok(spec) => {
            spec.validate()
                .map_err(|e| ParseError::Validate(e.to_string()))?;
            Ok(ParsedSpec {
                spec,
                extra_fields: HashMap::new(),
            })
        }
        Err(strict_err) => {
            // Permissive fallback: parse as untyped Value, partition out unknown fields
            let value: Value = serde_yml::from_str(yaml)
                .map_err(|e| ParseError::Yaml(format!("{} (strict was: {})", e, strict_err)))?;

            let obj = match value {
                Value::Object(m) => m,
                _ => {
                    return Err(ParseError::Validate(
                        "spec must be a YAML mapping".into(),
                    ))
                }
            };

            let mut known = serde_json::Map::new();
            let mut extras = HashMap::new();
            for (k, v) in obj {
                if KNOWN_TOP_LEVEL_FIELDS.contains(&k.as_str()) {
                    known.insert(k, v);
                } else {
                    extras.insert(k, v);
                }
            }

            // If there were no extras, the strict failure was due to a structural/validation
            // problem (e.g. missing required field), not unknown fields — surface original error.
            if extras.is_empty() {
                return Err(ParseError::Validate(strict_err.to_string()));
            }

            let cleaned = Value::Object(known);
            let spec: HumaneAutomationSpec = serde_json::from_value(cleaned)
                .map_err(|e| ParseError::Validate(e.to_string()))?;
            spec.validate()
                .map_err(|e| ParseError::Validate(e.to_string()))?;

            Ok(ParsedSpec {
                spec,
                extra_fields: extras,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_parse_succeeds_on_valid() {
        let yaml = include_str!("test_fixtures/valid/simple.yaml");
        let result = parse_humane_v1(yaml);
        assert!(result.is_ok(), "got error: {:?}", result.err());
        assert!(!result.unwrap().has_unknown_fields());
    }

    #[test]
    fn strict_parse_fails_on_missing_name() {
        let yaml = include_str!("test_fixtures/invalid/missing_name.yaml");
        let err = parse_humane_v1(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("name"), "expected 'name' in error, got: {}", msg);
    }

    #[test]
    fn strict_parse_fails_on_bad_subscription() {
        let yaml = include_str!("test_fixtures/invalid/bad_subscription.yaml");
        let err = parse_humane_v1(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("schedule") || msg.contains("cron") || msg.contains("every"),
            "expected schedule/cron/every reference in error, got: {}",
            msg
        );
    }

    #[test]
    fn unknown_top_level_fields_are_silently_accepted() {
        // After removing deny_unknown_fields from HumaneAutomationSpec, the
        // permissive fallback path is no longer reached for unknown extra
        // top-level fields — strict pass succeeds and just drops them. The
        // permissive fallback in parse.rs remains for the case where strict
        // fails for OTHER reasons (e.g. nested struct error). Phase 2 may
        // reintroduce extras-tracking once we decide what shape to give
        // spec_version / icon / store / type-of-thing.
        let yaml = include_str!("test_fixtures/invalid/unknown_field.yaml");
        let parsed = parse_humane_v1(yaml).expect("unknown fields accepted");
        assert_eq!(parsed.spec.name, "x");
        // extras may or may not be populated depending on whether the strict
        // pass succeeded — we don't assert either way for Phase 1.
    }
}
