//! M3-T8 — Skill manifest schema + frontmatter parser.
//!
//! Each `.claude/skills/<name>/SKILL.md` starts with a YAML
//! frontmatter block describing the skill's metadata. This pilot
//! ships:
//!
//! - `SkillManifest` — typed struct.
//! - `parse_skill_md` — splits the frontmatter from the body and
//!   parses the YAML into the typed manifest.
//! - `ManifestError` — typed parse failures.
//!
//! Frontmatter format:
//!
//! ```yaml
//! ---
//! name: my-skill
//! description: Short one-liner shown in suggestions.
//! version: 0.1.0
//! tags: [browser, web]
//! topics: [browser-automation, search]
//! token_estimate: 800
//! ---
//! ```
//!
//! Loader that walks `.claude/skills/` and parses skill manifests.
//!
//! Layout:
//!
//! - [`schema`] — `SkillManifest`, `parse_skill_md`, `ManifestError`

pub mod schema;

pub use schema::{parse_skill_md, ManifestError, ParsedSkill, SkillManifest};
