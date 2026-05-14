# Phase 3b-α — Bundled Skills + Capability Mapping — Design

**Status:** Draft → ready to plan
**Author:** Claude (brainstormed with Ryan, 2026-05-14)
**Date:** 2026-05-14
**Scope:** Phase 3b first slice. Follow-ups (β / γ / δ) deferred — see § 9.

---

## 1. Problem

Phase 3a shipped the marketplace UI + automation install, but install is **incomplete**:

- `install_human` ([src-tauri/src/automation/marketplace/mod.rs:198](../../../src-tauri/src/automation/marketplace/mod.rs)) parses the spec, records `requires_skills` / `requires_mcps` as name lists, and writes the `automation_specs` row. **It does not fetch or place the skill bundle files** referenced by `requires.skills[].bundled` + `files`, and it does not register or verify the MCPs in `requires.mcps[]`.
- DHP specs (e.g. xiaohongshu-keyword-monitor/spec.yaml) drive every browser-based step via the bundled skill files. Without them present at `~/.uclaw/skills/<...>/SKILL.md`, the automation **cannot execute** — the LLM is told to call `browser_run` with a `.claude/skills/<skill_id>/index.js` path that doesn't exist on disk.
- The 「我的应用」 tab is a Phase 3a stub. Users have no surface to see which skills / capabilities a given automation pulled in or to uninstall an automation.

### 1.1 Reality check on `requires.mcps[]`

A scan of the DHP registry (34 entries) shows **only one MCP id is ever declared**: `ai-browser` (in 25 of 34). In hello-halo, `ai-browser` is **not a standard MCP** — it's the Electron app's in-process browser subsystem (see `hello-halo/src/renderer/stores/ai-browser.store.ts:109` registering `halo-ai-browser`). In uClaw, the equivalent capability already exists as the chromiumoxide-based `src-tauri/src/browser/` module, wired into the agent dispatcher as a built-in tool.

So `requires.mcps: [{id: "ai-browser"}]` is **not** a "go install this MCP server" directive — it's a **capability declaration** that the host's built-in browser tool must be available. uClaw needs a tiny mapping table that recognises this case; **no MCP server install happens for it**. The "general external MCP install" problem is real but separate, deferred to Phase 3b-γ.

---

## 2. Goals

1. **Working installs.** After `install_marketplace_human(slug)` completes, every file the bundled skill needs is present on disk and discoverable by `SkillsRegistry`, and every `requires.mcps[]` entry has been validated (built-in or warning).
2. **Surface what was installed.** 「我的应用」 tab lists installed automations and, under each, the bundled skills and required capabilities pulled in.
3. **Clean uninstall.** Uninstalling an automation removes the automation row, its bundled skill files, and the corresponding `SkillsRegistry` scan directory — no orphan files, no other automation's skills harmed.

## 3. Non-goals (deferred to Phase 3b-β/γ/δ)

- **Phase 3b-β** — skill bundle updates. When an automation is upgraded and its bundled skill set or skill versions change, what to keep / replace / prune. Worth its own design pass once we have real version-bump events from the DHP repo.
- **Phase 3b-γ** — standalone skill / MCP marketplace entries. DHP registry currently exposes only `type: automation` entries. When DHP adds `type: skill` or `type: mcp` (or we proxy other registries like Smithery), the marketplace UI will start showing standalone cards, and the install path will need a non-bundled branch.
- **Phase 3b-δ** — multi-registry + capability map upgrade. Once γ lands, the built-in `ai-browser → builtin-browser` mapping is no longer the only entry; the capability table grows into a DB or JSON resource with user-configurable rules. Out of scope here.

## 4. Architecture

### 4.1 New table: `automation_installed_skills`

Migration **V22** — next free integer after CLAUDE.md's open claims (V19 workspace-skill-tags, V20 + V21 humane-automation).

```sql
CREATE TABLE IF NOT EXISTS automation_installed_skills (
    automation_slug TEXT NOT NULL,
    skill_id        TEXT NOT NULL,
    installed_at    INTEGER NOT NULL,
    file_count      INTEGER NOT NULL,
    PRIMARY KEY (automation_slug, skill_id)
);
CREATE INDEX IF NOT EXISTS idx_aut_inst_skills_slug
    ON automation_installed_skills(automation_slug);
```

`file_count` is a cheap drift detector — if a future re-scan finds fewer files than recorded, something deleted them. Not used for logic in this PR; only for diagnostics.

### 4.2 Skill bundle layout (confirmed against live DHP repo)

```
packages/digital-humans/<automation_slug>/
├── spec.yaml
└── skills/
    └── <skill_id>/
        ├── SKILL.md
        └── <other files listed in spec.requires.skills[].files>
```

Local install layout (per-automation isolation):

```
~/.uclaw/skills/
├── <user-handwritten-skill>/             # user-managed flat dir, untouched
└── _marketplace/                          # marketplace-owned subtree (gitignored convention)
    └── <automation_slug>/
        └── <skill_id>/
            ├── SKILL.md
            └── ...
```

The `_marketplace/` prefix:
- Visually separates marketplace-managed files from user-written ones.
- Lets the uninstall path delete `_marketplace/<automation_slug>/` without ever touching user skills.
- The leading `_` matches conventional "private subtree" naming (similar to `__pycache__` or `_build`).

### 4.3 Install flow extensions

`install_human` ([mod.rs:198](../../../src-tauri/src/automation/marketplace/mod.rs)) gains three new phases between "parse spec" and "write automation_specs row":

```
┌──────────────────────────────────────────────────────────┐
│ fetching_spec       (existing)                           │
│ parsing             (existing)                           │
│ fetching_skills     (new) — bundled skill files          │
│ validating_caps     (new) — requires.mcps[] capability check │
│ installing          (existing) — automation_specs row    │
│ registering_skills  (new) — SkillsRegistry scan + write installed_skills │
│ activating          (existing)                           │
│ complete            (existing)                           │
└──────────────────────────────────────────────────────────┘
```

**Phase: fetching_skills.** Walk `spec.requires.skills[]`. For each entry where `bundled == true`:

1. For each file in `entry.files`, GET `<registry.base_url>/<registry.path>/skills/<skill_id>/<file>`.
2. Write to staging dir `~/.uclaw/skills/.staging/<automation_slug>/<skill_id>/<file>`.
3. After all files of all bundled skills are written successfully, **atomic rename** `.staging/<automation_slug>/` → `_marketplace/<automation_slug>/`.

On any error during fetch:
- `fs::remove_dir_all("~/.uclaw/skills/.staging/<automation_slug>/")`
- Abort install; don't write `automation_specs` row.
- Return `Err` with a phase=`fetching_skills` progress event.

The staging dir is the rollback insurance — partial state never reaches the real skills tree.

**Phase: validating_caps.** Walk `spec.requires.mcps[]`. For each entry:

```rust
match resolve_capability(&entry.id) {
    Some(BuiltinCapability::Browser) => Ok(()),       // ai-browser → built-in
    None => {
        // Don't abort; surface a warning toast via progress channel + return Ok.
        emit(
            "validating_caps",
            percent,
            Some(&format!(
                "automation requires MCP '{}' which is not yet supported; install completed but the automation may not run",
                entry.id
            )),
        );
        Ok(())
    }
}
```

Non-blocking on purpose — the user can install, see the warning, and decide. Phase 3b-γ converts the warning into an actionable install path.

**Phase: registering_skills.** After `automation_specs` row insert:
1. `INSERT INTO automation_installed_skills (...)` one row per bundled skill installed.
2. Ensure `SkillsRegistry` has a scan dir for `~/.uclaw/skills/_marketplace/<automation_slug>/`. Add it if missing (use new provenance `SkillProvenance::Marketplace`).
3. Trigger a registry re-scan so the freshly installed skills are immediately discoverable by the agent dispatcher.

### 4.4 Uninstall flow

New Tauri command `uninstall_marketplace_human(slug: String)`:

```rust
1. Read automation_installed_skills WHERE automation_slug = ?
2. DELETE FROM automation_specs WHERE source = 'marketplace'
                            AND source_ref = ?
3. DELETE FROM automation_installed_skills WHERE automation_slug = ?
4. fs::remove_dir_all("~/.uclaw/skills/_marketplace/<slug>/")
   (ignore NotFound; log + ignore other errors)
5. SkillsRegistry::remove_scan_dir + trigger re-scan
6. Emit a marketplace-update event so the UI invalidates its installed-list cache.
```

Order matters: DB rows go first so that if the FS delete fails we don't leave a "ghost installed" row.

### 4.5 Capability map

`src-tauri/src/automation/capability_map.rs` (new file, ~30 lines):

```rust
pub enum BuiltinCapability {
    Browser,  // mapped to src-tauri/src/browser/ + agent built-in tool
}

pub fn resolve_capability(mcp_id: &str) -> Option<BuiltinCapability> {
    match mcp_id {
        "ai-browser" => Some(BuiltinCapability::Browser),
        _ => None,
    }
}
```

Deliberately a `match` in code, not a config table. The only mapping today is `ai-browser`. Phase 3b-γ rewrites this into a DB-backed `mcp_capability_rules` table when there are >2 mappings AND we need user-configurable overrides. Doing it now is YAGNI.

### 4.6 SkillsRegistry extension

`SkillProvenance` gains a `Marketplace` variant. `SkillsRegistry` already supports multiple `scan_dirs` via `add_scan_dir` (`skills.rs:505`) — no API change needed there; we just add new dirs at install time and remove at uninstall.

A minor change to `app.rs` boot: on startup, walk `~/.uclaw/skills/_marketplace/` and `add_scan_dir` each direct child as `Marketplace` provenance. This way a user who deletes `uclaw.db` but keeps `~/.uclaw/skills/` still sees their marketplace skills (defensive — the DB is the source of truth, but the FS is the actual disk state).

---

## 5. UI: 「我的应用」 tab (`AppsView`)

New file `ui/src/components/automation/AppsView.tsx`.

```
┌─ AppsView ────────────────────────────────────────────────┐
│                                                            │
│  以下是已安装数字人随附的 skill / 能力依赖。               │
│  独立 skill / MCP 商店在 Phase 3b 后续切片开放。          │
│                                                            │
│  ┌─ [📰] 小红书关键词监控   v4.0.0    [卸载] ────────┐   │
│  │                                            ▼      │   │
│  │  [展开收起，default 收起]                          │   │
│  └──────────────────────────────────────────────────┘   │
│  ┌─ [💼] AI 每日新闻播报    v1.0.0    [卸载] ────────┐   │
│  │  [展开]                                           │   │
│  │   Bundled Skills                                   │   │
│  │     ▪ xhs-search · Browser script that collects... │   │
│  │       ~/.uclaw/skills/_marketplace/.../xhs-search/ │   │
│  │   Required Capabilities                            │   │
│  │     ✓ ai-browser  · 已映射到 uClaw 内建浏览器       │   │
│  └──────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────┘
```

### 5.1 Data shape

New Tauri command `list_installed_marketplace_automations() -> Vec<InstalledAutomation>`:

```rust
pub struct InstalledAutomation {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub icon: Option<String>,           // category keyword, fed to CategoryIcon
    pub bundled_skills: Vec<InstalledSkillBrief>,
    pub required_capabilities: Vec<CapabilityCheck>,
}

pub struct InstalledSkillBrief {
    pub skill_id: String,
    pub description: Option<String>,    // pulled from SKILL.md frontmatter at scan time, cached in registry
    pub install_path: String,
    pub file_count: i64,
}

pub struct CapabilityCheck {
    pub mcp_id: String,
    pub status: CapabilityStatus,       // "mapped" | "missing"
    pub mapped_to: Option<String>,      // "uClaw 内建浏览器" when status=mapped
}
```

Composed by joining `automation_specs` × `automation_installed_skills` × in-memory `SkillsRegistry` (for descriptions) × `capability_map::resolve_capability`.

### 5.2 UI conventions (uClaw Design DNA — same contract as Phase 3a)

- Card: `rounded-xl border border-border/50 bg-card` + no shadow
- Expand transition: `motion/react` `duration: 0.22, ease: [0.32, 0.72, 0, 1]`
- Type sizes: `text-[14px]` card title, `text-[13px]` skill row, `text-[11px]` group header / meta
- Capability checks: `bg-success-bg / text-success` ✓ for mapped, `bg-muted / text-muted-foreground` ⚠ for missing
- Uninstall button: secondary style with `confirm()` modal (avoid accidental deletes)

### 5.3 Empty state

When `list_installed_marketplace_automations()` returns []:

```
[暂无已安装的数字人]

去 [应用商店] 安装一个，或者关闭此面板回到聊天。
```

`应用商店` is a button that flips `automationsSubviewAtom` to `'store'`.

---

## 6. Error handling

| Failure mode | Behaviour |
|---|---|
| HTTP 404 on skill file fetch | Roll back staging, abort install, surface `phase=fetching_skills` error toast with the missing path |
| Network timeout on fetch | Same as 404 — staging is the rollback boundary |
| Disk full during write | Same; the staging dir is the only thing written before rename |
| `~/.uclaw/skills/_marketplace/<slug>/` already exists (e.g. user re-installs after a manual delete left rows in DB) | Delete the existing dir first; treat as "re-install" |
| `automation_installed_skills` row exists but FS missing | On boot, log a warning + lazy-recover by deleting the row (the next install will write it fresh) |
| Capability lookup miss | Non-fatal toast warning; install completes |
| SkillsRegistry re-scan fails | Log error; the next agent invocation will retry the scan — not user-visible |

## 7. Tests

### 7.1 Rust (inline `#[cfg(test)]`)

- `installs_bundled_skills_atomically` — full happy path: spec with `bundled: true` skill, mock HTTP server serving the files, assert files land at the right path AND `automation_installed_skills` has the expected row.
- `rolls_back_staging_on_fetch_error` — one of the bundle files 404s; assert no `_marketplace/<slug>/` exists AND no `automation_specs` row exists AND no `automation_installed_skills` row exists.
- `uninstall_removes_files_and_rows` — install then uninstall; assert all three side effects undone, AND user's flat `~/.uclaw/skills/<some-user-skill>/` is untouched.
- `capability_map_resolves_ai_browser` — `resolve_capability("ai-browser")` returns `Some(Browser)`; everything else returns `None`.
- `validate_capabilities_warning_does_not_abort_install` — install with `requires.mcps: [{id: "foobar"}]`; assert install completes AND a warning was emitted on the progress channel.

### 7.2 Vitest (frontend)

- `AppsView.test.tsx` — renders cards, expands on click, shows skills + capabilities, calls `uninstall_marketplace_human` with the right slug.
- `marketplace-i18n.test.ts` already exists; extend with cases for `description` field on `InstalledSkillBrief` falling back gracefully when SKILL.md has no description.

---

## 8. Migration registry update

CLAUDE.md's "Active migration registry" gets one new row:

| V | What | Status |
|---|---|---|
| V22 | automation_installed_skills + indexes | **this PR** |

If `humane-automation` (V20 + V21 open) or `workspace-skill-tags` (V19 open) lands before this PR, re-check; current claim is V22 assuming all three open PRs land first. If two of them collide we bump again — confirm at merge time, not now.

---

## 9. Phase 3b roadmap (for context, not in scope here)

| Sub-slice | Scope |
|---|---|
| **Phase 3b-α (THIS PR)** | Bundled skill install + capability map + Apps tab |
| Phase 3b-β | Skill bundle upgrade flow when automation version bumps |
| Phase 3b-γ | Standalone skill / MCP marketplace entries (requires DHP repo schema additions; not pure-uClaw work) |
| Phase 3b-δ | Multi-registry sources (automation_registries table + UI) and capability map → table |
| Phase 3b-ε | Proxy adapters: Smithery / official MCP Registry / SkillHub |
| Phase 3b-ζ | Local hello-halo workspace as a registry source |

α is the prerequisite for everything downstream — every other slice assumes installs actually work.

---

## 10. Done criteria

- [ ] V22 migration is idempotent (re-running boot doesn't error)
- [ ] Installing xiaohongshu-keyword-monitor results in `~/.uclaw/skills/_marketplace/xiaohongshu-keyword-monitor/xhs-search/SKILL.md` AND `index.js` present
- [ ] Agent dispatch can find `xhs-search` via `SkillsRegistry.lookup`
- [ ] Uninstalling the same automation removes the directory AND the registry row AND the scan dir
- [ ] Installing an automation that requires an unmapped MCP produces a warning toast but completes
- [ ] 「我的应用」 tab lists the installed automation, expand shows the bundled skill, capability is ✓
- [ ] All Rust + Vitest tests pass
- [ ] CLAUDE.md migration registry updated to reflect V22 claim
