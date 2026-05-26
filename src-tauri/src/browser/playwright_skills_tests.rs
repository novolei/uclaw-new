use std::path::Path;

use super::*;

#[test]
fn compatible_skill_is_enabled() {
    let skill = PlaywrightSkillManifest {
        name: "playwright-navigate".to_string(),
        source_version: "1.0.0".to_string(),
        required_capabilities: vec!["navigate".to_string()],
        hash: "abc".to_string(),
    };

    let report = classify_playwright_skill(&skill);
    assert_eq!(report.status, PlaywrightSkillCompatibilityStatus::Enabled);
}

#[test]
fn raw_shell_skill_is_unavailable() {
    let skill = PlaywrightSkillManifest {
        name: "playwright-raw-shell".to_string(),
        source_version: "1.0.0".to_string(),
        required_capabilities: vec!["raw_shell".to_string()],
        hash: "abc".to_string(),
    };

    let report = classify_playwright_skill(&skill);
    assert_eq!(
        report.status,
        PlaywrightSkillCompatibilityStatus::Unavailable
    );
    assert_eq!(
        report.reason.as_deref(),
        Some("unsupported_capability:raw_shell")
    );
}

#[test]
fn managed_dir_uses_builtin_skills_tree() {
    let dir = managed_playwright_skills_dir(Path::new("/tmp/uclaw"));

    assert_eq!(
        dir,
        Path::new("/tmp/uclaw").join("builtin-skills/playwright-cli")
    );
}

#[test]
fn managed_path_detection_requires_builtin_parent() {
    assert!(is_managed_playwright_skill_path(Path::new(
        "/tmp/uclaw/builtin-skills/playwright-cli/navigate/SKILL.md"
    )));
    assert!(!is_managed_playwright_skill_path(Path::new(
        "/tmp/uclaw/skills/playwright-cli/navigate/SKILL.md"
    )));
}

#[test]
fn ensure_managed_skills_seeds_adapter_guardrail_skills() {
    let tmp = tempfile::TempDir::new().unwrap();

    let root = ensure_managed_playwright_skills(tmp.path()).unwrap();

    let automation =
        std::fs::read_to_string(root.join("playwright-browser-automation/SKILL.md")).unwrap();
    assert!(automation.contains("playwright-browser-automation"));
    assert!(automation.contains("Browser Runtime Adapter"));
    assert!(automation.contains("Do not run arbitrary Playwright shell commands"));
}
