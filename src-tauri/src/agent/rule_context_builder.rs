// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::fs;
use serde::Deserialize;
use globset::{Glob, GlobSetBuilder};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    pub glob: String,
    pub instructions: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RulesConfig {
    pub rules: Vec<Rule>,
}

pub struct RuleContextBuilder;

impl RuleContextBuilder {
    /// Dynamically scans rule files and builds a consolidated, matched rule prompt block.
    pub fn build_context(workspace_root: &Path, active_files: &[PathBuf]) -> String {
        let mut instructions = Vec::new();

        // 1. Scan and parse .uclawrules YAML if it exists
        let uclawrules_path = workspace_root.join(".uclawrules");
        if uclawrules_path.exists() {
            if let Ok(content) = fs::read_to_string(&uclawrules_path) {
                match serde_yml::from_str::<RulesConfig>(&content) {
                    Ok(config) => {
                        let mut builder = GlobSetBuilder::new();
                        let mut valid_rules = Vec::new();
                        for rule in config.rules {
                            if let Ok(g) = Glob::new(&rule.glob) {
                                builder.add(g);
                                valid_rules.push(rule);
                            }
                        }
                        if let Ok(set) = builder.build() {
                            for active in active_files {
                                if let Ok(rel_path) = active.strip_prefix(workspace_root) {
                                    let matches = set.matches(rel_path);
                                    for idx in matches {
                                        let rule_inst = &valid_rules[idx].instructions;
                                        if !instructions.contains(&rule_inst.trim().to_string()) {
                                            instructions.push(rule_inst.trim().to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse .uclawrules YAML: {}", e);
                    }
                }
            }
        }

        // 2. Scan and extract Markdown sections from .cursorrules or AGENTS.md
        let markdown_files = vec![".cursorrules", "AGENTS.md"];
        for file in markdown_files {
            let path = workspace_root.join(file);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    let extracted = extract_relevant_sections(&content);
                    if !extracted.is_empty() {
                        instructions.push(extracted);
                    }
                }
            }
        }

        if instructions.is_empty() {
            return String::new();
        }

        let mut block = String::new();
        block.push_str("\n\n### ACTIVE PROJECT RULES\n");
        block.push_str("The following project-specific rules match the current files or symbols and MUST be followed strictly:\n\n");
        for inst in instructions {
            block.push_str(&inst);
            block.push_str("\n\n");
        }
        block
    }

    /// Context continuity P3 (Decision 9) — the file-independent half of the
    /// project rules: `AGENTS.md` / `.cursorrules` rule sections only, in the
    /// same `### ACTIVE PROJECT RULES` block format as [`Self::build_context`].
    /// Frozen into the session frame. Empty when there are no sections.
    pub fn build_static_context(workspace_root: &Path) -> String {
        Self::build_context_from(Vec::new(), workspace_root)
    }

    /// Context continuity P3 (Decision 9) — `.uclawrules` glob rules matched
    /// against `files` (this session's file working set). Returns
    /// `(rule_id, instructions)` in `.uclawrules` order, deduped by
    /// instructions; `rule_id` = first 12 hex chars of sha256(instructions).
    /// Independent of the order of `files`. Absolute paths are made relative
    /// to `workspace_root`; paths outside it are ignored.
    pub fn glob_matched_rules(workspace_root: &Path, files: &[PathBuf]) -> Vec<(String, String)> {
        let Ok(content) = fs::read_to_string(workspace_root.join(".uclawrules")) else {
            return Vec::new();
        };
        let config = match serde_yml::from_str::<RulesConfig>(&content) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to parse .uclawrules YAML: {}", e);
                return Vec::new();
            }
        };
        let rel_files: Vec<&Path> = files
            .iter()
            .filter_map(|f| {
                if f.is_absolute() {
                    f.strip_prefix(workspace_root).ok()
                } else {
                    Some(f.as_path())
                }
            })
            .collect();
        let mut out: Vec<(String, String)> = Vec::new();
        for rule in config.rules {
            let Ok(glob) = Glob::new(&rule.glob) else { continue };
            let matcher = glob.compile_matcher();
            if !rel_files.iter().any(|f| matcher.is_match(f)) {
                continue;
            }
            let inst = rule.instructions.trim().to_string();
            if inst.is_empty() || out.iter().any(|(_, i)| *i == inst) {
                continue;
            }
            let id = hex::encode(Sha256::digest(inst.as_bytes()))[..12].to_string();
            out.push((id, inst));
        }
        out
    }

    /// Context continuity P3 — sha256 (lowercase hex) over the contents of
    /// the static rule source files (`uclaw.md`, `.cursorrules`, `AGENTS.md`,
    /// in that order; missing = empty) — exactly what `<project_rules_update>`
    /// re-sends. `.uclawrules` is excluded: its glob rules go per turn as
    /// `<project_rule>` blocks, so editing it must not re-announce the rest.
    pub fn rules_source_hash(workspace_root: &Path) -> String {
        let mut h = Sha256::new();
        for name in ["uclaw.md", ".cursorrules", "AGENTS.md"] {
            let content = fs::read(workspace_root.join(name)).unwrap_or_default();
            h.update(name.as_bytes());
            h.update([0u8]);
            h.update((content.len() as u64).to_le_bytes());
            h.update(&content);
        }
        hex::encode(h.finalize())
    }

    /// Markdown-section half of [`Self::build_context`], appended after
    /// `instructions` (already-matched glob rules), rendered as the block.
    fn build_context_from(mut instructions: Vec<String>, workspace_root: &Path) -> String {
        for file in [".cursorrules", "AGENTS.md"] {
            if let Ok(content) = fs::read_to_string(workspace_root.join(file)) {
                let extracted = extract_relevant_sections(&content);
                if !extracted.is_empty() {
                    instructions.push(extracted);
                }
            }
        }
        render_rules_block(instructions)
    }
}

fn render_rules_block(instructions: Vec<String>) -> String {
    if instructions.is_empty() {
        return String::new();
    }
    let mut block = String::new();
    block.push_str("\n\n### ACTIVE PROJECT RULES\n");
    block.push_str("The following project-specific rules match the current files or symbols and MUST be followed strictly:\n\n");
    for inst in instructions {
        block.push_str(&inst);
        block.push_str("\n\n");
    }
    block
}

/// Helper to parse Markdown content and extract sections with headers containing key rule words
fn extract_relevant_sections(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut extracted = Vec::new();
    let mut i = 0;

    let keywords = ["critical", "always", "never", "rule", "contract", "instruction", "intelligence"];

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if (trimmed.starts_with("## ") || trimmed.starts_with("### ")) && is_relevant_header(trimmed, &keywords) {
            let header_level = if trimmed.starts_with("### ") { 3 } else { 2 };
            extracted.push(line);
            i += 1;

            while i < lines.len() {
                let next_line = lines[i];
                let next_trimmed = next_line.trim();

                // Stop if we hit a header of equal or higher level (less or equal number of hash characters)
                if next_trimmed.starts_with("#") {
                    let next_level = next_trimmed.chars().take_while(|&c| c == '#').count();
                    if next_level <= header_level {
                        break;
                    }
                }

                extracted.push(next_line);
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    extracted.join("\n").trim().to_string()
}

fn is_relevant_header(header: &str, keywords: &[&str]) -> bool {
    let lower = header.to_lowercase();
    keywords.iter().any(|&kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_relevant_sections() {
        let md = "
# Intro
Some text here.

## Critical rules (inlined for Cursor)
- Do not write to memory_graph
- Use uclaw_utils_home

## Other section
Unimportant stuff.
";
        let ext = extract_relevant_sections(md);
        assert!(ext.contains("Critical rules"));
        assert!(ext.contains("Do not write to memory_graph"));
        assert!(!ext.contains("Other section"));
    }

    const RULES_YAML: &str = "rules:\n  - glob: \"src/**/*.rs\"\n    instructions: \"Rust rule\"\n  - glob: \"ui/**/*.tsx\"\n    instructions: \"TSX rule\"\n  - glob: \"**/*.md\"\n    instructions: \"Docs rule\"\n  - glob: \"src/**\"\n    instructions: \"Rust rule\"\n";

    #[test]
    fn glob_matched_rules_are_deterministic_and_session_scoped() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join(".uclawrules"), RULES_YAML).unwrap();
        let a = vec![
            root.join("README.md"),
            PathBuf::from("ui/app/x.tsx"),
            root.join("src/agent/mod.rs"),
            PathBuf::from("/elsewhere/src/z.rs"),
        ];
        let mut b = a.clone();
        b.reverse();
        let ra = RuleContextBuilder::glob_matched_rules(root, &a);
        let rb = RuleContextBuilder::glob_matched_rules(root, &b);
        assert_eq!(ra, rb, "file order must not matter");
        let insts: Vec<&str> = ra.iter().map(|(_, i)| i.as_str()).collect();
        assert_eq!(insts, vec!["Rust rule", "TSX rule", "Docs rule"], ".uclawrules order, deduped");
        for (id, inst) in &ra {
            assert_eq!(id.len(), 12);
            assert_eq!(*id, hex::encode(Sha256::digest(inst.as_bytes()))[..12].to_string());
        }
        // Session-scoped: only files of THIS working set match.
        let only_docs = RuleContextBuilder::glob_matched_rules(root, &[root.join("docs/a.md")]);
        assert_eq!(only_docs.len(), 1);
        assert_eq!(only_docs[0].1, "Docs rule");
        assert!(RuleContextBuilder::glob_matched_rules(root, &[]).is_empty());
    }

    #[test]
    fn rules_source_hash_changes_when_agents_md_changes() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let empty = RuleContextBuilder::rules_source_hash(root);
        assert_eq!(empty, RuleContextBuilder::rules_source_hash(root), "stable");
        fs::write(root.join("AGENTS.md"), "## Critical rules\n- a\n").unwrap();
        let h1 = RuleContextBuilder::rules_source_hash(root);
        assert_ne!(empty, h1);
        fs::write(root.join("AGENTS.md"), "## Critical rules\n- b\n").unwrap();
        let h2 = RuleContextBuilder::rules_source_hash(root);
        assert_ne!(h1, h2);
        fs::write(root.join("uclaw.md"), "x").unwrap();
        assert_ne!(h2, RuleContextBuilder::rules_source_hash(root));
    }

    #[test]
    fn rules_source_hash_ignores_uclawrules() {
        // Glob rules ride the per-turn `<project_rule>` blocks; editing them
        // must not re-announce the static rules (`<project_rules_update>`).
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("uclaw.md"), "x").unwrap();
        let before = RuleContextBuilder::rules_source_hash(root);
        fs::write(root.join(".uclawrules"), RULES_YAML).unwrap();
        assert_eq!(before, RuleContextBuilder::rules_source_hash(root));
        fs::write(root.join(".cursorrules"), "## Critical rules\n- c\n").unwrap();
        assert_ne!(before, RuleContextBuilder::rules_source_hash(root), "static sources still count");
    }

    #[test]
    fn build_static_context_has_only_markdown_sections() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join(".uclawrules"), RULES_YAML).unwrap();
        fs::write(root.join("AGENTS.md"), "## Critical rules\n- never X\n\n## Other\nno\n").unwrap();
        let s = RuleContextBuilder::build_static_context(root);
        assert!(s.starts_with("\n\n### ACTIVE PROJECT RULES\n"));
        assert!(s.contains("never X"));
        assert!(!s.contains("Rust rule"));
        // Same format as build_context without active files.
        assert_eq!(s, RuleContextBuilder::build_context(root, &[]));
        assert_eq!(RuleContextBuilder::build_static_context(&root.join("missing")), "");
    }
}
