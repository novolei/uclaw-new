//! Legacy TOML → Humane YAML migrator. (Phase 1 § 7.1)
//!
//! Used by the V20c data-fixup migration to convert pre-Phase-1 automation_specs
//! rows (toml_content column) into equivalent Humane YAML in the new spec_yaml /
//! spec_json columns. The legacy TOML format is read-only after this migration —
//! installation paths only accept Humane YAML going forward.

use crate::automation::protocol::humane_v1::{
    HumaneAutomationSpec, ScheduleSubscription, Subscription,
};
use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("yaml serialise: {0}")]
    Yaml(String),
}

pub struct MigratedSpec {
    pub spec: HumaneAutomationSpec,
    pub yaml: String,
    pub original_toml: String,
}

// ---------------------------------------------------------------------------
// Legacy shape — mirrored from src-tauri/src/automation/spec.rs
//
// AutomationSpec uses:
//   - description: String (non-optional, defaults to "")
//   - trigger: TriggerConfig (required, tagged with `kind` discriminant,
//     snake_case variants: "cron" | "once" | "manual")
//     - Cron  { expression: String }
//     - Once  { at: String }        -- RFC-3339 datetime string, not i64
//     - Manual                      -- unit variant
//   - task: String
//   - max_iterations: Option<u32>
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LegacyTomlSpec {
    name: String,
    #[serde(default)]
    description: String,
    trigger: LegacyTrigger,
    task: String,
    #[serde(default)]
    max_iterations: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LegacyTrigger {
    Cron { expression: String },
    Once { at: String },
    Manual,
}

pub fn migrate_legacy_toml(toml_content: &str) -> Result<MigratedSpec, MigrateError> {
    let legacy: LegacyTomlSpec = toml::from_str(toml_content)?;

    let subscriptions = match &legacy.trigger {
        LegacyTrigger::Cron { expression } => vec![Subscription::Schedule(ScheduleSubscription {
            cron: Some(expression.clone()),
            every: None,
        })],
        // Once → drop (the datetime is past-meaningful only; no Humane equivalent)
        LegacyTrigger::Once { .. } => vec![],
        // Manual → no subscriptions (user-triggered)
        LegacyTrigger::Manual => vec![],
    };

    let spec = HumaneAutomationSpec {
        kind: "automation".into(),
        name: legacy.name,
        version: "0.0.0".into(),
        author: "uclaw-migrated".into(),
        description: if legacy.description.is_empty() {
            "Migrated from TOML v1".into()
        } else {
            legacy.description
        },
        system_prompt: legacy.task,
        subscriptions,
        config_schema: vec![],
        requires: Default::default(),
        filters: vec![],
        memory_schema: None,
        output: None,
        escalation: None,
        permissions: vec![],
        browser_login: vec![],
        i18n: HashMap::new(),
    };

    let yaml = serde_yml::to_string(&spec).map_err(|e| MigrateError::Yaml(e.to_string()))?;
    Ok(MigratedSpec {
        spec,
        yaml,
        original_toml: toml_content.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;

    // The real legacy TOML uses #[serde(tag = "kind", rename_all = "snake_case")]
    // so the trigger is an inline table with kind = "cron" | "once" | "manual".

    #[test]
    fn migrates_cron_trigger() {
        let toml = r#"
name = "Test"
description = "desc"
task = "Do thing"

[trigger]
kind = "cron"
expression = "0 8 * * *"
"#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert_eq!(migrated.spec.name, "Test");
        assert_eq!(migrated.spec.description, "desc");
        assert_eq!(migrated.spec.system_prompt, "Do thing");
        assert_eq!(migrated.spec.subscriptions.len(), 1);
        match &migrated.spec.subscriptions[0] {
            Subscription::Schedule(s) => assert_eq!(s.cron.as_deref(), Some("0 8 * * *")),
            _ => panic!("expected schedule subscription"),
        }
        // Round-trip through Humane parser
        let parsed: HumaneAutomationSpec =
            serde_yml::from_str(&migrated.yaml).expect("parses YAML");
        parsed.validate().expect("migrated YAML must validate");
    }

    #[test]
    fn migrates_manual_trigger_to_empty_subscriptions() {
        let toml = r#"
name = "Test"
task = "Do thing"

[trigger]
kind = "manual"
"#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert!(migrated.spec.subscriptions.is_empty());
    }

    #[test]
    fn migrates_once_trigger_drops_subscription() {
        let toml = r#"
name = "Test"
task = "Do thing"

[trigger]
kind = "once"
at = "2024-01-01T08:00:00Z"
"#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert!(migrated.spec.subscriptions.is_empty());
    }

    #[test]
    fn empty_description_gets_default() {
        let toml = r#"
name = "NoDesc"
task = "Some task"

[trigger]
kind = "manual"
"#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert_eq!(migrated.spec.description, "Migrated from TOML v1");
    }

    #[test]
    fn original_toml_preserved() {
        let toml = "name = \"X\"\ntask = \"Y\"\n\n[trigger]\nkind = \"manual\"\n";
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert_eq!(migrated.original_toml, toml);
    }

    #[test]
    fn migrates_cron_validates_humane_spec() {
        let toml = r#"
name = "Daily Digest"
description = "Sends a daily summary"
task = "Summarise yesterday's emails and send to Slack"
max_iterations = 5

[trigger]
kind = "cron"
expression = "0 9 * * 1-5"
"#;
        let migrated = migrate_legacy_toml(toml).expect("migrates");
        assert_eq!(migrated.spec.name, "Daily Digest");
        migrated.spec.validate().expect("spec must validate");
        // YAML round-trip
        let reparsed: HumaneAutomationSpec =
            serde_yml::from_str(&migrated.yaml).expect("re-parses");
        reparsed.validate().expect("re-parses and validates");
    }

    #[test]
    fn invalid_toml_returns_error() {
        let result = migrate_legacy_toml("not valid [ toml");
        assert!(matches!(result, Err(MigrateError::Toml(_))));
    }
}
