// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::fs;
use serde::Deserialize;
use globset::{Glob, GlobSetBuilder};
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
}
