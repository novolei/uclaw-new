//! Skills loader — loads Agent skills from SKILL.md files.
//!
//! Skills are defined in SKILL.md files with YAML frontmatter (parsed via serde_yml)
//! followed by a markdown body that serves as the skill's prompt content.
//! This module discovers, parses, scores, and manages skill configurations.

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ─── Constants ──────────────────────────────────────────────────────────

/// Maximum number of keywords allowed per skill.
const MAX_KEYWORDS_PER_SKILL: usize = 20;
/// Maximum number of regex patterns allowed per skill.
const MAX_PATTERNS_PER_SKILL: usize = 5;
/// Maximum number of tags allowed per skill.
const MAX_TAGS_PER_SKILL: usize = 10;
/// Minimum keyword/tag length to prevent overly broad matching.
const MIN_KEYWORD_TAG_LENGTH: usize = 3;
/// Maximum SKILL.md file size (64 KiB).
const MAX_SKILL_FILE_SIZE: u64 = 64 * 1024;
/// Maximum total discovered skills.
const MAX_DISCOVERED_SKILLS: usize = 100;
/// Default max recursion depth for directory scanning.
const DEFAULT_MAX_SCAN_DEPTH: usize = 3;

// ─── Scoring constants ──────────────────────────────────────────────────

const KEYWORD_EXACT_SCORE: u32 = 10;
const KEYWORD_SUBSTRING_SCORE: u32 = 5;
const TAG_MATCH_SCORE: u32 = 3;
const PATTERN_MATCH_SCORE: u32 = 20;
const MAX_KEYWORD_SCORE: u32 = 30;
const MAX_TAG_SCORE: u32 = 15;
const MAX_PATTERN_SCORE: u32 = 40;
/// Default max context tokens per skill.
const DEFAULT_MAX_CONTEXT_TOKENS: usize = 2000;
/// Default global token budget for all active skills.
const DEFAULT_MAX_TOTAL_CONTEXT_TOKENS: usize = 4000;

// ─── Parse Error ────────────────────────────────────────────────────────

/// Errors that can occur when parsing a SKILL.md file.
#[derive(Debug, thiserror::Error)]
pub enum SkillParseError {
    #[error("Missing YAML frontmatter delimiters")]
    MissingFrontmatter,
    #[error("Invalid YAML frontmatter: {0}")]
    InvalidYaml(String),
    #[error("Prompt body is empty")]
    EmptyPrompt,
    #[error("Invalid skill name '{0}': must match [a-zA-Z0-9][a-zA-Z0-9._-]{{0,63}}")]
    InvalidName(String),
    #[error("File too large: {size} bytes (max {max})")]
    FileTooLarge { size: u64, max: u64 },
}

// ─── Skill Name Validation ──────────────────────────────────────────────

static SKILL_NAME_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$").unwrap());

/// Validate a skill name.
pub fn validate_skill_name(name: &str) -> bool {
    SKILL_NAME_RE.is_match(name)
}

/// Normalize a skill title for case-insensitive lookup. Mirrors the
/// normalization applied by the proactive scenario when it stores skills,
/// so look-up by user/LLM-provided strings (which may differ in case,
/// whitespace, or trailing punctuation) hits the canonical row.
///
/// Used by both `record_skill_cited` (citation chip → cited_count bump)
/// and `load_skill` (agent-driven full-body fetch).
pub fn normalize_skill_title(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(|c: char| {
            matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '。' | '，' | '；' | '：' | '！' | '？')
        })
        .to_string()
}

// ─── Activation Criteria ────────────────────────────────────────────────

/// Activation criteria parsed from SKILL.md frontmatter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivationCriteria {
    /// Keywords that trigger this skill.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Keywords that veto this skill.
    #[serde(default)]
    pub exclude_keywords: Vec<String>,
    /// Regex patterns for complex matching.
    #[serde(default)]
    pub patterns: Vec<String>,
    /// Tags for broad category matching.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Maximum context tokens this skill should consume.
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
}

fn default_max_context_tokens() -> usize {
    DEFAULT_MAX_CONTEXT_TOKENS
}

impl ActivationCriteria {
    /// Enforce limits on keywords, patterns, and tags.
    pub fn enforce_limits(&mut self) {
        self.keywords.retain(|k| k.len() >= MIN_KEYWORD_TAG_LENGTH);
        self.keywords.truncate(MAX_KEYWORDS_PER_SKILL);
        self.exclude_keywords
            .retain(|k| k.len() >= MIN_KEYWORD_TAG_LENGTH);
        self.exclude_keywords.truncate(MAX_KEYWORDS_PER_SKILL);
        self.patterns.truncate(MAX_PATTERNS_PER_SKILL);
        self.tags.retain(|t| t.len() >= MIN_KEYWORD_TAG_LENGTH);
        self.tags.truncate(MAX_TAGS_PER_SKILL);
    }
}

// ─── Parameter Definition ───────────────────────────────────────────────

/// A parameter definition within a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParameter {
    pub name: String,
    #[serde(default = "default_param_type")]
    pub r#type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default: Option<String>,
}

fn default_param_type() -> String {
    "string".to_string()
}

// ─── Tool Definition ────────────────────────────────────────────────────

/// Tool definition within a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for tool parameters.
    #[serde(default)]
    pub parameters: serde_json::Value,
    /// Command to execute (for shell-based skills).
    #[serde(default)]
    pub command: Option<String>,
}

// ─── Skill Manifest ─────────────────────────────────────────────────────

/// Skill metadata parsed from SKILL.md YAML frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub category: String,
    /// Activation criteria for matching.
    #[serde(default)]
    pub activation: ActivationCriteria,
    /// Skill parameters.
    #[serde(default)]
    pub parameters: Vec<SkillParameter>,
    /// Dependencies (other skill names).
    #[serde(default)]
    pub requires: Vec<String>,
    /// Tool definitions provided by this skill.
    #[serde(default)]
    pub tools: Vec<SkillToolDef>,
    /// Path to the skill directory (not serialized from YAML).
    #[serde(skip)]
    pub path: PathBuf,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_enabled() -> bool {
    true
}

// ─── Loaded Skill ───────────────────────────────────────────────────────

/// A fully loaded skill ready for activation.
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub manifest: SkillManifest,
    /// Raw prompt content (markdown body after frontmatter).
    pub prompt_content: String,
    /// Pre-compiled regex patterns from activation criteria.
    pub compiled_patterns: Vec<Regex>,
    /// Pre-computed lowercased keywords for scoring.
    pub lowercased_keywords: Vec<String>,
    /// Pre-computed lowercased exclude keywords for veto scoring.
    pub lowercased_exclude_keywords: Vec<String>,
    /// Pre-computed lowercased tags for scoring.
    pub lowercased_tags: Vec<String>,
}

// ─── SKILL.md Parser ────────────────────────────────────────────────────

/// Parse a SKILL.md file from its raw content string.
pub fn parse_skill_md(content: &str, path: PathBuf) -> Result<LoadedSkill, SkillParseError> {
    // Strip optional UTF-8 BOM
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);

    // Find YAML frontmatter between --- markers
    let trimmed = content.trim_start_matches(['\n', '\r']);
    if !trimmed.starts_with("---") {
        return Err(SkillParseError::MissingFrontmatter);
    }

    let after_first = &trimmed[3..];
    let after_first_line = match after_first.find('\n') {
        Some(pos) => &after_first[pos + 1..],
        None => return Err(SkillParseError::MissingFrontmatter),
    };

    // Find closing ---
    let yaml_end =
        find_closing_delimiter(after_first_line).ok_or(SkillParseError::MissingFrontmatter)?;
    let yaml_str = &after_first_line[..yaml_end];

    // Parse YAML frontmatter
    let mut manifest: SkillManifest =
        serde_yml::from_str(yaml_str).map_err(|e| SkillParseError::InvalidYaml(e.to_string()))?;
    manifest.path = path;

    // Validate skill name
    if !validate_skill_name(&manifest.name) {
        return Err(SkillParseError::InvalidName(manifest.name.clone()));
    }

    // Enforce activation limits
    manifest.activation.enforce_limits();

    // Extract prompt content (everything after the closing --- line)
    let after_yaml = &after_first_line[yaml_end..];
    let prompt_start = after_yaml
        .find('\n')
        .map(|p| p + 1)
        .unwrap_or(after_yaml.len());
    let prompt_content = after_yaml[prompt_start..]
        .trim_start_matches('\n')
        .to_string();

    // Build loaded skill
    let compiled_patterns = compile_patterns(&manifest.activation.patterns);
    let lowercased_keywords = to_lowercase_vec(&manifest.activation.keywords);
    let lowercased_exclude_keywords = to_lowercase_vec(&manifest.activation.exclude_keywords);
    let lowercased_tags = to_lowercase_vec(&manifest.activation.tags);

    // If no prompt body AND no system_prompt in manifest, that's fine for tool-only skills
    // But we still set prompt_content to whatever we found
    Ok(LoadedSkill {
        manifest,
        prompt_content,
        compiled_patterns,
        lowercased_keywords,
        lowercased_exclude_keywords,
        lowercased_tags,
    })
}

/// Find the position of a closing `---` delimiter on its own line.
fn find_closing_delimiter(content: &str) -> Option<usize> {
    let mut pos = 0;
    for line in content.lines() {
        if line.trim() == "---" {
            return Some(pos);
        }
        pos += line.len() + 1; // +1 for newline
    }
    None
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    const MAX_REGEX_SIZE: usize = 1 << 16;
    patterns
        .iter()
        .filter_map(|p| {
            match RegexBuilder::new(p).size_limit(MAX_REGEX_SIZE).build() {
                Ok(re) => Some(re),
                Err(e) => {
                    tracing::warn!("Invalid activation regex '{}': {}", p, e);
                    None
                }
            }
        })
        .collect()
}

fn to_lowercase_vec(items: &[String]) -> Vec<String> {
    items.iter().map(|s| s.to_lowercase()).collect()
}

// ─── Skill Scoring ──────────────────────────────────────────────────────

/// Score a skill against a user message. Returns 0 if vetoed by exclude_keywords.
pub fn score_skill(skill: &LoadedSkill, message: &str) -> u32 {
    let message_lower = message.to_lowercase();

    // Exclusion veto
    if skill
        .lowercased_exclude_keywords
        .iter()
        .any(|excl| message_lower.contains(excl.as_str()))
    {
        return 0;
    }

    let mut score: u32 = 0;

    // Keyword scoring
    let mut keyword_score: u32 = 0;
    for kw in &skill.lowercased_keywords {
        if message_lower
            .split_whitespace()
            .any(|word| word.trim_matches(|c: char| !c.is_alphanumeric()) == kw.as_str())
        {
            keyword_score += KEYWORD_EXACT_SCORE;
        } else if message_lower.contains(kw.as_str()) {
            keyword_score += KEYWORD_SUBSTRING_SCORE;
        }
    }
    score += keyword_score.min(MAX_KEYWORD_SCORE);

    // Tag scoring
    let mut tag_score: u32 = 0;
    for tag in &skill.lowercased_tags {
        if message_lower.contains(tag.as_str()) {
            tag_score += TAG_MATCH_SCORE;
        }
    }
    score += tag_score.min(MAX_TAG_SCORE);

    // Regex pattern scoring
    let mut regex_score: u32 = 0;
    for re in &skill.compiled_patterns {
        if re.is_match(message) {
            regex_score += PATTERN_MATCH_SCORE;
        }
    }
    score += regex_score.min(MAX_PATTERN_SCORE);

    score
}

// ─── Parameter Validation ───────────────────────────────────────────────

/// Validation error for skill parameters.
#[derive(Debug, Clone, Serialize)]
pub struct ParamValidationError {
    pub param_name: String,
    pub message: String,
}

/// Validate parameters against a skill's parameter definitions.
/// Returns a list of validation errors (empty means all valid).
pub fn validate_params(
    skill: &SkillManifest,
    provided: &HashMap<String, String>,
) -> Vec<ParamValidationError> {
    let mut errors = Vec::new();
    for param_def in &skill.parameters {
        let value = provided.get(&param_def.name);
        if param_def.required && value.map_or(true, |v| v.is_empty()) {
            if param_def.default.is_none() {
                errors.push(ParamValidationError {
                    param_name: param_def.name.clone(),
                    message: format!("Required parameter '{}' is missing", param_def.name),
                });
            }
        }
        // Type validation
        if let Some(val) = value {
            if !validate_param_type(&param_def.r#type, val) {
                errors.push(ParamValidationError {
                    param_name: param_def.name.clone(),
                    message: format!(
                        "Parameter '{}' expects type '{}', got '{}'",
                        param_def.name, param_def.r#type, val
                    ),
                });
            }
        }
    }
    errors
}

/// Basic type validation.
fn validate_param_type(expected_type: &str, value: &str) -> bool {
    match expected_type {
        "string" => true,
        "number" | "integer" => value.parse::<f64>().is_ok(),
        "boolean" => matches!(value, "true" | "false" | "1" | "0"),
        "url" => value.starts_with("http://") || value.starts_with("https://"),
        _ => true, // unknown type, accept
    }
}

// ─── Skills Registry ────────────────────────────────────────────────────

/// Skills registry — discovers, manages, and matches skills.
pub struct SkillsRegistry {
    /// All loaded skills keyed by name.
    skills: HashMap<String, LoadedSkill>,
    /// Manually disabled skill names.
    disabled: std::collections::HashSet<String>,
    /// Directories to scan for skills.
    scan_dirs: Vec<PathBuf>,
    /// Maximum recursion depth for directory scanning.
    max_scan_depth: usize,
    /// Maximum active skills for injection.
    max_active_skills: usize,
    /// Maximum total context tokens for all active skills.
    max_total_context_tokens: usize,
}

impl SkillsRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            disabled: std::collections::HashSet::new(),
            scan_dirs: Vec::new(),
            max_scan_depth: DEFAULT_MAX_SCAN_DEPTH,
            max_active_skills: 3,
            max_total_context_tokens: DEFAULT_MAX_TOTAL_CONTEXT_TOKENS,
        }
    }

    /// Add a directory to scan for skills.
    pub fn add_scan_dir(&mut self, dir: PathBuf) {
        if !self.scan_dirs.contains(&dir) {
            self.scan_dirs.push(dir);
        }
    }

    /// Register a loaded skill.
    pub fn register(&mut self, skill: LoadedSkill) {
        tracing::info!("Registered skill: {}", skill.manifest.name);
        self.skills.insert(skill.manifest.name.clone(), skill);
    }

    /// Unregister a skill by name.
    pub fn unregister(&mut self, name: &str) -> Option<SkillManifest> {
        if let Some(loaded) = self.skills.remove(name) {
            self.disabled.remove(name);
            tracing::info!("Unregistered skill: {}", name);
            Some(loaded.manifest)
        } else {
            None
        }
    }

    /// Enable a skill.
    pub fn enable(&mut self, name: &str) -> bool {
        if self.skills.contains_key(name) {
            self.disabled.remove(name);
            tracing::info!("Enabled skill: {}", name);
            true
        } else {
            false
        }
    }

    /// Disable a skill.
    pub fn disable(&mut self, name: &str) -> bool {
        if self.skills.contains_key(name) {
            self.disabled.insert(name.to_string());
            tracing::info!("Disabled skill: {}", name);
            true
        } else {
            false
        }
    }

    /// Check if a skill is enabled.
    pub fn is_enabled(&self, name: &str) -> bool {
        self.skills.contains_key(name) && !self.disabled.contains(name)
    }

    /// Get a skill's manifest by name.
    pub fn get(&self, name: &str) -> Option<&SkillManifest> {
        self.skills.get(name).map(|s| &s.manifest)
    }

    /// Get a loaded skill by name.
    pub fn get_loaded(&self, name: &str) -> Option<&LoadedSkill> {
        self.skills.get(name)
    }

    /// List all registered skill manifests.
    pub fn list(&self) -> Vec<&SkillManifest> {
        let mut skills: Vec<&SkillManifest> = self.skills.values().map(|s| &s.manifest).collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// List enabled skill manifests.
    pub fn list_enabled(&self) -> Vec<&SkillManifest> {
        self.list()
            .into_iter()
            .filter(|s| !self.disabled.contains(&s.name))
            .collect()
    }

    /// Match skills against user input using deterministic scoring.
    /// Returns matched skills sorted by relevance, limited by max_active_skills and token budget.
    pub fn match_skills(&self, message: &str) -> Vec<&LoadedSkill> {
        if message.is_empty() {
            return vec![];
        }

        let enabled_skills: Vec<&LoadedSkill> = self
            .skills
            .values()
            .filter(|s| !self.disabled.contains(&s.manifest.name))
            .collect();

        let mut scored: Vec<(&LoadedSkill, u32)> = enabled_skills
            .into_iter()
            .filter_map(|skill| {
                let score = score_skill(skill, message);
                if score > 0 { Some((skill, score)) } else { None }
            })
            .collect();

        // Sort by score descending
        scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));

        // Apply candidate limit and token budget
        let mut result = Vec::new();
        let mut budget_remaining = self.max_total_context_tokens;

        for (skill, _score) in scored {
            if result.len() >= self.max_active_skills {
                break;
            }
            let declared = skill.manifest.activation.max_context_tokens;
            let approx_tokens = (skill.prompt_content.len() as f64 * 0.25) as usize;
            let token_cost = if approx_tokens > declared * 2 {
                approx_tokens
            } else {
                declared
            }
            .max(1);

            if token_cost <= budget_remaining {
                budget_remaining -= token_cost;
                result.push(skill);
            }
        }

        result
    }

    /// Match a skill by slash command (e.g. "/skill-name" → find "skill-name").
    pub fn match_slash_command(&self, input: &str) -> Option<&LoadedSkill> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let cmd = trimmed[1..].split_whitespace().next()?;
        self.skills
            .get(cmd)
            .filter(|s| !self.disabled.contains(&s.manifest.name))
    }

    /// Format a single static/borrowed skill for injection into the system prompt.
    ///
    /// Returns `None` if no skill with that exact name is registered (or it's
    /// in the disabled set). Used by the `/<skill-name>` slash command path in
    /// `send_agent_message` — the resolver gets back the formatted prompt
    /// string ready to push into `agent_messages` as a system note.
    pub fn format_for_injection(&self, name: &str) -> Option<String> {
        self.skills
            .get(name)
            .filter(|s| !self.disabled.contains(&s.manifest.name))
            .map(|skill| format_skill_prompt(skill))
    }

    /// Build system prompt injection for matched skills.
    pub fn build_skill_prompt(&self, message: &str) -> String {
        // First check for slash command
        if let Some(skill) = self.match_slash_command(message) {
            return format_skill_prompt(skill);
        }

        // Otherwise, match by scoring
        let matched = self.match_skills(message);
        if matched.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        for skill in &matched {
            parts.push(format_skill_prompt(skill));
        }
        parts.join("\n\n")
    }

    /// Get combined system prompt for all enabled skills (non-matching, just all enabled).
    pub fn combined_system_prompt(&self) -> String {
        let mut parts = Vec::new();
        for skill in self.list_enabled() {
            if let Some(loaded) = self.skills.get(&skill.name) {
                if !loaded.prompt_content.is_empty() {
                    parts.push(format!(
                        "## Skill: {}\n{}",
                        skill.name, loaded.prompt_content
                    ));
                }
            }
        }
        parts.join("\n\n")
    }

    /// Discover skills from all scan directories.
    pub fn discover(&mut self) -> Vec<String> {
        let mut discovered_names = Vec::new();
        let mut total_count = 0;

        let dirs = self.scan_dirs.clone();
        for dir in &dirs {
            if total_count >= MAX_DISCOVERED_SKILLS {
                tracing::warn!("Skill discovery cap reached ({})", MAX_DISCOVERED_SKILLS);
                break;
            }
            let found = Self::discover_from_dir(
                dir,
                MAX_DISCOVERED_SKILLS - total_count,
                0,
                self.max_scan_depth,
            );
            for (name, loaded) in found {
                if !self.skills.contains_key(&name) {
                    tracing::info!("Discovered skill: {} ({})", name, loaded.manifest.version);
                    self.skills.insert(name.clone(), loaded);
                    discovered_names.push(name);
                    total_count += 1;
                }
            }
        }

        tracing::info!(
            "Skill discovery complete: {} new, {} total",
            discovered_names.len(),
            self.skills.len()
        );
        discovered_names
    }

    /// Reload all skills from scan directories (clear and re-discover).
    pub fn reload(&mut self) -> Vec<String> {
        let disabled = self.disabled.clone();
        self.skills.clear();
        let names = self.discover();
        // Restore disabled state
        self.disabled = disabled;
        names
    }

    /// Discover skills from a single directory, recursing into subdirectories.
    fn discover_from_dir(
        dir: &Path,
        remaining_cap: usize,
        current_depth: usize,
        max_depth: usize,
    ) -> Vec<(String, LoadedSkill)> {
        let mut results = Vec::new();

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    tracing::debug!("Skills directory does not exist: {:?}", dir);
                } else {
                    tracing::warn!("Failed to read skills directory {:?}: {}", dir, e);
                }
                return results;
            }
        };

        // Check for SKILL.md directly in this directory (flat layout)
        let direct_skill = dir.join("SKILL.md");
        if direct_skill.exists() && direct_skill.is_file() {
            if let Some(loaded) = load_skill_file(&direct_skill, dir) {
                let name = loaded.manifest.name.clone();
                results.push((name, loaded));
            }
        }

        for entry in entries.flatten() {
            if results.len() >= remaining_cap {
                break;
            }

            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Skip hidden files/dirs
            if name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() && skill_md.is_file() {
                    // Subdirectory layout: dir/<name>/SKILL.md
                    if let Some(loaded) = load_skill_file(&skill_md, &path) {
                        let skill_name = loaded.manifest.name.clone();
                        results.push((skill_name, loaded));
                    }
                } else if current_depth < max_depth {
                    // Bundle directory: recurse
                    let sub_results = Self::discover_from_dir(
                        &path,
                        remaining_cap - results.len(),
                        current_depth + 1,
                        max_depth,
                    );
                    results.extend(sub_results);
                }
            }
        }

        results
    }

    /// Extract parameters from user input based on a skill's parameter definitions.
    pub fn extract_params(
        skill: &SkillManifest,
        input: &str,
    ) -> HashMap<String, String> {
        let mut params = HashMap::new();
        // For slash commands: /skill-name arg1 arg2 --param=value
        let parts: Vec<&str> = input.split_whitespace().collect();

        // Skip the command itself
        let args: Vec<&str> = if parts.first().map_or(false, |p| p.starts_with('/')) {
            parts[1..].to_vec()
        } else {
            parts
        };

        // Parse --key=value and --key value pairs
        let mut i = 0;
        let mut positional_idx = 0;
        while i < args.len() {
            let arg = args[i];
            if arg.starts_with("--") {
                let key_value = &arg[2..];
                if let Some((key, value)) = key_value.split_once('=') {
                    params.insert(key.to_string(), value.to_string());
                } else if i + 1 < args.len() {
                    params.insert(key_value.to_string(), args[i + 1].to_string());
                    i += 1;
                }
            } else {
                // Positional parameter
                if positional_idx < skill.parameters.len() {
                    params.insert(
                        skill.parameters[positional_idx].name.clone(),
                        arg.to_string(),
                    );
                    positional_idx += 1;
                }
            }
            i += 1;
        }

        // Fill in defaults for missing params
        for param_def in &skill.parameters {
            if !params.contains_key(&param_def.name) {
                if let Some(ref default) = param_def.default {
                    params.insert(param_def.name.clone(), default.clone());
                }
            }
        }

        params
    }
}

impl Default for SkillsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Load and parse a single SKILL.md file.
fn load_skill_file(skill_md_path: &Path, skill_dir: &Path) -> Option<LoadedSkill> {
    // Check file size
    match std::fs::metadata(skill_md_path) {
        Ok(meta) => {
            if meta.len() > MAX_SKILL_FILE_SIZE {
                tracing::warn!(
                    "SKILL.md too large ({} bytes): {:?}",
                    meta.len(),
                    skill_md_path
                );
                return None;
            }
        }
        Err(e) => {
            tracing::warn!("Cannot stat {:?}: {}", skill_md_path, e);
            return None;
        }
    }

    let content = match std::fs::read_to_string(skill_md_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to read {:?}: {}", skill_md_path, e);
            return None;
        }
    };

    match parse_skill_md(&content, skill_dir.to_path_buf()) {
        Ok(loaded) => Some(loaded),
        Err(e) => {
            tracing::warn!("Failed to parse {:?}: {}", skill_md_path, e);
            // Fallback: create a minimal skill from the directory name
            let name = skill_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            if validate_skill_name(&name) {
                let manifest = SkillManifest {
                    name: name.clone(),
                    version: "0.1.0".into(),
                    description: format!("Skill from {:?} (parse error: {})", skill_dir, e),
                    author: String::new(),
                    enabled: true,
                    category: "general".into(),
                    activation: ActivationCriteria::default(),
                    parameters: Vec::new(),
                    requires: Vec::new(),
                    tools: Vec::new(),
                    path: skill_dir.to_path_buf(),
                };
                Some(LoadedSkill {
                    manifest,
                    prompt_content: content,
                    compiled_patterns: vec![],
                    lowercased_keywords: vec![],
                    lowercased_exclude_keywords: vec![],
                    lowercased_tags: vec![],
                })
            } else {
                None
            }
        }
    }
}

/// Format a single skill's prompt for injection into the system prompt.
fn format_skill_prompt(skill: &LoadedSkill) -> String {
    let mut prompt = format!(
        "<skill name=\"{}\" version=\"{}\">",
        skill.manifest.name, skill.manifest.version
    );
    if !skill.prompt_content.is_empty() {
        prompt.push('\n');
        prompt.push_str(&skill.prompt_content);
    }
    prompt.push_str("\n</skill>");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_skill_md() -> &'static str {
        r#"---
name: writing-assistant
version: "1.0.0"
description: Professional writing help
author: uclaw
enabled: true
category: productivity
activation:
  keywords: ["write", "edit", "proofread"]
  patterns: ["(?i)\\b(write|draft)\\b.*\\bemail\\b"]
  tags: ["writing", "email"]
  max_context_tokens: 2000
parameters:
  - name: tone
    type: string
    required: false
    description: Writing tone
    default: professional
  - name: language
    type: string
    required: true
    description: Target language
---

You are a professional writing assistant. Help the user write, edit, and proofread documents.
"#
    }

    #[test]
    fn test_parse_valid_skill() {
        let loaded = parse_skill_md(sample_skill_md(), PathBuf::from("/tmp/test")).unwrap();
        assert_eq!(loaded.manifest.name, "writing-assistant");
        assert_eq!(loaded.manifest.version, "1.0.0");
        assert_eq!(loaded.manifest.activation.keywords.len(), 3);
        assert_eq!(loaded.manifest.parameters.len(), 2);
        assert!(loaded.prompt_content.contains("professional writing assistant"));
    }

    #[test]
    fn test_parse_minimal() {
        let content = "---\nname: minimal\n---\n\nHello world.\n";
        let loaded = parse_skill_md(content, PathBuf::from("/tmp")).unwrap();
        assert_eq!(loaded.manifest.name, "minimal");
        assert_eq!(loaded.manifest.version, "0.1.0");
    }

    #[test]
    fn test_parse_missing_frontmatter() {
        let content = "Just some text without frontmatter.";
        assert!(parse_skill_md(content, PathBuf::from("/tmp")).is_err());
    }

    #[test]
    fn test_score_keyword_exact() {
        let skill = parse_skill_md(sample_skill_md(), PathBuf::from("/tmp")).unwrap();
        let score = score_skill(&skill, "Please write an email");
        assert!(score > 0);
    }

    #[test]
    fn test_score_excludes_veto() {
        let content = r#"---
name: test-skill
activation:
  keywords: ["write"]
  exclude_keywords: ["ignore"]
---

Test prompt.
"#;
        let skill = parse_skill_md(content, PathBuf::from("/tmp")).unwrap();
        let score = score_skill(&skill, "write but ignore this");
        assert_eq!(score, 0);
    }

    #[test]
    fn test_validate_params_required_missing() {
        let loaded = parse_skill_md(sample_skill_md(), PathBuf::from("/tmp")).unwrap();
        let params: HashMap<String, String> = HashMap::new();
        let errors = validate_params(&loaded.manifest, &params);
        assert!(errors.iter().any(|e| e.param_name == "language"));
    }

    #[test]
    fn test_validate_params_type_check() {
        let content = r#"---
name: test
parameters:
  - name: count
    type: number
    required: true
---

Test.
"#;
        let skill = parse_skill_md(content, PathBuf::from("/tmp")).unwrap();
        let mut params = HashMap::new();
        params.insert("count".into(), "not-a-number".into());
        let errors = validate_params(&skill.manifest, &params);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_extract_params_slash_command() {
        let content = r#"---
name: test
parameters:
  - name: file
    type: string
    required: true
  - name: mode
    type: string
    default: auto
---

Test.
"#;
        let skill = parse_skill_md(content, PathBuf::from("/tmp")).unwrap();
        let params = SkillsRegistry::extract_params(&skill.manifest, "/test myfile.txt --mode=fast");
        assert_eq!(params.get("file").map(|s| s.as_str()), Some("myfile.txt"));
        assert_eq!(params.get("mode").map(|s| s.as_str()), Some("fast"));
    }

    #[test]
    fn test_slash_command_matching() {
        let mut registry = SkillsRegistry::new();
        let skill = parse_skill_md(sample_skill_md(), PathBuf::from("/tmp")).unwrap();
        registry.register(skill);
        assert!(registry.match_slash_command("/writing-assistant").is_some());
        assert!(registry.match_slash_command("/nonexistent").is_none());
        assert!(registry.match_slash_command("not a command").is_none());
    }

    #[test]
    fn test_skill_name_validation() {
        assert!(validate_skill_name("my-skill"));
        assert!(validate_skill_name("skill_v2"));
        assert!(!validate_skill_name(""));
        assert!(!validate_skill_name("has spaces"));
        assert!(!validate_skill_name("-starts-dash"));
    }

    #[test]
    fn normalize_skill_title_handles_case_whitespace_punctuation() {
        assert_eq!(normalize_skill_title("Stock Research"), "stock research");
        assert_eq!(normalize_skill_title("  Stock  Research  "), "stock research");
        assert_eq!(normalize_skill_title("Stock Research."), "stock research");
        assert_eq!(normalize_skill_title("Stock Research！"), "stock research");
        assert_eq!(normalize_skill_title("API_KEY-Blacklist"), "api_key-blacklist");
        assert_eq!(normalize_skill_title(""), "");
    }

    /// PR-mattpocock-2: verify the vendored skills under `skills/borrowed/`
    /// all parse cleanly under uClaw's `SkillManifest` schema. mattpocock's
    /// frontmatter is `{ name, description, disable-model-invocation? }` —
    /// uClaw's required fields are just `name` (all others have serde defaults),
    /// and unknown fields are silently ignored. So this should round-trip.
    #[test]
    fn borrowed_skills_parse_under_uclaw_schema() {
        let manifest_dir = std::env::current_dir()
            .ok()
            .and_then(|p| p.parent().map(|x| x.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let borrowed_dir = manifest_dir.join("skills/borrowed");
        // Skip silently if we're not running from the repo workspace —
        // some test runners place CWD in `target/`.
        if !borrowed_dir.exists() {
            return;
        }
        let expected_names = [
            "diagnose", "tdd", "zoom-out",
            "handoff", "grill-me", "caveman", "write-a-skill",
        ];
        for name in expected_names {
            let path = borrowed_dir.join(name).join("SKILL.md");
            assert!(path.exists(),
                "missing borrowed skill: {}", path.display());
            let content = std::fs::read_to_string(&path).unwrap();
            let loaded = parse_skill_md(&content, path.clone())
                .unwrap_or_else(|e| panic!("borrowed skill {} failed to parse: {:?}", name, e));
            assert_eq!(loaded.manifest.name, name,
                "name in frontmatter must match directory name");
            assert!(!loaded.manifest.description.is_empty(),
                "borrowed skill {} must have a description (mattpocock convention)", name);
        }
    }
}
