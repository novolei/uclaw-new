//! Single, sync, `&AppState`-free construction source for the 9 core workspace
//! tools. Both the main-session descriptor builders (`builtin_descriptors`) and
//! the AgentTeamOrchestrator delegate closure (`tauri_commands`) build core tools
//! through here, so per-tool config wiring lives in exactly one place.
//!
//! Async/stateful tools (browser/skill/memu/plugins) are NOT here — they need
//! `&AppState`/async and stay inline in `registry_build` / the team closure.

use std::path::Path;

use crate::agent::tools::builtin;
use crate::agent::tools::builtin::edit_verify::ProjectCheckCfg;
use crate::agent::tools::tool::ToolRegistry;

/// The one per-tool config seam. Add a new per-tool knob here + apply it in the
/// matching constructor below — nowhere else.
#[derive(Debug, Clone)]
pub struct ToolConfig {
    /// `Some` enables the best-effort post-edit project check on `EditTool`.
    pub edit_project_check: Option<ProjectCheckCfg>,
    /// `read_file` truncation cap (floor-clamped to 1000 inside the tool).
    pub read_file_max_chars: usize,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            edit_project_check: None,
            read_file_max_chars: builtin::file::MAX_READ_CHARS,
        }
    }
}

// ── Per-tool constructors — config wiring lives ONLY here ──────────────────

pub fn read_file_tool(ws: &Path, cfg: &ToolConfig) -> builtin::file::ReadFileTool {
    builtin::file::ReadFileTool::new(ws.to_path_buf()).with_max_read_chars(cfg.read_file_max_chars)
}

pub fn edit_tool(ws: &Path, cfg: &ToolConfig) -> builtin::edit::EditTool {
    let mut t = builtin::edit::EditTool::new(ws.to_path_buf());
    if let Some(pc) = &cfg.edit_project_check {
        t = t.with_project_check(true, pc.timeout_secs);
    }
    t
}

pub fn write_file_tool(ws: &Path) -> builtin::file::WriteFileTool {
    builtin::file::WriteFileTool::new(ws.to_path_buf())
}
pub fn get_file_skeleton_tool(ws: &Path) -> builtin::get_file_skeleton::GetFileSkeletonTool {
    builtin::get_file_skeleton::GetFileSkeletonTool::new(ws.to_path_buf())
}
pub fn grep_tool(ws: &Path) -> builtin::search::GrepTool {
    builtin::search::GrepTool::new(ws.to_path_buf())
}
pub fn glob_tool(ws: &Path) -> builtin::search::GlobTool {
    builtin::search::GlobTool::new(ws.to_path_buf())
}
pub fn web_fetch_tool() -> builtin::web::WebFetchTool {
    builtin::web::WebFetchTool::new()
}
pub fn http_request_tool() -> builtin::web::HttpRequestTool {
    builtin::web::HttpRequestTool::new()
}
pub fn bash_tool(ws: &Path) -> builtin::shell::BashTool {
    builtin::shell::BashTool::new(ws.to_path_buf())
}

/// Register all 9 core workspace tools, with config applied. One-line for callers.
pub fn register_core_tools(reg: &mut ToolRegistry, ws: &Path, cfg: &ToolConfig) {
    reg.register(read_file_tool(ws, cfg));
    reg.register(write_file_tool(ws));
    reg.register(get_file_skeleton_tool(ws));
    reg.register(grep_tool(ws));
    reg.register(glob_tool(ws));
    reg.register(web_fetch_tool());
    reg.register(http_request_tool());
    reg.register(edit_tool(ws, cfg));
    reg.register(bash_tool(ws));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tool::Tool;
    use std::path::PathBuf;

    fn ws() -> PathBuf { PathBuf::from("/tmp/__core_tools_test__") }

    #[test]
    fn register_core_tools_registers_all_nine() {
        let mut reg = ToolRegistry::new();
        register_core_tools(&mut reg, &ws(), &ToolConfig::default());
        for name in [
            "read_file", "write_file", "get_file_skeleton", "grep", "glob",
            "web_fetch", "http_request", "edit", "bash",
        ] {
            assert!(reg.get(name).is_some(), "missing core tool: {name}");
        }
        assert_eq!(reg.list_definitions().len(), 9);
    }

    #[test]
    fn tool_config_default_is_off_and_baseline_cap() {
        let cfg = ToolConfig::default();
        assert!(cfg.edit_project_check.is_none());
        assert_eq!(cfg.read_file_max_chars, builtin::file::MAX_READ_CHARS);
    }

    #[test]
    fn read_file_tool_constructs_with_cap() {
        let cfg = ToolConfig { edit_project_check: None, read_file_max_chars: 0 };
        let t = read_file_tool(&ws(), &cfg);
        assert_eq!(t.name(), "read_file");
    }

    #[test]
    fn edit_tool_constructs_with_project_check() {
        let cfg = ToolConfig {
            edit_project_check: Some(ProjectCheckCfg { timeout_secs: 7 }),
            read_file_max_chars: builtin::file::MAX_READ_CHARS,
        };
        let t = edit_tool(&ws(), &cfg);
        assert_eq!(t.name(), "edit");
    }
}
