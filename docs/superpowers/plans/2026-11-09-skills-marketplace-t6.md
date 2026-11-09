# Skills Marketplace (T6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Browse, inspect, install (global or into a workspace), list and uninstall skills from skills.sh and skillsmp — in the 万花筒 → 技能 module and from chat — with one install path behind both.

**Architecture:** A new `src-tauri/src/skills_marketplace/` module owns the provider clients, the GitHub file fetch, the install service and its own Tauri commands. The two existing agent tools (`skill_marketplace_search`, `skill_install_from_marketplace`) become thin wrappers over it, so every install records a V25 row and shows up in 已安装. The UI is a 市场 sub-tab of the existing Kaleidoscope Skills module plus a custom in-chat result card.

**Tech Stack:** Rust (reqwest 0.12, serde_yml, rusqlite, thiserror, mockito/tempfile in tests), React + Jotai + shadcn (`sheet`, `dropdown-menu`), Vitest.

**Spec:** `docs/superpowers/plans/2026-11-09-uclaw-pi-backport.md` § T6. Source of the port: uclaw-pi `src-tauri/src/skills_marketplace/`, `src-tauri/src/commands/skills_marketplace.rs`, `ui/src/views/Kaleidoscope/modules/Skills/{SkillsMarketplaceTab,SkillMarketplaceDetailDrawer}.tsx`, `ui/src/shared/tool-rendering/tool-renderers/skill-marketplace-search-result.tsx` (read with `git -C ../uclaw-pi show main:<path>`). Port behavior and tests, not uclaw-pi's file layout (its `commands/`, `services/`, `lib/bridge/`, `features/` and `shared/` folders don't exist here).

## Decisions that differ from uclaw-pi

1. **Workspace install = a real copy, not a symlink + activation tag.** uclaw-pi linked `<ws>/.uclaw/skills/<slug>` to the global copy and tagged it so its tag-filtered manifest showed it only in that workspace. Here the system-prompt skills manifest (`SkillsRegistry::format_for_system_prompt_xml`) ignores `spaces.skill_tags`, so a tag changes nothing, and a symlink into a project folder points at `~/.uclaw` (dangling for collaborators). Workspace install therefore writes the files to `<ws>/.uclaw/skills/<slug>/` only — the same place `skill_write scope=project` uses — and registers that dir as a `Project` scan dir. **Known limit:** the registry is process-global, so a workspace skill is visible in every session once registered (same as `skill_write` today). Per-session isolation is out of scope.
2. **Commands live in `skills_marketplace/commands.rs`**, not `tauri_commands.rs` (precedent: `preview/commands.rs`, `stt/commands.rs`). Still registered in `main.rs` `invoke_handler!`.
3. **Keep this repo's GitHub resolver.** `agent/tools/builtin/skill_marketplace_resolve.rs::resolve_skill_path` (trees API, finds `**/<skill_id>/SKILL.md`) is more robust than uclaw-pi's URL parsing; the new GitHub fetch builds on it.
4. **Uninstall calls `reload()`, not `discover()`** — `discover()` never drops removed skills (the Halo uninstall has this bug; fixing that is a separate PR).
5. **Default provider is `skills_sh` everywhere** (uclaw-pi's tab defaulted to skillsmp — inconsistent).

## Global Constraints

- No new migration: reuse V25 `marketplace_standalone_installs(slug PK, item_type, version, installed_at, mcp_server_id)` with `item_type = 'skill'`, `version = <SKILL.md blob sha>`.
- No new crates. HTTP timeout 8 s; user agent `uClaw/0.1`.
- File fetch limits: ≤ 32 files, ≤ 512 KB each, top-level files only, `SKILL.md` required.
- Global install dir: `<data_dir>/skills/_marketplace/<owner>__<repo>__<path with / → __>/` (identical to today's agent tool, so existing installs stay valid).
- API keys: `settings` rows `skills_sh_api_key`, `skillsmp_api_key`; never returned to the frontend (status bool only); trimmed; empty value deletes the row.
- skills.sh: keyed `GET /api/v1/skills/search?q=&limit=` (Bearer) → `{data:[…]}`; anonymous `GET /api/search?q=&limit=` → `{skills:[{id,skillId,name,installs,source}]}`; 401/403 with a key retries once anonymously; `limit` clamped 1–200. `list`/`detail`/`audit` need a key.
- skillsmp: `GET https://skillsmp.com/api/v1/skills/search?q=&limit=&sortBy=stars`, key optional, same 401/403 fallback, `limit` clamped 1–100; map `author→source`, `stars→installs`, `githubUrl→install_url`, `skillUrl→url`.
- User-facing copy is Simplified Chinese. Every frontend change ships in the same PR as its backend.
- Pre-existing failures to ignore: Rust `shell::tests::test_daemon_mode_approval_unchanged`, `browser::runtime_pack_ipc::tests::print_real_status_diagnostics`, three `browser::provider_execution::tests::*cli_route*`; Vitest two Kaleidoscope `MemoryGraphView` tests; tsc baseline 71 error lines.

## Review Focus

1. **Re-install / update after upstream removed a file** — the old file must not linger in the install dir (delete, then write). Test in Task 3.
2. **Hostile file names from a provider** (`../x`, `/etc/x`, `C:\x`, `a\b`, `.`) — rejected before any write. Test in Task 3.
3. **Uninstalling a slug that belongs to a Halo `_standalone` automation install** (same V25 table) — must not touch it. Test in Task 4.
4. **Anonymous skills.sh rate limit (HTTP 429)** — the user sees "请求太频繁，请稍后再试或在设置中填写 API Key", not a raw error. Test in Task 1 (error variant) and Task 5 (copy).
5. **Workspace install for a workspace with an empty `spaces.path`** — clear error, nothing written globally. Test in Task 4.

---

## PR 1 — Marketplace browse / install / uninstall (backend + 市场 tab)

### Task 1: Module scaffold, types, skills.sh client

**Files:**
- Create: `src-tauri/src/skills_marketplace/mod.rs`, `types.rs`, `skills_sh.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod skills_marketplace;` after `pub mod skills;`)

**Interfaces:**
- Produces (`types.rs`, all `Serialize + Deserialize`, camelCase on the wire):
  - `SkillSummary { id, slug, name, source: String, installs: u64, source_type: String, install_url: String, url: String, description: String }` (all but `id/slug/name/source` `#[serde(default)]`)
  - `SkillFile { path: String, contents: String }`; `SkillDetail { id, source, slug, hash: String, files: Vec<SkillFile> }`
  - `SkillAudit { audits: Vec<SkillAuditEntry> }`; `SkillAuditEntry { status: String, risk_level: String, summary: String }`
  - `InstallScope { Global, Workspace }` (lowercase); `MarketplaceProvider { SkillsSh (default), Skillsmp }` (snake_case)
  - `MarketplaceError { MissingApiKey, RateLimited, Http(String), Install(String), Invalid(String) }` (thiserror)
- Produces (`skills_sh.rs`): `SkillsShClient::new(api_key: Option<String>)`, `::with_base(base: &str, api_key: Option<String>)`, `async fn search(&self, q: &str, limit: u32) -> Result<Vec<SkillSummary>, MarketplaceError>`, `list(view: &str, page: u32, per_page: u32)`, `detail(id: &str) -> SkillDetail`, `audit(id: &str) -> SkillAudit`.

- [ ] **Step 1: Write failing mockito tests in `skills_sh.rs`**: `search_parses_data_array` (keyed → `data`), `no_key_searches_anonymous_endpoint` (anon row `id` copied to `install_url`), `rejected_key_falls_back_to_anonymous_search` (401 then anon 200), `rate_limit_maps_to_rate_limited` (429 → `MarketplaceError::RateLimited`), `detail_requires_key` (`None` key → `MissingApiKey`), `detail_parses_files`, `audit_parses_risk_level`. Port the first three plus detail/audit from uclaw-pi `client.rs`.
- [ ] **Step 2: Run** `cd src-tauri && cargo test --lib skills_marketplace::skills_sh` — expect compile failure / FAIL.
- [ ] **Step 3: Implement** types and client per Global Constraints.
- [ ] **Step 4: Run** the same command — all PASS.
- [ ] **Step 5: Commit** `feat(skills_marketplace): module scaffold + skills.sh client`

### Task 2: skillsmp client + GitHub skill fetch

**Files:**
- Create: `src-tauri/src/skills_marketplace/skillsmp.rs`, `github.rs`
- Modify: `src-tauri/src/agent/tools/builtin/skill_marketplace.rs` (move its GitHub contents-download loop into `github.rs`; the tool calls the new fn — behavior unchanged)

**Interfaces:**
- Consumes: `resolve_skill_path(owner, repo, skill_id) -> ResolvedSkill { dir_path, branch }` (existing).
- Produces: `SkillsmpClient::new(api_key: Option<String>)`, `::with_base(..)`, `async fn search(&self, q: &str, limit: u32) -> Result<Vec<SkillSummary>, MarketplaceError>`.
- Produces (`github.rs`):
  - `pub struct GithubSkillRef { owner, repo, dir_path, git_ref: String }`
  - `pub fn parse_github_url(url: &str) -> Option<GithubSkillRef>` — `https://github.com/{o}/{r}/(tree|blob)/{ref}/{path}`
  - `pub fn parse_source(source: &str) -> Option<(String, String, Option<String>)>` — `owner/repo[/path]`
  - `pub async fn fetch_skill_dir(r: &GithubSkillRef) -> Result<SkillDetail, MarketplaceError>` (+ `fetch_skill_dir_with_base(api_base, r)` for tests); `slug = owner__repo__path`, `hash` = SKILL.md sha; limits per Global Constraints.
  - `pub async fn resolve_source(source: &str, skill_id: Option<&str>) -> Result<GithubSkillRef, MarketplaceError>` — URL form via `parse_github_url`; `owner/repo/path` direct; `owner/repo` + `skill_id` via `resolve_skill_path`.

- [ ] **Step 1: Write failing tests**: skillsmp `search_normalizes_skillsmp_rows`, `rejected_key_falls_back_to_anonymous`, `search_anonymous_sends_no_auth_header` (port); github `parses_tree_url`, `rejects_non_github_and_too_shallow` (port), `parse_source_splits_owner_repo_path`, `fetch_skill_dir_requires_skill_md` and `fetch_skill_dir_rejects_oversize_file` (mockito contents API, `download_url` pointing back at the mock server).
- [ ] **Step 2: Run** `cargo test --lib skills_marketplace` — FAIL.
- [ ] **Step 3: Implement**; repoint the agent install tool's download loop at `fetch_skill_dir`.
- [ ] **Step 4: Run** `cargo test --lib skills_marketplace` and `cargo test --lib skill_marketplace` — PASS (existing tool tests unchanged).
- [ ] **Step 5: Commit** `feat(skills_marketplace): skillsmp client + shared GitHub skill fetch`

### Task 3: Install service

**Files:**
- Create: `src-tauri/src/skills_marketplace/install.rs`

**Interfaces:**
- Produces:
  - `pub fn flatten_slug(id: &str) -> String`
  - `pub fn write_skill_files(root: &Path, slug: &str, detail: &SkillDetail) -> Result<PathBuf, MarketplaceError>` — removes an existing `root/slug` first, validates every path component, writes files.
  - `pub fn remove_skill_dir(root: &Path, slug: &str) -> Result<(PathBuf, bool), MarketplaceError>` — `(dir, existed)`.
  - `pub fn global_root(data_dir: &Path) -> PathBuf` (`data_dir/skills/_marketplace`); `pub fn workspace_root(ws: &Path) -> PathBuf` (`ws/.uclaw/skills`).
  - V25: `record_install(conn, slug, hash)`, `read_install_version(conn, slug) -> Option<String>`, `remove_install_row(conn, slug)`, `needs_update(conn, slug, latest_hash) -> bool` (all `item_type = 'skill'` only).

- [ ] **Step 1: Write failing tests** (tempfile + in-memory V25 table): port `slug_flattens_id`, `global_install_writes_files_under_marketplace`, `rejects_path_traversal`, `rejects_dot_slug_and_windows_abs_and_backslash`, `remove_skill_dir_deletes_install_and_guards_slug`, `records_and_reads_install_row`; add `reinstall_drops_files_removed_upstream` (install `{SKILL.md, a.md}`, reinstall `{SKILL.md}` → `a.md` gone) and `workspace_install_writes_a_real_copy` (`workspace_root(ws)/slug/SKILL.md` is a regular file, not a symlink).
- [ ] **Step 2: Run** `cargo test --lib skills_marketplace::install` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(skills_marketplace): install service (global + workspace copy, V25 tracking)`

### Task 4: Tauri commands

**Files:**
- Create: `src-tauri/src/skills_marketplace/commands.rs`
- Modify: `src-tauri/src/main.rs` (`invoke_handler!`: 7 commands)

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces (core fns are plain and tested; `#[tauri::command]` wrappers only pull `AppState`):
  - `pub(crate) fn read_api_key(conn: &Connection, provider: MarketplaceProvider) -> Option<String>`
  - `pub(crate) fn workspace_path(conn: &Connection, workspace_id: &str) -> Result<PathBuf, MarketplaceError>` — `spaces.path`, error `Invalid("工作区没有路径")` when empty/missing.
  - `pub(crate) async fn install_core(detail: SkillDetail, scope: InstallScope, ws: Option<&Path>, data_dir: &Path, conn: &Connection, registry: &mut SkillsRegistry) -> Result<String /*slug*/, MarketplaceError>` — Global: write under `global_root`, `add_scan_dir(dir, Marketplace)`; Workspace: write under `workspace_root(ws)`, `add_scan_dir(workspace_root(ws), Project)`; both `discover()`, `record_install`.
  - `pub(crate) fn uninstall_core(slug, data_dir, workspace_paths: &[PathBuf], conn, registry) -> Result<(), MarketplaceError>` — remove global dir if present + `remove_scan_dir`; remove `workspace_root(p)/slug` for each workspace; if nothing existed anywhere → `Ok(())` and leave V25 untouched (protects Halo `_standalone` rows); else `remove_install_row` + `registry.reload()`.
  - Commands: `search_skill_marketplace(query, limit?: u32 = 20, provider?)`, `list_skill_marketplace(view? = "trending", page? = 0, provider?)` (skillsmp → `[]`), `get_skill_marketplace_detail(id, provider?, source?)` (skills.sh+key → API detail; else `fetch_skill_dir(resolve_source(source or id))`), `get_skill_marketplace_audit(id, provider?)` (skillsmp/no key → empty), `check_skill_marketplace_update(id, provider?, source?) -> bool`, `install_skill_from_marketplace(id, scope, workspace_id?, provider?, source?) -> String`, `uninstall_skill_from_marketplace(slug)`. Errors map to `crate::error::Error` with the Chinese message of the variant (`RateLimited` → "请求太频繁，请稍后再试或在设置中填写 API Key").

- [ ] **Step 1: Write failing tests** (in-memory DB with `settings`, `spaces`, V25; tempdir data_dir; `SkillsRegistry::new()`): `install_global_registers_and_records`, `install_workspace_writes_copy_and_registers_project_dir`, `workspace_path_errors_when_empty` (Review Focus 5: assert global root stays empty), `uninstall_removes_files_row_and_reloads` (skill gone from `registry.list()`), `uninstall_unknown_slug_leaves_standalone_row` (Review Focus 3: pre-insert a V25 row for `_standalone` slug with no marketplace dir → row survives), `read_api_key_none_when_unset`.
- [ ] **Step 2: Run** `cargo test --lib skills_marketplace::commands` — FAIL.
- [ ] **Step 3: Implement**; register all 7 in `main.rs`.
- [ ] **Step 4: Run** tests — PASS; `cargo build 2>&1 | grep -E "^error"` — empty.
- [ ] **Step 5: Commit** `feat(skills_marketplace): Tauri commands (search/list/detail/audit/update/install/uninstall)` — body notes the `main.rs` registration as an adjacent edit.

### Task 5: 市场 sub-tab + detail drawer

**Files:**
- Modify: `ui/src/lib/types.ts` (add `MarketplaceProvider`, `MarketplaceSkillSummary`, `MarketplaceSkillDetail`, `MarketplaceSkillAudit`; widen the skill provenance type to include `'marketplace' | 'plugin'`)
- Modify: `ui/src/lib/tauri-bridge.ts` (7 functions: `searchSkillsMarketplace`, `listSkillsMarketplace`, `getSkillMarketplaceDetail`, `getSkillMarketplaceAudit`, `checkSkillMarketplaceUpdate`, `installSkillFromMarketplace`, `uninstallSkillFromMarketplace`)
- Create: `ui/src/views/Kaleidoscope/modules/Skills/SkillsMarketplaceTab.tsx`, `SkillMarketplaceDetailDrawer.tsx`, `SkillsMarketplaceTab.test.tsx`
- Modify: `ui/src/views/Kaleidoscope/modules/Skills/SkillsModule.tsx` (filter tab `{ value: 'marketplace', label: '市场' }`, render the tab)

**Interfaces:**
- Consumes: Task 4 command names/args (camelCase in `invoke`: `query, limit, provider, view, page, id, source, scope, workspaceId, slug`).
- Produces: `SkillsMarketplaceTab({ query, onError })`; `SkillMarketplaceDetailDrawer({ open, onOpenChange, skill, provider, onInstalled })`.

Behavior (port uclaw-pi, with fixes): provider toggle defaults to `skills_sh`; empty query on skills.sh shows trending, on skillsmp shows a hint; 300 ms debounce; each row has [全局] / [本工作区] (本工作区 disabled when no `activeWorkspaceIdAtom`); row click opens the drawer (SKILL.md preview, audit risk badge HIGH/中/低/未审计, HIGH asks to confirm before install); 已安装 section lists `listStandaloneInstalls()` with `itemType === 'skill'`, each with 卸载 → 确认卸载？[确认][取消]; error copy maps `RateLimited` / missing key.

- [ ] **Step 1: Write failing tests** (`vi.mock('@/lib/tauri-bridge')`): `defaults_to_skills_sh_and_shows_trending`, `install_global_calls_bridge_with_scope_global`, `workspace_button_disabled_without_active_workspace`, `uninstall_requires_confirmation_then_calls_bridge`, `rate_limited_error_shows_chinese_hint`.
- [ ] **Step 2: Run** `cd ui && npx vitest run src/views/Kaleidoscope/modules/Skills` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests — PASS; `npx tsc --noEmit 2>&1 | wc -l` — ≤ 71.
- [ ] **Step 5: Commit** `feat(ui): skills marketplace tab + detail drawer in 万花筒 → 技能`

---

## PR 2 — API keys

### Task 6: Key status/set commands + key card

**Files:**
- Modify: `src-tauri/src/skills_marketplace/commands.rs`, `src-tauri/src/main.rs`
- Create: `ui/src/views/Kaleidoscope/modules/Skills/MarketplaceApiKeysDialog.tsx` (+ `.test.tsx`)
- Modify: `ui/src/lib/tauri-bridge.ts`, `SkillsMarketplaceTab.tsx` (a "API Key" button in the tab header opens the dialog)

**Interfaces:**
- Produces: `pub(crate) fn set_api_key(conn, provider, value: &str)` (trim; empty → DELETE; else INSERT OR REPLACE); commands `get_marketplace_api_key_status() -> { skillsSh: bool, skillsmp: bool }`, `set_marketplace_api_key(provider, key)`. Bridge: `getMarketplaceApiKeyStatus`, `setMarketplaceApiKey`.

- [ ] **Step 1: Write failing tests**: Rust `api_key_round_trips_and_clears_without_echo` (set → status true → read returns value server-side; set "  " → status false); Vitest `dialog_shows_status_never_the_key` (status true → shows 已设置, input empty), `saving_calls_bridge_and_clears_input`.
- [ ] **Step 2: Run** both — FAIL.
- [ ] **Step 3: Implement**; register the 2 commands.
- [ ] **Step 4: Run** — PASS; tsc ≤ 71.
- [ ] **Step 5: Commit** `feat(skills_marketplace): marketplace API keys (status-only) + key dialog`

---

## PR 3 — One install path for the agent + in-chat install card

### Task 7: Agent tools delegate to the module

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/skill_marketplace.rs`, `src-tauri/src/agent/tools/registry_build.rs:298-309`

**Interfaces:**
- Consumes: `read_api_key`, `SkillsShClient`, `SkillsmpClient`, `resolve_source`, `fetch_skill_dir`, `install_core`.
- Produces: `SkillMarketplaceSearchTool::new(skills_sh_key: Option<String>, skillsmp_key: Option<String>)` (keys read in `registry_build` from `state.db`); params `query`, `limit` (default 8, max 20), `provider` (`skills_sh` | `skillsmp`, default `skills_sh`); results sorted by installs desc (stable); output `{ok, query, provider, resultCount, results:[{id,name,source,installs,installUrl,description}]}`. `skill_install_from_marketplace` gains `scope` (`global` default | `workspace` → session workspace) and installs via `install_core`, so it records V25; still emits `agent:skill-installed`.

- [ ] **Step 1: Write failing tests**: port `sort_by_popularity_orders_desc_and_is_stable`, `to_result_json_maps_fields`; add `install_tool_records_v25_row` (tempdir + in-memory DB, `fetch` stubbed via a `SkillDetail` fixture fed to `install_core`).
- [ ] **Step 2: Run** `cargo test --lib skill_marketplace` — FAIL.
- [ ] **Step 3: Implement**; delete the now-unused direct HTTP code in the tool.
- [ ] **Step 4: Run** — PASS; full `cargo test --lib` shows only the pre-existing failures.
- [ ] **Step 5: Commit** `refactor(agent): marketplace tools delegate to skills_marketplace (one install path)`

### Task 8: In-chat search result card

**Files:**
- Create: `ui/src/components/agent/tool-renderers/SkillMarketplaceSearchResult.tsx` (+ `.test.tsx`)
- Modify: `ui/src/components/agent/tool-renderers/index.tsx` (`case 'skill_marketplace_search'`)
- Modify: `ui/src/hooks/useGlobalAgentListeners.ts` (listen `agent:skill-installed` → bump a new `skillsRefreshAtom` in `ui/src/atoms/agent-atoms.ts`)
- Modify: `ui/src/views/Kaleidoscope/modules/Skills/SkillsMarketplaceTab.tsx` and `SkillsModule.tsx` (refetch the 已安装 list / skill list when `skillsRefreshAtom` changes)

**Interfaces:**
- Consumes: `installSkillFromMarketplace` (Task 5), tool JSON (Task 7).
- Produces: `SkillMarketplaceSearchResult({ result: string, isError: boolean })`.

Behavior (port uclaw-pi `ecef9383` design): parse tool JSON (provider fallback `skills_sh`); rows sorted by installs; per-row state `idle → installing → installed | error`; Install button with dropdown 安装到全局 / 安装到本工作区 (本工作区 disabled without an active workspace); after install shows 已安装（全局/本工作区）; provider-neutral Chinese empty/error states.

- [ ] **Step 1: Write failing tests**: `renders_rows_sorted_by_installs`, `install_global_shows_installed_label`, `invalid_json_falls_back_to_default_renderer_text`.
- [ ] **Step 2: Run** `npx vitest run src/components/agent/tool-renderers` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS; full Vitest shows only the 2 pre-existing failures; tsc ≤ 71.
- [ ] **Step 5: Commit** `feat(ui): in-chat skill marketplace card with install-to-workspace`

---

## Out of scope / follow-ups

- Per-session isolation of workspace skills (registry is process-global).
- Halo `uninstall_marketplace_item` calls `discover()` instead of `reload()` — separate fix PR.
- Dangling `create_user_skill` / `delete_user_skill` bridge calls with no Rust command — separate fix.
- Update-available badges (`check_skill_marketplace_update` is wired but no UI uses it, same as uclaw-pi).
