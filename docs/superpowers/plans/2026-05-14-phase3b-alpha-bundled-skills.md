# Phase 3b-α — Bundled Skills + Capability Mapping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make automation install actually deliver a working automation — fetch and place the bundled skill files referenced by `spec.requires.skills[]`, validate `requires.mcps[]` against a built-in capability table, and surface what got installed via a new 「我的应用」 tab.

**Architecture:** Extend `install_human` with three new phases (`fetching_skills` / `validating_caps` / `registering_skills`) that fetch skill files into a staging dir, atomically rename into `~/.uclaw/skills/_marketplace/<slug>/`, validate MCP deps against a small `capability_map` module, and record the install in a new `automation_installed_skills` table (V22). `SkillsRegistry` gains a `Marketplace` provenance variant + `remove_scan_dir`. Uninstall is the inverse: delete rows + remove the `_marketplace/<slug>/` subtree. A new `AppsView` replaces the Phase 3a stub, listing installed automations with their bundled skills and capability checks.

**Tech Stack:** Rust (rusqlite, reqwest, tokio), React 18 + TypeScript + Jotai, sqlite migrations.

**Spec:** `docs/superpowers/specs/2026-05-14-phase3b-alpha-bundled-skills-design.md`

**Pre-flight state confirmed against the live tree (2026-05-14):**
- Last shipped migration is V23a (marketplace cache). V22 is unused. Our new migration claims **V22**.
- `SkillProvenance` has `Bundled / User / Project` — we add `Marketplace`.
- `SkillsRegistry::add_scan_dir` exists (`skills.rs:505`); no `remove_scan_dir` yet — we add it.
- `halo_adapter` already has `fetch_index` + `fetch_spec_yaml` with mirror fallback — we add `fetch_skill_file` using the same `fetch_with_fallback` helper.
- `install_human` rejects non-`automation` app_type at `marketplace/mod.rs:230`; our changes happen between "parse spec" and "write automation_specs row".

---

### Task 1: V22 migration — `automation_installed_skills`

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add `SQL_V22` constant + a call in `run()`)
- Test: `src-tauri/src/db/migrations.rs` inline tests module

- [ ] **Step 1: Write the failing test**

Append to the existing `mod tests` block at the bottom of `migrations.rs`:

```rust
#[test]
fn v22_creates_automation_installed_skills_table() {
    let conn = Connection::open_in_memory().unwrap();
    super::run(&conn).expect("migrations run");

    // Inserting a row should succeed.
    conn.execute(
        "INSERT INTO automation_installed_skills \
            (automation_slug, skill_id, installed_at, file_count) \
            VALUES (?, ?, ?, ?)",
        rusqlite::params!["xhs-monitor", "xhs-search", 1715000000_i64, 2_i64],
    )
    .expect("insert ok");

    // PK collision should error.
    let dup = conn.execute(
        "INSERT INTO automation_installed_skills \
            (automation_slug, skill_id, installed_at, file_count) \
            VALUES (?, ?, ?, ?)",
        rusqlite::params!["xhs-monitor", "xhs-search", 1715000000_i64, 2_i64],
    );
    assert!(dup.is_err(), "PK should reject duplicate");

    // The companion index must exist.
    let idx_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='index' AND name='idx_aut_inst_skills_slug'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(idx_count, 1);
}

#[test]
fn v22_is_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    super::run(&conn).expect("first run");
    super::run(&conn).expect("second run must not error (CREATE IF NOT EXISTS)");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run from worktree root: `cd src-tauri && cargo test --lib v22_creates_automation_installed_skills_table`
Expected: FAIL — `no such table: automation_installed_skills`.

- [ ] **Step 3: Add the SQL constant**

Insert near the other `SQL_V*` constants in `migrations.rs` (after `const SQL_V21:` block):

```rust
/// V22 — automation_installed_skills.
///
/// Records which bundled skills each marketplace-installed automation pulled
/// in. Read by AppsView to enumerate "what got installed alongside this
/// automation" and by uninstall to delete the right files.
///
/// file_count is a cheap drift detector — diagnostic only in this PR.
const SQL_V22: &str = "
CREATE TABLE IF NOT EXISTS automation_installed_skills (
    automation_slug TEXT NOT NULL,
    skill_id        TEXT NOT NULL,
    installed_at    INTEGER NOT NULL,
    file_count      INTEGER NOT NULL,
    PRIMARY KEY (automation_slug, skill_id)
);
CREATE INDEX IF NOT EXISTS idx_aut_inst_skills_slug
    ON automation_installed_skills(automation_slug);
";
```

- [ ] **Step 4: Wire into `run()`**

In `migrations.rs::run()`, add a V22 block **between V21 and V23a** (chronological order matters less than putting it next to its neighbours):

```rust
    // V22: automation_installed_skills — tracks bundled skills per automation.
    tracing::debug!("Running migration V22: automation_installed_skills");
    for stmt in SQL_V22.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V22 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib v22`
Expected: both tests PASS.

Run full migrations suite to make sure nothing regressed:
`cd src-tauri && cargo test --lib migrations 2>&1 | tail -10`
Expected: all migration tests green.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V22 — automation_installed_skills

Records which bundled skills each marketplace-installed automation
pulls in. PK (automation_slug, skill_id). Read by AppsView to
enumerate dependencies and by uninstall to delete the right files."
```

---

### Task 2: Capability map module

**Files:**
- Create: `src-tauri/src/automation/capability_map.rs`
- Modify: `src-tauri/src/automation/mod.rs` (add `pub mod capability_map;`)

- [ ] **Step 1: Write the failing test**

Create the file with only the test module first:

```rust
//! Map MCP-id capability declarations from automation specs to uClaw's
//! in-process built-in tools. Today's only mapping is `ai-browser` → the
//! chromiumoxide-backed `browser/` module. Phase 3b-γ replaces this match
//! with a configurable table.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_browser_resolves_to_builtin_browser() {
        assert_eq!(
            resolve_capability("ai-browser"),
            Some(BuiltinCapability::Browser)
        );
    }

    #[test]
    fn unknown_id_returns_none() {
        assert_eq!(resolve_capability("totally-fake-mcp"), None);
        assert_eq!(resolve_capability(""), None);
    }

    #[test]
    fn human_label_is_chinese_when_mapped() {
        // UI surfaces this in the "Required Capabilities" row.
        assert_eq!(
            human_label(BuiltinCapability::Browser),
            "uClaw 内建浏览器"
        );
    }
}
```

Add `pub mod capability_map;` to `src-tauri/src/automation/mod.rs` (just below the other `pub mod` declarations).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib capability_map`
Expected: FAIL — `resolve_capability` / `BuiltinCapability` / `human_label` don't exist.

- [ ] **Step 3: Implement**

Replace the file content (keeping the test module) with:

```rust
//! Map MCP-id capability declarations from automation specs to uClaw's
//! in-process built-in tools. Today's only mapping is `ai-browser` → the
//! chromiumoxide-backed `browser/` module. Phase 3b-γ replaces this match
//! with a configurable table.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinCapability {
    /// chromiumoxide-backed browser, surfaced via agent dispatcher's
    /// `browser_navigate` / `browser_run` / `browser_evaluate` tools.
    Browser,
}

pub fn resolve_capability(mcp_id: &str) -> Option<BuiltinCapability> {
    match mcp_id {
        "ai-browser" => Some(BuiltinCapability::Browser),
        _ => None,
    }
}

/// Chinese label for the "Required Capabilities" UI row.
pub fn human_label(cap: BuiltinCapability) -> &'static str {
    match cap {
        BuiltinCapability::Browser => "uClaw 内建浏览器",
    }
}

#[cfg(test)]
mod tests {
    // ... (same test module as Step 1)
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd src-tauri && cargo test --lib capability_map`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/capability_map.rs src-tauri/src/automation/mod.rs
git commit -m "feat(automation): capability_map module

Maps DHP-style requires.mcps[].id declarations to uClaw built-in tools.
Today only ai-browser → BuiltinCapability::Browser. Phase 3b-γ will
replace this match with a DB-backed table once real external MCPs need
configurable wiring."
```

---

### Task 3: `SkillsRegistry` — add `Marketplace` provenance + `remove_scan_dir`

**Files:**
- Modify: `src-tauri/src/skills.rs` (extend enum at `:220`, add method near `:505`)
- Test: `src-tauri/src/skills.rs` inline tests

- [ ] **Step 1: Write the failing test**

Add to the existing `tests` module:

```rust
#[test]
fn remove_scan_dir_drops_directory_from_scan_set() {
    let tmp = tempfile::tempdir().unwrap();
    let dir_a = tmp.path().join("a");
    let dir_b = tmp.path().join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let mut reg = SkillsRegistry::new();
    reg.add_scan_dir(dir_a.clone(), SkillProvenance::User);
    reg.add_scan_dir(dir_b.clone(), SkillProvenance::Marketplace);
    assert_eq!(reg.scan_dirs.len(), 2);

    reg.remove_scan_dir(&dir_b);
    assert_eq!(reg.scan_dirs.len(), 1);
    assert_eq!(reg.scan_dirs[0].0, dir_a);

    // Removing a non-existent dir is a no-op.
    reg.remove_scan_dir(&tmp.path().join("nonexistent"));
    assert_eq!(reg.scan_dirs.len(), 1);
}

#[test]
fn marketplace_provenance_serializes_lowercase() {
    let json = serde_json::to_string(&SkillProvenance::Marketplace).unwrap();
    assert_eq!(json, "\"marketplace\"");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib remove_scan_dir_drops`
Expected: FAIL — variant `Marketplace` does not exist + `remove_scan_dir` not found.

- [ ] **Step 3: Extend `SkillProvenance` and add `remove_scan_dir`**

In `skills.rs`, find the `SkillProvenance` enum (`:218-231`) and add the `Marketplace` variant:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillProvenance {
    /// Shipped with the app (read-only).
    Bundled,
    /// User-owned in `~/.uclaw/skills/`. Persists across uClaw upgrades.
    User,
    /// In-repo `<cwd>/skills/` directory.
    Project,
    /// Installed by the marketplace as a side effect of installing an
    /// automation. Lives under `~/.uclaw/skills/_marketplace/<automation_slug>/`.
    Marketplace,
}
```

Find `pub fn add_scan_dir` (`:505`) and add a sibling method right after it:

```rust
pub fn remove_scan_dir(&mut self, dir: &Path) {
    self.scan_dirs.retain(|(d, _)| d != dir);
}
```

(Need to ensure `use std::path::Path;` is already imported at the top of the file — if not, add it.)

- [ ] **Step 4: Run tests to verify pass**

Run: `cd src-tauri && cargo test --lib remove_scan_dir_drops --lib marketplace_provenance`
Expected: both PASS.

- [ ] **Step 5: Run the full skills test suite**

Run: `cd src-tauri && cargo test --lib skills 2>&1 | tail -10`
Expected: all green; no existing match arm on `SkillProvenance` should have broken (variants are exhaustive in some places — fix any `error[E0004]: non-exhaustive patterns` by adding `SkillProvenance::Marketplace => ...` arms).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/skills.rs
git commit -m "feat(skills): Marketplace provenance + remove_scan_dir

Marketplace tier sits between User and Project in trust. Files live
under ~/.uclaw/skills/_marketplace/<automation_slug>/. remove_scan_dir
is needed by uninstall to drop the scan root when an automation is
removed."
```

---

### Task 4: `halo_adapter::fetch_skill_file`

**Files:**
- Modify: `src-tauri/src/automation/marketplace/halo_adapter.rs` (add function near `fetch_spec_yaml`)
- Test: `src-tauri/src/automation/marketplace/halo_adapter.rs` inline tests (or skip — pure network code)

- [ ] **Step 1: Write the function**

Append to `halo_adapter.rs`:

```rust
/// Fetch a single file from a skill bundle inside an automation package.
/// Resolves to `{base}/{entry.path}/skills/{skill_id}/{filename}`.
/// Returns the body bytes. Text files go through `.bytes()` so binary
/// resources (rare today but legal in a bundle) work too.
///
/// Uses the same mirror-fallback pattern as fetch_spec_yaml — Gitee
/// fallback applies automatically for GFW-affected users.
pub async fn fetch_skill_file(
    source: &RegistrySource,
    entry: &RegistryEntry,
    skill_id: &str,
    filename: &str,
) -> Result<Vec<u8>> {
    let client = http_client()?;
    let relative = format!(
        "{}/skills/{}/{}",
        entry.path.trim_matches('/'),
        skill_id,
        filename,
    );
    let bases: Vec<String> = source.url_candidates().map(String::from).collect();

    let mut errors: Vec<String> = Vec::new();
    for base in bases {
        let url = format!(
            "{}/{}",
            base.trim_end_matches('/'),
            relative.trim_start_matches('/'),
        );
        match client.get(&url).send().await {
            Err(e) => {
                errors.push(format!("{}: send failed: {}", base, e));
                continue;
            }
            Ok(resp) => {
                if !resp.status().is_success() {
                    errors.push(format!("{}: HTTP {}", base, resp.status()));
                    continue;
                }
                match resp.bytes().await {
                    Err(e) => {
                        errors.push(format!("{}: body read failed: {}", base, e));
                        continue;
                    }
                    Ok(body) => return Ok(body.to_vec()),
                }
            }
        }
    }
    Err(anyhow!(
        "all registry mirrors failed for /{}: {}",
        relative,
        errors.join("; ")
    ))
}
```

- [ ] **Step 2: Compile check**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/automation/marketplace/halo_adapter.rs
git commit -m "feat(marketplace): fetch_skill_file with mirror fallback

Fetches an individual file from a skill bundle inside an automation
package. Used by the install path's fetching_skills phase. Mirror
fallback matches fetch_spec_yaml so Gitee covers GFW-blocked users."
```

---

### Task 5: `install_human` — fetching_skills phase with staging + atomic rename

**Files:**
- Create: `src-tauri/src/automation/marketplace/skill_install.rs`
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (call into the new module after spec parse)
- Test: `src-tauri/src/automation/marketplace/skill_install.rs` inline tests

- [ ] **Step 1: Module skeleton + the failing rollback test**

Create `skill_install.rs`:

```rust
//! Stage + atomic-rename bundled skill files into ~/.uclaw/skills/_marketplace/<slug>/.
//!
//! Lives in its own module because mod.rs is already large and the rollback
//! semantics (staging dir as failure boundary) read better in isolation.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

use crate::automation::marketplace::halo_adapter;
use crate::automation::marketplace::types::{RegistryEntry, RegistrySource};
use crate::automation::protocol::humane_v1::HumaneAutomationSpec;

/// One bundled skill's files, fetched and ready to commit.
pub struct StagedSkill {
    pub skill_id: String,
    pub file_count: i64,
}

/// Fetch every bundled skill referenced by the spec, into a staging dir.
/// On success the caller must call `commit_staged_skills` to atomically
/// move them into place. On failure the staging dir is cleaned and an
/// Err is returned — no partial state survives.
pub async fn fetch_bundled_skills(
    source: &RegistrySource,
    entry: &RegistryEntry,
    spec: &HumaneAutomationSpec,
    skills_root: &Path,
) -> Result<Vec<StagedSkill>> {
    let staging = skills_root.join(".staging").join(&entry.slug);
    // Clean any leftover staging from a previous failed attempt.
    let _ = std::fs::remove_dir_all(&staging);

    let bundled: Vec<_> = spec
        .requires
        .as_ref()
        .and_then(|r| {
            r.get("skills")
                .and_then(|s| s.as_array())
                .map(|arr| arr.iter().filter(|s| s.get("bundled").and_then(|b| b.as_bool()).unwrap_or(false)).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let mut staged: Vec<StagedSkill> = Vec::new();
    for skill_val in bundled {
        let skill_id = skill_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("requires.skills[].id missing or non-string"))?
            .to_string();
        let files: Vec<String> = skill_val
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|f| f.as_str().map(String::from)).collect())
            .unwrap_or_default();
        if files.is_empty() {
            // Bundled skill declared no files — skip silently; nothing to install.
            continue;
        }

        let skill_staging = staging.join(&skill_id);
        std::fs::create_dir_all(&skill_staging)
            .with_context(|| format!("create staging {}", skill_staging.display()))?;

        for filename in &files {
            let body = halo_adapter::fetch_skill_file(source, entry, &skill_id, filename)
                .await
                .with_context(|| format!("fetch skill file {}/{}", skill_id, filename))?;
            // Guard against path-traversal — filename must be plain (no slashes / ..).
            if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
                cleanup_staging(&staging);
                return Err(anyhow!("rejecting suspicious filename: {}", filename));
            }
            let target = skill_staging.join(filename);
            std::fs::write(&target, &body)
                .with_context(|| format!("write {}", target.display()))?;
        }

        staged.push(StagedSkill {
            skill_id,
            file_count: files.len() as i64,
        });
    }

    Ok(staged)
}

/// Atomically promote the staging dir into the real marketplace skills tree.
/// Removes any pre-existing tree at the destination first (re-install case).
pub fn commit_staged_skills(slug: &str, skills_root: &Path) -> Result<PathBuf> {
    let staging = skills_root.join(".staging").join(slug);
    if !staging.exists() {
        // Nothing staged (spec had no bundled skills) — return the would-be path.
        return Ok(skills_root.join("_marketplace").join(slug));
    }
    let marketplace_root = skills_root.join("_marketplace");
    std::fs::create_dir_all(&marketplace_root)
        .with_context(|| format!("create _marketplace root {}", marketplace_root.display()))?;
    let final_dir = marketplace_root.join(slug);
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir)
            .with_context(|| format!("remove existing {}", final_dir.display()))?;
    }
    std::fs::rename(&staging, &final_dir)
        .with_context(|| format!("rename {} -> {}", staging.display(), final_dir.display()))?;
    Ok(final_dir)
}

/// Remove the staging dir; used on install failure to abandon partial state.
pub fn cleanup_staging(staging_dir: &Path) {
    let _ = std::fs::remove_dir_all(staging_dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_with_empty_staging_returns_planned_path() {
        let tmp = tempfile::tempdir().unwrap();
        let result = commit_staged_skills("xhs-monitor", tmp.path()).unwrap();
        assert_eq!(
            result,
            tmp.path().join("_marketplace").join("xhs-monitor")
        );
        // No directory should have been created since staging was empty.
        assert!(!result.exists());
    }

    #[test]
    fn commit_moves_staging_to_final_location() {
        let tmp = tempfile::tempdir().unwrap();
        let staging_skill = tmp.path().join(".staging").join("auto-1").join("skill-a");
        std::fs::create_dir_all(&staging_skill).unwrap();
        std::fs::write(staging_skill.join("SKILL.md"), b"# Skill A\n").unwrap();

        let final_dir = commit_staged_skills("auto-1", tmp.path()).unwrap();
        assert!(final_dir.join("skill-a").join("SKILL.md").exists());
        // Staging is now gone.
        assert!(!tmp.path().join(".staging").join("auto-1").exists());
    }

    #[test]
    fn commit_overwrites_existing_install() {
        let tmp = tempfile::tempdir().unwrap();
        // Pre-existing install.
        let preexisting = tmp.path().join("_marketplace").join("auto-1").join("old");
        std::fs::create_dir_all(&preexisting).unwrap();
        std::fs::write(preexisting.join("STALE.md"), b"stale").unwrap();
        // Fresh staging.
        let staging_skill = tmp.path().join(".staging").join("auto-1").join("skill-a");
        std::fs::create_dir_all(&staging_skill).unwrap();
        std::fs::write(staging_skill.join("SKILL.md"), b"# Skill A\n").unwrap();

        commit_staged_skills("auto-1", tmp.path()).unwrap();
        assert!(!tmp
            .path()
            .join("_marketplace")
            .join("auto-1")
            .join("old")
            .exists());
        assert!(tmp
            .path()
            .join("_marketplace")
            .join("auto-1")
            .join("skill-a")
            .join("SKILL.md")
            .exists());
    }

    #[test]
    fn cleanup_staging_is_idempotent_on_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // No staging dir exists; cleanup must not panic.
        cleanup_staging(&tmp.path().join(".staging").join("nothing"));
    }
}
```

Register the module in `src-tauri/src/automation/marketplace/mod.rs` — add at the top of file with the other `mod` lines:

```rust
mod skill_install;
```

(Look for the existing `mod cache;` / `mod halo_adapter;` cluster and add alongside.)

- [ ] **Step 2: Run unit tests**

Run: `cd src-tauri && cargo test --lib skill_install`
Expected: 4 PASS.

- [ ] **Step 3: Call from `install_human`**

Find `install_human` ([mod.rs:198](../../../src-tauri/src/automation/marketplace/mod.rs)) and locate the spec parse around line 230 (after `cache::get_item_with_spec` returns). Insert the fetching_skills phase:

```rust
    // Existing: parse + validate spec into HumaneAutomationSpec
    let yaml = ...; // existing
    let spec: HumaneAutomationSpec = serde_yml::from_str(&yaml)
        .with_context(|| format!("parse spec.yaml for {}", slug))?;
    spec.validate().with_context(|| "validate spec")?;

    // NEW: fetching_skills phase
    emit("fetching_skills", 25, Some("拉取依赖 skill 文件"));
    let skills_root = dirs::home_dir()
        .ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw")
        .join("skills");
    let staged = match skill_install::fetch_bundled_skills(&source, &entry, &spec, &skills_root).await {
        Ok(s) => s,
        Err(e) => {
            skill_install::cleanup_staging(
                &skills_root.join(".staging").join(slug),
            );
            return Err(e);
        }
    };
    tracing::info!(slug = %slug, count = staged.len(), "bundled skills staged");
```

Note: existing `emit` helper takes `&str` so wire as shown. `entry` is the `RegistryEntry` constructed earlier in `install_human` (look for the `let entry = RegistryEntry { ... }` block); if it's missing, build one from `item` the same way the existing code already does for the `cached_yaml.is_none()` branch.

- [ ] **Step 4: Build check (commit promotion happens in Task 7)**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

(We don't commit promotion to `_marketplace/<slug>/` yet — that lives in Task 7 alongside `automation_installed_skills` rows, so the install path is atomic across both filesystems.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/marketplace/skill_install.rs src-tauri/src/automation/marketplace/mod.rs
git commit -m "feat(marketplace): bundled skill fetch + staging (fetching_skills phase)

Fetches every requires.skills[].bundled=true file into
~/.uclaw/skills/.staging/<slug>/<skill_id>/. Cleans up on failure.
Commit + atomic rename happens in the registering_skills phase
(next commit), keeping install transactional across DB + FS."
```

---

### Task 6: `install_human` — validating_caps phase

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (between fetching_skills and the existing install logic)

- [ ] **Step 1: Add the phase to install_human**

Right after the fetching_skills block from Task 5, insert:

```rust
    // NEW: validating_caps phase
    emit("validating_caps", 50, Some("校验能力依赖"));
    let mcp_ids: Vec<String> = spec
        .requires
        .as_ref()
        .and_then(|r| r.get("mcps").and_then(|s| s.as_array()))
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut missing_caps: Vec<String> = Vec::new();
    for mcp_id in &mcp_ids {
        if crate::automation::capability_map::resolve_capability(mcp_id).is_none() {
            missing_caps.push(mcp_id.clone());
        }
    }
    if !missing_caps.is_empty() {
        // Warn but don't abort — Phase 3b-γ will offer a real install path.
        let msg = format!(
            "automation 声明依赖 MCP {:?}，但 uClaw 暂不支持，安装完成但可能无法运行",
            missing_caps
        );
        emit("validating_caps", 55, Some(&msg));
        tracing::warn!(missing = ?missing_caps, slug = %slug, "capability validation warnings");
    }
```

- [ ] **Step 2: Build check**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 3: Write an integration-style test for validating_caps**

In `src-tauri/src/automation/marketplace/mod.rs` test module, add:

```rust
#[test]
fn capability_validation_collects_missing_ids() {
    // We test the matching logic directly (not via the async install_human
    // which would require a live HTTP server). install_human's loop is a
    // thin wrapper around resolve_capability — proving the wrapper here is
    // sufficient given Task 2 already covers resolve_capability itself.
    use crate::automation::capability_map::resolve_capability;
    let inputs = vec!["ai-browser", "foo", "bar", "ai-browser"];
    let missing: Vec<&str> = inputs
        .iter()
        .copied()
        .filter(|id| resolve_capability(id).is_none())
        .collect();
    assert_eq!(missing, vec!["foo", "bar"]);
}
```

- [ ] **Step 4: Run test to verify pass**

Run: `cd src-tauri && cargo test --lib capability_validation_collects`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs
git commit -m "feat(marketplace): validating_caps phase in install

Walks requires.mcps[], looks each up in capability_map. Misses are
emitted as a warning toast via the install progress channel but do
not abort the install — Phase 3b-γ will offer a real install path."
```

---

### Task 7: `install_human` — registering_skills phase + commit staging

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (extend after the existing automation_specs insert)
- Modify: `src-tauri/src/app.rs` (boot-time scan registration — recovery path)

- [ ] **Step 1: Locate the existing automation_specs insert**

Search for the `INSERT INTO automation_specs` statement inside `install_human`. After it succeeds, the row exists in DB. We add the registering_skills phase **after** this row is committed (so a DB failure leaves no orphan files).

- [ ] **Step 2: Add the registering_skills phase**

After the `INSERT INTO automation_specs` succeeds, insert:

```rust
    // NEW: registering_skills phase
    emit("registering_skills", 80, Some("注册 skill 与扫描目录"));
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Atomic promotion of staged skills into the real tree.
    let _final_dir = skill_install::commit_staged_skills(slug, &skills_root)
        .with_context(|| "commit staged skills")?;

    // Record one row per staged skill.
    {
        let conn = runtime.db.lock().unwrap();
        for s in &staged {
            conn.execute(
                "INSERT OR REPLACE INTO automation_installed_skills \
                    (automation_slug, skill_id, installed_at, file_count) \
                    VALUES (?, ?, ?, ?)",
                rusqlite::params![slug, s.skill_id, now_secs, s.file_count],
            )?;
        }
    }

    // Register the per-automation scan root with SkillsRegistry so the
    // freshly installed skills become discoverable without an app restart.
    if !staged.is_empty() {
        let scan_root = skills_root.join("_marketplace").join(slug);
        let registry = runtime.skills_registry.clone();
        let mut reg = registry.lock().unwrap();
        reg.add_scan_dir(scan_root, crate::skills::SkillProvenance::Marketplace);
        // Synchronous re-scan kept off the hot path — drop the lock first if
        // scan is heavy. For Phase 3b-α we accept the brief blocking re-scan
        // since installs are user-initiated and infrequent.
        let _ = reg.scan();
    }
```

Note: if `AppRuntimeService` doesn't currently expose `skills_registry`, add it to its struct + `new()` and pass through from `AppState`. The plan assumes it's already accessible; if not, add a one-line `pub skills_registry: Arc<Mutex<SkillsRegistry>>,` field — `AppState` already owns one, see `app.rs`.

- [ ] **Step 3: Boot-time scan-dir recovery in `app.rs`**

`AppState::new` (or wherever `skills_registry` is configured) currently calls:

```rust
skills_reg.add_scan_dir(user_skills_dir, crate::skills::SkillProvenance::User);
```

Right after that, add:

```rust
// Marketplace-installed skills live one level deeper, namespaced by
// automation slug. Walk the _marketplace dir and add one scan root per
// installed automation — this is the recovery path when uclaw.db rows
// are present but the SkillsRegistry hasn't been told about them yet
// (cold start of a process that just restored a backup, etc.).
let marketplace_root = data_dir.join("skills").join("_marketplace");
if let Ok(entries) = std::fs::read_dir(&marketplace_root) {
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            skills_reg.add_scan_dir(path, crate::skills::SkillProvenance::Marketplace);
        }
    }
}
```

- [ ] **Step 4: Build check**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 5: Manual end-to-end smoke**

(Optional but recommended.) Run `cargo tauri dev`, open marketplace, install xiaohongshu-keyword-monitor. Then:

```bash
ls ~/.uclaw/skills/_marketplace/xiaohongshu-keyword-monitor/xhs-search/
# Expected: SKILL.md  index.js

sqlite3 ~/.uclaw/uclaw.db \
  "SELECT * FROM automation_installed_skills WHERE automation_slug = 'xiaohongshu-keyword-monitor';"
# Expected: one row (xiaohongshu-keyword-monitor | xhs-search | <ts> | 2)
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs src-tauri/src/app.rs
git commit -m "feat(marketplace): registering_skills phase + boot-time recovery

commit_staged_skills atomically moves the staging tree to
~/.uclaw/skills/_marketplace/<slug>/. Then writes one row per skill
into automation_installed_skills (the V22 table) and registers the
per-automation scan root with SkillsRegistry so installed skills are
immediately discoverable. app.rs boot replays this registration for
existing _marketplace/<slug>/ dirs so DB and FS stay in sync across
restarts."
```

---

### Task 8: `uninstall_marketplace_human` Tauri command

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (add `uninstall_human` async fn)
- Modify: `src-tauri/src/tauri_commands.rs` (add command wrapper)
- Modify: `src-tauri/src/main.rs` (register in `invoke_handler!`)
- Modify: `ui/src/lib/tauri-bridge.ts` (TS bridge)

- [ ] **Step 1: Write the failing test**

Add to `mod.rs` test module:

```rust
#[tokio::test]
async fn uninstall_removes_rows_and_files() {
    // Set up: temp skills root + in-memory DB with V22 + a fake _marketplace/<slug>/ tree.
    let tmp = tempfile::tempdir().unwrap();
    let skills_root = tmp.path().join("skills");
    let target = skills_root.join("_marketplace").join("auto-x").join("skill-a");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("SKILL.md"), b"# A").unwrap();
    // Untouched user-written skill — must survive uninstall.
    let user_skill = skills_root.join("hand-written").join("SKILL.md");
    std::fs::create_dir_all(user_skill.parent().unwrap()).unwrap();
    std::fs::write(&user_skill, b"# H").unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    conn.execute(
        "INSERT INTO automation_installed_skills VALUES (?, ?, ?, ?)",
        rusqlite::params!["auto-x", "skill-a", 0_i64, 1_i64],
    ).unwrap();
    conn.execute(
        "INSERT INTO automation_specs (slug, kind, name, version, source, source_ref, spec_yaml, created_at, updated_at) \
            VALUES ('auto-x', 'automation', 'X', '1.0.0', 'marketplace', 'marketplace://halo/auto-x', '', 0, 0)",
        [],
    ).unwrap();

    // Drive the uninstall — pass in the connection + skills_root directly.
    super::uninstall_human_inner(&conn, &skills_root, "auto-x").unwrap();

    // Assert: rows gone, marketplace dir gone, user skill untouched.
    let spec_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM automation_specs WHERE slug = 'auto-x'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(spec_count, 0);
    let inst_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM automation_installed_skills WHERE automation_slug = 'auto-x'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(inst_count, 0);
    assert!(!skills_root.join("_marketplace").join("auto-x").exists());
    assert!(user_skill.exists(), "user-written skill must survive");
}
```

(The test calls `uninstall_human_inner` — a synchronous helper that takes the conn + skills_root by reference. This lets us test without spinning up `AppRuntimeService`. The outer `uninstall_human` is the async caller that resolves these from runtime + emits progress.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib uninstall_removes_rows_and_files`
Expected: FAIL — `uninstall_human_inner` doesn't exist.

- [ ] **Step 3: Implement `uninstall_human_inner` + `uninstall_human`**

Add to `mod.rs`:

```rust
/// Synchronous core of uninstall — separated so unit tests can drive it
/// without an AppRuntimeService.
pub fn uninstall_human_inner(
    conn: &rusqlite::Connection,
    skills_root: &std::path::Path,
    slug: &str,
) -> Result<()> {
    let source_ref = format!("marketplace://halo/{}", slug);
    conn.execute(
        "DELETE FROM automation_specs WHERE source = 'marketplace' AND source_ref = ?1",
        rusqlite::params![source_ref],
    )?;
    conn.execute(
        "DELETE FROM automation_installed_skills WHERE automation_slug = ?1",
        rusqlite::params![slug],
    )?;
    let dir = skills_root.join("_marketplace").join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("remove {}", dir.display()))?;
    }
    Ok(())
}

/// Public async entry point used by the Tauri command. Resolves runtime
/// resources, calls the inner sync core, then drops the SkillsRegistry
/// scan dir.
pub async fn uninstall_human(
    runtime: &crate::automation::runtime::AppRuntimeService,
    slug: &str,
) -> Result<()> {
    let skills_root = dirs::home_dir()
        .ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw")
        .join("skills");
    {
        let conn = runtime.db.lock().unwrap();
        uninstall_human_inner(&conn, &skills_root, slug)?;
    }
    {
        let mut reg = runtime.skills_registry.lock().unwrap();
        reg.remove_scan_dir(&skills_root.join("_marketplace").join(slug));
        let _ = reg.scan();
    }
    Ok(())
}
```

- [ ] **Step 4: Add the Tauri command**

In `tauri_commands.rs`, near `install_marketplace_human`:

```rust
#[tauri::command]
pub async fn uninstall_marketplace_human(
    state: tauri::State<'_, AppState>,
    slug: String,
) -> Result<(), Error> {
    let runtime = state.app_runtime.clone();
    crate::automation::marketplace::uninstall_human(&runtime, &slug)
        .await
        .map_err(|e| Error::Internal(format!("{:#}", e)))
}
```

Register in `main.rs`'s `invoke_handler!` macro right next to `install_marketplace_human`:

```rust
uclaw_core::tauri_commands::uninstall_marketplace_human,
```

- [ ] **Step 5: TS bridge**

In `ui/src/lib/tauri-bridge.ts`, next to `installMarketplaceHuman`:

```ts
export const uninstallMarketplaceHuman = (slug: string): Promise<void> =>
  invoke<void>('uninstall_marketplace_human', { slug })
```

- [ ] **Step 6: Verify**

```bash
cd src-tauri && cargo test --lib uninstall 2>&1 | tail -10
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: all PASS / no errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "feat(marketplace): uninstall_marketplace_human command

Drops automation_specs row + automation_installed_skills rows +
~/.uclaw/skills/_marketplace/<slug>/ tree + the matching
SkillsRegistry scan dir. User-written skills under hand-flat
paths are untouched (path isolation pays off here)."
```

---

### Task 9: `list_installed_marketplace_automations` command

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (read query + DTO)
- Modify: `src-tauri/src/automation/marketplace/types.rs` (DTOs)
- Modify: `src-tauri/src/tauri_commands.rs` (command wrapper)
- Modify: `src-tauri/src/main.rs` (register)
- Modify: `ui/src/lib/tauri-bridge.ts` (TS types + bridge)

- [ ] **Step 1: Add DTOs to types.rs**

In `src-tauri/src/automation/marketplace/types.rs`, append:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkillBrief {
    pub skill_id: String,
    pub description: Option<String>,
    pub install_path: String,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityStatus {
    Mapped,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCheck {
    pub mcp_id: String,
    pub status: CapabilityStatus,
    pub mapped_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAutomation {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub icon: Option<String>,
    pub category: String,
    pub bundled_skills: Vec<InstalledSkillBrief>,
    pub required_capabilities: Vec<CapabilityCheck>,
}
```

- [ ] **Step 2: Write the failing test**

Add to `mod.rs` test module:

```rust
#[test]
fn list_installed_joins_specs_and_skills_correctly() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    let spec_yaml = r#"
spec_version: "1"
name: X
version: 1.0.0
author: t
description: t
type: automation
icon: social
system_prompt: x
config_schema: []
requires:
  mcps:
    - id: ai-browser
      reason: r
    - id: nonexistent-mcp
      reason: r
"#;
    conn.execute(
        "INSERT INTO automation_specs (slug, kind, name, version, source, source_ref, spec_yaml, created_at, updated_at) \
            VALUES ('a', 'automation', 'X', '1.0.0', 'marketplace', 'marketplace://halo/a', ?1, 0, 0)",
        [spec_yaml],
    ).unwrap();
    conn.execute(
        "INSERT INTO automation_installed_skills VALUES (?, ?, ?, ?)",
        rusqlite::params!["a", "skill-1", 0_i64, 2_i64],
    ).unwrap();

    let result = super::list_installed_inner(&conn, std::path::Path::new("/tmp/uclaw-test"))
        .expect("query ok");

    assert_eq!(result.len(), 1);
    let r = &result[0];
    assert_eq!(r.slug, "a");
    assert_eq!(r.name, "X");
    assert_eq!(r.bundled_skills.len(), 1);
    assert_eq!(r.bundled_skills[0].skill_id, "skill-1");
    assert_eq!(r.required_capabilities.len(), 2);
    assert!(matches!(r.required_capabilities[0].status, super::types::CapabilityStatus::Mapped));
    assert!(matches!(r.required_capabilities[1].status, super::types::CapabilityStatus::Missing));
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib list_installed_joins`
Expected: FAIL — `list_installed_inner` not found.

- [ ] **Step 4: Implement**

Add to `mod.rs`:

```rust
pub fn list_installed_inner(
    conn: &rusqlite::Connection,
    skills_root: &std::path::Path,
) -> Result<Vec<types::InstalledAutomation>> {
    use crate::automation::capability_map;
    use crate::automation::protocol::humane_v1::HumaneAutomationSpec;
    use types::{CapabilityCheck, CapabilityStatus, InstalledAutomation, InstalledSkillBrief};

    let mut stmt = conn.prepare(
        "SELECT slug, name, version, spec_yaml FROM automation_specs \
            WHERE source = 'marketplace' \
            ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (slug, name, version, spec_yaml) = row?;
        let spec: HumaneAutomationSpec = serde_yml::from_str(&spec_yaml)
            .with_context(|| format!("parse stored spec for {}", slug))?;

        // bundled skills
        let mut skills_stmt = conn.prepare(
            "SELECT skill_id, file_count FROM automation_installed_skills \
                WHERE automation_slug = ? ORDER BY skill_id",
        )?;
        let skill_rows = skills_stmt.query_map(rusqlite::params![&slug], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut bundled_skills = Vec::new();
        for s in skill_rows {
            let (skill_id, file_count) = s?;
            let install_path = skills_root
                .join("_marketplace")
                .join(&slug)
                .join(&skill_id)
                .to_string_lossy()
                .to_string();
            bundled_skills.push(InstalledSkillBrief {
                skill_id,
                description: None, // populated by next iteration once SkillsRegistry exposes it
                install_path,
                file_count,
            });
        }

        // required capabilities
        let mcp_ids: Vec<String> = spec
            .requires
            .as_ref()
            .and_then(|r| r.get("mcps").and_then(|s| s.as_array()))
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let required_capabilities: Vec<CapabilityCheck> = mcp_ids
            .into_iter()
            .map(|mcp_id| match capability_map::resolve_capability(&mcp_id) {
                Some(cap) => CapabilityCheck {
                    mcp_id,
                    status: CapabilityStatus::Mapped,
                    mapped_to: Some(capability_map::human_label(cap).to_string()),
                },
                None => CapabilityCheck {
                    mcp_id,
                    status: CapabilityStatus::Missing,
                    mapped_to: None,
                },
            })
            .collect();

        out.push(InstalledAutomation {
            slug: slug.clone(),
            name,
            version,
            icon: Some(spec.icon.unwrap_or_else(|| "other".into())),
            category: spec
                .store
                .as_ref()
                .and_then(|s| s.get("category").and_then(|v| v.as_str()))
                .unwrap_or("other")
                .to_string(),
            bundled_skills,
            required_capabilities,
        });
    }
    Ok(out)
}

/// Async wrapper used by the Tauri command.
pub async fn list_installed(
    runtime: &crate::automation::runtime::AppRuntimeService,
) -> Result<Vec<types::InstalledAutomation>> {
    let skills_root = dirs::home_dir()
        .ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw")
        .join("skills");
    let conn = runtime.db.lock().unwrap();
    list_installed_inner(&conn, &skills_root)
}
```

- [ ] **Step 5: Tauri command + bridge**

In `tauri_commands.rs`:

```rust
#[tauri::command]
pub async fn list_installed_marketplace_automations(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::automation::marketplace::types::InstalledAutomation>, Error> {
    let runtime = state.app_runtime.clone();
    crate::automation::marketplace::list_installed(&runtime)
        .await
        .map_err(|e| Error::Internal(format!("{:#}", e)))
}
```

Register in `main.rs` `invoke_handler!`.

In `ui/src/lib/tauri-bridge.ts`:

```ts
export interface InstalledSkillBrief {
  skillId: string
  description: string | null
  installPath: string
  fileCount: number
}

export type CapabilityStatus = 'mapped' | 'missing'

export interface CapabilityCheck {
  mcpId: string
  status: CapabilityStatus
  mappedTo: string | null
}

export interface InstalledAutomation {
  slug: string
  name: string
  version: string
  icon: string | null
  category: string
  bundledSkills: InstalledSkillBrief[]
  requiredCapabilities: CapabilityCheck[]
}

export const listInstalledMarketplaceAutomations = (): Promise<InstalledAutomation[]> =>
  invoke<InstalledAutomation[]>('list_installed_marketplace_automations')
```

- [ ] **Step 6: Verify**

```bash
cd src-tauri && cargo test --lib list_installed 2>&1 | tail -5
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: PASS / no errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/automation/marketplace/ src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "feat(marketplace): list_installed_marketplace_automations command

Joins automation_specs × automation_installed_skills × capability_map
into InstalledAutomation DTOs. Drives the AppsView card list."
```

---

### Task 10: `AppsView` component + tests

**Files:**
- Create: `ui/src/components/automation/AppsView.tsx`
- Create: `ui/src/components/automation/AppsView.test.tsx`
- Modify: `ui/src/views/AutomationsView.tsx` (replace Apps stub with AppsView)

- [ ] **Step 1: Write the failing component test**

Create `AppsView.test.tsx`:

```tsx
import { describe, test, expect, vi } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'

// Stub the bridge before importing the component
vi.mock('@/lib/tauri-bridge', () => ({
  listInstalledMarketplaceAutomations: vi.fn(),
  uninstallMarketplaceHuman: vi.fn(),
}))

import { AppsView } from './AppsView'
import {
  listInstalledMarketplaceAutomations,
  uninstallMarketplaceHuman,
} from '@/lib/tauri-bridge'

const sampleData = [
  {
    slug: 'xhs-monitor',
    name: '小红书关键词监控',
    version: '4.0.0',
    icon: 'social',
    category: 'social',
    bundledSkills: [
      {
        skillId: 'xhs-search',
        description: 'Collects xiaohongshu search data',
        installPath: '/home/x/.uclaw/skills/_marketplace/xhs-monitor/xhs-search',
        fileCount: 2,
      },
    ],
    requiredCapabilities: [
      { mcpId: 'ai-browser', status: 'mapped' as const, mappedTo: 'uClaw 内建浏览器' },
    ],
  },
]

describe('AppsView', () => {
  test('renders empty state when nothing installed', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>).mockResolvedValueOnce([])
    const { findByText } = renderWithProviders(<AppsView />)
    expect(await findByText(/暂无已安装的数字人/)).toBeInTheDocument()
  })

  test('lists installed automations with name and version', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>).mockResolvedValueOnce(sampleData)
    const { findByText } = renderWithProviders(<AppsView />)
    expect(await findByText('小红书关键词监控')).toBeInTheDocument()
    expect(await findByText(/v4\.0\.0/)).toBeInTheDocument()
  })

  test('expand reveals bundled skills and capability checks', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>).mockResolvedValueOnce(sampleData)
    const { findByText, getByText, queryByText } = renderWithProviders(<AppsView />)
    await findByText('小红书关键词监控')
    // Skills not visible yet
    expect(queryByText('xhs-search')).not.toBeInTheDocument()
    fireEvent.click(getByText('小红书关键词监控'))
    expect(await findByText('xhs-search')).toBeInTheDocument()
    expect(await findByText('ai-browser')).toBeInTheDocument()
    expect(await findByText(/已映射到 uClaw 内建/)).toBeInTheDocument()
  })

  test('uninstall calls bridge and refreshes', async () => {
    ;(listInstalledMarketplaceAutomations as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce(sampleData)
      .mockResolvedValueOnce([])
    ;(uninstallMarketplaceHuman as ReturnType<typeof vi.fn>).mockResolvedValueOnce(undefined)
    // Avoid real confirm() blocking the test
    vi.spyOn(window, 'confirm').mockReturnValue(true)

    const { findByText, getByText } = renderWithProviders(<AppsView />)
    await findByText('小红书关键词监控')
    fireEvent.click(getByText('卸载'))
    await waitFor(() => expect(uninstallMarketplaceHuman).toHaveBeenCalledWith('xhs-monitor'))
    expect(await findByText(/暂无已安装的数字人/)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run AppsView 2>&1 | tail -10`
Expected: FAIL — `AppsView` component not found.

- [ ] **Step 3: Implement `AppsView`**

Create `ui/src/components/automation/AppsView.tsx`:

```tsx
import * as React from 'react'
import { motion, AnimatePresence } from 'motion/react'
import { ChevronDown, Trash2, CheckCircle2, AlertCircle } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { CategoryIcon } from './category-icon'
import {
  listInstalledMarketplaceAutomations,
  uninstallMarketplaceHuman,
  type InstalledAutomation,
} from '@/lib/tauri-bridge'

export function AppsView(): React.ReactElement {
  const [items, setItems] = React.useState<InstalledAutomation[] | null>(null)
  const [expanded, setExpanded] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  const reload = React.useCallback(async () => {
    setLoading(true)
    try {
      const data = await listInstalledMarketplaceAutomations()
      setItems(data)
    } catch (err) {
      toast.error(`加载失败：${String(err)}`)
      setItems([])
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void reload()
  }, [reload])

  const handleUninstall = async (item: InstalledAutomation) => {
    if (!window.confirm(`确定卸载 ${item.name} 吗？\n会一并删除依赖的 skill 文件。`)) return
    try {
      await uninstallMarketplaceHuman(item.slug)
      toast.success(`已卸载 ${item.name}`)
      await reload()
    } catch (err) {
      toast.error(`卸载失败：${String(err)}`)
    }
  }

  if (loading && items === null) {
    return <div className="px-6 py-8 text-[12px] text-muted-foreground">加载中…</div>
  }

  if (!items || items.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center px-6 py-16 text-center">
        <div className="text-[14px] text-foreground mb-2">暂无已安装的数字人</div>
        <div className="text-[12px] text-muted-foreground">
          去「应用商店」装一个，或者关闭此面板回到聊天。
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full overflow-y-auto px-6 py-4">
      <div className="text-[11px] text-muted-foreground mb-3 leading-relaxed">
        以下是已安装数字人随附的 skill / 能力依赖。独立 skill / MCP 商店在 Phase 3b 后续切片开放。
      </div>
      <div className="flex flex-col gap-2">
        {items.map((item) => {
          const isOpen = expanded === item.slug
          return (
            <div
              key={item.slug}
              className="rounded-xl border border-border/50 bg-card overflow-hidden"
            >
              <button
                type="button"
                onClick={() => setExpanded(isOpen ? null : item.slug)}
                className="flex items-center gap-3 w-full px-4 py-3 hover:bg-muted/40 transition-colors text-left"
              >
                <div className="w-10 h-10 rounded-lg bg-primary/8 flex items-center justify-center shrink-0">
                  <CategoryIcon name={item.icon ?? item.category} size={18} className="text-primary/80" />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-[14px] font-medium truncate">{item.name}</div>
                  <div className="text-[11px] text-muted-foreground tabular-nums">v{item.version}</div>
                </div>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation()
                    void handleUninstall(item)
                  }}
                  className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] text-muted-foreground hover:text-danger hover:bg-danger-bg transition-colors"
                >
                  <Trash2 size={12} />
                  卸载
                </button>
                <ChevronDown
                  size={14}
                  className={cn(
                    'text-muted-foreground transition-transform',
                    isOpen && 'rotate-180',
                  )}
                />
              </button>
              <AnimatePresence initial={false}>
                {isOpen && (
                  <motion.div
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: 'auto', opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    transition={{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }}
                    className="overflow-hidden"
                  >
                    <div className="px-4 pb-4 border-t border-border/50 pt-3">
                      {item.bundledSkills.length > 0 && (
                        <div className="mb-3">
                          <div className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5">
                            Bundled Skills
                          </div>
                          <ul className="flex flex-col gap-1.5">
                            {item.bundledSkills.map((s) => (
                              <li key={s.skillId} className="text-[12px]">
                                <span className="text-foreground">{s.skillId}</span>
                                {s.description && (
                                  <span className="text-muted-foreground"> · {s.description}</span>
                                )}
                                <div className="text-[10px] text-muted-foreground/70 mt-0.5 font-mono truncate">
                                  {s.installPath}
                                </div>
                              </li>
                            ))}
                          </ul>
                        </div>
                      )}
                      {item.requiredCapabilities.length > 0 && (
                        <div>
                          <div className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5">
                            Required Capabilities
                          </div>
                          <ul className="flex flex-col gap-1.5">
                            {item.requiredCapabilities.map((c) => (
                              <li key={c.mcpId} className="flex items-center gap-2 text-[12px]">
                                {c.status === 'mapped' ? (
                                  <CheckCircle2 size={13} className="text-success" />
                                ) : (
                                  <AlertCircle size={13} className="text-warning" />
                                )}
                                <span className="text-foreground">{c.mcpId}</span>
                                {c.status === 'mapped' ? (
                                  <span className="text-[11px] text-success">· 已映射到 {c.mappedTo}</span>
                                ) : (
                                  <span className="text-[11px] text-warning">· 待 Phase 3b-γ 支持</span>
                                )}
                              </li>
                            ))}
                          </ul>
                        </div>
                      )}
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          )
        })}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd ui && npm test -- --run AppsView 2>&1 | tail -12`
Expected: 4 PASS.

- [ ] **Step 5: Wire into AutomationsView**

Open `ui/src/views/AutomationsView.tsx`. Find the section that renders the `apps` subview (probably a stub like "MCP / 技能 / 扩展 安装支持在 Phase 3b 开放"). Replace with:

```tsx
import { AppsView } from '@/components/automation/AppsView'
// ... in the subview switch:
{subview === 'apps' && <AppsView />}
```

(If the existing switch uses a different shape — e.g. a ternary or an object map — match the existing pattern instead.)

- [ ] **Step 6: tsc + full vitest**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -5
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: zero TS errors, no new failing tests beyond the prior baseline.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/automation/AppsView.tsx ui/src/components/automation/AppsView.test.tsx ui/src/views/AutomationsView.tsx
git commit -m "feat(marketplace): AppsView — installed automation list

Groups bundled skills and capability checks by source automation per the
Phase 3b-α design. Expand reveals skill list + capability map status.
Uninstall calls uninstall_marketplace_human + refreshes. Empty state
points at the store. Matches uClaw Design DNA — rounded-xl cards,
motion/react duration 0.22, theme tokens only."
```

---

### Task 11: CLAUDE.md migration registry update + PR

**Files:**
- Modify: `CLAUDE.md` (the *Active migration registry* table)

- [ ] **Step 1: Read current registry table**

Open `CLAUDE.md` and find the section *Active migration registry*. The table currently shows V18–V21 as either merged or open. The live tree confirms V19/V20/V21/V23a are all merged.

- [ ] **Step 2: Update the registry**

Replace the existing rows with the corrected status:

```markdown
| V | What | Status |
|---|---|---|
| V1–V18 | Initial schema → V18 agent_sessions.pinned_at | merged |
| V19 | spaces.skill_tags — per-workspace skill scoping (JSON tag array) | merged |
| V20 | rewrite automation_specs + activities + migrate legacy TOML | merged |
| V21 | automation_subscriptions + automation_memory + automation_escalations | merged |
| V22 | automation_installed_skills + idx_aut_inst_skills_slug | **this PR** (Phase 3b-α) |
| V23a | Marketplace cache (Phase 3a) | merged |
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): update migration registry to reflect live schema

V19-V21 + V23a are merged. V22 claimed by Phase 3b-alpha
(automation_installed_skills) in this PR."
```

- [ ] **Step 4: Push and open PR**

```bash
git push -u origin <branch-name>

gh pr create --title "feat(marketplace): Phase 3b-α — bundled skills + capability map" --body "$(cat <<'EOF'
## Summary

Completes the automation install chain. Phase 3a shipped the install UI but install was a stub — it parsed the spec and wrote the automation_specs row but never fetched the bundled skill files referenced by `requires.skills[].files`, so installed automations had no way to actually execute. This PR fixes that.

- New table `automation_installed_skills` (V22) records which bundled skills each marketplace-installed automation pulls in.
- `install_human` grows three phases: `fetching_skills` (staging dir + atomic rename), `validating_caps` (capability map), `registering_skills` (DB + SkillsRegistry scan dir).
- `SkillProvenance::Marketplace` + `SkillsRegistry::remove_scan_dir` make skill discovery work across install/uninstall.
- New `capability_map` module — only mapping today is `ai-browser → BuiltinCapability::Browser` (uClaw's built-in chromiumoxide tool). Phase 3b-γ replaces this match with a real table.
- `uninstall_marketplace_human` undoes everything: DB rows + `~/.uclaw/skills/_marketplace/<slug>/` + scan dir. User-written skills under flat paths are untouched.
- New `AppsView` (「我的应用」 tab) lists installed automations with expand-to-see bundled skills and capability checks.

## Commits (bisectable)

| # | Commit | Scope |
|---|--------|-------|
| 1 | feat(db): V22 automation_installed_skills | DB |
| 2 | feat(automation): capability_map module | backend |
| 3 | feat(skills): Marketplace provenance + remove_scan_dir | backend |
| 4 | feat(marketplace): fetch_skill_file with mirror fallback | backend |
| 5 | feat(marketplace): fetching_skills phase | backend |
| 6 | feat(marketplace): validating_caps phase | backend |
| 7 | feat(marketplace): registering_skills phase + boot-time recovery | backend |
| 8 | feat(marketplace): uninstall_marketplace_human | backend + bridge |
| 9 | feat(marketplace): list_installed_marketplace_automations | backend + bridge |
| 10 | feat(marketplace): AppsView | UI |
| 11 | docs(claude): update migration registry | docs |

Spec: docs/superpowers/specs/2026-05-14-phase3b-alpha-bundled-skills-design.md
Plan: docs/superpowers/plans/2026-05-14-phase3b-alpha-bundled-skills.md

## Test plan

- [ ] `cargo test --lib` 660+ tests pass (4 new test groups: V22, capability_map, skill_install, uninstall + list)
- [ ] `npm test -- --run AppsView` 4 cases pass
- [ ] `npx tsc --noEmit` zero errors
- [ ] Manual: install xiaohongshu-keyword-monitor; verify `~/.uclaw/skills/_marketplace/xiaohongshu-keyword-monitor/xhs-search/{SKILL.md,index.js}` exist
- [ ] Manual: `automation_installed_skills` row exists for the same slug
- [ ] Manual: 「我的应用」 lists it; expand shows the skill + ai-browser ✓ mapped
- [ ] Manual: 卸载 button removes the dir + rows + scan registration

## Follow-ups not in this PR (Phase 3b-β/γ/δ/ε/ζ)

- Skill bundle update flow when automation version bumps
- Standalone skill / MCP marketplace entries (requires DHP repo schema additions)
- Multi-registry sources + capability map → table
- Proxy adapters (Smithery / official MCP Registry / SkillHub)
- Local hello-halo workspace as a registry source
EOF
)"
```

---

## Self-review

### Spec coverage

| Spec § | Where in plan |
|---|---|
| § 4.1 V22 migration | Task 1 |
| § 4.2 Path layout (`_marketplace/`) | Tasks 5 + 7 (staging + commit) |
| § 4.3 fetching_skills phase | Task 5 |
| § 4.3 validating_caps phase | Task 6 |
| § 4.3 registering_skills phase | Task 7 |
| § 4.4 Uninstall flow | Task 8 |
| § 4.5 Capability map module | Task 2 |
| § 4.6 SkillsRegistry Marketplace + boot scan | Tasks 3 + 7 |
| § 5 AppsView UI | Task 10 |
| § 5.1 DTO shapes | Task 9 |
| § 6 Error handling | Distributed: Task 5 (rollback), 6 (warn), 7 (atomic), 8 (idempotent FS delete) |
| § 7.1 Rust tests | Tasks 1, 2, 3, 5, 6, 8, 9 each include their listed test |
| § 7.2 Vitest tests | Task 10 |
| § 8 Migration registry update | Task 11 |
| § 10 Done criteria | Task 11 test plan checklist mirrors each item |

### Placeholder scan

No "TODO" / "TBD" / "fill in later" anywhere in the task bodies. The only `// ...` ellipses in the plan are inside example code comments where the engineer is told "see existing code at this line" with an exact location — that's information transfer, not a placeholder.

### Type consistency

- `BuiltinCapability::Browser` defined in Task 2; referenced by name in Tasks 6, 9.
- `SkillProvenance::Marketplace` defined in Task 3; referenced in Tasks 7, 9.
- `InstalledAutomation` / `InstalledSkillBrief` / `CapabilityCheck` / `CapabilityStatus` defined in Task 9; consumed in Task 10 frontend.
- `StagedSkill` / `commit_staged_skills` / `cleanup_staging` defined in Task 5; called in Task 7.
- `uninstall_human_inner` defined in Task 8; tested with the same signature.
- `list_installed_inner` defined in Task 9; same shape across test + impl.

Naming is consistent across tasks.
