//! Translation + staging for standalone (non-bundled) marketplace items.
//!
//! - `type: skill` → a uClaw SKILL.md written under
//!   ~/.uclaw/skills/_marketplace/_standalone/<slug>/.
//! - `type: mcp`   → a crate::mcp::McpServerConfig the caller registers with
//!   the MCP manager.
//!
//! Lives in its own module — parallel to skill_install.rs (3b-α's bundled-skill
//! staging) — so mod.rs stays focused on orchestration.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::automation::protocol::humane_v1::{HumaneAutomationSpec, McpServerBlock};

/// Render a `type: skill` spec into SKILL.md text — YAML frontmatter
/// (name + description) followed by the system_prompt as the body.
pub fn render_skill_md(spec: &HumaneAutomationSpec) -> String {
    #[derive(serde::Serialize)]
    struct Frontmatter<'a> {
        name: &'a str,
        description: &'a str,
    }
    let fm = serde_yml::to_string(&Frontmatter {
        name: &spec.name,
        description: &spec.description,
    })
    .unwrap_or_else(|_| format!("name: {}\ndescription: {}\n", spec.name, spec.description));
    format!("---\n{}---\n\n{}\n", fm, spec.system_prompt)
}

/// Stage a standalone skill's SKILL.md under skills_root/.staging/_standalone/<slug>/
/// then atomic-rename into skills_root/_marketplace/_standalone/<slug>/.
/// Returns the final directory. Cleans staging + returns Err on any failure.
pub fn install_skill_files(slug: &str, skill_md: &str, skills_root: &Path) -> Result<PathBuf> {
    let staging = skills_root.join(".staging").join("_standalone").join(slug);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("create staging {}", staging.display()))?;
    std::fs::write(staging.join("SKILL.md"), skill_md)
        .with_context(|| "write staged SKILL.md")?;

    let final_root = skills_root.join("_marketplace").join("_standalone");
    std::fs::create_dir_all(&final_root)
        .with_context(|| format!("create {}", final_root.display()))?;
    let final_dir = final_root.join(slug);
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir)
            .with_context(|| format!("remove existing {}", final_dir.display()))?;
    }
    std::fs::rename(&staging, &final_dir)
        .with_context(|| format!("rename {} -> {}", staging.display(), final_dir.display()))?;
    Ok(final_dir)
}

/// Substitute `{{config.<key>}}` occurrences in an MCP env map with values
/// from the user config. A reference whose key is absent is left literal
/// (logged by the caller) — better than silently dropping the variable.
pub fn substitute_env(
    env: &HashMap<String, String>,
    user_config: &serde_json::Value,
) -> HashMap<String, String> {
    let cfg = user_config.as_object();
    env.iter()
        .map(|(k, v)| {
            let mut out = v.clone();
            if let Some(obj) = cfg {
                for (ck, cv) in obj {
                    let token = format!("{{{{config.{}}}}}", ck);
                    let replacement = match cv {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    out = out.replace(&token, &replacement);
                }
            }
            (k.clone(), out)
        })
        .collect()
}

/// Translate a `type: mcp` spec's mcp_server block into an McpServerConfig.
/// `id` is a fresh UUID; the caller stores `slug` separately to link back.
pub fn build_mcp_config(
    slug: &str,
    spec: &HumaneAutomationSpec,
    block: &McpServerBlock,
    user_config: &serde_json::Value,
) -> crate::mcp::McpServerConfig {
    crate::mcp::McpServerConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: spec.name.clone(),
        description: format!("{} (marketplace://halo/{})", spec.description, slug),
        transport_type: crate::mcp::TransportType::Stdio,
        command: block.command.clone(),
        args: block.args.clone(),
        env: substitute_env(&block.env, user_config),
        url: None,
        enabled: true,
        auto_approve: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_spec() -> HumaneAutomationSpec {
        serde_yml::from_str(
            "spec_version: \"1\"\nname: Summariser\nversion: 1.0.0\nauthor: t\n\
             description: Summarises text\ntype: skill\nsystem_prompt: You summarise.\n",
        )
        .unwrap()
    }

    #[test]
    fn render_skill_md_has_frontmatter_and_body() {
        let md = render_skill_md(&skill_spec());
        assert!(md.starts_with("---\n"));
        assert!(md.contains("name: Summariser"));
        assert!(md.contains("description: Summarises text"));
        assert!(md.trim_end().ends_with("You summarise."));
    }

    #[test]
    fn install_skill_files_stages_then_promotes() {
        let tmp = tempfile::tempdir().unwrap();
        let final_dir = install_skill_files("summariser", "---\nname: x\n---\nbody\n", tmp.path()).unwrap();
        assert!(final_dir.join("SKILL.md").exists());
        assert_eq!(
            final_dir,
            tmp.path().join("_marketplace").join("_standalone").join("summariser"),
        );
        assert!(!tmp.path().join(".staging").join("_standalone").join("summariser").exists());
    }

    #[test]
    fn install_skill_files_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        install_skill_files("s", "---\nname: v1\n---\nold\n", tmp.path()).unwrap();
        install_skill_files("s", "---\nname: v2\n---\nnew\n", tmp.path()).unwrap();
        let content = std::fs::read_to_string(
            tmp.path().join("_marketplace").join("_standalone").join("s").join("SKILL.md"),
        ).unwrap();
        assert!(content.contains("new"));
        assert!(!content.contains("old"));
    }

    #[test]
    fn substitute_env_replaces_config_refs() {
        let mut env = HashMap::new();
        env.insert("DB".to_string(), "{{config.db_url}}".to_string());
        env.insert("STATIC".to_string(), "literal".to_string());
        let cfg = serde_json::json!({ "db_url": "postgres://x" });
        let out = substitute_env(&env, &cfg);
        assert_eq!(out.get("DB").map(String::as_str), Some("postgres://x"));
        assert_eq!(out.get("STATIC").map(String::as_str), Some("literal"));
    }

    #[test]
    fn substitute_env_leaves_unknown_refs_literal() {
        let mut env = HashMap::new();
        env.insert("X".to_string(), "{{config.missing}}".to_string());
        let out = substitute_env(&env, &serde_json::json!({}));
        assert_eq!(out.get("X").map(String::as_str), Some("{{config.missing}}"));
    }

    #[test]
    fn build_mcp_config_translates_block() {
        let spec: HumaneAutomationSpec = serde_yml::from_str(
            "spec_version: \"1\"\nname: PG\nversion: 1.0.0\nauthor: t\n\
             description: postgres\ntype: mcp\nmcp_server:\n  command: npx\n  args: [\"-y\", \"pg\"]\n",
        ).unwrap();
        let block = spec.mcp_server.clone().unwrap();
        let cfg = build_mcp_config("pg-mcp", &spec, &block, &serde_json::json!({}));
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args, vec!["-y", "pg"]);
        assert_eq!(cfg.name, "PG");
        assert!(!cfg.id.is_empty());
    }
}
