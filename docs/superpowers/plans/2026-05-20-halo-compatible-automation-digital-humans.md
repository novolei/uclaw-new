# Halo-Compatible Automation Digital Humans Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Halo-compatible digital-human automation in uClaw by adding a built-in app bundle loader, a real `ai-browser` facade, Halo-style runtime/IM/permission semantics, and a Halo-like automation frontend without breaking existing uClaw automation, live-room, marketplace, or gbrain behavior.

**Architecture:** Keep `automation_specs`, `AppRuntimeService`, `BrowserContextManager`, `channels`, and `live_room` as uClaw canonical layers. Add additive compatibility adapters around them: manifest-driven built-ins, installed-app metadata, `.claude/skills` sync, `browser_run` execution, app-specific IM sessions, strict permission resolution, and frontend reshaping.

**Tech Stack:** Rust/Tauri backend, rusqlite migrations, existing uClaw browser CDP context, React/Jotai frontend, Vitest/Testing Library, Rust unit tests.

---

## File Structure

- Create `src-tauri/src/automation/builtin_apps.rs`: manifest-driven built-in app loader and skill sync.
- Modify `src-tauri/src/automation/mod.rs`: export `builtin_apps`.
- Modify `src-tauri/src/automation/runtime/service.rs`: remove embedded seed constants, call loader, carry browser context manager, build run-scoped browser facade tools.
- Modify `src-tauri/src/automation/runtime/tool_registry.rs`: register Halo-compatible tool aliases and notify tools with permission checks.
- Modify `src-tauri/src/browser/tools.rs`: make `browser_run_script` execute scripts and add `browser_run` alias.
- Modify `src-tauri/src/browser/context.rs`: add page-context evaluation helper with JSON params and timeout.
- Modify `src-tauri/src/browser/script_runner.rs`: extend allowlist to `.claude/skills` materialized automation skills.
- Create `src-tauri/src/automation/compat.rs`: Halo-compatible status, overrides, permission resolver, login notice helpers.
- Modify `src-tauri/src/db/migrations.rs`: add additive app metadata columns or side table.
- Modify `src-tauri/src/channels/dispatcher.rs`: use app-specific IM identity keys and preserve existing trigger phrase path.
- Modify `src-tauri/src/automation/runtime/chat_sessions.rs`: align identity handling with `app-chat:{specId}:{channelType}:{chatId}`.
- Modify `src-tauri/src/app.rs`: construct and inject `BrowserContextManager` before `AppRuntimeService`.
- Create `src-tauri/resources/builtin-automations/manifest.json`: canonical built-in digital-human manifest.
- Move/copy existing built-ins to `src-tauri/resources/builtin-automations/<id>/spec.yaml` and skills folders.
- Modify `ui/src/components/automation/AutomationHub.tsx`: Halo-like digital-human shell.
- Modify `ui/src/components/automation/SpecRunSurface.tsx`: Chat/Dynamic/Settings tabs.
- Modify `ui/src/components/automation/SpecSettingsView.tsx`: Halo-style settings sections and YAML toggle.
- Modify `ui/src/lib/tauri-bridge.ts`: add installed-app metadata commands and mock bridge coverage.
- Add tests under `ui/src/components/automation/*.test.tsx` and Rust module tests near changed modules.

## 保全 Rules

- Do not delete or rename `automation_specs`.
- Do not remove the existing Douyin live-room executor.
- Do not expose full gbrain globally to live-room specs; all knowledge reads/writes remain platform/room scoped.
- Do not grant AI browser, shell, email, or IM-send to external IM guest chat unless an installed app explicitly resolves that permission.
- Do not store passwords or login credentials in spec rows.
- Do not make built-in refresh overwrite user config, enabled status, granted/denied permissions, uninstalled state, or run history.

### Task 1: Add Additive Halo-Compatible Metadata

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Create: `src-tauri/src/automation/compat.rs`
- Modify: `src-tauri/src/automation/mod.rs`
- Test: Rust migration/compat unit tests in `compat.rs`

- [ ] **Step 1: Write compatibility type tests**

Add tests that prove status and permission resolution are deterministic:

```rust
#[test]
fn resolve_permission_denied_wins() {
    let row = PermissionResolutionInput {
        spec_permissions: vec!["ai-browser".into()],
        granted: vec!["ai-browser".into()],
        denied: vec!["ai-browser".into()],
        default_allowed: false,
    };
    assert!(!resolve_automation_permission("ai-browser", &row));
}

#[test]
fn resolve_permission_spec_default_allows_declared_permission() {
    let row = PermissionResolutionInput {
        spec_permissions: vec!["ai-browser".into()],
        granted: vec![],
        denied: vec![],
        default_allowed: false,
    };
    assert!(resolve_automation_permission("ai-browser", &row));
}

#[test]
fn app_status_maps_enabled_to_active_or_paused() {
    assert_eq!(AutomationAppStatus::from_enabled_error(true, None), AutomationAppStatus::Active);
    assert_eq!(AutomationAppStatus::from_enabled_error(false, None), AutomationAppStatus::Paused);
    assert_eq!(
        AutomationAppStatus::from_enabled_error(true, Some("login required")),
        AutomationAppStatus::NeedsLogin
    );
}
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml automation::compat --lib`

Expected before implementation: compile failure because `compat` and the types do not exist.

- [ ] **Step 3: Implement `compat.rs`**

Create:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationAppStatus {
    Active,
    Paused,
    Error,
    NeedsLogin,
    WaitingUser,
    Uninstalled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutomationUserOverrides {
    pub frequency: Option<String>,
    pub notification_level: Option<String>,
    pub model_source_id: Option<String>,
    pub model_id: Option<String>,
    pub login_notice_dismissed: bool,
}

#[derive(Debug, Clone)]
pub struct PermissionResolutionInput {
    pub spec_permissions: Vec<String>,
    pub granted: Vec<String>,
    pub denied: Vec<String>,
    pub default_allowed: bool,
}

pub fn resolve_automation_permission(permission: &str, input: &PermissionResolutionInput) -> bool {
    if input.denied.iter().any(|p| p == permission) {
        return false;
    }
    if input.granted.iter().any(|p| p == permission) {
        return true;
    }
    if input.spec_permissions.iter().any(|p| p == permission) {
        return true;
    }
    input.default_allowed
}

impl AutomationAppStatus {
    pub fn from_enabled_error(enabled: bool, error: Option<&str>) -> Self {
        if let Some(err) = error {
            if err.to_ascii_lowercase().contains("login") {
                return Self::NeedsLogin;
            }
            return Self::Error;
        }
        if enabled { Self::Active } else { Self::Paused }
    }
}
```

Export it in `src-tauri/src/automation/mod.rs` with `pub mod compat;`.

- [ ] **Step 4: Add additive migration**

Add nullable fields if they do not already exist:

```sql
ALTER TABLE automation_specs ADD COLUMN status TEXT;
ALTER TABLE automation_specs ADD COLUMN user_overrides_json TEXT;
ALTER TABLE automation_specs ADD COLUMN browser_login TEXT;
ALTER TABLE automation_specs ADD COLUMN uninstalled_at INTEGER;
```

Use the existing migration helper style that safely ignores duplicate-column errors.

- [ ] **Step 5: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml automation::compat --lib`

Expected: tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/automation/compat.rs src-tauri/src/automation/mod.rs
git commit -m "feat(automation): add halo-compatible app metadata"
```

### Task 2: Replace Embedded Built-In Seeding With Manifest Loader

**Files:**
- Create: `src-tauri/src/automation/builtin_apps.rs`
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/automation/mod.rs`
- Create: `src-tauri/resources/builtin-automations/manifest.json`
- Move/copy: `src-tauri/resources/automation-specs/*.yaml` into bundle folders
- Move/copy: `src-tauri/resources/automation-skills/*` into bundle folders
- Test: unit tests in `builtin_apps.rs`

- [ ] **Step 1: Write loader tests**

Create tests that use a temporary resources root:

```rust
#[test]
fn manifest_loader_discovers_specs_and_skills() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("builtin-automations");
    std::fs::create_dir_all(root.join("bilibili-comment-auto-reply/skills/bili-get-messages")).unwrap();
    std::fs::write(root.join("manifest.json"), r#"{"apps":["bilibili-comment-auto-reply"]}"#).unwrap();
    std::fs::write(root.join("bilibili-comment-auto-reply/spec.yaml"), "id: bilibili-comment-auto-reply\nname: B站评论自动回复\nversion: 2.0.0\n").unwrap();
    std::fs::write(root.join("bilibili-comment-auto-reply/skills/bili-get-messages/index.js"), "async (params) => ({ok:true, params})").unwrap();

    let apps = load_builtin_apps(&root).unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].id, "bilibili-comment-auto-reply");
    assert_eq!(apps[0].skills.len(), 1);
}
```

- [ ] **Step 2: Run the test and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml automation::builtin_apps --lib`

Expected before implementation: compile failure because `builtin_apps` does not exist.

- [ ] **Step 3: Implement loader**

Implement structs:

```rust
#[derive(Debug, Clone)]
pub struct BuiltinAutomationApp {
    pub id: String,
    pub spec_yaml: String,
    pub spec_path: PathBuf,
    pub skills: Vec<BuiltinAutomationSkill>,
}

#[derive(Debug, Clone)]
pub struct BuiltinAutomationSkill {
    pub id: String,
    pub root: PathBuf,
    pub index_js: PathBuf,
}
```

`load_builtin_apps(root)` reads `manifest.json`, then each `<id>/spec.yaml` and `<id>/skills/*/index.js`.

- [ ] **Step 4: Implement skill sync**

Add `sync_builtin_skills(app, workspace_root)` that copies each skill directory into:

```text
<workspace_root>/.claude/skills/<skill_id>/
```

Use create-dir-all and copy only files under the skill root. Reject symlinks escaping the bundle root.

- [ ] **Step 5: Move current built-ins into manifest**

Create:

```json
{"apps":["douyin-live-room-moderator","bilibili-comment-auto-reply"]}
```

Then place:

```text
src-tauri/resources/builtin-automations/douyin-live-room-moderator/spec.yaml
src-tauri/resources/builtin-automations/douyin-live-room-moderator/skills/...
src-tauri/resources/builtin-automations/bilibili-comment-auto-reply/spec.yaml
src-tauri/resources/builtin-automations/bilibili-comment-auto-reply/skills/bili-get-messages/index.js
src-tauri/resources/builtin-automations/bilibili-comment-auto-reply/skills/bili-reply/index.js
```

- [ ] **Step 6: Update `AppRuntimeService::seed_builtin_specs`**

Replace embedded constants with `load_builtin_apps`. Preserve existing behavior:

- update spec yaml/version for existing built-ins,
- insert missing built-ins,
- preserve enabled/config/permissions,
- skip rows with `uninstalled_at IS NOT NULL`.

- [ ] **Step 7: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml seed_builtin_specs --lib
cargo test --manifest-path src-tauri/Cargo.toml automation::builtin_apps --lib
```

Expected: all focused tests pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/automation/builtin_apps.rs src-tauri/src/automation/mod.rs src-tauri/src/automation/runtime/service.rs src-tauri/resources/builtin-automations
git commit -m "feat(automation): load builtin digital humans from manifest"
```

### Task 3: Implement Real Halo-Compatible `browser_run`

**Files:**
- Modify: `src-tauri/src/browser/context.rs`
- Modify: `src-tauri/src/browser/tools.rs`
- Modify: `src-tauri/src/browser/script_runner.rs`
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Test: browser script path policy tests and local fixture execution test

- [ ] **Step 1: Write failing unit tests for script path policy**

Add cases that allow `.claude/skills/bili-get-messages/index.js` and reject path traversal:

```rust
#[test]
fn allows_materialized_claude_skill_script() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().to_path_buf();
    let builtin = temp.path().join("builtin");
    let script = workspace.join(".claude/skills/bili-get-messages/index.js");
    std::fs::create_dir_all(script.parent().unwrap()).unwrap();
    std::fs::write(&script, "async () => ({ok:true})").unwrap();

    let policy = ScriptPathPolicy::new(builtin, workspace.clone(), workspace.clone());
    let resolved = policy.resolve(".claude/skills/bili-get-messages/index.js").unwrap();
    assert_eq!(resolved, script);
}

#[test]
fn rejects_claude_skill_traversal() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().to_path_buf();
    let policy = ScriptPathPolicy::new(temp.path().join("builtin"), workspace.clone(), workspace.clone());
    assert!(policy.resolve(".claude/skills/../../secret.js").is_err());
}
```

- [ ] **Step 2: Run path tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml script_runner --lib`

Expected before implementation: allow test fails.

- [ ] **Step 3: Extend path policy**

Allow resolved paths under:

```text
<workspace_root>/.claude/skills/
<workspace_root>/.uclaw/skills/
<builtin_root>/
```

Keep canonicalization checks so traversal cannot escape.

- [ ] **Step 4: Add page evaluation helper**

Add a helper shaped like:

```rust
pub async fn evaluate_script_with_params(
    &self,
    tab_id: &str,
    source: &str,
    params: serde_json::Value,
    timeout_ms: u64,
) -> anyhow::Result<serde_json::Value>
```

Wrap scripts as:

```javascript
(async () => {
  const __uclawParams = <json>;
  const __uclawUserFn = <script source without trailing semicolon>;
  return await __uclawUserFn(__uclawParams);
})()
```

Execute through the existing CDP/evaluate path and return JSON.

- [ ] **Step 5: Make `BrowserRunScriptTool` execute**

Add `ctx_mgr: Arc<BrowserContextManager>` to the tool struct. In `execute`:

1. Resolve file path.
2. Read source.
3. Resolve target tab from `params.tab_id`, `params.tabId`, or current active tab.
4. Call `evaluate_script_with_params`.
5. Return:

```json
{"ok":true,"sessionId":"...","file":"...","result":{...}}
```

On timeout or JS error return `ok:false` with `error`, `file`, `sessionId`, and `durationMs`.

- [ ] **Step 6: Add Halo alias `browser_run`**

Create a `BrowserRunTool` wrapper whose `name()` returns `browser_run` and delegates to the same implementation. Keep `browser_run_script` for existing Douyin live-room code.

- [ ] **Step 7: Inject browser manager into automation runtime**

Change `AppRuntimeService::new` to accept `Option<Arc<BrowserContextManager>>`. Update `src-tauri/src/app.rs` so `BrowserContextManager` is created before `AppRuntimeService`, then pass the clone into the runtime.

- [ ] **Step 8: Register run-scoped browser tools**

When a spec has `ai-browser`, register the Halo-compatible names. Use a run session id:

```text
automation:{spec_id}:{activity_id}
```

Do not reuse a global `"automation-live-room"` browser session for all specs.

- [ ] **Step 9: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml script_runner --lib
cargo test --manifest-path src-tauri/Cargo.toml automation::runtime::service --lib
```

Expected: tests pass. Manual browser-run fixture can be performed after frontend smoke.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/browser/context.rs src-tauri/src/browser/tools.rs src-tauri/src/browser/script_runner.rs src-tauri/src/automation/runtime/service.rs src-tauri/src/app.rs
git commit -m "feat(browser): add halo-compatible browser_run facade"
```

### Task 4: Align Runtime Reports, Stop Semantics, And Live-Room Completion

**Files:**
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/automation/live_room/runtime.rs`
- Modify: `src-tauri/src/automation/live_room/types.rs`
- Test: live-room runtime unit tests

- [ ] **Step 1: Write stop-report tests**

Add tests that assert:

- a manual stop records stop reason `user_requested`,
- a page-ended signal records stop reason `live_ended`,
- final report includes replies, moderation actions, knowledge items, skipped comments, and errors.

- [ ] **Step 2: Implement stop token**

Add a cancellation token per active automation run in `AppRuntimeService`. Expose an internal method:

```rust
pub async fn request_stop(&self, spec_id: &str, reason: AutomationStopReason) -> anyhow::Result<()>
```

Use existing pause/disable commands to call this before disabling a spec.

- [ ] **Step 3: Add live-ended adapter signal**

Extend live-room adapter result types with:

```rust
pub live_ended: bool
```

Douyin `scan_comments` returns this when page text or DOM state indicates the live room has ended.

- [ ] **Step 4: Generate final report**

At run completion, write one structured report object and a Markdown report. Include:

```text
duration
trigger source
stop reason
room/platform
reply count
warning count
mute/remove count
knowledge items extracted
comments skipped
errors
```

- [ ] **Step 5: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml live_room --lib`

Expected: tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/runtime/service.rs src-tauri/src/automation/live_room
git commit -m "feat(automation): add live-room stop reports"
```

### Task 5: Add App-Specific IM Sessions And Notify Tools

**Files:**
- Modify: `src-tauri/src/channels/dispatcher.rs`
- Modify: `src-tauri/src/automation/runtime/chat_sessions.rs`
- Modify: `src-tauri/src/automation/runtime/tool_registry.rs`
- Test: dispatcher tests in `channels/dispatcher.rs`

- [ ] **Step 1: Write identity isolation test**

Add a test where the same IM user triggers two specs and assert two different sessions:

```rust
let key_a = app_chat_identity_key("spec_a", "wecom_bot", "UIN_1");
let key_b = app_chat_identity_key("spec_b", "wecom_bot", "UIN_1");
assert_ne!(key_a, key_b);
assert_eq!(key_a, "app-chat:spec_a:wecom_bot:UIN_1");
```

- [ ] **Step 2: Implement identity helper**

Create:

```rust
pub fn app_chat_identity_key(spec_id: &str, channel_type: &str, chat_id: &str) -> String {
    format!("app-chat:{spec_id}:{channel_type}:{chat_id}")
}
```

Use it in `run_automation_via_im`.

- [ ] **Step 3: Add stop/clear commands**

Before dispatching an automation IM message, recognize exact commands:

```text
/stop
/clear
停止
清空
```

`/stop` requests cancellation for the app chat run. `/clear` clears the app chat session history for that identity key.

- [ ] **Step 4: Add notify tools**

Register `notify_channel` and `notify_bot` only when permission `im-notify` is resolved true. The tools send through the existing `ChannelManager` and return `{ "dispatched": true }` or a structured error.

- [ ] **Step 5: Preserve guest safety**

Keep existing IM guest denial of shell and AI browser for generic agent chat. Only automation app runs can receive their spec-declared browser permission.

- [ ] **Step 6: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml channels::dispatcher --lib`

Expected: tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/channels/dispatcher.rs src-tauri/src/automation/runtime/chat_sessions.rs src-tauri/src/automation/runtime/tool_registry.rs
git commit -m "feat(automation): isolate app IM sessions"
```

### Task 6: Frontend Halo-Compatible Digital Human UX

**Files:**
- Modify: `ui/src/components/automation/AutomationHub.tsx`
- Modify: `ui/src/components/automation/SpecRunSurface.tsx`
- Modify: `ui/src/components/automation/SpecSettingsView.tsx`
- Modify: `ui/src/components/automation/SpecRunHeader.tsx`
- Modify: `ui/src/components/automation/ChatThreadsTab.tsx`
- Modify: `ui/src/lib/tauri-bridge.ts`
- Test: `ui/src/components/automation/AutomationHub.test.tsx`

- [ ] **Step 1: Write UI tests**

Add tests that render with mock specs and assert:

```tsx
expect(screen.getByText('我的数字人')).toBeInTheDocument()
expect(screen.getByText('聊天')).toBeInTheDocument()
expect(screen.getByText('动态')).toBeInTheDocument()
expect(screen.getByText('设置')).toBeInTheDocument()
expect(screen.getByText('AI 浏览器')).toBeInTheDocument()
expect(screen.getByText('所需登录')).toBeInTheDocument()
```

- [ ] **Step 2: Run UI tests and verify failure**

Run: `pnpm vitest run ui/src/components/automation/AutomationHub.test.tsx`

Expected before implementation: missing text assertions fail.

- [ ] **Step 3: Update shell labels and layout**

Change sidebar title from `数字人` to `我的数字人`, add app count, keep `SpecList`, and add a bottom `创建数字人` action if the existing create/import command is available.

- [ ] **Step 4: Update header**

Show icon, name, status text, last run, workspace, and actions:

```text
立即执行
暂停/恢复
浏览器
```

Use icons from `lucide-react`; keep compact operational layout.

- [ ] **Step 5: Update tabs**

Rename tabs to:

```text
聊天
动态
设置
```

Map current activity/run surface into `动态`, current settings into `设置`, and existing chat threads into `聊天`.

- [ ] **Step 6: Add settings sections**

Render sections for:

```text
计划任务
运行时
AI 浏览器
电子邮件
IM 推送
所需登录
系统通知
消息通道
配置
开发者信息
危险操作
```

Use existing data where available; show unconfigured/disabled state without inventing fake credentials.

- [ ] **Step 7: Run UI tests**

Run:

```bash
pnpm vitest run ui/src/components/automation/AutomationHub.test.tsx
pnpm typecheck
```

Expected: tests pass and typecheck passes.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/automation ui/src/lib/tauri-bridge.ts
git commit -m "feat(ui): add halo-style digital human automation UX"
```

### Task 7: End-To-End Verification

**Files:**
- Modify only if verification exposes bugs.

- [ ] **Step 1: Run backend focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml seed_builtin_specs --lib
cargo test --manifest-path src-tauri/Cargo.toml automation::builtin_apps --lib
cargo test --manifest-path src-tauri/Cargo.toml script_runner --lib
cargo test --manifest-path src-tauri/Cargo.toml channels::dispatcher --lib
cargo test --manifest-path src-tauri/Cargo.toml live_room --lib
```

Expected: all focused tests pass.

- [ ] **Step 2: Run frontend focused tests**

Run:

```bash
pnpm vitest run ui/src/components/automation/AutomationHub.test.tsx
pnpm typecheck
```

Expected: tests and typecheck pass.

- [ ] **Step 3: Manual smoke in app**

Run the desktop app with the project's normal dev command. Verify:

- B站评论自动回复 appears under 我的数字人.
- Douyin live-room moderator appears under 我的数字人.
- Settings shows AI browser, required login, message channels, config, YAML.
- Clicking 浏览器 opens or focuses the uClaw browser.
- Manual run creates an activity and report.
- IM-bound spec creates an app-specific chat thread.
- Douyin live-room run can be stopped by the user and produces a final report.

- [ ] **Step 4: Risk review**

Before final merge, inspect:

```bash
git diff --stat main...
rg -n "browser_run_script_execution_not_connected|automation-live-room|permissions_denied|browser_login|app-chat:" src-tauri/src ui/src
```

Expected: no inert browser-run path remains, no global live-room browser session is used for every spec, and permission/login/app-chat strings are present in the intended modules.

- [ ] **Step 5: Commit fixes**

If verification required fixes:

```bash
git add <changed files>
git commit -m "fix(automation): stabilize halo-compatible digital humans"
```

If no fixes were required, do not create an empty commit.

## Self-Review

- Spec coverage: built-ins, browser facade, IM integration, permissions, login state, frontend UX, live-room stop/report, concurrent specs, and gbrain room/platform isolation are all covered.
- Placeholder scan: the plan intentionally avoids unresolved placeholders. Deferred future work is limited to an optional external MCP wrapper around the in-process facade.
- Type consistency: `AutomationAppStatus`, `AutomationUserOverrides`, `PermissionResolutionInput`, `resolve_automation_permission`, `app_chat_identity_key`, and `browser_run` are introduced before use.
- Risk posture: the plan uses additive migrations, preserves current uClaw runtime truths, and treats real browser execution plus real live-room moderation as explicitly permissioned behavior.
