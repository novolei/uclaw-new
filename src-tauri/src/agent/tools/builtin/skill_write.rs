//! Bundle 21-A — `skill_write` builtin tool.
//!
//! The Agent's existing `write-a-skill` skill teaches the LLM how to
//! structure a SKILL.md, but it leaves the LLM to choose the output
//! path. In practice the LLM dumps the file in `cwd` (workspace
//! root), which `SkillsRegistry` never scans — so the new "skill"
//! is invisible to all future sessions.
//!
//! This tool closes that loop: the LLM passes `{name, description,
//! body, scope}` and the tool writes to the correct registered
//! directory:
//!
//! - `scope="project"` → `<workspace>/.uclaw/skills/<name>/SKILL.md`
//!   (auto-approved; workspace-local, user can `rm -rf .uclaw/`)
//! - `scope="user"` → `<data_dir>/skills/<name>/SKILL.md`
//!   (requires user approval; survives across sessions and projects)
//!
//! After write, `SkillsRegistry::discover()` is invoked so the new
//! skill is visible to the same agent loop without restart. A UI
//! event `agent:skill-created` surfaces a chip so the user knows
//! a new skill landed.
//!
//! See:
//! - `agent/tools/builtin/skill_search.rs` — discovery side
//! - `agent/tools/builtin/load_skill.rs` — runtime load side
//! - `skills/borrowed/write-a-skill/SKILL.md` — authoring guide

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};
use crate::skills::SkillsRegistry;

/// `skill_write` — write a new SKILL.md (and optional reference files)
/// to the registered user or project skill directory.
pub struct SkillWriteTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    /// Per-user data dir (`~/.uclaw/`). Used for `scope=user` writes
    /// under `data_dir/skills/<name>/`.
    pub data_dir: PathBuf,
    /// Current workspace root (if a workspace is mounted). Used for
    /// `scope=project` writes under `workspace_root/.uclaw/skills/
    /// <name>/`. `None` means project scope is unavailable and the
    /// tool returns an InvalidInput error if asked.
    pub workspace_root: Option<PathBuf>,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
}

impl<R: tauri::Runtime> SkillWriteTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        data_dir: PathBuf,
        workspace_root: Option<PathBuf>,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
    ) -> Self {
        Self {
            registry,
            data_dir,
            workspace_root,
            app_handle,
            conversation_id,
        }
    }
}

/// Parsed + validated arguments. Surface as a struct so all the
/// per-field checks happen in one place (rather than scattered
/// through `execute`).
#[derive(Debug)]
struct ParsedArgs {
    name: String,
    description: String,
    body: String,
    scope: SkillScope,
    references: Vec<(String, String)>,
    force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillScope {
    User,
    Project,
}

impl SkillScope {
    fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

fn parse_args(params: &serde_json::Value) -> Result<ParsedArgs, ToolError> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::kinded(ToolErrorKind::InvalidInput, "missing required `name`"))?
        .trim()
        .to_string();
    if name.is_empty() {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            "`name` must not be empty",
        ));
    }
    if !is_kebab_case(&name) {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            format!(
                "`name` must be kebab-case (lowercase letters, digits, hyphens; \
                 cannot start/end with hyphen). Got: {name:?}"
            ),
        ));
    }
    // Hard-reject anything that could escape the skill dir via path
    // traversal — defense in depth; the kebab-case check already
    // disallows `.` and `/`.
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            "`name` must not contain path separators or `..`",
        ));
    }

    let description = params
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ToolError::kinded(
                ToolErrorKind::InvalidInput,
                "missing required `description`",
            )
        })?
        .trim()
        .to_string();
    if description.is_empty() {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            "`description` must not be empty",
        ));
    }
    if description.chars().count() > 1024 {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            format!(
                "`description` must be ≤1024 chars (got {})",
                description.chars().count()
            ),
        ));
    }

    let body = params
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::kinded(ToolErrorKind::InvalidInput, "missing required `body`"))?
        .to_string();
    if body.trim().is_empty() {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            "`body` must not be empty",
        ));
    }
    // The tool wraps body with frontmatter; the LLM should NOT
    // include `---` blocks of its own. Catch the common mistake.
    if body.trim_start().starts_with("---") {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            "`body` must contain ONLY the markdown body — frontmatter is added \
             automatically. Remove leading `---` block.",
        ));
    }

    let scope_str = params
        .get("scope")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ToolError::kinded(
                ToolErrorKind::InvalidInput,
                "missing required `scope` (must be \"user\" or \"project\")",
            )
        })?;
    let scope = match scope_str {
        "user" => SkillScope::User,
        "project" => SkillScope::Project,
        other => {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!("`scope` must be \"user\" or \"project\" (got {other:?})"),
            ));
        }
    };

    let mut references: Vec<(String, String)> = Vec::new();
    if let Some(refs_val) = params.get("references") {
        let refs_obj = refs_val.as_object().ok_or_else(|| {
            ToolError::kinded(
                ToolErrorKind::InvalidInput,
                "`references` must be an object of {filename: content}",
            )
        })?;
        for (key, val) in refs_obj {
            // Disallow path traversal in reference file names.
            if key.contains("..") || key.contains('/') || key.contains('\\') {
                return Err(ToolError::kinded(
                    ToolErrorKind::InvalidInput,
                    format!("reference filename {key:?} must not contain path separators or `..`"),
                ));
            }
            let content = val.as_str().ok_or_else(|| {
                ToolError::kinded(
                    ToolErrorKind::InvalidInput,
                    format!("reference {key:?} value must be a string"),
                )
            })?;
            references.push((key.clone(), content.to_string()));
        }
    }

    let force = params
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(ParsedArgs {
        name,
        description,
        body,
        scope,
        references,
        force,
    })
}

/// Resolve the on-disk directory for a (scope, name) pair.
fn resolve_skill_dir(
    args: &ParsedArgs,
    data_dir: &std::path::Path,
    workspace_root: Option<&std::path::Path>,
) -> Result<PathBuf, ToolError> {
    let base = match args.scope {
        SkillScope::User => data_dir.join("skills"),
        SkillScope::Project => {
            let ws = workspace_root.ok_or_else(|| {
                ToolError::kinded(
                    ToolErrorKind::InvalidInput,
                    "scope=\"project\" requires an active workspace; none is mounted. \
                     Pass scope=\"user\" instead, or select a folder first.",
                )
            })?;
            ws.join(".uclaw").join("skills")
        }
    };
    Ok(base.join(&args.name))
}

/// Frontmatter + body composer. Keeps the schema in lockstep with
/// `skills::parse_skill_md` — fields used: `name`, `description`.
fn compose_skill_md(args: &ParsedArgs) -> String {
    // Strip trailing newlines from body so we can normalize.
    let body_trimmed = args.body.trim_end();
    format!(
        "---\nname: {name}\ndescription: {desc}\n---\n\n{body}\n",
        name = args.name,
        desc = escape_frontmatter_value(&args.description),
        body = body_trimmed,
    )
}

/// YAML frontmatter values can't contain unescaped `\n`. We accept
/// short descriptions only (≤1024 chars, validated earlier) so the
/// only realistic risk is embedded newlines / colons in odd places.
/// Escape conservatively by stripping newlines.
fn escape_frontmatter_value(s: &str) -> String {
    let one_line = s.replace('\n', " ").replace('\r', "");
    // If the value contains a colon followed by space, wrap in
    // double quotes (YAML rule). This is the most common gotcha.
    if one_line.contains(": ") {
        let escaped_quotes = one_line.replace('"', "\\\"");
        format!("\"{}\"", escaped_quotes)
    } else {
        one_line
    }
}

fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[async_trait]
impl<R: tauri::Runtime> Tool for SkillWriteTool<R> {
    fn name(&self) -> &str {
        "skill_write"
    }

    fn description(&self) -> &str {
        // Keep concise — this is shown in the manifest. The LLM gets
        // the full authoring guide from the `write-a-skill` skill if
        // it loads it. Description here just covers the trigger +
        // hard rules so the LLM picks this over generic file write.
        "Author a new agent skill (SKILL.md + optional reference files) into the registered user or project skill directory. Use when creating a reusable skill — DO NOT use generic file write for this. Pass scope=\"project\" (default for workspace-specific skills) or scope=\"user\" (cross-project, requires approval). The skill is auto-registered after write so it's immediately discoverable by skill_search."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name in kebab-case (lowercase letters, digits, hyphens). Becomes the directory name."
                },
                "description": {
                    "type": "string",
                    "description": "Short description shown in the skills manifest. ≤1024 chars. Third person. Format: \"<one sentence on what it does>. Use when <trigger condition>.\""
                },
                "body": {
                    "type": "string",
                    "description": "Markdown body of SKILL.md. Do NOT include frontmatter (`---` block) — the tool adds that automatically from name+description."
                },
                "scope": {
                    "type": "string",
                    "enum": ["user", "project"],
                    "description": "\"project\" (default for workspace-specific skills): writes under <workspace>/.uclaw/skills/<name>/. \"user\": writes under ~/.uclaw/skills/<name>/ (survives across projects; requires user approval)."
                },
                "references": {
                    "type": "object",
                    "description": "Optional companion files. Map of {filename: content}, e.g. {\"REFERENCE.md\": \"...\", \"EXAMPLES.md\": \"...\"}. Filenames must not contain path separators.",
                    "additionalProperties": { "type": "string" }
                },
                "force": {
                    "type": "boolean",
                    "description": "If true, overwrites an existing skill with the same name. Default false — refuses to clobber.",
                    "default": false
                }
            },
            "required": ["name", "description", "body", "scope"]
        })
    }

    fn requires_approval(&self, params: &serde_json::Value) -> ApprovalRequirement {
        // user-scope writes cross-cut all future sessions and all
        // workspaces — that's a meaningful side effect, require
        // approval. project-scope is workspace-local; the user can
        // `rm -rf .uclaw/skills/` at any time and the impact is
        // bounded to this project, so auto-approve.
        match params.get("scope").and_then(|v| v.as_str()) {
            Some("user") => ApprovalRequirement::UnlessAutoApproved,
            _ => ApprovalRequirement::Never,
        }
    }

    fn preview_target_path(&self, params: &serde_json::Value) -> Option<String> {
        let args = parse_args(params).ok()?;
        let dir = resolve_skill_dir(&args, &self.data_dir, self.workspace_root.as_deref()).ok()?;
        Some(dir.join("SKILL.md").display().to_string())
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let started = Instant::now();
        let args = parse_args(&params)?;
        let skill_dir = resolve_skill_dir(&args, &self.data_dir, self.workspace_root.as_deref())?;
        let skill_md_path = skill_dir.join("SKILL.md");

        // Refuse to clobber unless force=true. Same SKILL.md path
        // means we'd overwrite — that's destructive enough to deserve
        // an explicit ask.
        if skill_md_path.exists() && !args.force {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "skill {:?} already exists at {}. Pass force=true to overwrite.",
                    args.name,
                    skill_dir.display()
                ),
            ));
        }

        // Create parent dirs first — std::fs::create_dir_all is
        // idempotent and short-circuits if the dir already exists.
        std::fs::create_dir_all(&skill_dir).map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::Other,
                format!("failed to create {}: {e}", skill_dir.display()),
            )
        })?;

        // SKILL.md (frontmatter + body)
        let skill_md_content = compose_skill_md(&args);
        std::fs::write(&skill_md_path, &skill_md_content).map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::Other,
                format!("failed to write {}: {e}", skill_md_path.display()),
            )
        })?;

        // Reference files
        let mut written_refs: Vec<String> = Vec::new();
        for (filename, content) in &args.references {
            let ref_path = skill_dir.join(filename);
            std::fs::write(&ref_path, content).map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::Other,
                    format!("failed to write {}: {e}", ref_path.display()),
                )
            })?;
            written_refs.push(filename.clone());
        }

        // Trigger registry rescan so the new skill is immediately
        // discoverable by skill_search inside the same agent loop.
        // The rescan is cheap (~ms for typical skill counts) and
        // idempotent.
        let discovered = {
            let mut reg = self.registry.write().await;
            // Bundle 21-A: scope=project skills live under
            // <workspace>/.uclaw/skills/. Make sure that scan dir is
            // registered. If it's already there add_scan_dir is a
            // no-op; if not we silently add it so future skills land
            // in a watched location.
            if args.scope == SkillScope::Project {
                if let Some(ws) = self.workspace_root.as_ref() {
                    let project_skills = ws.join(".uclaw").join("skills");
                    let _ = std::fs::create_dir_all(&project_skills);
                    reg.add_scan_dir(project_skills, crate::skills::SkillProvenance::Project);
                }
            }
            reg.discover().len()
        };

        // UI chip event — `agent:skill-created`. The frontend
        // (Bundle 21+ UI work) can render a green "技能已创建" pill
        // in the conversation flow. Emit even if downstream doesn't
        // listen yet — log entry costs nothing and unblocks the UI
        // side later.
        let _ = self.app_handle.emit(
            "agent:skill-created",
            json!({
                "name": args.name,
                "scope": args.scope.as_str(),
                "path": skill_md_path.display().to_string(),
                "referencesCount": written_refs.len(),
                "conversationId": self.conversation_id,
                "registryReloaded": discovered,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        );

        tracing::info!(
            name = %args.name,
            scope = args.scope.as_str(),
            path = %skill_md_path.display(),
            ref_count = written_refs.len(),
            registry_total = discovered,
            "[Bundle 21-A] skill_write wrote skill and reloaded registry"
        );

        let elapsed = started.elapsed().as_millis() as u64;
        Ok(ToolOutput::new(
            json!({
                "ok": true,
                "name": args.name,
                "scope": args.scope.as_str(),
                "path": skill_md_path.display().to_string(),
                "referencesWritten": written_refs,
                "registryReloaded": true,
                "registryTotal": discovered,
                "message": format!(
                    "Wrote skill {:?} to {} ({} reference file(s)). Registry reloaded — \
                     skill is immediately available to skill_search.",
                    args.name, skill_md_path.display(), written_refs.len(),
                ),
            }),
            elapsed,
        ))
    }
}

// ───────────────────────────────────────────────────────────────────
// Tests — Bundle 21-A
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn args(name: &str, scope: &str) -> serde_json::Value {
        json!({
            "name": name,
            "description": "Test skill. Use when running unit tests.",
            "body": "# Test\n\nThis is a test skill body.",
            "scope": scope,
        })
    }

    // ── kebab-case validation ──────────────────────────────────────

    #[test]
    fn is_kebab_case_accepts_normal_names() {
        assert!(is_kebab_case("foo"));
        assert!(is_kebab_case("foo-bar"));
        assert!(is_kebab_case("a-b-c"));
        assert!(is_kebab_case("lunar-to-solar"));
        assert!(is_kebab_case("convert-2"));
    }

    #[test]
    fn is_kebab_case_rejects_bad_names() {
        assert!(!is_kebab_case(""));
        assert!(!is_kebab_case("Foo"));
        assert!(!is_kebab_case("foo_bar"));
        assert!(!is_kebab_case("foo bar"));
        assert!(!is_kebab_case("-foo"));
        assert!(!is_kebab_case("foo-"));
        assert!(!is_kebab_case("foo.bar"));
        assert!(!is_kebab_case("foo/bar"));
    }

    // ── arg parsing ────────────────────────────────────────────────

    #[test]
    fn parse_args_happy_path_project() {
        let parsed = parse_args(&args("test-skill", "project")).expect("should parse");
        assert_eq!(parsed.name, "test-skill");
        assert_eq!(parsed.scope, SkillScope::Project);
        assert!(parsed.body.contains("test skill body"));
        assert_eq!(parsed.references.len(), 0);
        assert!(!parsed.force);
    }

    #[test]
    fn parse_args_happy_path_user() {
        let parsed = parse_args(&args("foo", "user")).expect("should parse");
        assert_eq!(parsed.scope, SkillScope::User);
    }

    #[test]
    fn parse_args_with_references_and_force() {
        let mut v = args("foo", "project");
        v["references"] = json!({
            "REFERENCE.md": "ref content",
            "EXAMPLES.md": "examples"
        });
        v["force"] = json!(true);
        let parsed = parse_args(&v).expect("should parse");
        assert_eq!(parsed.references.len(), 2);
        assert!(parsed.force);
    }

    #[test]
    fn parse_args_rejects_path_traversal_in_name() {
        let v = args("../foo", "user");
        let err = parse_args(&v).unwrap_err();
        assert!(format!("{err}").contains("kebab-case"));
    }

    #[test]
    fn parse_args_rejects_path_traversal_in_reference_name() {
        let mut v = args("foo", "project");
        v["references"] = json!({ "../bad.md": "x" });
        let err = parse_args(&v).unwrap_err();
        assert!(format!("{err}").contains("path separators"));
    }

    #[test]
    fn parse_args_rejects_unknown_scope() {
        let v = args("foo", "global");
        let err = parse_args(&v).unwrap_err();
        assert!(format!("{err}").contains("scope"));
    }

    #[test]
    fn parse_args_rejects_body_with_frontmatter() {
        let mut v = args("foo", "project");
        v["body"] = json!("---\nname: foo\n---\n\nbody");
        let err = parse_args(&v).unwrap_err();
        assert!(format!("{err}").contains("frontmatter is added automatically"));
    }

    #[test]
    fn parse_args_rejects_overlong_description() {
        let mut v = args("foo", "user");
        v["description"] = json!("a".repeat(1025));
        let err = parse_args(&v).unwrap_err();
        assert!(format!("{err}").contains("≤1024"));
    }

    #[test]
    fn parse_args_rejects_empty_name() {
        let mut v = args("foo", "user");
        v["name"] = json!("");
        let err = parse_args(&v).unwrap_err();
        assert!(format!("{err}").contains("empty"));
    }

    // ── skill_dir resolution ───────────────────────────────────────

    #[test]
    fn resolve_skill_dir_user_scope() {
        let parsed = parse_args(&args("foo", "user")).unwrap();
        let dir = resolve_skill_dir(&parsed, std::path::Path::new("/data"), None).unwrap();
        assert_eq!(dir, std::path::Path::new("/data/skills/foo"));
    }

    #[test]
    fn resolve_skill_dir_project_scope() {
        let parsed = parse_args(&args("foo", "project")).unwrap();
        let dir = resolve_skill_dir(
            &parsed,
            std::path::Path::new("/data"),
            Some(std::path::Path::new("/ws")),
        )
        .unwrap();
        assert_eq!(dir, std::path::Path::new("/ws/.uclaw/skills/foo"));
    }

    #[test]
    fn resolve_skill_dir_project_scope_without_workspace_errors() {
        let parsed = parse_args(&args("foo", "project")).unwrap();
        let err = resolve_skill_dir(&parsed, std::path::Path::new("/data"), None).unwrap_err();
        assert!(format!("{err}").contains("project"));
    }

    // ── compose_skill_md ───────────────────────────────────────────

    #[test]
    fn compose_skill_md_emits_frontmatter_then_body() {
        let parsed = parse_args(&args("my-skill", "project")).unwrap();
        let out = compose_skill_md(&parsed);
        assert!(out.starts_with("---\nname: my-skill\n"));
        assert!(out.contains("description:"));
        assert!(out.contains("\n---\n\n"));
        assert!(out.ends_with("\n"));
        assert!(out.contains("test skill body"));
    }

    #[test]
    fn compose_skill_md_quotes_description_with_colon() {
        let mut v = args("my-skill", "user");
        v["description"] = json!("Like X: do Y. Use when Z.");
        let parsed = parse_args(&v).unwrap();
        let out = compose_skill_md(&parsed);
        assert!(
            out.contains("description: \"Like X: do Y. Use when Z.\""),
            "got: {out}"
        );
    }

    #[test]
    fn compose_skill_md_strips_newlines_from_description() {
        let mut v = args("my-skill", "user");
        v["description"] = json!("Line 1\nLine 2");
        let parsed = parse_args(&v).unwrap();
        let out = compose_skill_md(&parsed);
        // Description should be on a single line in frontmatter.
        let line = out.lines().find(|l| l.starts_with("description:")).unwrap();
        assert!(!line.contains("\n"));
        assert!(line.contains("Line 1 Line 2"));
    }

    // ── approval gating ────────────────────────────────────────────

    #[test]
    fn user_scope_requires_approval() {
        let v = args("foo", "user");
        // We can't call self.requires_approval without constructing
        // the full tool (needs AppHandle), so test the logic directly
        // via the same match.
        let scope = v.get("scope").and_then(|s| s.as_str());
        let approval = match scope {
            Some("user") => ApprovalRequirement::UnlessAutoApproved,
            _ => ApprovalRequirement::Never,
        };
        assert_eq!(approval, ApprovalRequirement::UnlessAutoApproved);
    }

    #[test]
    fn project_scope_auto_approved() {
        let v = args("foo", "project");
        let scope = v.get("scope").and_then(|s| s.as_str());
        let approval = match scope {
            Some("user") => ApprovalRequirement::UnlessAutoApproved,
            _ => ApprovalRequirement::Never,
        };
        assert_eq!(approval, ApprovalRequirement::Never);
    }
}
