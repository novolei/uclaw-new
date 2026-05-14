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

/// Deserialize `subscriptions:` leniently — items that don't match our strict
/// Subscription enum shape are skipped with a warn log instead of failing the
/// whole spec parse. This is the bridge that lets real DHP marketplace specs
/// (newer subscription shape: `{id, source: {type, config}, frequency}`)
/// install alongside specs that use our Phase 1 mirror shape (flat
/// `{type, cron, every, path}`). Phase 3b adds a normaliser that converts the
/// newer shape into our enum so we don't lose subscriptions silently.
fn deserialize_subscriptions_lenient<'de, D>(
    deserializer: D,
) -> Result<Vec<Subscription>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<serde_json::Value> = serde::Deserialize::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(raw.len());
    for (idx, item) in raw.into_iter().enumerate() {
        match serde_json::from_value::<Subscription>(item.clone()) {
            Ok(s) => out.push(s),
            Err(e) => {
                tracing::warn!(
                    index = idx,
                    error = %e,
                    raw = %item,
                    "subscription skipped — Phase 1 schema mismatch (likely DHP newer shape; Phase 3b will normalise)"
                );
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Top-level spec (spec § 4.1)
// ---------------------------------------------------------------------------

// `deny_unknown_fields` removed: real DHP marketplace specs include fields
// uClaw Phase 1 doesn't model (spec_version, icon, store, type-of-thing).
// Strict-mode rejection broke installation of every real spec and the
// permissive fallback in parse.rs::parse_humane_v1 had its own
// "serde_json re-parse loses field rename" bug. Simpler model: silently
// accept extra top-level fields, garde still validates the fields we DO
// model. Phase 2 may reinstate strict mode once Phase 1 surveys all
// extra-field shapes and types them.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
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

    // DHP marketplace specs use a *newer* subscription shape than uClaw Phase 1
    // mirrored: `[{id, source: {type, config}, frequency: {default, min, max}}]`
    // rather than our flat `[{type, cron, every, path, ...}]`. Strict parsing
    // would reject every real spec. Lenient deserializer below tries each item
    // as a Subscription; failures are logged + skipped so the rest of the spec
    // can still be installed. Phase 3b will add a second deserializer branch
    // that recognises the DHP shape and maps to ours.
    #[garde(dive)]
    #[serde(default, deserialize_with = "deserialize_subscriptions_lenient")]
    pub subscriptions: Vec<Subscription>,
    #[garde(dive)]
    #[serde(default)]
    pub config_schema: Vec<InputDef>,
    // requires used to be `Requires { mcps: Vec<String>, skills: Vec<String> }`
    // — Phase 1 mirror. DHP marketplace specs use a richer shape:
    //   requires:
    //     mcps:
    //       - id: ai-browser
    //         reason: ...
    //     skills:
    //       - id: xhs-search
    //         reason: ...
    //         bundled: true
    //         files: [...]
    // Strict struct rejected every browser-using spec (xiaohongshu, boss-job,
    // bilibili, etc.). Phase 3a lenient — store as JSON; the registry-level
    // `requires_mcps` / `requires_skills` string arrays on RegistryEntry
    // (from index.json) are what we surface in StoreDetail's "依赖" tab.
    // Phase 3b will type this richer shape properly.
    #[garde(skip)]
    #[serde(default)]
    pub requires: Option<serde_json::Value>,
    #[garde(dive)]
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    // memory_schema and escalation in production DHP specs are loose
    // configuration objects whose exact shape varies (memory_schema is a
    // {field_name: {type, description}} map; escalation is {enabled,
    // timeout_hours} or other free form). Our Phase 1 strict structs only
    // matched one possible shape and rejected most real-world specs. Store
    // as raw JSON for Phase 1 — Phase 2 will type these once we've surveyed
    // enough live specs to know what shapes need first-class support.
    // MemorySchema and EscalationConfig structs are kept below for the
    // future strict path; they're #[allow(dead_code)] for now.
    #[garde(skip)]
    #[serde(default)]
    pub memory_schema: Option<serde_json::Value>,
    #[garde(dive)]
    pub output: Option<OutputConfig>,
    #[garde(skip)]
    #[serde(default)]
    pub escalation: Option<serde_json::Value>,
    // Permissions are validated at runtime against the user-granted set, not at parse time.
    #[garde(skip)]
    #[serde(default)]
    pub permissions: Vec<Permission>,
    // browser_login: real DHP specs use `[{url, label}]` matching our struct,
    // but the strict BrowserLoginEntry's `#[garde(url)]` + `length(min=1)`
    // rejects entries with unusual URL shapes (e.g. localhost:port, mobile
    // deep-links). Phase 3a lenient — accept any JSON shape. Phase 4 may
    // reinstate strict validation once we survey deployed labels/urls.
    #[garde(skip)]
    #[serde(default)]
    pub browser_login: serde_json::Value,
    // i18n strings are free-form display text; no schema constraint is meaningful here.
    #[garde(skip)]
    #[serde(default)]
    pub i18n: HashMap<String, I18nLocaleBlock>,
}

// ---------------------------------------------------------------------------
// Subscription discriminated union — spec § 4.2
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Subscription {
    Schedule(#[garde(dive)] ScheduleSubscription),
    File(#[garde(dive)] FileSubscription),
    Webhook(#[garde(dive)] WebhookSubscription),
    Webpage(#[garde(dive)] WebpageSubscription),
    Rss(#[garde(dive)] RssSubscription),
    Wecom(#[garde(dive)] WecomSubscription),
    Custom(#[garde(dive)] CustomSubscription),
}

// ScheduleSubscription — requires cron OR every (cross-field rule).
// Manual impl because garde's #[garde(custom)] only sees a single field value,
// not its siblings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScheduleSubscription {
    pub cron: Option<String>,
    pub every: Option<String>,
}

impl garde::Validate for ScheduleSubscription {
    type Context = ();
    fn validate_into(
        &self,
        _ctx: &Self::Context,
        parent: &mut dyn FnMut() -> garde::Path,
        report: &mut garde::Report,
    ) {
        if self.cron.is_none() && self.every.is_none() {
            report.append(
                parent(),
                garde::Error::new("schedule requires cron or every"),
            );
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct FileSubscription {
    #[garde(length(min = 1))]
    pub pattern: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct WebhookSubscription {
    #[garde(pattern("^[a-z0-9-/_]+$"))]
    pub path: String,
    #[garde(skip)]
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct WebpageSubscription {
    #[garde(url)]
    pub url: String,
    #[garde(length(min = 1))]
    pub selector: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct RssSubscription {
    #[garde(url)]
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct WecomSubscription {
    #[garde(skip)]
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct CustomSubscription {
    #[garde(length(min = 1))]
    pub provider: String,
    #[garde(length(min = 1))]
    pub key: String,
    #[garde(skip)]
    #[serde(default)]
    pub config: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Permission enum — spec § 4.3
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    AiBrowser,
    Notification,
    Filesystem,
    Network,
    Shell,
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// FilterRule — spec § 4.3
// ---------------------------------------------------------------------------

fn valid_op(op: &str, _: &()) -> garde::Result {
    if matches!(op, "eq" | "ne" | "contains" | "matches" | "gt" | "lt") {
        Ok(())
    } else {
        Err(garde::Error::new(format!("unsupported op: {}", op)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FilterRule {
    #[garde(length(min = 1))]
    pub field: String,
    #[garde(custom(valid_op))]
    pub op: String,
    #[garde(skip)]
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// InputDef — spec § 4.3
// ---------------------------------------------------------------------------

fn valid_input_type(t: &str, _: &()) -> garde::Result {
    // Accept all input types we've seen across the DHP marketplace + hello-halo.
    // StoreInstallDialog (Phase 3a) renders unknown types as text inputs with
    // a warning, so parse must not reject — UI fallback covers the long tail.
    if matches!(
        t,
        "string" | "number" | "boolean" | "secret" | "text" | "select" | "url" | "email" | "json" | "array"
    ) {
        Ok(())
    } else {
        // Don't reject — Phase 3a UI degrades to <input type=text> for unknowns
        // (with a small warning chip). Log the unknown type so we can track
        // emerging conventions for proper modelling.
        tracing::warn!(input_type = %t, "InputDef has unrecognised type — falling back to text input");
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct InputDef {
    #[garde(length(min = 1))]
    pub key: String,
    #[garde(length(min = 1))]
    pub label: String,
    #[garde(custom(valid_input_type))]
    pub r#type: String,
    #[garde(skip)]
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[garde(skip)]
    #[serde(default)]
    pub required: bool,
    #[garde(skip)]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Requires — spec § 4.3
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct Requires {
    #[garde(skip)]
    #[serde(default)]
    pub mcps: Vec<String>,
    #[garde(skip)]
    #[serde(default)]
    pub skills: Vec<String>,
}

// ---------------------------------------------------------------------------
// MemorySchema — spec § 4.4
// ---------------------------------------------------------------------------

/// Strict MemorySchema shape — preserved for Phase 2 typing work but
/// currently unused (the live field on HumaneAutomationSpec accepts
/// arbitrary JSON). See the comment near memory_schema above.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemorySchema {
    #[garde(length(min = 1))]
    pub description: String,
    #[garde(skip)]
    #[serde(default)]
    pub initial: Option<String>,
}

// ---------------------------------------------------------------------------
// OutputConfig — spec § 4.4
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OutputConfig {
    #[garde(skip)]
    #[serde(default)]
    pub channels: Vec<String>,
    #[garde(skip)]
    pub default_level: Option<String>,
}

// ---------------------------------------------------------------------------
// EscalationConfig + EscalationChoice — spec § 4.4
// ---------------------------------------------------------------------------

/// Strict EscalationConfig shape — preserved for Phase 2 typing work but
/// currently unused (the live field on HumaneAutomationSpec accepts
/// arbitrary JSON). See the comment near escalation above.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EscalationConfig {
    #[garde(length(min = 1))]
    pub description: String,
    #[garde(length(min = 2), dive)]
    pub choices: Vec<EscalationChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EscalationChoice {
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub label: String,
    #[garde(skip)]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// BrowserLoginEntry — spec § 4.4
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BrowserLoginEntry {
    #[garde(url)]
    pub url: String,
    #[garde(length(min = 1))]
    pub label: String,
}

// ---------------------------------------------------------------------------
// I18nLocaleBlock — spec § 4.4
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct I18nLocaleBlock {
    #[garde(skip)]
    #[serde(default)]
    pub name: Option<String>,
    #[garde(skip)]
    #[serde(default)]
    pub description: Option<String>,
    #[garde(skip)]
    #[serde(default)]
    pub system_prompt: Option<String>,
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

    #[test]
    fn parses_all_subscription_types() {
        let yaml = include_str!("test_fixtures/valid/all_subscription_types.yaml");
        let spec: HumaneAutomationSpec = serde_yml::from_str(yaml).expect("parses");
        spec.validate().expect("validates");
        assert_eq!(spec.subscriptions.len(), 8);
        assert!(matches!(spec.subscriptions[0], Subscription::Schedule(_)));
        assert!(matches!(spec.subscriptions[1], Subscription::Schedule(_)));
        assert!(matches!(spec.subscriptions[2], Subscription::File(_)));
        assert!(matches!(spec.subscriptions[3], Subscription::Webhook(_)));
        assert!(matches!(spec.subscriptions[4], Subscription::Webpage(_)));
        assert!(matches!(spec.subscriptions[5], Subscription::Rss(_)));
        assert!(matches!(spec.subscriptions[6], Subscription::Wecom(_)));
        assert!(matches!(spec.subscriptions[7], Subscription::Custom(_)));
    }

    #[test]
    fn schedule_requires_cron_or_every() {
        let yaml = "type: automation\nname: x\nversion: 0.1.0\nauthor: x\ndescription: x\nsystem_prompt: x\nsubscriptions:\n  - { type: schedule }";
        let spec: HumaneAutomationSpec = serde_yml::from_str(yaml).unwrap();
        assert!(spec.validate().is_err());
    }

    #[test]
    fn full_featured_round_trip() {
        let yaml = include_str!("test_fixtures/valid/full_featured.yaml");
        let spec: HumaneAutomationSpec = serde_yml::from_str(yaml).expect("parses");
        spec.validate().expect("validates");

        // Serialize back and re-parse — idempotency
        let yaml2 = serde_yml::to_string(&spec).expect("re-serialises");
        let spec2: HumaneAutomationSpec = serde_yml::from_str(&yaml2).expect("re-parses");
        spec2.validate().expect("re-validates");
        // Serialise spec2 again — must equal yaml2 byte-for-byte (deterministic round trip)
        let yaml3 = serde_yml::to_string(&spec2).expect("third serialise");
        assert_eq!(yaml2, yaml3, "non-deterministic round trip");
    }

    #[test]
    fn escalation_accepts_arbitrary_json_shape() {
        // Real DHP marketplace specs use various escalation shapes (e.g.
        // `{enabled: true, timeout_hours: 24}` in ai-daily-news), not the
        // {description, choices} form our Phase 1 strict struct expected.
        // The field is now stored as raw JSON; strict shape revisits in Phase 2.
        let dhp_style = r#"
type: automation
name: AI Daily News
version: 1.0.0
author: openkursar
description: x
system_prompt: x
escalation:
  enabled: true
  timeout_hours: 24
"#;
        let spec: HumaneAutomationSpec = serde_yml::from_str(dhp_style).expect("parses");
        spec.validate().expect("validates");
        assert!(spec.escalation.is_some());
    }

    #[test]
    fn memory_schema_accepts_field_map_shape() {
        // DHP-style: memory_schema is a map of field-name → {type, description}
        let dhp_style = r#"
type: automation
name: Test
version: 1.0.0
author: x
description: x
system_prompt: x
memory_schema:
  last_run_date:
    type: string
    description: 最后一次运行的日期
  last_news_count:
    type: number
    description: 上次推送的新闻条数
"#;
        let spec: HumaneAutomationSpec = serde_yml::from_str(dhp_style).expect("parses");
        spec.validate().expect("validates");
        assert!(spec.memory_schema.is_some());
    }
}
