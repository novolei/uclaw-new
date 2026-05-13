//! Rust mirror of hello-halo's AutomationSpec Zod schema. Filled in Tasks 2-4.

use std::collections::HashMap;

use garde::Validate;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Custom validator: `kind` field must equal "automation"
// ---------------------------------------------------------------------------

fn must_be_automation(value: &str, _: &()) -> garde::Result {
    if value == "automation" {
        Ok(())
    } else {
        Err(garde::Error::new(format!(
            "type must be 'automation', got '{}'",
            value
        )))
    }
}

// ---------------------------------------------------------------------------
// Top-level spec (spec § 4.1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HumaneAutomationSpec {
    #[serde(rename = "type")]
    #[garde(custom(must_be_automation))]
    pub kind: String, // must equal "automation"

    #[garde(length(min = 1, max = 100))]
    pub name: String,
    #[garde(pattern("^\\d+\\.\\d+\\.\\d+"))]
    pub version: String,
    #[garde(length(min = 1, max = 100))]
    pub author: String,
    #[garde(length(min = 1, max = 500))]
    pub description: String,
    #[garde(length(min = 1))]
    pub system_prompt: String,

    #[garde(dive)]
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[garde(dive)]
    #[serde(default)]
    pub config_schema: Vec<InputDef>,
    #[garde(dive)]
    #[serde(default)]
    pub requires: Requires,
    #[garde(dive)]
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    #[garde(dive)]
    pub memory_schema: Option<MemorySchema>,
    #[garde(dive)]
    pub output: Option<OutputConfig>,
    #[garde(dive)]
    pub escalation: Option<EscalationConfig>,
    #[garde(skip)]
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[garde(dive)]
    #[serde(default)]
    pub browser_login: Vec<BrowserLoginEntry>,
    #[garde(skip)]
    #[serde(default)]
    pub i18n: HashMap<String, I18nLocaleBlock>,
}

// ---------------------------------------------------------------------------
// Stub types — placeholders replaced in Tasks 3-4
//
// garde derive does not support unit structs, so we implement Validate
// manually for each stub. All stubs are trivially valid (no constraints yet).
// ---------------------------------------------------------------------------

/// Subscription discriminated union — Task 3 replaces with full enum.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Subscription; // placeholder

impl garde::Validate for Subscription {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// InputDef — Task 4 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputDef; // placeholder

impl garde::Validate for InputDef {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// FilterRule — Task 4 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterRule; // placeholder

impl garde::Validate for FilterRule {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// MemorySchema — Task 4 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemorySchema; // placeholder

impl garde::Validate for MemorySchema {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// OutputConfig — Task 4 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig; // placeholder

impl garde::Validate for OutputConfig {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// EscalationConfig — Task 4 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EscalationConfig; // placeholder

impl garde::Validate for EscalationConfig {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// Permission — Task 4 replaces with full enum (§ 4.3).
/// Unit struct stub; simple.yaml declares no permissions so this never deserialises in Task 2 tests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Permission; // placeholder

impl garde::Validate for Permission {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// BrowserLoginEntry — Task 4 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowserLoginEntry; // placeholder

impl garde::Validate for BrowserLoginEntry {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// Requires — Task 3 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Requires; // placeholder

impl garde::Validate for Requires {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

/// I18nLocaleBlock — Task 4 replaces with full struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct I18nLocaleBlock; // placeholder

impl garde::Validate for I18nLocaleBlock {
    type Context = ();
    fn validate_into(&self, _ctx: &(), _parent: &mut dyn FnMut() -> garde::Path, _report: &mut garde::Report) {}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;

    const SIMPLE: &str = include_str!("test_fixtures/valid/simple.yaml");

    #[test]
    fn parses_and_validates_simple_spec() {
        let spec: HumaneAutomationSpec = serde_yml::from_str(SIMPLE).expect("parses");
        spec.validate().expect("validates");
        assert_eq!(spec.name, "Test Spec");
        assert_eq!(spec.kind, "automation");
    }

    #[test]
    fn rejects_wrong_kind() {
        let yaml = SIMPLE.replace("type: automation", "type: not_automation");
        let spec: HumaneAutomationSpec = serde_yml::from_str(&yaml).unwrap();
        assert!(spec.validate().is_err());
    }
}
