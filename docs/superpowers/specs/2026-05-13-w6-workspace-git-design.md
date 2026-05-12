# W6 — Workspace Git Integration — Design

> **Status**: spec. Implementation plan: `docs/superpowers/plans/2026-05-13-w6-workspace-git-pr-a-backbone.md` (PR A) + `docs/superpowers/plans/2026-05-13-w6-workspace-git-pr-b-ui.md` (PR B).

## 0. Goal

Port if2Ai's workspace-with-git frontend + backend framework into uClaw. Phase 1+2 surface: branch list/switch/create chip in the chat composer, status/diff/branches workbench dialog, commit + commit-push-PR composer chip with a graceful `gh`-missing draft fallback. All shell-out, no `git2`/libgit2.

Deferred from if2Ai's full surface: worktrees, slash command handlers, file-tree status decorations.

## 1. Scope summary

**Phase 1 — branch operations:**
- `BranchPicker` chip (search, list, switch, create, "untracked changes" indicator, no-repo→init affordance)
- Backend: `status`, `branch list/current/default/checkout/create`, `init_repo`

**Phase 2 — workbench + commit/PR flow:**
- `GitWorkbenchDialog` (status / diff / branches tabs, `ViewState<T>` 5-state machine)
- `GitActionsPicker` chip (commit / commit+push+PR, draft fallback when `gh` missing)
- Backend: `commit_all_with_message`, `gh pr create`, `git push --set-upstream`, `gitCommitPushPr` one-shot composite

## 2. Architecture sketch

```
src-tauri/src/git/                       ← new top-level Rust module
├── mod.rs                          (~30)   — re-exports
├── error.rs                        (~140)  — GitError enum + Display/Error/From impls
├── command.rs                      (~150)  — git_stdout / git_ok / git_stdout_no_locks + async variants
├── repo.rs                         (~50)   — find_git_root / is_inside_repo / init_repo
├── branch.rs                       (~150)  — list_branches_verbose / current / default / checkout / create_and_checkout / slugify
├── status.rs                       (~120)  — DiffMode {Stat, Full} + read_status / read_diff_with_mode
├── commit.rs                       (~140)  — CommitMessageFile RAII tempfile + commit_all_with_message
├── github/
│   ├── mod.rs                      (~15)   — is_gh_available
│   └── pr.rs                       (~180)  — PrCreateRequest / create_async / push_branch_set_upstream
└── tests.rs                        (~250)  — 18 unit tests

src-tauri/src/tauri_commands_git.rs (~400)  — ~20 #[tauri::command] wrappers + sandbox helpers
                                              (sibling of tauri_commands.rs; kept separate because
                                              folding 400 LOC into the 6000+ LOC main commands file
                                              would compound an existing god-file)

ui/src/modules/git/
├── api.ts                          (~280)  — IPC typed-wrapper layer + parseBranchList + uncommittedFromStatus
└── api.test.ts                     (~90)   — 7 helper tests

ui/src/components/chat/git/
├── BranchPicker.tsx                (~430)  — port of if2Ai BranchPicker
├── GitWorkbenchDialog.tsx          (~420)  — port of if2Ai dialog
├── GitActionsPicker.tsx            (~180)  — outer chip + Mode state machine + menu (split from if2Ai's 717 LOC)
├── GitActionsPickerForms.tsx       (~280)  — Commit / CreateBranch / PR form sub-views
├── GitActionsPickerDraftPr.tsx     (~160)  — gh-missing PrDraftView + shellAnsiCQuote helper
└── GitChipsRow.tsx                 (~60)   — shared wrapper used by both composers
```

**LOC budget total:** ~2200 Rust + ~2200 TS = ~4400 LOC. PR A delivers ~2400 LOC (backbone, all Rust + IPC layer + api.ts), PR B delivers ~2000 LOC (UI components + composer wiring).

## 3. Backend detail

### 3.1 Module layout — verbatim port from if2Ai

Port `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/` into `src-tauri/src/git/`. 8 files, ~1400 LOC of source + 250 LOC of tests. **Two deviations from if2Ai:**

1. Drop `worktree.rs` (out of scope)
2. Use `Result<T, String>` for all Tauri commands (mirrors if2Ai 1:1; avoids cross-cutting churn against uClaw's `crate::error::Error`)

Every other file ports verbatim. Function signatures, error variants, helper boundaries — all preserved.

### 3.2 Error type

Direct port of if2Ai's `GitError`:

```rust
pub enum GitError {
    Io(io::Error),
    Utf8(FromUtf8Error),
    NonZeroExit { program: String, args: Vec<String>, exit_code: Option<i32>, stderr: String, stdout: String },
    MissingBinary(&'static str),
    NotARepository,
    NoBranch,
    NoWorkspaceChanges,
    CommitMessageRequired,
    EmptyCommitMessage,
    MissingRequired(&'static str),
    Parse(String),
    Internal(String),
}
```

Plus `impl Display`, `impl Error`, `From<io::Error>`, `From<FromUtf8Error>`, `From<GitError> for String`, and the `GitError::from_output(program, args, &Output)` constructor.

### 3.3 Command runner — `--no-optional-locks` discipline preserved

Read ops route through `git_stdout_no_locks(cwd, args)` which prepends `--no-optional-locks` to every argv. Mutating ops use plain `git_stdout` / `git_ok` so concurrent IDE writes can't silently race them. Direct port of if2Ai's discipline.

### 3.4 IPC surface — ~20 Tauri commands

New file `src-tauri/src/tauri_commands_git.rs` (~400 LOC). Re-exported through `tauri_commands.rs` (one `pub use` line) and registered in `main.rs`'s `invoke_handler!` macro under a `// ─── Git Commands ───` comment block.

Full command list (all `async fn`, return `Result<T, String>`):

| Command | Signature | Sandbox |
|---|---|---|
| `git_status` | `(cwd) -> Result<Option<String>, _>` | read |
| `git_diff` | `(cwd, full: Option<bool>) -> Result<Option<String>, _>` | read |
| `git_is_repo` | `(cwd) -> Result<bool, _>` | read |
| `git_init_repo` | `(cwd) -> Result<(), _>` | write |
| `git_branches` | `(cwd) -> Result<String, _>` | read |
| `git_current_branch` | `(cwd) -> Result<String, _>` | read |
| `git_default_branch` | `(cwd) -> Result<String, _>` | read |
| `git_checkout_branch` | `(cwd, name) -> Result<(), _>` | write |
| `git_create_branch` | `(cwd, name) -> Result<(), _>` | write |
| `git_commit` | `(cwd, message) -> Result<CommitOutcome, _>` | write |
| `git_commit_push_pr` | `(cwd, title, body, branch_hint) -> Result<String, _>` | write |
| `gh_available` | `() -> Result<bool, _>` | none |
| `gh_create_pr` | `(cwd, title, body, base) -> Result<CreatePrResponse, _>` | write |
| `gh_create_issue` | `(cwd, title, body) -> Result<String, _>` | write |

Plus 6 internal-only helpers for backbone tests (init, list_mounts mocking, etc.).

### 3.5 Sandbox model — read-permissive, write-gated

```rust
async fn assert_cwd_in_any_mount(state: &AppState, cwd: &str) -> Result<PathBuf, String>
async fn assert_cwd_in_editable_mounts(state: &AppState, cwd: &str) -> Result<PathBuf, String>
```

Both walk `state.files_rail_list_mounts(None).await` and canonicalize-compare. The W3 `MountRoot.editable: bool` flag is the gate:
- Workspace mounts default `editable: true`
- AttachedDirs default `editable: false` until the user opts in via the existing W3 mount toggle

Read ops call `assert_cwd_in_any_mount` (status, diff, branches list, current branch). Mutating ops call `assert_cwd_in_editable_mounts` (checkout, commit, push, PR create). Both canonicalize the candidate `cwd` and every mount path to defend against symlink and macOS case-insensitivity attacks.

### 3.6 Concurrency boundaries

```rust
async fn run_blocking<F, T>(work: F) -> Result<T, String>
where F: FnOnce() -> Result<T, String> + Send + 'static, T: Send + 'static
```

Local git ops: wrapped in `run_blocking` (`tokio::task::spawn_blocking`). Network ops (`gh pr create`, `gh issue create`, `git push`): `tokio::process::Command` directly so they don't park a blocking-pool worker for the GitHub round-trip.

### 3.7 Audit integration

Mutating commands emit `git_op:{checkout,create_branch,commit,push,pr_create}` events through uClaw's `observability/` tracing. Each event payload: `{ cwd, op, outcome, duration_ms }`. Read commands skip audit.

Not borrowed from if2Ai: the `tool_execution_{started,finished,failed}` event names — wrong semantic for our case.

### 3.8 `gh` graceful degradation

`is_gh_available()` wraps `command_exists("gh")` (walks PATH manually, honors `PATHEXT` on Windows). When `false`, the frontend `GitActionsPicker` transitions to `prDraft` mode with a copy-able shell command (see §4.3).

When `gh` is present but unauthenticated, `gh pr create` returns `NonZeroExit` with stderr like "gh auth login required". The frontend's `catch` handler detects `/gh\b|MissingBinary/i` in the error message and reroutes to the same draft fallback. Direct port of if2Ai's resilience pattern.

### 3.9 Backend tests (`src-tauri/src/git/tests.rs`)

18 unit tests against a `tempfile::TempDir` repo seeded with `git init` + baseline commit:

| Submodule | Tests |
|---|---|
| `branch_tests` | list / current / switch / create / slugify (8) |
| `commit_tests` | empty-message rejection / clean-tree skip / message-file lifecycle (4) |
| `status_tests` | clean / dirty / stat-vs-full (3) |
| `sandbox_tests` | accept-mount / reject-outside / reject-non-editable (3) |

Baseline 395 → 413 Rust tests.

## 4. Frontend detail

### 4.1 IPC layer (`ui/src/modules/git/api.ts`, ~280 LOC)

Verbatim port of if2Ai's `src/modules/git/api.ts`. Single sanctioned entry point — components NEVER `invoke()` git commands directly. Exports the ~20 wrapper functions + the pure `parseBranchList(raw)` helper (strips `(HEAD detached at …)` and worktree-occupied `+` rows; flags `*` rows as current).

Error handling: pure delegation. Wrappers `return invoke<T>(cmd, args)` and let promises reject with backend's stringified error. Callers wrap with `try/catch` and decide whether to toast or branch into error sub-view.

Plus `uncommittedFromStatus(raw: string | null): number` — counts non-empty lines after the `## branch` header in `--short --branch` output. 7-LOC helper, verbatim port from if2Ai's `BranchPicker.tsx:56-64`.

### 4.2 BranchPicker — port + 2 wiring changes

Verbatim port of `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/BranchPicker.tsx`. 435 LOC, under cap, single file.

Two wiring changes:

1. **`cwd` source**: instead of if2Ai's `currentProject.workdir`, read from `activeWorkspaceCwdAtom` (new derived atom returning the current workspace mount's `path`)
2. **`files_rail:change` subscription**: 500ms-debounced `gitStatus(cwd)` re-fetch on file watcher events so the "未提交的更改：N 个文件" indicator stays fresh during streamed agent writes

Everything else preserved verbatim:
- Search input with autofocus + `<Search/>` icon
- Branch list with `<GitBranch/>` icon + `<Check/>` on current + `<Loader2/>` on busy
- "+ 创建并检出新分支..." inline toggle form
- "无 Git 仓库 · 点击初始化" amber-styled affordance for `isGitRepo === false`
- Dirty-tree confirmation banner with "返回" + "仍要切换" CTAs
- All Tailwind class strings, all keyboard handlers, all Chinese labels

**Auto-init UX deviation**: uClaw adds a 3s confirmation toast before `git init` runs (`您将在 ~/Documents/workground/ 初始化 Git 仓库？`). Rationale: workspace is a long-lived user dir; surprise repo creation is bad. Toast intercepts the `onInitRepo` callback chain.

### 4.3 GitWorkbenchDialog — verbatim port

Direct port of `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitWorkbenchDialog.tsx`. 419 LOC, under cap.

- Three tabs: 状态 / 差异 / 分支
- `ViewState<T>` discriminated union: `idle | loading | empty | ready | error`
- `PlainTextView` chunking: `LINE_PAGE_SIZE = 500`, `FULL_EXPAND_WARN_THRESHOLD = 5000`
- Diff stat ⇄ full toggle preserved
- Refresh button in header re-runs `reload(tab)` for current tab only
- No syntax highlighting (raw text); W4d's DiffRenderer will be a superset later

### 4.4 GitActionsPicker — 3-file split, behavior verbatim

if2Ai's source is 717 LOC — over uClaw's 400 hard cap. Split along **internal function boundaries already present in if2Ai's monolith**:

```
GitActionsPicker.tsx           (~180)  — outer chip + popover + Mode state machine + menu sub-view
GitActionsPickerForms.tsx      (~280)  — Commit / CreateBranch / PR form sub-views
GitActionsPickerDraftPr.tsx    (~160)  — PrDraftView + shellAnsiCQuote helper
```

`Mode` state machine ported verbatim:

```ts
type Mode =
  | { kind: 'menu' }
  | { kind: 'commit' }
  | { kind: 'createBranch' }
  | { kind: 'pr' }
  | { kind: 'busy'; label: string }
  | { kind: 'success'; message: string }
  | { kind: 'error'; message: string }
  | { kind: 'prDraft'; title: string; body: string }
```

Commit-message composition: **manual textarea only**. No LLM suggestion. Future enhancement (W7+): "✨ Suggest" button that asks the agent for a Conventional-Commits message from `git diff --cached`. Explicitly out of W6 scope.

PR flow: single button "提交并打开 PR" calls `gitCommitPushPr({ cwd, title, body })` — one IPC does stage-all + commit + push + `gh pr create`. Response: pre-formatted human-readable string rendered verbatim in `success` mode.

`gh` fallback: `prDraft` mode renders a copy-able `gh pr create --title $'…' --body $'…'` shell command with ANSI-C escaping via `shellAnsiCQuote(value)`. Verbatim port.

### 4.5 GitChipsRow — dual-composer wrapper

```tsx
interface GitChipsRowProps { cwd: string | undefined; className?: string }
```

Single ~60-LOC component composing `WorkspacePill` + `BranchPicker` + `GitActionsPicker` + `GitWorkbenchDialog`. Imported by **both** `ui/src/components/chat/ChatInput.tsx` (Chat mode) AND `ui/src/components/agent/AgentView.tsx` (Agent mode) per CLAUDE.md dual-composer rule. Both pass `cwd={activeWorkspaceCwd}` from the derived atom. Visual position: bottom-left of the composer, matching the user's screenshot.

### 4.6 WorkspacePill

Reuses existing uClaw workspace-display affordance (`atoms/workspace*.ts`). Click opens the workspace switcher (existing modal). Pure composition — no new behavior.

### 4.7 Frontend tests

| Surface | Tests |
|---|---|
| `parseBranchList` (api.ts) | 4 fixtures: clean / `*` current / `+` worktree-locked / `(HEAD detached at sha)` |
| `uncommittedFromStatus` (api.ts) | 3 fixtures: clean / single line / multiline |
| `BranchPicker` (jsdom + RTL) | 2: renders list from mocked `gitBranches`, no-repo state offers init button |
| `GitActionsPicker` (jsdom + RTL) | 2: renders menu, draft fallback on `ghAvailable=false` |

Baseline 296 → 307 UI tests.

## 5. Verbatim port discipline (per user request)

Default behavior at port time: **copy if2Ai source verbatim.** Adaptation permitted only when:

1. uClaw's 300 LOC soft / 400 LOC hard module limit forces a split (GitActionsPicker only)
2. Cross-file dependencies don't exist in uClaw (`ProjectManager` → `files_rail_list_mounts`)
3. CLAUDE.md rules require it (dual-composer wiring, auto-init confirmation toast)

Everything else preserved unchanged:
- Tailwind class strings
- JSX structure + Radix component compositions
- Icon choices (lucide-react)
- Spacing / typography (`text-[12px]`, `text-[11.5px]`, etc.)
- Animation timing
- Keyboard handling + ARIA attributes
- Prop names + callback names
- Chinese labels (提交, 创建分支, 已提交, 工作区干净已跳过提交, etc.)
- The amber colors for warning states (no-repo, dirty-tree-checkout)

Every implementer prompt for PR B will carry the directive:

> Open `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/<Component>.tsx` FIRST and read it in full. Your job is to port it to uClaw, NOT to redesign or modernize. Preserve Tailwind class strings, JSX structure, prop names, callback names, Chinese labels, and ARIA attributes verbatim. The only changes permitted: (a) the documented `cwd` source swap, (b) the `files_rail:change` subscription, (c) splitting if the file would exceed 400 LOC.

### Theme token mapping

Both projects build on shadcn/Tailwind so tokens overlap nearly 1:1 (`bg-popover`, `text-foreground`, `text-muted-foreground`, `border-border`, `ring-ring`, `bg-accent`, `text-accent-foreground`). Any if2ai-prefixed CSS classes encountered at port time (e.g. `the-finals-selected-menu-item`) get either:

- (a) ported into uClaw's `globals.css` verbatim, or
- (b) replaced with equivalent token-utility composition

decided per-class at port time.

## 6. Phasing — two PRs

### PR A — Backbone (~2400 LOC, ~10 commits)

1. `docs(plan): W6 PR A plan`
2. `feat(git): error module + GitError type`
3. `feat(git): command runner + lock handling`
4. `feat(git): repo helpers`
5. `feat(git): branch helpers + slugify`
6. `feat(git): status + diff helpers + DiffMode`
7. `feat(git): CommitMessageFile RAII + commit_all_with_message`
8. `feat(git): github::pr + push_branch_set_upstream`
9. `feat(git): IPC commands + sandbox + observability wiring`
10. `feat(git): frontend api.ts + parseBranchList tests`
11. `test(git): 18 backend tests + 7 helper tests`

### PR B — UI (~2000 LOC, ~10 commits, after PR A merges)

1. `docs(plan): W6 PR B plan`
2. `feat(git): WorkspacePill (or reuse)`
3. `feat(git): BranchPicker — search/list/switch`
4. `feat(git): BranchPicker — create form + dirty-tree guard`
5. `feat(git): BranchPicker — no-repo init affordance + confirmation toast`
6. `feat(git): GitWorkbenchDialog`
7. `feat(git): GitActionsPicker menu + state machine`
8. `feat(git): GitActionsPickerForms`
9. `feat(git): GitActionsPickerDraftPr`
10. `feat(git): GitChipsRow + wire into ChatInput + AgentView`
11. `test(git): 4 jsdom tests + manual checklist`

## 7. Test plan summary

| Layer | New tests | Baseline | After W6 |
|---|---|---|---|
| Rust git helpers | 18 | 395 | 413 |
| `parseBranchList` / `uncommittedFromStatus` | 7 | (UI) | (UI) |
| BranchPicker / GitActionsPicker jsdom | 4 | (UI) | (UI) |
| **UI total** | **11** | 296 | 307 |

### Manual checklist (PR B)

- BranchPicker opens, lists branches, search filters, switch works
- "Uncommitted changes: N" updates when external edit hits the file watcher
- No-repo state offers init affordance; confirmation toast prevents surprise
- GitWorkbenchDialog tabs each load + refresh independently; diff stat ⇄ full toggle works
- GitActionsPicker: commit (clean tree → skip; dirty → created)
- Commit-push-PR single-button flow renders URL in success state
- `gh missing` → form shows draft view with copy-able command
- 11-theme spot check on all chips + dialog
- Wired into BOTH `ChatInput` and `AgentView` (regression for dual-composer rule)

## 8. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Sandbox bypass via symlinks across mounts | low | high | Canonicalize candidate + every mount path on every IPC |
| `git init` in workspace surprises users | medium | low | 3s confirmation toast before init |
| Audit event volume during streamed agent edits | medium | low | Only mutating ops emit events |
| `gh auth` not run by user | high | low | Draft fallback ports verbatim; stderr surfaces via error sub-view |
| `git` binary missing on user's PATH | low | high | First-run `command_exists("git")` probe; surface settings-level error |
| `gitStatus` race with concurrent IDE git ops | medium | low | `--no-optional-locks` on reads (already in design) |
| Workspace switch mid-operation | low | low | BranchPicker / GitActionsPicker close popovers + reset state on `cwd` prop change |
| `MountRoot.editable=false` blocks legitimate workflow | medium | medium | Document W3 mount toggle; possible future inline "Make editable" affordance |
| Theme-token divergence (if2ai-prefixed classes) | low | low | Per-class decision at port time: either port into uClaw `globals.css` or replace with equivalents |

## 9. Out of scope (deferred)

- **Slash commands** (`/diff`, `/git`, `/commit-push-pr`) — uClaw uses skills/tools instead. A future Rust `agent/tools/builtin/git_*` tool could wrap the same IPC commands.
- **Worktrees** — `git worktree {list,add,remove,prune}`. Deferred indefinitely until use case emerges.
- **Inline file blame / git log** — read-only history. Add later if signal warrants.
- **Merge / rebase / stash / cherry-pick** — destructive ops; defer.
- **File-tree decorations** (M/A/D badges) — would require a debounced status cache keyed on `files_rail:change` events. Do NOT replicate if2Ai's "shell out fresh every call" pattern for ambient decorations.
- **Multi-remote support** — `git push --set-upstream origin <branch>` hardcodes `origin`. Direct port from if2Ai.
- **LLM-suggested commit messages** — agent-assisted "✨ Suggest" button. Plausible W7.
- **GitHub-issue creation surface** — backend supports `gh_create_issue` but PR B has no UI affordance.

## 10. Reference borrows summary (`/Users/ryanliu/Documents/IfAI/if2Ai`)

| Borrow | Source | Status |
|---|---|---|
| `modules/git/` 8-file layout + 1-concern-per-file discipline | `src-tauri/src/modules/git/{mod,error,command,...}.rs` | Verbatim (minus `worktree.rs`) |
| `CommitMessageFile` RAII tempfile pattern | `src-tauri/src/modules/git/commit.rs:40-83` | Verbatim |
| `--no-optional-locks` for read ops | `src-tauri/src/modules/git/command.rs:99-104` | Verbatim |
| `assert_cwd_in_registered_projects` sandbox check | `src-tauri/src/commands/git.rs:62-87` | Adapted: `ProjectManager` → `files_rail_list_mounts` + `MountRoot.editable` |
| `tokio::spawn_blocking` for local git, `tokio::process` for `gh` | `src-tauri/src/modules/git/command.rs:38-86` | Verbatim |
| `ViewState<T>` 5-state machine | `src/components/chat/GitWorkbenchDialog.tsx:43-48` | Verbatim |
| BranchPicker UI + dirty-tree confirm + no-repo init | `src/components/chat/BranchPicker.tsx` | Verbatim + `cwd` swap + `files_rail:change` refresh |
| GitActionsPicker `Mode` discriminated union state machine | `src/components/chat/GitActionsPicker.tsx:73-87` | Verbatim; split into 3 files |
| `gitCommitPushPr` one-shot composite | `src-tauri/src/commands/git.rs` + `src/modules/git/api.ts` | Verbatim |
| ANSI-C `$'…'` escaping for draft PR commands | `src/components/chat/GitActionsPicker.tsx:709-717` | Verbatim |
| Single sanctioned IPC layer (`src/modules/git/api.ts`) | if2Ai docs §9 | Verbatim |

Explicitly **NOT** borrowed:
- `ProjectManager` → replaced with `files_rail_list_mounts`
- Slash command handlers → uClaw uses skills/tools
- Audit event names `tool_execution_{started,finished,failed}` → using `git_op:*` family instead
- Worktree commands → out of scope
- if2Ai's `max-h-[420px]` no-virtualization diff container in chat cards → not relevant (workbench uses 500-line chunking)

## 11. Self-review

**Placeholder scan**: no "TBD" / "TODO" / vague requirements remain.

**Internal consistency**: `MountRoot.editable` referenced consistently across §3.5 (sandbox), §8 (risk), §9 (out-of-scope cache pattern). `gitCommitPushPr` referenced consistently across §3.4 (IPC table), §4.4 (PR flow). `Mode` state machine in §4.4 matches the 8 variants from if2Ai's source.

**Scope check**: each PR (~2200 LOC) is one implementation-plan-sized chunk. PR A ships independently testable (all Rust + IPC + api.ts + tests green); PR B builds on PR A's IPC layer.

**Ambiguity check**: "verbatim port" definition (§5) is exhaustive; the 3 permitted-adaptation cases are enumerated. `Result<T, String>` decision (§3.1) prevents any confusion vs uClaw's `crate::error::Error`. Auto-init confirmation toast (§4.2) is explicit about being a uClaw deviation from if2Ai.
