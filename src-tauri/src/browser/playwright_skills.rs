use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const PLAYWRIGHT_BUILTIN_SKILLS_DIR_NAME: &str = "playwright-cli";
const PLAYWRIGHT_BUILTIN_SKILLS_PARENT: &str = "builtin-skills";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSkillManifest {
    pub name: String,
    pub source_version: String,
    pub required_capabilities: Vec<String>,
    pub hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightSkillCompatibilityStatus {
    Enabled,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSkillCompatibilityReport {
    pub name: String,
    pub status: PlaywrightSkillCompatibilityStatus,
    pub reason: Option<String>,
}

pub fn managed_playwright_skills_dir(data_dir: &Path) -> PathBuf {
    data_dir
        .join(PLAYWRIGHT_BUILTIN_SKILLS_PARENT)
        .join(PLAYWRIGHT_BUILTIN_SKILLS_DIR_NAME)
}

pub fn ensure_managed_playwright_skills(data_dir: &Path) -> std::io::Result<PathBuf> {
    let root = managed_playwright_skills_dir(data_dir);
    write_skill_if_missing(
        &root,
        "playwright-browser-automation",
        PLAYWRIGHT_BROWSER_AUTOMATION_SKILL,
    )?;
    write_skill_if_missing(
        &root,
        "playwright-browser-diagnostics",
        PLAYWRIGHT_BROWSER_DIAGNOSTICS_SKILL,
    )?;
    Ok(root)
}

fn write_skill_if_missing(root: &Path, slug: &str, content: &str) -> std::io::Result<()> {
    let skill_dir = root.join(slug);
    std::fs::create_dir_all(&skill_dir)?;
    let skill_path = skill_dir.join("SKILL.md");
    if !skill_path.exists() {
        std::fs::write(skill_path, content)?;
    }
    Ok(())
}

pub fn is_managed_playwright_skill_path(path: &Path) -> bool {
    let mut saw_parent = false;
    for component in path.components() {
        let name = component.as_os_str();
        if saw_parent && name == PLAYWRIGHT_BUILTIN_SKILLS_DIR_NAME {
            return true;
        }
        saw_parent = name == PLAYWRIGHT_BUILTIN_SKILLS_PARENT;
    }
    false
}

const PLAYWRIGHT_BROWSER_AUTOMATION_SKILL: &str = r#"---
name: playwright-browser-automation
version: "1.0.0"
description: Use official Playwright browser automation through uClaw Browser Runtime Adapter.
activation:
  tags: ["navigate", "click", "type", "screenshot"]
---
# Playwright Browser Automation

Use this skill when a task needs browser navigation, clicking, typing, screenshots, or page inspection.

Do not run arbitrary Playwright shell commands. Convert the request into browser automation intent and route execution through uClaw Browser Runtime Adapter.
"#;

const PLAYWRIGHT_BROWSER_DIAGNOSTICS_SKILL: &str = r#"---
name: playwright-browser-diagnostics
version: "1.0.0"
description: Inspect Playwright browser state, snapshots, traces, and route evidence through uClaw Browser Runtime Adapter.
activation:
  tags: ["snapshot", "trace", "screenshot"]
---
# Playwright Browser Diagnostics

Use this skill when a task needs accessibility snapshots, screenshots, tracing, or route evidence.

Do not call raw MCP tools directly. Prefer Playwright CLI first unless the Browser Runtime route evidence selects the built-in Playwright MCP adapter for the task.
"#;

pub fn classify_playwright_skill(
    skill: &PlaywrightSkillManifest,
) -> PlaywrightSkillCompatibilityReport {
    let supported = [
        "navigate",
        "click",
        "type",
        "snapshot",
        "screenshot",
        "trace",
    ];
    for capability in &skill.required_capabilities {
        if !supported.contains(&capability.as_str()) {
            return PlaywrightSkillCompatibilityReport {
                name: skill.name.clone(),
                status: PlaywrightSkillCompatibilityStatus::Unavailable,
                reason: Some(format!("unsupported_capability:{capability}")),
            };
        }
    }
    PlaywrightSkillCompatibilityReport {
        name: skill.name.clone(),
        status: PlaywrightSkillCompatibilityStatus::Enabled,
        reason: None,
    }
}

#[cfg(test)]
#[path = "playwright_skills_tests.rs"]
mod playwright_skills_tests;
