# Live Room Automation Douyin Moderator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a browser-powered, room-scoped live-room automation runtime with Douyin as the first adapter and a built-in moderator spec that can reply, learn to gbrain, and execute real moderation actions.

**Architecture:** Add a browser/gbrain capability bridge for automation, then build a platform-neutral live-room contract with a Douyin adapter implemented through restricted page-context scripts. Each installed spec instance targets exactly one platform/room and runs concurrently with isolated browser context, cursor, moderation ledger, gbrain namespace, and rate limits.

**Self-review correction:** This must not become a parallel automation framework. The live-room loop is a specialized executor path inside the existing automation runtime, selected by explicit spec metadata. It creates one run session, one scoped browser context, and one activity row for the live session, then ticks every `poll_interval_seconds` inside that run until stopped. Do not model the 30-second monitor as repeated schedule-triggered automation runs.

**Real-action correction:** The user confirmed first-version room-manager actions default to real execution. The implementation may keep strong verification gates, but `send_reply`, `warn_user`, `mute_user`, and `remove_user` must have an executable path before the feature is considered complete. If a deterministic script cannot verify the target/action, the adapter must fall back to a constrained `browser_task` action and only return success after the page confirms the intended action.

**Tech Stack:** Rust/Tauri v2, chromiumoxide browser tools, existing automation runtime, gbrain MCP, rusqlite, React 18 + Jotai, Vitest, Rust unit tests, uClaw harness fixtures.

---

## File Structure

- Modify `src-tauri/src/automation/runtime/service.rs`: register browser/gbrain tools for automation when the spec declares capabilities.
- Modify `src-tauri/src/automation/runtime/service.rs`: dispatch the live-room executor for specs with `x_uclaw_runtime.kind = live_room_moderator`.
- Create `src-tauri/src/automation/runtime/tool_registry.rs`: focused builder for automation tool registries, split out of `service.rs`.
- Modify `src-tauri/src/automation/permissions.rs`: map new live-room and scoped gbrain tool names to permissions.
- Create `src-tauri/src/browser/script_runner.rs`: restricted `browser_run_script` path validation and execution helper.
- Modify `src-tauri/src/browser/tools.rs`: expose `BrowserRunScriptTool`.
- Create `src-tauri/src/gbrain/scoped.rs`: scoped search/get/put helpers for prefixes formatted as `live/{platform}/{room_id}/`.
- Create `src-tauri/src/automation/live_room/types.rs`: contract DTOs and config types.
- Create `src-tauri/src/automation/live_room/policy.rs`: reply/moderation/rate-limit policy, pure and heavily tested.
- Create `src-tauri/src/automation/live_room/runtime.rs`: long-running tick loop and per-run state keys.
- Create `src-tauri/src/automation/live_room/adapters/mod.rs`: adapter trait and registry.
- Create `src-tauri/src/automation/live_room/adapters/douyin.rs`: Douyin adapter that calls built-in scripts and validates identities.
- Create `src-tauri/resources/live-room/douyin/*.js`: page-context scripts for enter/scan/reply/warn/mute/remove.
- Create `src-tauri/resources/automation-specs/douyin-live-moderator.yaml`: built-in automation spec.
- Modify `src-tauri/src/automation/protocol/humane_v1.rs`: accept config schema select options if current parsing drops them.
- Modify `ui/src/components/automation/SpecSettingsView.tsx`: render platform/room/login fields clearly.
- Modify `ui/src/components/automation/SpecRunSurface.tsx`: show live-room tick and moderation trace.
- Modify `ui/src/components/automation/AutomationHub.tsx`: distinguish concurrent live-room specs by platform and room.
- Add tests beside each changed module; no `tests/` integration directory.

---

### Task 1: Split Automation Tool Registry and Register Browser Tools by Permission

**Files:**
- Create: `src-tauri/src/automation/runtime/tool_registry.rs`
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/automation/runtime/mod.rs`
- Test: `src-tauri/src/automation/runtime/tool_registry.rs`

- [ ] **Step 1: Write failing tests for browser registration**

Add this test module to the new file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    #[test]
    fn browser_tools_are_absent_without_ai_browser_permission() {
        let names = planned_tool_names(&[], false);
        assert!(!names.contains(&"browser_task".to_string()));
        assert!(!names.contains(&"browser_run_script".to_string()));
    }

    #[test]
    fn browser_tools_are_present_with_ai_browser_permission() {
        let names = planned_tool_names(&[Permission::AiBrowser], false);
        assert!(names.contains(&"browser_task".to_string()));
        assert!(names.contains(&"browser_task_resume".to_string()));
        assert!(names.contains(&"retry_with_browser_agent".to_string()));
        assert!(names.contains(&"browser_run_script".to_string()));
    }

    #[test]
    fn scoped_gbrain_tools_are_present_when_gbrain_declared() {
        let names = planned_tool_names(&[Permission::AiBrowser], true);
        assert!(names.contains(&"gbrain_room_search".to_string()));
        assert!(names.contains(&"gbrain_room_get_page".to_string()));
        assert!(names.contains(&"gbrain_room_put_page".to_string()));
        assert!(!names.contains(&"gbrain_search".to_string()));
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::runtime::tool_registry::tests -- --nocapture
```

Expected: fail because `tool_registry.rs` and `planned_tool_names` do not exist.

- [ ] **Step 3: Add the planning helper and module export**

Create `src-tauri/src/automation/runtime/tool_registry.rs`:

```rust
use crate::automation::protocol::humane_v1::Permission;

pub fn planned_tool_names(spec_permissions: &[Permission], gbrain_declared: bool) -> Vec<String> {
    let mut names = vec![
        "read".to_string(),
        "write".to_string(),
        "grep".to_string(),
        "glob".to_string(),
        "web_fetch".to_string(),
        "http_request".to_string(),
        "edit".to_string(),
        "bash".to_string(),
        "report_to_user".to_string(),
        "notify_user".to_string(),
        "request_escalation".to_string(),
        "memory".to_string(),
    ];
    if spec_permissions.contains(&Permission::AiBrowser) {
        names.extend([
            "browser_task",
            "browser_task_resume",
            "retry_with_browser_agent",
            "browser_run_script",
        ].into_iter().map(str::to_string));
    }
    if gbrain_declared {
        names.extend([
            "gbrain_room_search",
            "gbrain_room_get_page",
            "gbrain_room_put_page",
        ].into_iter().map(str::to_string));
    }
    names
}
```

Modify `src-tauri/src/automation/runtime/mod.rs`:

```rust
pub mod tool_registry;
```

- [ ] **Step 4: Refactor `service.rs` registry construction to call the new builder**

Replace the initial contents of `tool_registry.rs` with this builder plus the `planned_tool_names` helper from Step 3:

```rust
use std::{path::PathBuf, sync::Arc};

use crate::agent::tools::{builtin, tool::ToolRegistry};
use crate::automation::protocol::humane_v1::Permission;

pub struct AutomationToolRegistryDeps {
    pub workspace_root: PathBuf,
    pub spec_permissions: Vec<Permission>,
    pub gbrain_declared: bool,
}

pub fn build_base_registry(deps: AutomationToolRegistryDeps) -> Arc<ToolRegistry> {
    let mut tools = ToolRegistry::new();
    register_base_tools(&mut tools, deps.workspace_root);
    Arc::new(tools)
}

pub fn register_base_tools(tools: &mut ToolRegistry, workspace_root: PathBuf) {
    let ws = workspace_root;
    tools.register(builtin::file::ReadFileTool::new(ws.clone()));
    tools.register(builtin::file::WriteFileTool::new(ws.clone()));
    tools.register(builtin::search::GrepTool::new(ws.clone()));
    tools.register(builtin::search::GlobTool::new(ws.clone()));
    tools.register(builtin::web::WebFetchTool::new());
    tools.register(builtin::web::HttpRequestTool::new());
    tools.register(builtin::edit::EditTool::new(ws.clone()));
    tools.register(builtin::shell::BashTool::new(ws));
}
```

Then change `AppRuntimeService::build_automation_tool_registry` in `service.rs` to call `build_base_registry`:

```rust
pub fn build_automation_tool_registry(
    &self,
    workspace_root: &std::path::Path,
) -> Arc<crate::agent::tools::tool::ToolRegistry> {
    crate::automation::runtime::tool_registry::build_base_registry(
        crate::automation::runtime::tool_registry::AutomationToolRegistryDeps {
            workspace_root: workspace_root.to_path_buf(),
            spec_permissions: Vec::new(),
            gbrain_declared: false,
        },
    )
}
```

This task only extracts the base registry. Task 8 replaces the temporary empty `spec_permissions` and `gbrain_declared` values with real spec-aware inputs.

- [ ] **Step 5: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::runtime::tool_registry::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/runtime/tool_registry.rs src-tauri/src/automation/runtime/mod.rs src-tauri/src/automation/runtime/service.rs
git commit -m "refactor(automation): split tool registry planning"
```

---

### Task 2: Add `browser_run_script` with Restricted Paths

**Files:**
- Create: `src-tauri/src/browser/script_runner.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Modify: `src-tauri/src/browser/tools.rs`
- Test: `src-tauri/src/browser/script_runner.rs`

- [ ] **Step 1: Write path policy tests**

Create `src-tauri/src/browser/script_runner.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_builtin_adapter_script() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let resolved = policy.resolve("douyin/scan_comments.js").unwrap();
        assert!(resolved.ends_with("douyin/scan_comments.js"));
    }

    #[test]
    fn allows_workspace_relative_script() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let resolved = policy.resolve("./scripts/live.js").unwrap();
        assert_eq!(resolved, std::path::PathBuf::from("/workspace/scripts/live.js"));
    }

    #[test]
    fn rejects_absolute_path_outside_allowed_roots() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let err = policy.resolve("/tmp/steal.js").unwrap_err().to_string();
        assert!(err.contains("path_not_allowed"));
    }

    #[test]
    fn rejects_non_js_files() {
        let policy = ScriptPathPolicy::new(
            "/app/resources/live-room".into(),
            "/workspace".into(),
            "/Users/example".into(),
        );
        let err = policy.resolve("./scripts/live.txt").unwrap_err().to_string();
        assert!(err.contains("expected_js_file"));
    }
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cd src-tauri && cargo test --lib browser::script_runner::tests -- --nocapture
```

Expected: fail because types are missing.

- [ ] **Step 3: Implement path policy**

Add:

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum BrowserScriptError {
    #[error("expected_js_file")]
    ExpectedJsFile,
    #[error("path_not_allowed: {0}")]
    PathNotAllowed(String),
}

#[derive(Debug, Clone)]
pub struct ScriptPathPolicy {
    builtin_root: PathBuf,
    workspace_root: PathBuf,
    home_dir: PathBuf,
}

impl ScriptPathPolicy {
    pub fn new(builtin_root: PathBuf, workspace_root: PathBuf, home_dir: PathBuf) -> Self {
        Self { builtin_root, workspace_root, home_dir }
    }

    pub fn resolve(&self, file: &str) -> Result<PathBuf, BrowserScriptError> {
        let raw = Path::new(file);
        let resolved = if raw.is_absolute() {
            raw.to_path_buf()
        } else if file.starts_with("douyin/") || file.starts_with("shared/") {
            self.builtin_root.join(file)
        } else {
            self.workspace_root.join(file)
        };
        if resolved.extension().and_then(|e| e.to_str()) != Some("js") {
            return Err(BrowserScriptError::ExpectedJsFile);
        }
        if is_under(&resolved, &self.builtin_root)
            || is_under(&resolved, &self.workspace_root)
            || is_allowed_skill_path(&resolved, &self.workspace_root, &self.home_dir)
        {
            return Ok(resolved);
        }
        Err(BrowserScriptError::PathNotAllowed(resolved.display().to_string()))
    }
}

fn is_under(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn is_allowed_skill_path(path: &Path, workspace_root: &Path, home_dir: &Path) -> bool {
    let text = path.to_string_lossy();
    let marker = "/.claude/skills/";
    let Some(idx) = text.find(marker) else { return false; };
    let root = Path::new(&text[..idx]);
    root == home_dir || workspace_root == root || workspace_root.starts_with(root)
}
```

Modify `src-tauri/src/browser/mod.rs`:

```rust
pub mod script_runner;
```

- [ ] **Step 4: Add tool shell to `browser/tools.rs`**

Add this tool type near the other browser tools:

```rust
pub struct BrowserRunScriptTool {
    pub session_id: String,
    pub workspace_root: std::path::PathBuf,
    pub builtin_root: std::path::PathBuf,
}

#[async_trait]
impl Tool for BrowserRunScriptTool {
    fn name(&self) -> &str { "browser_run_script" }

    fn description(&self) -> &str {
        "Execute an approved JavaScript file in the active browser page context. The file path must be inside the built-in live-room adapter root, the automation workspace, or an allowed skill directory."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": { "type": "string" },
                "params": { "type": "object" },
                "timeout_ms": { "type": "integer", "minimum": 1000, "maximum": 120000 }
            },
            "required": ["file"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let file = params["file"].as_str()
            .ok_or_else(|| ToolError::Execution("file is required".to_string()))?;
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let policy = crate::browser::script_runner::ScriptPathPolicy::new(
            self.builtin_root.clone(),
            self.workspace_root.clone(),
            home,
        );
        let resolved = policy.resolve(file)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::new(
            serde_json::json!({
                "ok": false,
                "error": "browser_run_script_execution_not_connected",
                "resolvedPath": resolved,
                "sessionId": self.session_id,
            }),
            0,
        ))
    }
}
```

This step wires validation and the tool surface only. Task 8 connects it to the automation registry, and the adapter tasks can then replace the temporary `browser_run_script_execution_not_connected` response with page execution.

- [ ] **Step 5: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib browser::script_runner::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/script_runner.rs src-tauri/src/browser/mod.rs src-tauri/src/browser/tools.rs
git commit -m "feat(browser): add restricted script runner tool"
```

---

### Task 3: Add Room-Scoped gbrain Helpers

**Files:**
- Create: `src-tauri/src/gbrain/scoped.rs`
- Modify: `src-tauri/src/gbrain/mod.rs`
- Modify: `src-tauri/src/automation/permissions.rs`
- Test: `src-tauri/src/gbrain/scoped.rs`

- [ ] **Step 1: Write scope tests**

Create tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_prefix_is_platform_and_room_scoped() {
        let scope = GbrainRoomScope::new("douyin", "room-123", KnowledgeScope::RoomOnly).unwrap();
        assert_eq!(scope.room_prefix(), "live/douyin/room-123/");
        assert_eq!(scope.allowed_prefixes(), vec!["live/douyin/room-123/"]);
    }

    #[test]
    fn shared_prefix_requires_explicit_scope() {
        let scope = GbrainRoomScope::new("douyin", "room-123", KnowledgeScope::RoomPlusPlatform).unwrap();
        assert_eq!(
            scope.allowed_prefixes(),
            vec!["live/douyin/room-123/", "live/douyin/shared/"]
        );
    }

    #[test]
    fn rejects_unscoped_page_slug() {
        let scope = GbrainRoomScope::new("douyin", "room-123", KnowledgeScope::RoomOnly).unwrap();
        assert!(scope.validate_slug("projects/private").is_err());
    }
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cd src-tauri && cargo test --lib gbrain::scoped::tests -- --nocapture
```

Expected: fail because module is missing.

- [ ] **Step 3: Implement scoped types**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeScope {
    RoomOnly,
    RoomPlusPlatform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbrainRoomScope {
    pub platform: String,
    pub room_id: String,
    pub knowledge_scope: KnowledgeScope,
}

impl GbrainRoomScope {
    pub fn new(platform: &str, room_id: &str, knowledge_scope: KnowledgeScope) -> Result<Self, String> {
        let platform = sanitize_segment(platform);
        let room_id = sanitize_segment(room_id);
        if platform.is_empty() || room_id.is_empty() {
            return Err("invalid_gbrain_room_scope".to_string());
        }
        Ok(Self { platform, room_id, knowledge_scope })
    }

    pub fn room_prefix(&self) -> String {
        format!("live/{}/{}/", self.platform, self.room_id)
    }

    pub fn allowed_prefixes(&self) -> Vec<String> {
        let mut prefixes = vec![self.room_prefix()];
        if self.knowledge_scope == KnowledgeScope::RoomPlusPlatform {
            prefixes.push(format!("live/{}/shared/", self.platform));
        }
        prefixes
    }

    pub fn validate_slug(&self, slug: &str) -> Result<(), String> {
        if self.allowed_prefixes().iter().any(|prefix| slug.starts_with(prefix)) {
            Ok(())
        } else {
            Err("gbrain_slug_out_of_room_scope".to_string())
        }
    }
}

fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() { Some(ch.to_ascii_lowercase()) }
            else if ch == '-' || ch == '_' { Some('-') }
            else { None }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
```

Modify `src-tauri/src/gbrain/mod.rs`:

```rust
pub mod scoped;
```

- [ ] **Step 4: Map scoped gbrain tools to permission**

Modify `src-tauri/src/automation/permissions.rs`:

```rust
"gbrain_room_search" | "gbrain_room_get_page" | "gbrain_room_put_page" => Some(Permission::AiBrowser),
```

Add this comment immediately above the match arm:

```rust
// Live-room gbrain tools are exposed only with the browser-capable built-in spec.
// A later protocol revision can split this into a dedicated Knowledge permission.
```

- [ ] **Step 5: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib gbrain::scoped::tests automation::permissions::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/gbrain/scoped.rs src-tauri/src/gbrain/mod.rs src-tauri/src/automation/permissions.rs
git commit -m "feat(gbrain): add room scoped knowledge helpers"
```

---

### Task 4: Add Live-Room Contract Types and Moderation Policy

**Files:**
- Create: `src-tauri/src/automation/live_room/mod.rs`
- Create: `src-tauri/src/automation/live_room/types.rs`
- Create: `src-tauri/src/automation/live_room/policy.rs`
- Modify: `src-tauri/src/automation/mod.rs`
- Test: `src-tauri/src/automation/live_room/policy.rs`

- [ ] **Step 1: Write policy tests**

Create `policy.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::live_room::types::*;

    fn comment(author_id: &str, text: &str, at: i64) -> LiveComment {
        LiveComment {
            platform: "douyin".into(),
            platform_comment_id: format!("{author_id}-{at}"),
            author_id: author_id.into(),
            author_name: author_id.into(),
            text: text.into(),
            timestamp_ms: at,
            badges: vec![],
            is_new: true,
        }
    }

    #[test]
    fn repeated_spam_warns_first_then_mutes_after_two_warnings() {
        let mut ledger = ModerationLedger::default();
        let cfg = ModerationConfig::default();
        let comments = vec![
            comment("u1", "buy now", 0),
            comment("u1", "buy now", 10_000),
            comment("u1", "buy now", 20_000),
            comment("u1", "buy now", 30_000),
            comment("u1", "buy now", 40_000),
        ];
        let first = decide_moderation(&cfg, &mut ledger, &comments, 60_000);
        assert_eq!(first.actions[0].kind, ModerationActionKind::Warn);
        let second = decide_moderation(&cfg, &mut ledger, &comments, 90_000);
        assert_eq!(second.actions[0].kind, ModerationActionKind::Warn);
        let third = decide_moderation(&cfg, &mut ledger, &comments, 120_000);
        assert_eq!(third.actions[0].kind, ModerationActionKind::Mute);
    }

    #[test]
    fn whitelisted_users_are_never_punished() {
        let mut ledger = ModerationLedger::default();
        let cfg = ModerationConfig {
            whitelisted_author_ids: vec!["host".into()],
            ..ModerationConfig::default()
        };
        let comments = vec![comment("host", "spam", 0), comment("host", "spam", 1)];
        let decision = decide_moderation(&cfg, &mut ledger, &comments, 60_000);
        assert!(decision.actions.is_empty());
    }
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::policy::tests -- --nocapture
```

Expected: fail because modules/types are missing.

- [ ] **Step 3: Add contract DTOs**

Create `types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LiveComment {
    pub platform: String,
    pub platform_comment_id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    pub timestamp_ms: i64,
    pub badges: Vec<String>,
    pub is_new: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationActionKind {
    Warn,
    Mute,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModerationAction {
    pub kind: ModerationActionKind,
    pub author_id: String,
    pub reason: String,
    pub evidence_comment_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModerationDecision {
    pub actions: Vec<ModerationAction>,
}

#[derive(Debug, Clone)]
pub struct ModerationConfig {
    pub spam_window_seconds: i64,
    pub spam_threshold: usize,
    pub whitelisted_author_ids: Vec<String>,
}

impl Default for ModerationConfig {
    fn default() -> Self {
        Self {
            spam_window_seconds: 60,
            spam_threshold: 5,
            whitelisted_author_ids: Vec::new(),
        }
    }
}
```

- [ ] **Step 4: Implement minimal policy**

Add:

```rust
use std::collections::HashMap;

use super::types::*;

#[derive(Debug, Clone, Default)]
pub struct ModerationLedger {
    warnings: HashMap<String, u32>,
}

pub fn decide_moderation(
    cfg: &ModerationConfig,
    ledger: &mut ModerationLedger,
    comments: &[LiveComment],
    now_ms: i64,
) -> ModerationDecision {
    let mut by_author: HashMap<&str, Vec<&LiveComment>> = HashMap::new();
    let window_start = now_ms - cfg.spam_window_seconds * 1000;
    for comment in comments.iter().filter(|c| c.timestamp_ms >= window_start) {
        if cfg.whitelisted_author_ids.contains(&comment.author_id) {
            continue;
        }
        by_author.entry(&comment.author_id).or_default().push(comment);
    }
    let mut actions = Vec::new();
    for (author_id, items) in by_author {
        if items.len() < cfg.spam_threshold {
            continue;
        }
        let warning_count = ledger.warnings.entry(author_id.to_string()).or_insert(0);
        let kind = if *warning_count >= 2 {
            ModerationActionKind::Mute
        } else {
            *warning_count += 1;
            ModerationActionKind::Warn
        };
        actions.push(ModerationAction {
            kind,
            author_id: author_id.to_string(),
            reason: "spam_repeated".to_string(),
            evidence_comment_ids: items.iter().map(|c| c.platform_comment_id.clone()).collect(),
        });
    }
    ModerationDecision { actions }
}
```

Create `mod.rs`:

```rust
pub mod policy;
pub mod types;
```

Modify `src-tauri/src/automation/mod.rs`:

```rust
pub mod live_room;
```

- [ ] **Step 5: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::policy::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/live_room src-tauri/src/automation/mod.rs
git commit -m "feat(automation): add live room moderation policy"
```

---

### Task 5: Add Live-Room Runtime State Keys for Concurrent Specs

**Files:**
- Create: `src-tauri/src/automation/live_room/runtime.rs`
- Modify: `src-tauri/src/automation/live_room/mod.rs`
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Test: `src-tauri/src/automation/live_room/runtime.rs`

- [ ] **Step 1: Write state-key tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_key_includes_spec_run_platform_and_room() {
        let key = LiveRunKey::new("spec-a", "run-1", "douyin", "room-9");
        assert_eq!(key.to_string(), "spec-a:run-1:douyin:room-9");
    }

    #[test]
    fn different_rooms_do_not_share_keys() {
        let a = LiveRunKey::new("spec-a", "run-1", "douyin", "room-a");
        let b = LiveRunKey::new("spec-a", "run-1", "douyin", "room-b");
        assert_ne!(a, b);
    }

    #[test]
    fn live_runtime_metadata_is_explicit() {
        let spec = serde_json::json!({
            "x_uclaw_runtime": {
                "kind": "live_room_moderator",
                "poll_interval_seconds": 30
            }
        });
        let runtime = LiveRuntimeMetadata::from_spec_json(&spec).unwrap();
        assert_eq!(runtime.kind, "live_room_moderator");
        assert_eq!(runtime.poll_interval_seconds, 30);
    }
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::runtime::tests -- --nocapture
```

Expected: fail.

- [ ] **Step 3: Implement key and state shell**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiveRunKey {
    pub spec_id: String,
    pub run_id: String,
    pub platform: String,
    pub room_id: String,
}

impl LiveRunKey {
    pub fn new(spec_id: &str, run_id: &str, platform: &str, room_id: &str) -> Self {
        Self {
            spec_id: spec_id.to_string(),
            run_id: run_id.to_string(),
            platform: platform.to_string(),
            room_id: room_id.to_string(),
        }
    }
}

impl std::fmt::Display for LiveRunKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}:{}", self.spec_id, self.run_id, self.platform, self.room_id)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LiveRunState {
    pub platform: String,
    pub room_id: String,
    pub host_id: Option<String>,
    pub tab_id: Option<String>,
    pub comment_cursor: Option<String>,
    pub last_tick_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRuntimeMetadata {
    pub kind: String,
    pub poll_interval_seconds: u64,
}

impl LiveRuntimeMetadata {
    pub fn from_spec_json(spec: &serde_json::Value) -> anyhow::Result<Self> {
        let raw = spec
            .get("x_uclaw_runtime")
            .ok_or_else(|| anyhow::anyhow!("x_uclaw_runtime_missing"))?;
        let kind = raw
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let poll_interval_seconds = raw
            .get("poll_interval_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        Ok(Self { kind, poll_interval_seconds })
    }
}
```

Modify `live_room/mod.rs`:

```rust
pub mod runtime;
```

- [ ] **Step 4: Add service dispatch test for live-room specs**

Add a pure helper in `service.rs` and test it beside the existing runtime tests:

```rust
fn automation_executor_kind(spec_json: &serde_json::Value) -> &'static str {
    if spec_json
        .get("x_uclaw_runtime")
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        == Some("live_room_moderator")
    {
        "live_room_moderator"
    } else {
        "agentic_loop"
    }
}
```

Test:

```rust
#[test]
fn live_room_spec_uses_live_room_executor() {
    let spec = serde_json::json!({
        "type": "automation",
        "x_uclaw_runtime": { "kind": "live_room_moderator" }
    });
    assert_eq!(automation_executor_kind(&spec), "live_room_moderator");
}
```

Then branch inside `execute_run` after run-session/activity creation and before the generic `run_agentic_loop` call:

```rust
if automation_executor_kind(&spec_value) == "live_room_moderator" {
    return crate::automation::live_room::runtime::execute_live_room_run(
        self,
        spec_id,
        activity_id,
        session_id,
        spec_value,
        payload,
        workspace_root,
    ).await;
}
```

The executor must reuse the existing activity row, permission set, provider resolution, tool registry, and stop/deactivation semantics. It must not start a separate scheduler or create a second activity ledger.

- [ ] **Step 5: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::runtime::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/live_room/runtime.rs src-tauri/src/automation/live_room/mod.rs src-tauri/src/automation/runtime/service.rs
git commit -m "feat(automation): isolate concurrent live room state"
```

---

### Task 6: Add Adapter Trait and Douyin Fixture Adapter

**Files:**
- Create: `src-tauri/src/automation/live_room/adapters/mod.rs`
- Create: `src-tauri/src/automation/live_room/adapters/douyin.rs`
- Modify: `src-tauri/src/automation/live_room/mod.rs`
- Test: `src-tauri/src/automation/live_room/adapters/douyin.rs`

- [ ] **Step 1: Write fixture adapter tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_fixture_comments() {
        let raw = serde_json::json!({
            "nextCursor": "c2",
            "comments": [
                {"id":"m1","userId":"u1","nickname":"Alice","text":"价格多少","ts":1000}
            ]
        });
        let batch = parse_scan_comments(raw).unwrap();
        assert_eq!(batch.next_cursor.as_deref(), Some("c2"));
        assert_eq!(batch.comments[0].platform, "douyin");
        assert_eq!(batch.comments[0].platform_comment_id, "m1");
        assert_eq!(batch.comments[0].author_id, "u1");
        assert_eq!(batch.comments[0].text, "价格多少");
    }
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::adapters::douyin::tests -- --nocapture
```

Expected: fail.

- [ ] **Step 3: Add adapter trait and parse DTO**

Create `adapters/mod.rs`:

```rust
pub mod douyin;

use async_trait::async_trait;
use super::types::LiveComment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentBatch {
    pub next_cursor: Option<String>,
    pub comments: Vec<LiveComment>,
}

#[async_trait]
pub trait LiveRoomAdapter: Send + Sync {
    fn platform(&self) -> &'static str;
}
```

Create `douyin.rs`:

```rust
use super::CommentBatch;
use crate::automation::live_room::types::LiveComment;

pub fn parse_scan_comments(raw: serde_json::Value) -> Result<CommentBatch, String> {
    let next_cursor = raw.get("nextCursor").and_then(|v| v.as_str()).map(str::to_string);
    let comments = raw
        .get("comments")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "douyin_comments_missing".to_string())?
        .iter()
        .map(|item| LiveComment {
            platform: "douyin".to_string(),
            platform_comment_id: item.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            author_id: item.get("userId").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            author_name: item.get("nickname").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            text: item.get("text").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            timestamp_ms: item.get("ts").and_then(|v| v.as_i64()).unwrap_or_default(),
            badges: Vec::new(),
            is_new: true,
        })
        .collect();
    Ok(CommentBatch { next_cursor, comments })
}
```

Modify `live_room/mod.rs`:

```rust
pub mod adapters;
```

- [ ] **Step 4: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::adapters::douyin::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/live_room/adapters src-tauri/src/automation/live_room/mod.rs
git commit -m "feat(automation): add douyin live room adapter"
```

---

### Task 7: Add Built-In Douyin Scripts and Spec

**Files:**
- Create: `src-tauri/resources/live-room/douyin/scan_comments.js`
- Create: `src-tauri/resources/live-room/douyin/enter_room.js`
- Create: `src-tauri/resources/live-room/douyin/send_reply.js`
- Create: `src-tauri/resources/live-room/douyin/warn_user.js`
- Create: `src-tauri/resources/live-room/douyin/mute_user.js`
- Create: `src-tauri/resources/live-room/douyin/remove_user.js`
- Create: `src-tauri/resources/automation-specs/douyin-live-moderator.yaml`
- Test: `src-tauri/src/automation/live_room/adapters/douyin.rs`

- [ ] **Step 1: Add script shape tests**

In `douyin.rs`, add a test that validates required script files exist relative to `CARGO_MANIFEST_DIR`:

```rust
#[test]
fn built_in_douyin_scripts_exist() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/live-room/douyin");
    for name in ["enter_room.js", "scan_comments.js", "send_reply.js", "warn_user.js", "mute_user.js", "remove_user.js"] {
        assert!(root.join(name).is_file(), "missing script {name}");
    }
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::adapters::douyin::tests::built_in_douyin_scripts_exist -- --nocapture
```

Expected: fail because scripts do not exist.

- [ ] **Step 3: Add script files with stable JSON contracts**

Create `scan_comments.js`:

```javascript
async (params) => {
  const cursor = params.cursor || null
  const nodes = Array.from(document.querySelectorAll('[data-e2e*="chat"], [class*="comment"], [class*="chat"]'))
  const comments = nodes
    .map((node, index) => {
      const text = (node.innerText || '').trim()
      if (!text) return null
      return {
        id: node.getAttribute('data-id') || `${Date.now()}-${index}`,
        userId: node.getAttribute('data-user-id') || node.querySelector('[data-user-id]')?.getAttribute('data-user-id') || `unknown-${index}`,
        nickname: node.querySelector('[class*="name"], [data-e2e*="name"]')?.textContent?.trim() || 'unknown',
        text,
        ts: Date.now()
      }
    })
    .filter(Boolean)
  return { nextCursor: String(comments.length ? comments[comments.length - 1].id : cursor || ''), comments }
}
```

Create `enter_room.js`:

```javascript
async (params) => {
  return {
    ok: true,
    status: 'entered',
    roomId: params.configuredRoomId || location.pathname.replace(/[^a-zA-Z0-9_-]/g, '-').replace(/^-+|-+$/g, '') || 'unknown-room',
    roomTitle: document.title || 'Douyin Live Room',
    hostId: null,
    url: location.href
  }
}
```

Create `send_reply.js` with an executable path:

```javascript
async (params) => {
  const text = String(params.text || '').trim()
  if (!text) return { ok: false, action: 'send_reply', error: 'empty_text' }
  const box = document.querySelector('[contenteditable="true"], textarea, input[type="text"]')
  if (!box) return { ok: false, action: 'send_reply', needsBrowserTask: true, error: 'input_not_found' }
  box.focus()
  if ('value' in box) box.value = text
  else box.textContent = text
  box.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }))
  const send = Array.from(document.querySelectorAll('button, [role="button"]'))
    .find((el) => /发送|send/i.test(el.innerText || el.getAttribute('aria-label') || ''))
  if (!send) return { ok: false, action: 'send_reply', needsBrowserTask: true, error: 'send_button_not_found' }
  send.click()
  return { ok: true, action: 'send_reply', text }
}
```

Create `warn_user.js` as a real public warning action:

```javascript
async (params) => {
  const name = params.authorName || params.authorId || '这位朋友'
  const reason = params.reason || '直播间规则'
  const text = `@${name} 请注意${reason}，请不要继续刷屏或发布不当内容。`
  return await (async (p) => {
    const box = document.querySelector('[contenteditable="true"], textarea, input[type="text"]')
    if (!box) return { ok: false, action: 'warn_user', needsBrowserTask: true, error: 'input_not_found', text }
    box.focus()
    if ('value' in box) box.value = p.text
    else box.textContent = p.text
    box.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: p.text }))
    const send = Array.from(document.querySelectorAll('button, [role="button"]'))
      .find((el) => /发送|send/i.test(el.innerText || el.getAttribute('aria-label') || ''))
    if (!send) return { ok: false, action: 'warn_user', needsBrowserTask: true, error: 'send_button_not_found', text }
    send.click()
    return { ok: true, action: 'warn_user', text }
  })({ text })
}
```

Create `mute_user.js` with verified-user semantics:

```javascript
async (params) => {
  const authorId = params.authorId || null
  const authorName = params.authorName || ''
  const candidates = Array.from(document.querySelectorAll('[data-user-id], [data-id], [class*="comment"], [class*="chat"]'))
  const target = candidates.find((node) => {
    const uid = node.getAttribute('data-user-id') || node.querySelector('[data-user-id]')?.getAttribute('data-user-id')
    const text = node.innerText || ''
    return (authorId && uid === authorId) || (authorName && text.includes(authorName))
  })
  if (!target) return { ok: false, action: 'mute_user', authorId, needsBrowserTask: true, error: 'target_not_found' }
  target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  target.click()
  const action = Array.from(document.querySelectorAll('button, [role="button"], [role="menuitem"]'))
    .find((el) => /禁言|mute/i.test(el.innerText || el.getAttribute('aria-label') || ''))
  if (!action) return { ok: false, action: 'mute_user', authorId, needsBrowserTask: true, error: 'mute_action_not_found' }
  action.click()
  return {
    ok: true,
    action: 'mute_user',
    authorId,
    reason: params.reason || '',
    verifiedAuthorId: authorId
  }
}
```

Create `remove_user.js` with verified-user semantics:

```javascript
async (params) => {
  const authorId = params.authorId || null
  const authorName = params.authorName || ''
  const candidates = Array.from(document.querySelectorAll('[data-user-id], [data-id], [class*="comment"], [class*="chat"]'))
  const target = candidates.find((node) => {
    const uid = node.getAttribute('data-user-id') || node.querySelector('[data-user-id]')?.getAttribute('data-user-id')
    const text = node.innerText || ''
    return (authorId && uid === authorId) || (authorName && text.includes(authorName))
  })
  if (!target) return { ok: false, action: 'remove_user', authorId, needsBrowserTask: true, error: 'target_not_found' }
  target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  target.click()
  const action = Array.from(document.querySelectorAll('button, [role="button"], [role="menuitem"]'))
    .find((el) => /踢出|移出|remove|kick/i.test(el.innerText || el.getAttribute('aria-label') || ''))
  if (!action) return { ok: false, action: 'remove_user', authorId, needsBrowserTask: true, error: 'remove_action_not_found' }
  action.click()
  return {
    ok: true,
    action: 'remove_user',
    authorId,
    reason: params.reason || '',
    verifiedAuthorId: authorId
  }
}
```

When any action script returns `{ needsBrowserTask: true }`, the Douyin adapter must call `browser_task` with a constrained task such as:

```text
In the already opened Douyin live room tab, perform only this moderator action: mute author_id=<id> author_name=<name> for reason=<reason>. Re-read the target identity before clicking. Do not act on any other user. Stop and report action_denied if the target or moderator menu cannot be verified.
```

The adapter returns success only when either the script or fallback reports a verified target and the intended action. It returns `action_denied`, `target_not_verified`, or `insufficient_permissions` otherwise.

- [ ] **Step 4: Add built-in spec YAML**

Create `douyin-live-moderator.yaml` with the config from the design spec, including:

```yaml
type: automation
name: Douyin Live Moderator
version: 0.1.0
author: uClaw Team
description: Room-scoped Douyin live moderator powered by uClaw browser automation and gbrain.
system_prompt: |
  You are the logged-in live room moderator assistant. Operate only in the configured platform and room.
permissions:
  - ai_browser
  - notification
requires:
  mcps:
    - id: gbrain
      reason: Search and update the room-scoped knowledge base.
browser_login:
  - url: https://www.douyin.com/
    label: Douyin
x_uclaw_runtime:
  kind: live_room_moderator
  poll_interval_seconds: 30
  action_mode_default: real
```

- [ ] **Step 5: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::live_room::adapters::douyin::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/resources/live-room/douyin src-tauri/resources/automation-specs/douyin-live-moderator.yaml src-tauri/src/automation/live_room/adapters/douyin.rs
git commit -m "feat(automation): add douyin live room assets"
```

---

### Task 8: Wire Automation Registry to Concrete Browser and Scoped gbrain Tools

**Files:**
- Modify: `src-tauri/src/automation/runtime/tool_registry.rs`
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/browser/tools.rs`
- Test: `src-tauri/src/automation/runtime/tool_registry.rs`

- [ ] **Step 1: Add registry tests for concrete names**

Extend Task 1 tests to instantiate the registry with test deps and assert `list_definitions()` contains:

```rust
assert!(defs.iter().any(|tool| tool.name == "browser_task"));
assert!(defs.iter().any(|tool| tool.name == "browser_run_script"));
assert!(defs.iter().any(|tool| tool.name == "gbrain_room_search"));
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cd src-tauri && cargo test --lib automation::runtime::tool_registry::tests -- --nocapture
```

Expected: fail until concrete registrations are present.

- [ ] **Step 3: Add concrete registration function**

Implement:

```rust
pub fn build_registry_with_capabilities(deps: AutomationToolRegistryDeps) -> Arc<ToolRegistry> {
    let mut tools = ToolRegistry::new();
    register_base_tools(&mut tools, deps.workspace_root.clone());

    if deps.spec_permissions.contains(&Permission::AiBrowser) {
        let builtin_root = deps.workspace_root.join(".uclaw/live-room");
        tools.register(crate::browser::tools::BrowserRunScriptTool {
            session_id: "automation-test-session".to_string(),
            workspace_root: deps.workspace_root.clone(),
            builtin_root,
        });
    }
    if deps.gbrain_declared {
        tools.register(crate::automation::runtime::tool_registry::ScopedGbrainSchemaTool::new("gbrain_room_search"));
        tools.register(crate::automation::runtime::tool_registry::ScopedGbrainSchemaTool::new("gbrain_room_get_page"));
        tools.register(crate::automation::runtime::tool_registry::ScopedGbrainSchemaTool::new("gbrain_room_put_page"));
    }
    Arc::new(tools)
}
```

This uses the helper added in Task 1:

```rust
register_base_tools(&mut tools, deps.workspace_root.clone());
```

Add this schema-only gbrain tool in `tool_registry.rs` so tests can assert tool exposure before the concrete MCP bridge lands:

```rust
pub struct ScopedGbrainSchemaTool {
    name: String,
}

impl ScopedGbrainSchemaTool {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

#[async_trait::async_trait]
impl crate::agent::tools::tool::Tool for ScopedGbrainSchemaTool {
    fn name(&self) -> &str { &self.name }

    fn description(&self) -> &str {
        "Room-scoped gbrain helper. Requires platform and room_id; unscoped access is rejected."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string" },
                "room_id": { "type": "string" },
                "query": { "type": "string" },
                "slug": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["platform", "room_id"]
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        Err(crate::agent::tools::tool::ToolError::Execution(
            format!("{} execution not connected", self.name)
        ))
    }
}
```

After this task, use the same constructors as the interactive path in `src-tauri/src/tauri_commands.rs` for `browser_task`, `browser_task_resume`, and `retry_with_browser_agent`, passing the automation `session_id` as the browser session ID.

- [ ] **Step 4: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib automation::runtime::tool_registry::tests -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/runtime/tool_registry.rs src-tauri/src/automation/runtime/service.rs src-tauri/src/browser/tools.rs
git commit -m "feat(automation): register browser tools for capable specs"
```

---

### Task 9: Add UI for Platform/Room and Concurrent Live Runs

**Files:**
- Modify: `ui/src/components/automation/SpecSettingsView.tsx`
- Modify: `ui/src/components/automation/SpecRunSurface.tsx`
- Modify: `ui/src/components/automation/AutomationHub.tsx`
- Test: `ui/src/components/automation/SpecRunSurface.test.tsx`
- Test: `ui/src/components/automation/AutomationHub.test.tsx`

- [ ] **Step 1: Write UI tests**

Add `SpecRunSurface.test.tsx` assertions:

```tsx
it('renders live room platform and room status', () => {
  render(<SpecRunSurface run={{
    id: 'run-1',
    status: 'running',
    metadata: { livePlatform: 'douyin', roomId: 'room-1', roomTitle: 'Launch Room' },
    latestTick: { commentsSeen: 5, warnings: 1, mutes: 0, removals: 0 },
  } as any} />)
  expect(screen.getByText(/douyin/i)).toBeInTheDocument()
  expect(screen.getByText(/Launch Room/)).toBeInTheDocument()
  expect(screen.getByText(/5/)).toBeInTheDocument()
})
```

Add `AutomationHub.test.tsx` assertions:

```tsx
it('distinguishes concurrent live room specs by platform and room', () => {
  render(<AutomationHub initialSpecs={[
    { id: 'a', name: 'Room A', livePlatform: 'douyin', roomId: 'room-a' },
    { id: 'b', name: 'Room B', livePlatform: 'douyin', roomId: 'room-b' },
  ] as any} />)
  expect(screen.getByText(/room-a/i)).toBeInTheDocument()
  expect(screen.getByText(/room-b/i)).toBeInTheDocument()
})
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cd ui && npm test -- --run SpecRunSurface AutomationHub
```

Expected: fail until props/rendering are added.

- [ ] **Step 3: Implement rendering**

Add a compact metadata row in `SpecRunSurface.tsx`:

```tsx
const live = (run as any).metadata
{live?.livePlatform && (
  <div className="flex items-center gap-2 text-xs text-muted-foreground">
    <span>{live.livePlatform}</span>
    <span>{live.roomTitle ?? live.roomId}</span>
    {run.latestTick && <span>{run.latestTick.commentsSeen} comments</span>}
  </div>
)}
```

Keep styling consistent with existing automation cards and do not add a nested card.

- [ ] **Step 4: Run tests**

Run:

```bash
cd ui && npm test -- --run SpecRunSurface AutomationHub
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/automation/SpecRunSurface.tsx ui/src/components/automation/SpecRunSurface.test.tsx ui/src/components/automation/AutomationHub.tsx ui/src/components/automation/AutomationHub.test.tsx ui/src/components/automation/SpecSettingsView.tsx
git commit -m "feat(ui): show live room automation state"
```

---

### Task 10: Add Harness Fixture and Verification Scorecard

**Files:**
- Create: `src-tauri/src/harness/cases/live_room/douyin-moderator-fixture.json`
- Modify: `src-tauri/src/harness/adapters/mod.rs`
- Create: `src-tauri/src/harness/adapters/live_room.rs`
- Create: `docs/superpowers/reports/live-room-douyin-moderator-scorecard.md`
- Test: `src-tauri/src/harness/adapters/live_room.rs`

- [ ] **Step 1: Write adapter tests**

Create `live_room.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorecard_requires_room_scope_and_moderation_evidence() {
        let trace = LiveRoomHarnessTrace {
            room_entered: true,
            comments_scanned: true,
            separate_concurrent_state: true,
            scoped_gbrain_recall: true,
            scoped_gbrain_write: true,
            mute_after_two_warnings: true,
            severe_remove_enabled: true,
            leaked_auth_material: false,
        };
        assert_eq!(grade_live_room_trace(&trace).verdict, "pass");
    }
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cd src-tauri && cargo test --lib harness::adapters::live_room::tests -- --nocapture
```

Expected: fail.

- [ ] **Step 3: Implement trace grader**

Add:

```rust
#[derive(Debug, Clone, Default)]
pub struct LiveRoomHarnessTrace {
    pub room_entered: bool,
    pub comments_scanned: bool,
    pub separate_concurrent_state: bool,
    pub scoped_gbrain_recall: bool,
    pub scoped_gbrain_write: bool,
    pub mute_after_two_warnings: bool,
    pub severe_remove_enabled: bool,
    pub leaked_auth_material: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRoomGrade {
    pub verdict: &'static str,
}

pub fn grade_live_room_trace(trace: &LiveRoomHarnessTrace) -> LiveRoomGrade {
    let pass = trace.room_entered
        && trace.comments_scanned
        && trace.separate_concurrent_state
        && trace.scoped_gbrain_recall
        && trace.scoped_gbrain_write
        && trace.mute_after_two_warnings
        && trace.severe_remove_enabled
        && !trace.leaked_auth_material;
    LiveRoomGrade { verdict: if pass { "pass" } else { "fail" } }
}
```

Modify `harness/adapters/mod.rs`:

```rust
pub mod live_room;
```

- [ ] **Step 4: Add fixture case and scorecard**

Create `src-tauri/src/harness/fixtures/live_room_douyin_moderator.json`:

```json
{
  "id": "live-room/douyin-moderator-fixture",
  "subject": "browser",
  "title": "Douyin moderator room-scoped fixture",
  "rooms": ["room-a", "room-b"],
  "assertions": [
    "comments scanned incrementally",
    "room-a and room-b cursors remain separate",
    "gbrain recall uses live/douyin/<room_id>/ prefix",
    "two warnings lead to mute",
    "severe violation can remove",
    "no auth material in trace"
  ]
}
```

Create `docs/superpowers/reports/live-room-douyin-moderator-scorecard.md`:

```markdown
# Live Room Douyin Moderator Scorecard

| Assertion | Status |
| --- | --- |
| Comments scanned incrementally | not_run |
| Room A and Room B cursors remain separate | not_run |
| gbrain recall uses `live/douyin/{room_id}/` prefix | not_run |
| gbrain writes use `live/douyin/{room_id}/` prefix | not_run |
| Two warnings lead to mute | not_run |
| Severe violation can remove | not_run |
| Auth material absent from trace | not_run |
```

- [ ] **Step 5: Run tests**

Run:

```bash
cd src-tauri && cargo test --lib harness::adapters::live_room::tests -- --nocapture
git diff --check -- src-tauri/src/harness docs/superpowers/reports
```

Expected: pass and no whitespace errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/harness/adapters/live_room.rs src-tauri/src/harness/adapters/mod.rs src-tauri/src/harness/cases/live_room/douyin-moderator-fixture.json docs/superpowers/reports/live-room-douyin-moderator-scorecard.md
git commit -m "test(harness): add live room moderator fixture"
```

---

## Final Verification

Run the focused backend tests:

```bash
cd src-tauri && cargo test --lib automation::live_room gbrain::scoped browser::script_runner harness::adapters::live_room -- --nocapture
```

Run the focused frontend tests:

```bash
cd ui && npm test -- --run SpecRunSurface AutomationHub
```

Run type/build checks:

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected:

- Focused Rust tests pass.
- Focused Vitest tests pass.
- `cargo build` prints no Rust errors.
- `tsc --noEmit` prints no TypeScript errors.

Do not claim real Douyin production readiness until a controlled-room smoke has run with a moderator account and the scripts have been validated against the live DOM/API.
