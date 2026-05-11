# Workspace Phase 3 — 真沙箱 + FileDropZone

**Status**: Spec
**Date**: 2026-05-11
**Author**: Ryan + Claude (uClaw repo)
**Phase**: 3 of 4
**Prerequisite**: [Phase 2 spec](./2026-05-11-workspace-phase2-design.md) merged as 72f8d73 on 2026-05-11

---

## 1. Background

Phase 2 pinned each agent session's tool registry to the session's workspace folder, so `pwd`, `read_file`, `glob`, etc. all start *inside* the workspace. But nothing stops the agent from then escaping. Today an agent in workspace `2222` can:

- `read_file /etc/passwd` — `ReadFileTool` accepts absolute paths, joins nothing, reads anything readable by the host user.
- `write_file ../../../tmp/foo` — `WriteFileTool` joins `..` segments back out via `workspace_root.join(path)` and writes outside.
- `edit ../../home/ryanliu/.zshrc` — same escape via `EditTool`.
- `bash --working_dir=/Users/ryanliu/Documents/other-repo` — `BashTool` accepts any working dir that exists.

The `SafetyManager` has tool-level approval modes but **no path concept at all**. The `should_approve(tool_name, args, …)` signature doesn't inspect arguments. So the only friction today is per-tool `ApprovalRequirement` (e.g. `write_file` asks once) — nothing about *where* the write lands.

Phase 3 closes this hole. It also wires the Phase-2-deferred FileDropZone: today the right-panel drop zone shows a visual hint but its `onDrop` is unwired in `SidePanel`, and there's no backend command to actually persist uploaded bytes.

## 2. Goals

1. **Path-aware safety layer**: every tool argument that names a filesystem path goes through a new `PathPolicy` check before `tool.execute()` runs. Decisions are Allow / Prompt / Block.
2. **Whitelist composition**: workspace.path + workspace.attached_dirs + session.attached_dirs + a new global `always_allowed_paths` list managed in Settings.
3. **Approval modal: path variant**. Out-of-whitelist paths reuse the existing approval modal flow with three actions: `Allow once`, `Allow this session`, `Deny`. Session grants live in memory keyed by `session_id`; user can later promote a session grant to permanent via Settings.
4. **Bash sandboxing scope**: only `working_dir` goes through the path check. Command-body path tokens stay with the existing `check_blocked` / risk heuristic (Phase 3 explicitly does not parse shell syntax).
5. **`upload_workspace_file` IPC**: write file bytes into `workspace.path`, sanitize filename, deduplicate via `(2)`, `(3)` suffix.
6. **FileDropZone wiring**: SidePanel passes a real `onDrop` (browser File → bytes → `upload_workspace_file`). Native folder drops via Tauri's `onDragDrop` window event → existing `attach_workspace_directory`.
7. **Settings → 安全 → 工作区沙箱** section: manage always-allowed paths + promote session grants.
8. **Build green at every commit**, ~8 commits, single PR.
9. **Tests**: ~15 Rust unit tests + 2-3 Vitest tests.

## 3. Non-Goals (deferred to Phase 4+)

- **Bash command-body path scanning**. The agent can still `cat /etc/passwd` if it picks a working_dir inside the workspace. Phase 3 only sandboxes the tool-argument layer, not the command-content layer. (Phase 4 may add this for `bash` specifically.)
- **Settings UI to change `OutOfWorkspacePathPolicy` mode**. Always Prompt. No "always Block" or "always Allow" knob — they exist as enum variants for future use but aren't user-exposed.
- **Drag-to-attach for session files**. Folder drops go workspace-level only. Session-level folder attachment stays through the existing UI button.
- **Multi-file batch progress UI**. Uploading 50 files just runs sequentially with a single "complete" toast.
- **Symlink-following toggle**. PathPolicy always follows symlinks via `Path::canonicalize` to prevent in-workspace-symlink-to-/etc escapes. Not user-configurable.
- **Migration of existing data**. No schema change. New `~/.uclaw/path_policy.json` file is created on first write.
- **Type unification** between `AgentWorkspace` and `Workspace`. Still Phase 4.

## 4. Detailed Design

### 4.1 New module: `safety/path_policy.rs`

```rust
pub struct PathPolicy {
    /// Persisted "always allowed" paths, global across sessions and
    /// workspaces. Survives app restart. Lives at ~/.uclaw/path_policy.json.
    global_allowed: Vec<PathBuf>,
    /// Ephemeral "allow for this session" paths granted via the approval
    /// modal. Cleared on app restart. Keyed by session_id.
    session_allowed: HashMap<String, Vec<PathBuf>>,
}

pub enum PathDecision {
    Allow,
    Prompt { reason: String },     // outside whitelist; show modal
    Block { reason: String },      // reserved; not user-triggerable in Phase 3
}

impl PathPolicy {
    pub fn load(config_dir: &Path) -> Self { /* read path_policy.json or default */ }
    pub fn save(&self) -> Result<(), Error> { /* write path_policy.json */ }

    pub fn check(
        &self,
        session_id: &str,
        workspace_root: &Path,
        workspace_attached: &[PathBuf],
        session_attached: &[PathBuf],
        candidate: &Path,            // absolute, already resolved by caller
    ) -> PathDecision { ... }

    pub fn list_global(&self) -> &[PathBuf];
    pub fn add_global(&mut self, p: PathBuf);
    pub fn remove_global(&mut self, p: &Path);

    pub fn list_for_session(&self, sid: &str) -> &[PathBuf];
    pub fn allow_for_session(&mut self, sid: &str, p: PathBuf);
    pub fn promote_session_to_global(&mut self, sid: &str, p: &Path);
}

/// Returns true if `candidate` is the same as or inside `root` after
/// canonicalization. If either path can't be canonicalized (doesn't
/// exist), falls back to a normalized lexical comparison that resolves
/// `..` segments. Always follows symlinks via canonicalize so an
/// in-workspace symlink to /etc cannot bypass the check.
pub(crate) fn is_under(candidate: &Path, root: &Path) -> bool { ... }
```

Decision algorithm in `check`:

1. If `candidate` is_under `workspace_root` → Allow.
2. For each `dir` in `workspace_attached ∪ session_attached`: if is_under → Allow.
3. For each `dir` in `global_allowed`: if is_under → Allow.
4. For each `dir` in `session_allowed[session_id]`: if is_under → Allow.
5. Otherwise → Prompt with reason `"Path {} is outside workspace and not in any allowed directory"`.

### 4.2 `SafetyManager` extension

New field `path_policy: PathPolicy` initialized alongside `policy` in `SafetyManager::new`.

New methods (proxy to `PathPolicy` + save):
- `check_paths(session_id, workspace_root, workspace_attached, session_attached, candidates) -> PathDecision` — short-circuits on first non-Allow.
- `allow_path_for_session(session_id, path)`
- `promote_path_to_global(session_id, path)`
- `list_always_allowed_paths()`
- `add_always_allowed_path(path)` / `remove_always_allowed_path(path)`
- `list_session_allowed_paths(session_id)`

In `Yolo` mode, `check_paths` short-circuits to `Allow` before consulting `path_policy` — matches the existing escape hatch.

### 4.3 Dispatcher hook

Extend the `Tool` trait with a default-implemented method:

```rust
fn path_args<'a>(&self, _args: &'a serde_json::Value) -> Vec<&'a str> {
    Vec::new()
}
```

Tool overrides:

| Tool | Path args |
|---|---|
| `read_file` | `args.path` |
| `write_file` | `args.path` |
| `edit` | `args.path` |
| `grep` | `args.path` (if present) |
| `glob` | `args.path` (if present) |
| `bash` | `args.working_dir` (if present) |

In `dispatcher.rs::dispatch_tool`, after `safety_manager.should_approve` resolves AutoApprove or RequireApproval has been satisfied, **before** `tool.execute(args)`:

```rust
let raw = tool.path_args(&args);
let resolved: Vec<PathBuf> = raw.iter()
    .map(|p| {
        let pb = PathBuf::from(p);
        if pb.is_absolute() { pb } else { workspace_root.join(pb) }
    })
    .collect();

// Pull attached dirs per the session's workspace.
let (ws_attached, sess_attached) = load_attached_dirs(&db, &session_id);

let decision = safety_manager.check_paths(
    &session_id, &workspace_root, &ws_attached, &sess_attached,
    &resolved,
);

match decision {
    PathDecision::Allow => { /* fall through to execute */ }
    PathDecision::Prompt { reason } => {
        // Reuse the PendingApprovals oneshot.
        let approval = request_path_approval(reason, paths, session_id, app_handle).await?;
        match approval {
            "once"             => { /* one-shot, fall through */ }
            "allow_session"    => {
                for p in &resolved {
                    safety_manager.allow_path_for_session(&session_id, p.clone());
                }
            }
            "deny"             => return Err(ToolError::Blocked("user denied".into())),
        }
    }
    PathDecision::Block { reason } => return Err(ToolError::Blocked(reason)),
}
```

`load_attached_dirs` is a tiny helper that runs two queries:
```sql
SELECT attached_dirs FROM spaces WHERE id = (SELECT space_id FROM agent_sessions WHERE id = ?)
SELECT attached_dirs FROM agent_sessions WHERE id = ?
```
parses both JSON strings into `Vec<PathBuf>`, returns `(workspace_attached, session_attached)`. Failures (missing rows, malformed JSON) → empty vec.

### 4.4 Approval modal: `kind: "path"` variant

Backend payload extension to `agent:need_approval` IPC event:

```jsonc
{
  "kind": "path",                       // existing kinds: "tool", "bash_command"
  "sessionId": "...",
  "toolName": "read_file",
  "paths": ["/etc/passwd"],             // resolved absolute paths
  "reason": "Path /etc/passwd is outside workspace 2222 and not in any allowed directory",
  "actions": ["once", "allow_session", "deny"]
}
```

Frontend `ApprovalModal` (already mounted in AppShell) renders three buttons. The existing oneshot resolver pattern is preserved; just one new branch in the modal component to handle `kind: "path"`.

### 4.5 `upload_workspace_file` IPC

```rust
#[tauri::command]
pub async fn upload_workspace_file(
    state: State<'_, AppState>,
    workspace_id: String,
    filename: String,
    content: Vec<u8>,
) -> Result<String, Error>  // returns absolute path of written file
```

Steps:
1. Look up `workspace_id` in `spaces`. Reject if not found.
2. Read `path` column. Reject if NULL or empty.
3. `mkdir_p` the workspace path (idempotent — Phase 2 should have created it, but heal).
4. Sanitize `filename`:
   - Strip path separators (`/`, `\`).
   - Reject leading `.` (no dotfile uploads).
   - Reject `..` substring.
   - Truncate to 200 chars including extension.
5. Resolve target = `<workspace.path>/<sanitized_filename>`.
6. Collision: if target exists, find smallest `n ≥ 2` such that `<stem> (n).<ext>` doesn't exist. Cap at 99; error if exceeded.
7. `tokio::fs::write(&target, &content)`. Error → IO error.
8. Return absolute path of `target` as String.

No `PathPolicy.check` call — destination is by construction inside the workspace.

### 4.6 FileDropZone wiring

**Browser-file path** (DataTransfer.files — bytes available, no real path on Tauri 2):

`SidePanel.tsx` adds `handleFilesDropped`:

```ts
const handleFilesDropped = async (files: File[]) => {
  if (!currentWorkspaceId) {
    toast.error('请先选择工作区')
    return
  }
  for (const file of files) {
    try {
      const buf = await file.arrayBuffer()
      const bytes = Array.from(new Uint8Array(buf))
      const writtenPath = await uploadWorkspaceFile(currentWorkspaceId, file.name, bytes)
      console.debug('[upload] wrote', writtenPath)
    } catch (err) {
      toast.error(`上传 ${file.name} 失败: ${err}`)
    }
  }
  setFilesVersion((v) => v + 1)
}
```

Pass `onDrop={handleFilesDropped}` to both `FileDropZone` instances. The existing FileDropZone component already calls `onDrop(files)` on drop; just no consumer was wired.

**Native folder-drop path** (Tauri's window-level event — real OS paths):

New top-level effect in `AgentView.tsx`:

```ts
React.useEffect(() => {
  const win = getCurrentWindow()
  let unlisten: UnlistenFn | undefined
  win.onDragDropEvent((evt) => {
    if (evt.payload.type === 'drop') {
      const paths = evt.payload.paths
      paths.forEach(async (p) => {
        const isDir = await pathIsDirectory(p)  // new tiny IPC
        if (isDir && currentWorkspaceId) {
          await attachWorkspaceDirectory(currentWorkspaceId, p)
          // wsAttachedMap is refreshed by attachWorkspaceDirectory's
          // existing return-value handler in WorkspaceFilesView
        }
      })
    }
  }).then((u) => { unlisten = u })
  return () => unlisten?.()
}, [currentWorkspaceId])
```

New tiny IPC `path_is_directory(path) -> bool` to disambiguate file vs folder; falls back to `false` on read error.

UI: FileDropZone hint per target — `session` → `"拖入会话目录"`, `workspace` → `"拖入工作区文件 / 文件夹"`.

Why two paths: browser File objects don't have real paths on Tauri 2 (security restriction). Tauri's native event gives paths but only fires for window-level drops, not React DragEvents on a specific element. Folders aren't represented at all in DataTransfer.files. So we pick whichever fits the drop type. File-on-element → React event → bytes. Folder-on-window → Tauri event → path attach.

### 4.7 Settings UI

New component `ui/src/components/settings/WorkspaceSandboxSettings.tsx`, mounted as a sub-section of the existing Safety settings tab.

**Section A: 始终允许的外部路径**
- List of `global_allowed` paths from `list_always_allowed_paths()`.
- Each row: path + `[删除]` button.
- `[+ 添加路径]` button → `openFolderDialog()` (Phase 2 IPC) → `add_always_allowed_path()`.
- After mutation: re-fetch list.

**Section B: 本会话已临时授权的外部路径**
- Only renders if `currentAgentSessionId` is set.
- List of `list_session_allowed_paths(session_id)`.
- Each row: path + `[升级为永久]` button → `promote_session_path_to_global(session_id, path)` → re-fetch both lists.
- Empty state: small italic hint `"本会话没有触发过外部路径授权"`.

New IPCs (registered in `invoke_handler!`):
- `list_always_allowed_paths() -> Vec<String>`
- `add_always_allowed_path(path: String)`
- `remove_always_allowed_path(path: String)`
- `list_session_allowed_paths(session_id: String) -> Vec<String>`
- `promote_session_path_to_global(session_id: String, path: String)`

All bridge wrappers in `tauri-bridge.ts` follow existing camelCase convention.

## 5. Persistence

- **`~/.uclaw/path_policy.json`** — new file. Schema:
  ```json
  {
    "globalAllowed": ["/Users/ryanliu/Documents/notes", "/tmp"]
  }
  ```
  Written via `serde_json::to_string_pretty`. Loaded at SafetyManager init; defaults to empty list if file missing.
- **Session-allowed paths** — in-memory only in `SafetyManager.path_policy.session_allowed`. Cleared on app restart. Intentional: a session grant is a "trust me for now" gesture; it shouldn't outlive the process.
- **No SQLite migration**. No new columns on `spaces` or `agent_sessions`. The `attached_dirs` column from Phase 2 is the source of truth for per-workspace and per-session allowed paths.

## 6. Error Handling

- **PathPolicy load failure** (corrupt JSON, missing file): log warning, start with empty `global_allowed`. App still boots.
- **PathPolicy save failure** (disk full, permission denied): bubble up to caller as `Error::Io`. Settings UI shows toast. The in-memory state still reflects the user's intent; next save attempt may succeed.
- **`upload_workspace_file` errors**: invalid filename → `Error::InvalidInput`; workspace missing → `Error::NotFound`; collision cap exceeded → `Error::Internal`; disk full → `Error::Io`. Each maps to a distinct toast message in the frontend.
- **Approval timeout**: out of scope. Same behavior as existing approvals — the agent loop waits indefinitely on the oneshot. (Existing Phase 1 `ApprovalModal` mount fix already covered the "no resolver" case.)

## 7. Testing

**Rust unit tests** (inline `#[cfg(test)]`):

| Module | Tests |
|---|---|
| `safety::path_policy::tests` | `is_under_simple`, `is_under_with_dotdot`, `is_under_symlink_to_outside_returns_false`, `check_inside_workspace_allows`, `check_inside_attached_allows`, `check_outside_all_prompts`, `check_after_session_allow_allows`, `check_after_global_allow_allows` |
| `safety::tests` (extended) | `yolo_mode_skips_path_check`, `path_policy_persists_round_trip` |
| `tauri_commands::upload_workspace_file_tests` | `sanitizes_filename`, `dedupes_on_collision`, `rejects_dotfile`, `rejects_missing_workspace` |
| `tauri_commands::path_ipc_tests` | `add_remove_global_round_trip`, `list_session_returns_grants`, `promote_session_clears_from_session_adds_to_global` |

**UI tests** (Vitest + RTL):

| Test file | Cases |
|---|---|
| `FileDropZone.test.tsx` | drop calls onDrop with File[]; disabled drop is no-op |
| `WorkspaceSandboxSettings.test.tsx` | renders global list; add button calls IPC; remove button calls IPC; promote button moves grant from session to global |

**Manual smoke checklist** (recorded in PR description):

1. **Sandbox happy path**: workspace `2222` → ask agent to `read_file /etc/hosts` → modal pops with three buttons → click "Allow this session" → second `read_file /etc/hosts` call doesn't pop.
2. **Sandbox restart**: same as (1), restart app, second call pops again.
3. **Sandbox promote**: same as (1), then Settings → 升级为永久 → restart → call doesn't pop, path appears in global list.
4. **Drop file**: drag `logo.png` from Finder onto workspace files section → file lands in `~/Documents/workground/2222/`, appears in FileBrowser within a frame.
5. **Drop folder**: drag a folder from Finder onto the workspace area → `attach_workspace_directory` IPC fires, folder appears in 附加目录 list.
6. **Drop file collision**: drop two files named `logo.png` → second one becomes `logo (2).png`.
7. **Yolo mode**: Settings → mode = Yolo → agent reads `/etc/hosts` without modal.

## 8. PR Shape (bisectable commits)

| # | Commit | LOC est |
|---|---|---|
| 1 | `safety/path_policy.rs` module + `is_under` + tests | ~150 |
| 2 | `SafetyManager` integration + path-policy persistence + tests | ~80 |
| 3 | `Tool::path_args` trait method + 6 tool overrides | ~50 |
| 4 | Dispatcher hook + path-approval modal payload variant | ~120 |
| 5 | `upload_workspace_file` IPC + tests | ~80 |
| 6 | `path_is_directory` IPC + bridge wrappers | ~30 |
| 7 | FileDropZone wiring in SidePanel + AgentView native onDragDrop | ~120 |
| 8 | Settings → WorkspaceSandboxSettings + 5 path-policy IPCs + tests | ~180 |

Total: ~810 LOC. Build green at every commit.

## 9. Open Questions / Risks

- **Symlink follow vs no-follow**: chose follow (via `canonicalize`). A workspace containing a symlink → `/etc` would otherwise be a trivial bypass. Trade-off: if a user *intentionally* symlinks `/Users/me/large-dataset` into their workspace, the agent gets that path treated as in-workspace — which is what they probably want.
- **Approval payload bloat**: a `read_file` over a 50-file batch (if such a tool ever existed) would balloon the modal. Phase 3 tools all take one path; not a concern. If batched-path tools land later, the modal will need a "select per-path" UI.
- **Race condition on session grant**: SafetyManager is `Arc<RwLock<…>>`. If two tool calls in the same session race past the approval point, both could trigger the modal. The oneshot/PendingApprovals pattern serializes within a single call but doesn't coordinate across calls. Phase 3 accepts this — duplicate prompts are annoying but not unsafe.
- **`upload_workspace_file` size cap**: not enforced. Tauri IPC has a soft limit around tens of MB before serialization slows. Phase 3 doesn't cap; if it becomes a problem we add `max_upload_bytes` to settings in Phase 4.
