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
    // serde(default) allows type:mcp specs (which have no system_prompt) to
    // deserialise without error. The garde length(min=1) rule still rejects
    // empty values when .validate() is called on automation specs, and
    // validate_common checks it explicitly for type:skill.
    #[garde(length(min = 1))]
    #[serde(default)]
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
    // type:mcp specs carry an mcp_server block describing how to start the server
    // process. Absent for automation/skill specs; optional here so existing specs
    // deserialise without error.
    #[garde(skip)]
    #[serde(default)]
    pub mcp_server: Option<McpServerBlock>,
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
// McpServerBlock — how to start the MCP server process (type:mcp specs)
// ---------------------------------------------------------------------------

/// `mcp_server` block from a `type: mcp` spec — how to start the MCP server
/// process. Shape fixed by DHP spec/app-spec.md §10. `garde(skip)` because
/// these are runtime-substituted values, not parse-time-validatable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct McpServerBlock {
    #[garde(skip)]
    pub command: String,
    #[garde(skip)]
    #[serde(default)]
    pub args: Vec<String>,
    #[garde(skip)]
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[garde(skip)]
    #[serde(default)]
    pub cwd: Option<String>,
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
    // Per-locale overlay for config_schema. Shape: { key: { label?, description?,
    // placeholder?, options? } }. Kept as raw JSON because per-input overlay shape
    // varies (esp. `options` which is a value→label map). Frontend looks up entries
    // by key — strict typing buys nothing here.
    #[garde(skip)]
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Cross-type validation
// ---------------------------------------------------------------------------

/// Lightweight validation shared by all package types. Unlike
/// `HumaneAutomationSpec::validate()` (which hard-requires `kind == "automation"`
/// via the `must_be_automation` garde validator), this checks only the fields
/// every type needs, plus the per-type minimum:
///   - all types: name / version / description non-empty
///   - `skill`: system_prompt non-empty
///   - `mcp`: an `mcp_server` block with a non-empty command
pub fn validate_common(spec: &HumaneAutomationSpec) -> Result<(), String> {
    if spec.name.trim().is_empty() {
        return Err("name is empty".into());
    }
    if spec.version.trim().is_empty() {
        return Err("version is empty".into());
    }
    if spec.description.trim().is_empty() {
        return Err("description is empty".into());
    }
    match spec.kind.as_str() {
        "skill" => {
            if spec.system_prompt.trim().is_empty() {
                return Err("type:skill requires a non-empty system_prompt".into());
            }
        }
        "mcp" => match &spec.mcp_server {
            Some(block) if !block.command.trim().is_empty() => {}
            Some(_) => return Err("type:mcp mcp_server.command is empty".into()),
            None => return Err("type:mcp requires an mcp_server block".into()),
        },
        _ => {}
    }
    Ok(())
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

    #[test]
    fn validate_common_accepts_skill_and_mcp() {
        // A type:skill spec — rejected by the automation .validate() (kind check),
        // but validate_common accepts it.
        let skill_yaml = r#"
spec_version: "1"
name: My Skill
version: 1.0.0
author: t
description: A handy skill
type: skill
system_prompt: You are a helpful skill.
"#;
        let skill: HumaneAutomationSpec = serde_yml::from_str(skill_yaml).expect("parses");
        assert!(skill.validate().is_err(), "automation validate() rejects type:skill");
        assert!(super::validate_common(&skill).is_ok(), "validate_common accepts type:skill");

        // A skill with an empty system_prompt fails validate_common.
        let bad_skill_yaml = r#"
spec_version: "1"
name: Bad Skill
version: 1.0.0
author: t
description: missing prompt
type: skill
system_prompt: ""
"#;
        let bad: HumaneAutomationSpec = serde_yml::from_str(bad_skill_yaml).expect("parses");
        assert!(super::validate_common(&bad).is_err(), "empty system_prompt rejected for skill");

        // A type:mcp spec with an mcp_server block.
        let mcp_yaml = r#"
spec_version: "1"
name: My MCP
version: 1.0.0
author: t
description: wraps a server
type: mcp
mcp_server:
  command: npx
  args: ["-y", "@modelcontextprotocol/server-postgres"]
  env:
    DATABASE_URL: "{{config.db_url}}"
"#;
        let mcp: HumaneAutomationSpec = serde_yml::from_str(mcp_yaml).expect("parses");
        assert!(super::validate_common(&mcp).is_ok(), "validate_common accepts type:mcp with mcp_server");
        let block = mcp.mcp_server.as_ref().expect("mcp_server parsed");
        assert_eq!(block.command, "npx");
        assert_eq!(block.args, vec!["-y", "@modelcontextprotocol/server-postgres"]);
        assert_eq!(block.env.get("DATABASE_URL").map(String::as_str), Some("{{config.db_url}}"));

        // A type:mcp spec WITHOUT an mcp_server block fails validate_common.
        let bad_mcp_yaml = r#"
spec_version: "1"
name: Bad MCP
version: 1.0.0
author: t
description: no server block
type: mcp
"#;
        let bad_mcp: HumaneAutomationSpec = serde_yml::from_str(bad_mcp_yaml).expect("parses");
        assert!(super::validate_common(&bad_mcp).is_err(), "type:mcp without mcp_server rejected");
    }

    #[test]
    fn parses_i18n_with_config_schema_overlay() {
        // DHP-style: i18n.<locale>.config_schema overlays label/description/
        // placeholder/options per input key. We keep it as raw JSON because the
        // frontend only looks up entries by key.
        let yaml = r#"
type: automation
name: Test
version: 1.0.0
author: test
description: en desc
system_prompt: irrelevant
config_schema:
  - key: keywords
    label: Search Keywords
    type: string
    required: true
    description: en desc
i18n:
  zh-CN:
    name: 中文名
    description: 中文描述
    config_schema:
      keywords:
        label: 监控关键词
        description: 中文描述
        placeholder: 关键词
        options:
          opt_a: 选项A
"#;
        let spec: HumaneAutomationSpec = serde_yml::from_str(yaml).expect("parses");
        spec.validate().expect("validates");
        let zh = spec.i18n.get("zh-CN").expect("zh-CN present");
        assert_eq!(zh.name.as_deref(), Some("中文名"));
        let cs = zh.config_schema.as_ref().expect("config_schema present");
        assert_eq!(cs["keywords"]["label"].as_str(), Some("监控关键词"));
        assert_eq!(cs["keywords"]["options"]["opt_a"].as_str(), Some("选项A"));
    }
}
