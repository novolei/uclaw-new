//! Skill manifest schema + frontmatter parser.

use serde::{Deserialize, Serialize};

/// Typed manifest extracted from `SKILL.md` frontmatter.
///
/// Fields are conservative — additional adapter-specific keys carried
/// as `extra` so we don't reject manifests with future fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    /// Semver-ish version string. Free-form (we don't enforce semver
    /// because some authors use date-based versions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Free-form tags (e.g. `["browser", "web"]`). Used by the M3-T1
    /// registry's `by_tag` filter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// kebab-lowercase topics matching the M2-A / M2-C convention.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
    /// Rough metadata token cost. The M2-H L3 selector uses this for
    /// budget gating. Defaults to 0 (caller may default-fill).
    #[serde(default)]
    pub token_estimate: usize,
}

/// Parser output: typed manifest + the body text after the
/// frontmatter (everything below the closing `---`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkill {
    pub manifest: SkillManifest,
    pub body: String,
}

/// Failure modes for [`parse_skill_md`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// No opening `---` fence at line 1.
    MissingFrontmatter,
    /// Opened with `---` but no closing fence found.
    UnterminatedFrontmatter,
    /// YAML parse failure inside the frontmatter block. Carries the
    /// underlying message.
    YamlInvalid(String),
    /// `name` field missing or empty.
    NameMissing,
    /// `description` field missing or empty.
    DescriptionMissing,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFrontmatter => write!(f, "skill manifest: missing frontmatter"),
            Self::UnterminatedFrontmatter => {
                write!(f, "skill manifest: unterminated frontmatter")
            }
            Self::YamlInvalid(m) => write!(f, "skill manifest: yaml invalid: {m}"),
            Self::NameMissing => write!(f, "skill manifest: name missing"),
            Self::DescriptionMissing => write!(f, "skill manifest: description missing"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// Parse a SKILL.md-style document. Splits the leading `---...---`
/// frontmatter from the body, deserializes the frontmatter as YAML
/// into [`SkillManifest`], and validates required fields.
///
/// Why we hand-split rather than use a crate: the project already
/// ships `serde_yaml` in some build configs, but to keep this pilot
/// dependency-free we parse the small subset of YAML we need
/// manually. M3-T8 commit 2 swaps in `serde_yaml` for full YAML
/// support — until then the parser handles flat key/value, plain
/// arrays `[a, b]`, integer values, and quoted/unquoted strings.
pub fn parse_skill_md(input: &str) -> Result<ParsedSkill, ManifestError> {
    let mut lines = input.lines();
    // First non-empty line must be the opening fence.
    let opener = lines.next().unwrap_or("");
    if opener.trim() != "---" {
        return Err(ManifestError::MissingFrontmatter);
    }
    let mut yaml_buf = String::new();
    let mut found_close = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_close = true;
            break;
        }
        yaml_buf.push_str(line);
        yaml_buf.push('\n');
    }
    if !found_close {
        return Err(ManifestError::UnterminatedFrontmatter);
    }
    let body: String = lines.collect::<Vec<&str>>().join("\n");

    let manifest = parse_minimal_yaml(&yaml_buf)?;
    if manifest.name.trim().is_empty() {
        return Err(ManifestError::NameMissing);
    }
    if manifest.description.trim().is_empty() {
        return Err(ManifestError::DescriptionMissing);
    }
    Ok(ParsedSkill { manifest, body })
}

/// Minimal flat-YAML subset parser. Handles:
/// - `key: value` (string value)
/// - `key: "quoted value"`
/// - `key: 42` (integer)
/// - `key: [a, b, c]` (inline list of strings)
/// - blank lines + lines starting with `#` (comments)
///
/// Anything else returns YamlInvalid. We don't need nested objects,
/// multi-line strings, etc. for the SKILL.md subset.
fn parse_minimal_yaml(input: &str) -> Result<SkillManifest, ManifestError> {
    let mut m = SkillManifest {
        name: String::new(),
        description: String::new(),
        version: None,
        tags: Vec::new(),
        topics: Vec::new(),
        token_estimate: 0,
    };
    for (idx, raw) in input.lines().enumerate() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Find first ':'.
        let colon = trimmed
            .find(':')
            .ok_or_else(|| ManifestError::YamlInvalid(format!("line {}: missing ':'", idx + 1)))?;
        let key = trimmed[..colon].trim().to_string();
        let value = trimmed[colon + 1..].trim().to_string();
        match key.as_str() {
            "name" => m.name = strip_quotes(&value),
            "description" => m.description = strip_quotes(&value),
            "version" => {
                if value.is_empty() {
                    m.version = None;
                } else {
                    m.version = Some(strip_quotes(&value));
                }
            }
            "tags" => m.tags = parse_inline_list(&value)?,
            "topics" => m.topics = parse_inline_list(&value)?,
            "token_estimate" => {
                m.token_estimate = value
                    .parse()
                    .map_err(|e: std::num::ParseIntError| ManifestError::YamlInvalid(e.to_string()))?;
            }
            // Unknown keys are tolerated for forward compat.
            _ => {}
        }
    }
    Ok(m)
}

fn strip_quotes(v: &str) -> String {
    let v = v.trim();
    if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
        || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
    {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

fn parse_inline_list(v: &str) -> Result<Vec<String>, ManifestError> {
    let v = v.trim();
    if v.is_empty() {
        return Ok(Vec::new());
    }
    if !(v.starts_with('[') && v.ends_with(']')) {
        return Err(ManifestError::YamlInvalid(format!(
            "expected inline list [a, b], got: {v}"
        )));
    }
    let inside = &v[1..v.len() - 1];
    if inside.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(inside
        .split(',')
        .map(|item| strip_quotes(item.trim()))
        .filter(|s| !s.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_manifest_doc() -> &'static str {
        "---\nname: my-skill\ndescription: A short description\nversion: 0.1.0\ntags: [browser, web]\ntopics: [browser-automation, search]\ntoken_estimate: 800\n---\n# Body\nThis is the body content.\n"
    }

    // ── happy path ────────────────────────────────────────────────

    #[test]
    fn parses_full_frontmatter() {
        let p = parse_skill_md(full_manifest_doc()).unwrap();
        assert_eq!(p.manifest.name, "my-skill");
        assert_eq!(p.manifest.description, "A short description");
        assert_eq!(p.manifest.version.as_deref(), Some("0.1.0"));
        assert_eq!(p.manifest.tags, vec!["browser", "web"]);
        assert_eq!(
            p.manifest.topics,
            vec!["browser-automation", "search"]
        );
        assert_eq!(p.manifest.token_estimate, 800);
        assert!(p.body.contains("# Body"));
    }

    // ── minimal manifest ──────────────────────────────────────────

    #[test]
    fn parses_minimal_required_fields() {
        let doc = "---\nname: tiny\ndescription: x\n---\nbody\n";
        let p = parse_skill_md(doc).unwrap();
        assert_eq!(p.manifest.name, "tiny");
        assert_eq!(p.manifest.description, "x");
        assert!(p.manifest.version.is_none());
        assert!(p.manifest.tags.is_empty());
        assert_eq!(p.manifest.token_estimate, 0);
    }

    // ── missing frontmatter ───────────────────────────────────────

    #[test]
    fn missing_frontmatter_error() {
        let doc = "no fences here\nname: nope\n";
        let err = parse_skill_md(doc).unwrap_err();
        assert_eq!(err, ManifestError::MissingFrontmatter);
    }

    #[test]
    fn unterminated_frontmatter_error() {
        let doc = "---\nname: x\ndescription: y\n";
        let err = parse_skill_md(doc).unwrap_err();
        assert_eq!(err, ManifestError::UnterminatedFrontmatter);
    }

    // ── required field validation ─────────────────────────────────

    #[test]
    fn missing_name_error() {
        let doc = "---\ndescription: y\n---\n";
        let err = parse_skill_md(doc).unwrap_err();
        assert_eq!(err, ManifestError::NameMissing);
    }

    #[test]
    fn empty_description_error() {
        let doc = "---\nname: x\ndescription: \"\"\n---\n";
        let err = parse_skill_md(doc).unwrap_err();
        assert_eq!(err, ManifestError::DescriptionMissing);
    }

    // ── quotes ─────────────────────────────────────────────────────

    #[test]
    fn quoted_value_strips_quotes() {
        let doc = "---\nname: x\ndescription: \"a long, comma-laden description\"\n---\n";
        let p = parse_skill_md(doc).unwrap();
        assert_eq!(p.manifest.description, "a long, comma-laden description");
    }

    #[test]
    fn single_quoted_value_strips() {
        let doc = "---\nname: x\ndescription: 'ok'\n---\n";
        let p = parse_skill_md(doc).unwrap();
        assert_eq!(p.manifest.description, "ok");
    }

    // ── inline list ────────────────────────────────────────────────

    #[test]
    fn parses_empty_inline_list() {
        let doc = "---\nname: x\ndescription: y\ntags: []\n---\n";
        let p = parse_skill_md(doc).unwrap();
        assert!(p.manifest.tags.is_empty());
    }

    #[test]
    fn parses_inline_list_with_quoted_entries() {
        let doc = "---\nname: x\ndescription: y\ntags: [\"a b\", c]\n---\n";
        let p = parse_skill_md(doc).unwrap();
        assert_eq!(p.manifest.tags, vec!["a b", "c"]);
    }

    #[test]
    fn malformed_list_returns_yaml_invalid() {
        let doc = "---\nname: x\ndescription: y\ntags: a, b, c\n---\n";
        // Missing brackets → YamlInvalid.
        let err = parse_skill_md(doc).unwrap_err();
        assert!(matches!(err, ManifestError::YamlInvalid(_)));
    }

    // ── token_estimate ────────────────────────────────────────────

    #[test]
    fn token_estimate_integer_parsed() {
        let doc = "---\nname: x\ndescription: y\ntoken_estimate: 1500\n---\n";
        let p = parse_skill_md(doc).unwrap();
        assert_eq!(p.manifest.token_estimate, 1500);
    }

    #[test]
    fn token_estimate_non_integer_errors() {
        let doc = "---\nname: x\ndescription: y\ntoken_estimate: many\n---\n";
        let err = parse_skill_md(doc).unwrap_err();
        assert!(matches!(err, ManifestError::YamlInvalid(_)));
    }

    // ── comments / unknown keys ───────────────────────────────────

    #[test]
    fn comments_and_blank_lines_ignored() {
        let doc = "---\n# a comment\n\nname: x\n# more comment\ndescription: y\n---\n";
        let p = parse_skill_md(doc).unwrap();
        assert_eq!(p.manifest.name, "x");
    }

    #[test]
    fn unknown_keys_tolerated() {
        let doc = "---\nname: x\ndescription: y\nfuture_field: value\n---\n";
        let p = parse_skill_md(doc).unwrap();
        // No error; unknown key silently ignored for forward compat.
        assert_eq!(p.manifest.name, "x");
    }

    // ── body extraction ───────────────────────────────────────────

    #[test]
    fn body_preserves_remaining_content() {
        let p = parse_skill_md(full_manifest_doc()).unwrap();
        assert!(p.body.contains("This is the body content"));
        // Body must NOT contain the frontmatter lines.
        assert!(!p.body.contains("name: my-skill"));
    }

    // ── serde ─────────────────────────────────────────────────────

    #[test]
    fn manifest_roundtrip_via_json() {
        let p = parse_skill_md(full_manifest_doc()).unwrap();
        let json = serde_json::to_string(&p.manifest).unwrap();
        let back: SkillManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(p.manifest, back);
    }

    // ── error Display ─────────────────────────────────────────────

    #[test]
    fn manifest_error_display() {
        let cases = [
            (
                ManifestError::MissingFrontmatter,
                "missing frontmatter",
            ),
            (
                ManifestError::UnterminatedFrontmatter,
                "unterminated",
            ),
            (
                ManifestError::YamlInvalid("oops".into()),
                "yaml invalid: oops",
            ),
            (ManifestError::NameMissing, "name missing"),
            (
                ManifestError::DescriptionMissing,
                "description missing",
            ),
        ];
        for (e, contains) in cases {
            assert!(e.to_string().contains(contains));
        }
    }
}
