//! jcode-inspired tool family metadata for the Capability Mesh.
//!
//! This module is registry metadata only. It does not change tool execution,
//! safety policy, or dispatcher routing.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolFamilyCard {
    pub family_id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub source_reference: &'static str,
    pub tool_ids: &'static [&'static str],
    pub capability_tags: &'static [&'static str],
    pub policy_tags: &'static [&'static str],
    pub event_profile: &'static [&'static str],
    pub harness_subject: &'static str,
    pub cost_tier: &'static str,
    pub reliability_tier: &'static str,
    pub execution_status: &'static str,
    pub requires_permission: bool,
}

pub const JCODE_INSPIRED_TOOL_FAMILY_CARDS: &[ToolFamilyCard] = &[
    ToolFamilyCard {
        family_id: "filesystem.search",
        title: "Filesystem Search",
        summary: "Search file names and file contents with deterministic, capped output.",
        source_reference: "jcode/src/tool/{grep.rs,glob.rs,agentgrep.rs}",
        tool_ids: &["search"],
        capability_tags: &["filesystem", "search", "grep", "glob", "context"],
        policy_tags: &["read_only", "path_policy"],
        event_profile: &["tool_call", "tool_result"],
        harness_subject: "tools.search",
        cost_tier: "local_low",
        reliability_tier: "stable",
        execution_status: "active",
        requires_permission: false,
    },
    ToolFamilyCard {
        family_id: "filesystem.read",
        title: "Filesystem Read",
        summary: "Read bounded workspace files and surface previewable artifacts.",
        source_reference: "jcode/src/tool/read.rs",
        tool_ids: &["file"],
        capability_tags: &["filesystem", "read", "artifact"],
        policy_tags: &["read_only", "path_policy"],
        event_profile: &["tool_call", "tool_result", "artifact_ref"],
        harness_subject: "tools.read",
        cost_tier: "local_low",
        reliability_tier: "stable",
        execution_status: "active",
        requires_permission: false,
    },
    ToolFamilyCard {
        family_id: "filesystem.write",
        title: "Filesystem Write",
        summary: "Create or overwrite workspace files behind preview and path policy.",
        source_reference: "jcode/src/tool/write.rs",
        tool_ids: &["file"],
        capability_tags: &["filesystem", "write", "artifact"],
        policy_tags: &["permission.required", "path_policy", "preview"],
        event_profile: &["permission_requested", "tool_call", "tool_result", "artifact_ref"],
        harness_subject: "tools.write",
        cost_tier: "local_low",
        reliability_tier: "guarded",
        execution_status: "active",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "filesystem.patch",
        title: "Filesystem Patch",
        summary: "Apply surgical edits and patch-like file changes with preview support.",
        source_reference: "jcode/src/tool/{edit.rs,patch.rs,apply_patch.rs,multiedit.rs}",
        tool_ids: &["edit"],
        capability_tags: &["filesystem", "patch", "edit", "diff"],
        policy_tags: &["permission.required", "path_policy", "preview"],
        event_profile: &["permission_requested", "tool_call", "tool_result", "artifact_ref"],
        harness_subject: "tools.patch",
        cost_tier: "local_low",
        reliability_tier: "guarded",
        execution_status: "active",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "shell.command",
        title: "Shell Command",
        summary: "Run bounded shell commands in the workspace sandbox.",
        source_reference: "jcode/src/tool/bash.rs",
        tool_ids: &["shell"],
        capability_tags: &["filesystem", "shell", "process"],
        policy_tags: &["permission.required", "path_policy", "sandbox"],
        event_profile: &["permission_requested", "tool_call", "tool_result"],
        harness_subject: "tools.shell",
        cost_tier: "local_medium",
        reliability_tier: "guarded",
        execution_status: "active",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "runtime.background",
        title: "Background Work",
        summary: "Expose background-capable shell/process semantics for future progress and checkpoint protocol.",
        source_reference: "jcode/src/tool/{bash.rs,bg.rs,batch.rs}",
        tool_ids: &[],
        capability_tags: &["runtime", "background", "progress", "checkpoint"],
        policy_tags: &["permission.required", "human_boundary", "cancelable"],
        event_profile: &["boundary_yield", "checkpoint", "tool_result"],
        harness_subject: "tools.background",
        cost_tier: "local_medium",
        reliability_tier: "planned",
        execution_status: "planned",
        requires_permission: true,
    },
    ToolFamilyCard {
        family_id: "context.session_search",
        title: "Session Search",
        summary: "Future Context Fabric access to prior task/session traces without adding a second memory store.",
        source_reference: "jcode/src/tool/{session_search.rs,conversation_search.rs}",
        tool_ids: &[],
        capability_tags: &["context", "session_search", "conversation_search", "trace"],
        policy_tags: &["read_only", "gbrain_primary", "no_memory_graph_write"],
        event_profile: &["context_access", "memory_recall"],
        harness_subject: "tools.session_search",
        cost_tier: "local_low",
        reliability_tier: "planned",
        execution_status: "planned",
        requires_permission: false,
    },
];

pub fn jcode_inspired_tool_family_cards() -> &'static [ToolFamilyCard] {
    JCODE_INSPIRED_TOOL_FAMILY_CARDS
}

pub fn tool_family_card(family_id: &str) -> Option<&'static ToolFamilyCard> {
    JCODE_INSPIRED_TOOL_FAMILY_CARDS
        .iter()
        .find(|card| card.family_id == family_id)
}

pub fn registry_tags_for_tool(tool_id: &str) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    for card in JCODE_INSPIRED_TOOL_FAMILY_CARDS {
        if card.execution_status != "active" {
            continue;
        }
        if !card.tool_ids.iter().any(|id| *id == tool_id) {
            continue;
        }
        tags.insert(format!("family:{}", card.family_id), "1".to_string());
        tags.insert(format!("harness:{}", card.harness_subject), "1".to_string());
        tags.insert(format!("cost:{}", card.cost_tier), "1".to_string());
        tags.insert(
            format!("reliability:{}", card.reliability_tier),
            "1".to_string(),
        );
        for tag in card.capability_tags {
            tags.insert(format!("tag:{tag}"), "1".to_string());
        }
        for tag in card.policy_tags {
            tags.insert(format!("policy:{tag}"), "1".to_string());
        }
        for event in card.event_profile {
            tags.insert(format!("event:{event}"), "1".to_string());
        }
    }
    tags
}

#[cfg(test)]
#[path = "tool_families_tests.rs"]
mod tests;
