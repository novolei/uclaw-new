# Phase 3b-γ — Standalone Skill / MCP Marketplace Entries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `type: skill` and `type: mcp` marketplace packages installable — restructure the install path into a dispatcher + three flat per-type functions, translate skill specs into uClaw `SKILL.md` files and MCP specs into registered MCP servers, track standalone installs in a new V25 table, and make the store UI type-aware.

**Architecture:** `install_human` becomes `install_automation` (body lifted verbatim). A new `install_marketplace_item` dispatcher matches `item.app_type` → `install_automation` / `install_standalone_skill` / `install_standalone_mcp`. Skill installs translate `spec.yaml` → `SKILL.md` under `~/.uclaw/skills/_marketplace/_standalone/<slug>/` (a managed namespace the 3b-α boot scan already walks). MCP installs translate the `mcp_server` block → `McpServerConfig` and register it with the MCP manager. A V25 `marketplace_standalone_installs` table tracks both. The store query stops filtering non-automation; `StoreDetail` / `InstallWizard` / `AppsTab` become type-aware.

**Tech Stack:** Rust (rusqlite, serde, serde_yml, garde, reqwest, tokio), React 18 + TypeScript + Jotai, Vitest.

**Spec:** `docs/superpowers/specs/2026-05-14-phase3b-gamma-standalone-items-design.md`

**Parallel work — reconciled 2026-05-14 (see spec § 11):**
- **Rebased onto `origin/main` `89dcfd9`** (was based on stale `032a368`). All line numbers below re-verified post-rebase.
- **Automation Phase 2a** (`worktree-automation-phase2a`, spec approved, not merged) **claims migration V24** — γ takes **V25** (Task 1). Phase 2a touches `automation/runtime/*` + `agent/*` + `channels.rs`; the only files it shares with γ are `tauri_commands.rs` + `main.rs` (both additive). No design conflict.
- **Kaleidoscope PR #169 — merged** (it's in the rebase). Added the Skills / Integrations / Memory Kaleidoscope modules + `update_mcp_server` + `mcp.rs` tests. It did **not** touch `StoreDetail.tsx` / `InstallWizard.tsx` / `AppsTab.tsx` / `automation/marketplace/*` / `db/migrations.rs` / `humane_v1.rs` — γ's core files are conflict-free. γ's standalone installs surface in the new Skills / Integrations modules automatically (operational view); AppsTab stays the marketplace-lifecycle view (Task 10). γ does not modify the #169 modules — the only related change is syncing the stale `SkillInfo.provenance` TS union (Task 7).

**Pre-flight state confirmed against the live tree (2026-05-14, post-rebase onto `89dcfd9`, worktree `worktree-phase3b-gamma-standalone-items`):**
- `install_human` at `src-tauri/src/automation/marketplace/mod.rs:455`, signature `(runtime, app_handle, slug, space_id, user_config, skills_registry, progress_channel) -> Result<HumaneSpecRow>`. The `app_type != "automation"` reject is at ~line 488. (`marketplace/mod.rs` untouched by PR #169.)
- `list_humans` filters `app_type == "automation"` at mod.rs:35; `query_marketplace_cached` is the paged cache query right after.
- Tauri commands: `install_marketplace_human` (tauri_commands.rs:5678), `uninstall_marketplace_human` (tauri_commands.rs:5703). Both registered in `main.rs:451-452`. Frontend bridge: `installMarketplaceHuman` (tauri-bridge.ts:1400) / `uninstallMarketplaceHuman` (tauri-bridge.ts:1410). **Keep the Tauri command names + bridge names unchanged** — only their bodies change to call the renamed internal dispatcher. Smaller diff, no `main.rs` invoke_handler churn for the rename.
- Last migration is V23a (`migrations.rs:1318`). V22 = `automation_installed_skills`. **V24 is claimed by the in-flight Automation Phase 2a branch** (not yet merged) — γ takes **V25**. The `run()` block target is "after the V23a block" (resilient to Phase 2a later inserting V24).
- `HumaneAutomationSpec` (humane_v1.rs:8): has `system_prompt: String`, `requires: Option<serde_json::Value>`, `config_schema: Vec<InputDef>`. **No `mcp_server` field** — Task 2 adds one. `kind` is validated by the `must_be_automation` garde custom validator (humane_v1.rs:12) — calling `.validate()` on a skill/mcp spec fails.
- `mcp_manager: SharedMcpManager` = `Arc<RwLock<McpManager>>` in `AppState` (app.rs:165). `add_mcp_server` (tauri_commands.rs:2079) shows the registration path: build `crate::mcp::McpServerConfig`, then `state.mcp_manager.write().await.add_server(config)`.
- `McpServerConfig` (mcp.rs:227): `{ id, name, description, transport_type, command, args, env, url, enabled, auto_approve }`.
- `InstallWizard` (`ui/src/components/automation/InstallWizard.tsx`): `STEPS = ['scope', 'config', 'confirm', 'progress']`; the stepper renders `['scope','config','confirm']`. For skill/mcp the `scope` step is skipped.
- `StoreDetail` non-automation stub at `ui/src/components/automation/StoreDetail.tsx:176` (`{appType} 安装在 Phase 3b 开放`); the install-button branch is `item.appType === 'automation' ? ... : <stub>` at line 147.

---

### Task 1: V25 migration — `marketplace_standalone_installs`

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add `SQL_V25` constant + a block in `run()`)
- Test: `src-tauri/src/db/migrations.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `migrations.rs`:

```rust
#[test]
fn v24_creates_marketplace_standalone_installs_table() {
    let conn = Connection::open_in_memory().unwrap();
    super::run(&conn).expect("migrations run");

    conn.execute(
        "INSERT INTO marketplace_standalone_installs \
            (slug, item_type, version, installed_at, mcp_server_id) \
            VALUES (?, ?, ?, ?, ?)",
        rusqlite::params!["my-skill", "skill", "1.0.0", 1715000000_i64, Option::<String>::None],
    ).expect("skill row insert ok");

    conn.execute(
        "INSERT INTO marketplace_standalone_installs \
            (slug, item_type, version, installed_at, mcp_server_id) \
            VALUES (?, ?, ?, ?, ?)",
        rusqlite::params!["my-mcp", "mcp", "2.0.0", 1715000000_i64, Some("srv-uuid-123")],
    ).expect("mcp row insert ok");

    // slug is PK — duplicate must error.
    let dup = conn.execute(
        "INSERT INTO marketplace_standalone_installs \
            (slug, item_type, version, installed_at, mcp_server_id) \
            VALUES (?, ?, ?, ?, ?)",
        rusqlite::params!["my-skill", "skill", "1.0.1", 1715000001_i64, Option::<String>::None],
    );
    assert!(dup.is_err(), "slug PK must reject duplicate");
}

#[test]
fn v24_is_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    super::run(&conn).expect("first run");
    super::run(&conn).expect("second run must not error");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib v24_creates_marketplace_standalone_installs_table`
Expected: FAIL — `no such table: marketplace_standalone_installs`.

- [ ] **Step 3: Add the SQL constant**

Insert near `SQL_V22` (after the `V23A_MARKETPLACE_CACHE` constant) in `migrations.rs`:

```rust
/// V25 — marketplace_standalone_installs.
///
/// Tracks standalone (non-bundled) skill and MCP marketplace installs so the
/// AppsTab can list them and uninstall can find what to remove. `mcp_server_id`
/// links a `type: mcp` install to its mcp_servers.json entry; NULL for skills.
const SQL_V25: &str = "
CREATE TABLE IF NOT EXISTS marketplace_standalone_installs (
    slug          TEXT PRIMARY KEY,
    item_type     TEXT NOT NULL,
    version       TEXT NOT NULL,
    installed_at  INTEGER NOT NULL,
    mcp_server_id TEXT
);
";
```

- [ ] **Step 4: Wire into `run()`**

In `migrations.rs::run()`, add a V25 block **after the V23a block** (~line 1324):

```rust
    // V25: marketplace_standalone_installs — tracks standalone skill/MCP installs.
    tracing::debug!("Running migration V25: marketplace_standalone_installs");
    for stmt in SQL_V25.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V25 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib v24`
Expected: both PASS.

Run: `cd src-tauri && cargo test --lib migrations 2>&1 | tail -10`
Expected: all migration tests green.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "feat(db): V25 — marketplace_standalone_installs

Tracks standalone skill/MCP marketplace installs. slug PK (installed at
most once); mcp_server_id links a type:mcp install to its
mcp_servers.json entry, NULL for skills. Read by AppsTab and uninstall."
```

---

### Task 2: Protocol — `validate_common` + `McpServerBlock` field

**Files:**
- Modify: `src-tauri/src/automation/protocol/humane_v1.rs` (add `McpServerBlock` struct + `mcp_server` field + `validate_common`)
- Test: `src-tauri/src/automation/protocol/humane_v1.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `humane_v1.rs`:

```rust
#[test]
fn validate_common_accepts_skill_and_mcp() {
    // A type:skill spec — rejected by the automation .validate() (kind check),
    // but validate_common accepts it.
    let skill_yaml = r#"
spec_version: "1"
name: My Skill
version: 1.0.0
author: t
description: A handy skill
type: skill
system_prompt: You are a helpful skill.
"#;
    let skill: HumaneAutomationSpec = serde_yml::from_str(skill_yaml).expect("parses");
    assert!(skill.validate().is_err(), "automation validate() rejects type:skill");
    assert!(super::validate_common(&skill).is_ok(), "validate_common accepts type:skill");

    // A skill with an empty system_prompt fails validate_common.
    let bad_skill_yaml = r#"
spec_version: "1"
name: Bad Skill
version: 1.0.0
author: t
description: missing prompt
type: skill
system_prompt: ""
"#;
    let bad: HumaneAutomationSpec = serde_yml::from_str(bad_skill_yaml).expect("parses");
    assert!(super::validate_common(&bad).is_err(), "empty system_prompt rejected for skill");

    // A type:mcp spec with an mcp_server block.
    let mcp_yaml = r#"
spec_version: "1"
name: My MCP
version: 1.0.0
author: t
description: wraps a server
type: mcp
mcp_server:
  command: npx
  args: ["-y", "@modelcontextprotocol/server-postgres"]
  env:
    DATABASE_URL: "{{config.db_url}}"
"#;
    let mcp: HumaneAutomationSpec = serde_yml::from_str(mcp_yaml).expect("parses");
    assert!(super::validate_common(&mcp).is_ok(), "validate_common accepts type:mcp with mcp_server");
    let block = mcp.mcp_server.as_ref().expect("mcp_server parsed");
    assert_eq!(block.command, "npx");
    assert_eq!(block.args, vec!["-y", "@modelcontextprotocol/server-postgres"]);
    assert_eq!(block.env.get("DATABASE_URL").map(String::as_str), Some("{{config.db_url}}"));

    // A type:mcp spec WITHOUT an mcp_server block fails validate_common.
    let bad_mcp_yaml = r#"
spec_version: "1"
name: Bad MCP
version: 1.0.0
author: t
description: no server block
type: mcp
"#;
    let bad_mcp: HumaneAutomationSpec = serde_yml::from_str(bad_mcp_yaml).expect("parses");
    assert!(super::validate_common(&bad_mcp).is_err(), "type:mcp without mcp_server rejected");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib validate_common_accepts_skill_and_mcp`
Expected: FAIL — `mcp_server` field + `validate_common` don't exist.

- [ ] **Step 3: Add `McpServerBlock` + the `mcp_server` field**

In `humane_v1.rs`, add the struct near the other supporting structs:

```rust
/// `mcp_server` block from a `type: mcp` spec — how to start the MCP server
/// process. Shape fixed by DHP spec/app-spec.md §10. `garde(skip)` because
/// these are runtime-substituted values, not parse-time-validatable.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, garde::Validate)]
pub struct McpServerBlock {
    #[garde(skip)]
    pub command: String,
    #[garde(skip)]
    #[serde(default)]
    pub args: Vec<String>,
    #[garde(skip)]
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[garde(skip)]
    #[serde(default)]
    pub cwd: Option<String>,
}
```

Add the field to `HumaneAutomationSpec` (near `memory_schema` / `output` — the other `Option` fields):

```rust
    #[garde(skip)]
    #[serde(default)]
    pub mcp_server: Option<McpServerBlock>,
```

- [ ] **Step 4: Add `validate_common`**

Add as a free function in `humane_v1.rs` (not a method — it's a cross-type check that deliberately bypasses the automation-only garde rules):

```rust
/// Lightweight validation shared by all package types. Unlike
/// `HumaneAutomationSpec::validate()` (which hard-requires `kind == "automation"`
/// via the `must_be_automation` garde validator), this checks only the fields
/// every type needs, plus the per-type minimum:
///   - all types: name / version / description non-empty
///   - `skill`: system_prompt non-empty
///   - `mcp`: an `mcp_server` block with a non-empty command
pub fn validate_common(spec: &HumaneAutomationSpec) -> Result<(), String> {
    if spec.name.trim().is_empty() {
        return Err("name is empty".into());
    }
    if spec.version.trim().is_empty() {
        return Err("version is empty".into());
    }
    if spec.description.trim().is_empty() {
        return Err("description is empty".into());
    }
    match spec.kind.as_str() {
        "skill" => {
            if spec.system_prompt.trim().is_empty() {
                return Err("type:skill requires a non-empty system_prompt".into());
            }
        }
        "mcp" => match &spec.mcp_server {
            Some(block) if !block.command.trim().is_empty() => {}
            Some(_) => return Err("type:mcp mcp_server.command is empty".into()),
            None => return Err("type:mcp requires an mcp_server block".into()),
        },
        _ => {}
    }
    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib validate_common_accepts_skill_and_mcp`
Expected: PASS.

Run: `cd src-tauri && cargo test --lib humane_v1 2>&1 | tail -10`
Expected: all green — the existing automation parse/validate tests still pass (the new `mcp_server` field is `Option`, `#[serde(default)]`, so existing automation specs deserialize fine).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/protocol/humane_v1.rs
git commit -m "feat(protocol): McpServerBlock field + validate_common

McpServerBlock types the type:mcp spec's mcp_server block
(command/args/env/cwd, shape per DHP app-spec.md §10). validate_common
is a cross-type field check that bypasses the automation-only
must_be_automation garde rule — used by the standalone skill/mcp
install paths which can't call the automation .validate()."
```

---

### Task 3: `standalone_install.rs` module — translation + staging

**Files:**
- Create: `src-tauri/src/automation/marketplace/standalone_install.rs`
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (add `mod standalone_install;`)
- Test: `src-tauri/src/automation/marketplace/standalone_install.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Module skeleton + failing tests**

Create `standalone_install.rs`:

```rust
//! Translation + staging for standalone (non-bundled) marketplace items.
//!
//! - `type: skill` → a uClaw SKILL.md written under
//!   ~/.uclaw/skills/_marketplace/_standalone/<slug>/.
//! - `type: mcp`   → a crate::mcp::McpServerConfig the caller registers with
//!   the MCP manager.
//!
//! Lives in its own module — parallel to skill_install.rs (3b-α's bundled-skill
//! staging) — so mod.rs stays focused on orchestration.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::automation::protocol::humane_v1::{HumaneAutomationSpec, McpServerBlock};

/// Render a `type: skill` spec into SKILL.md text — YAML frontmatter
/// (name + description) followed by the system_prompt as the body.
pub fn render_skill_md(spec: &HumaneAutomationSpec) -> String {
    // serde_yml round-trips strings with proper escaping; build a tiny
    // frontmatter struct rather than hand-formatting.
    #[derive(serde::Serialize)]
    struct Frontmatter<'a> {
        name: &'a str,
        description: &'a str,
    }
    let fm = serde_yml::to_string(&Frontmatter {
        name: &spec.name,
        description: &spec.description,
    })
    .unwrap_or_else(|_| format!("name: {}\ndescription: {}\n", spec.name, spec.description));
    format!("---\n{}---\n\n{}\n", fm, spec.system_prompt)
}

/// Stage a standalone skill's SKILL.md under skills_root/.staging/_standalone/<slug>/
/// then atomic-rename into skills_root/_marketplace/_standalone/<slug>/.
/// Returns the final directory. Cleans staging + returns Err on any failure.
pub fn install_skill_files(slug: &str, skill_md: &str, skills_root: &Path) -> Result<PathBuf> {
    let staging = skills_root.join(".staging").join("_standalone").join(slug);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("create staging {}", staging.display()))?;
    std::fs::write(staging.join("SKILL.md"), skill_md)
        .with_context(|| "write staged SKILL.md")?;

    let final_root = skills_root.join("_marketplace").join("_standalone");
    std::fs::create_dir_all(&final_root)
        .with_context(|| format!("create {}", final_root.display()))?;
    let final_dir = final_root.join(slug);
    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir)
            .with_context(|| format!("remove existing {}", final_dir.display()))?;
    }
    std::fs::rename(&staging, &final_dir)
        .with_context(|| format!("rename {} -> {}", staging.display(), final_dir.display()))?;
    Ok(final_dir)
}

/// Substitute `{{config.<key>}}` occurrences in an MCP env map with values
/// from the user config. A reference whose key is absent is left literal
/// (logged by the caller) — better than silently dropping the variable.
pub fn substitute_env(
    env: &HashMap<String, String>,
    user_config: &serde_json::Value,
) -> HashMap<String, String> {
    let cfg = user_config.as_object();
    env.iter()
        .map(|(k, v)| {
            let mut out = v.clone();
            if let Some(obj) = cfg {
                for (ck, cv) in obj {
                    let token = format!("{{{{config.{}}}}}", ck);
                    let replacement = match cv {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    out = out.replace(&token, &replacement);
                }
            }
            (k.clone(), out)
        })
        .collect()
}

/// Translate a `type: mcp` spec's mcp_server block into an McpServerConfig.
/// `id` is a fresh UUID; the caller stores `slug` separately to link back.
pub fn build_mcp_config(
    slug: &str,
    spec: &HumaneAutomationSpec,
    block: &McpServerBlock,
    user_config: &serde_json::Value,
) -> crate::mcp::McpServerConfig {
    crate::mcp::McpServerConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: spec.name.clone(),
        description: format!("{} (marketplace://halo/{})", spec.description, slug),
        transport_type: crate::mcp::TransportType::Stdio,
        command: block.command.clone(),
        args: block.args.clone(),
        env: substitute_env(&block.env, user_config),
        url: None,
        enabled: true,
        auto_approve: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_spec() -> HumaneAutomationSpec {
        serde_yml::from_str(
            "spec_version: \"1\"\nname: Summariser\nversion: 1.0.0\nauthor: t\n\
             description: Summarises text\ntype: skill\nsystem_prompt: You summarise.\n",
        )
        .unwrap()
    }

    #[test]
    fn render_skill_md_has_frontmatter_and_body() {
        let md = render_skill_md(&skill_spec());
        assert!(md.starts_with("---\n"));
        assert!(md.contains("name: Summariser"));
        assert!(md.contains("description: Summarises text"));
        assert!(md.trim_end().ends_with("You summarise."));
    }

    #[test]
    fn install_skill_files_stages_then_promotes() {
        let tmp = tempfile::tempdir().unwrap();
        let final_dir = install_skill_files("summariser", "---\nname: x\n---\nbody\n", tmp.path()).unwrap();
        assert!(final_dir.join("SKILL.md").exists());
        assert_eq!(
            final_dir,
            tmp.path().join("_marketplace").join("_standalone").join("summariser"),
        );
        // Staging is gone.
        assert!(!tmp.path().join(".staging").join("_standalone").join("summariser").exists());
    }

    #[test]
    fn install_skill_files_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        install_skill_files("s", "---\nname: v1\n---\nold\n", tmp.path()).unwrap();
        install_skill_files("s", "---\nname: v2\n---\nnew\n", tmp.path()).unwrap();
        let content = std::fs::read_to_string(
            tmp.path().join("_marketplace").join("_standalone").join("s").join("SKILL.md"),
        ).unwrap();
        assert!(content.contains("new"));
        assert!(!content.contains("old"));
    }

    #[test]
    fn substitute_env_replaces_config_refs() {
        let mut env = HashMap::new();
        env.insert("DB".to_string(), "{{config.db_url}}".to_string());
        env.insert("STATIC".to_string(), "literal".to_string());
        let cfg = serde_json::json!({ "db_url": "postgres://x" });
        let out = substitute_env(&env, &cfg);
        assert_eq!(out.get("DB").map(String::as_str), Some("postgres://x"));
        assert_eq!(out.get("STATIC").map(String::as_str), Some("literal"));
    }

    #[test]
    fn substitute_env_leaves_unknown_refs_literal() {
        let mut env = HashMap::new();
        env.insert("X".to_string(), "{{config.missing}}".to_string());
        let out = substitute_env(&env, &serde_json::json!({}));
        assert_eq!(out.get("X").map(String::as_str), Some("{{config.missing}}"));
    }

    #[test]
    fn build_mcp_config_translates_block() {
        let spec: HumaneAutomationSpec = serde_yml::from_str(
            "spec_version: \"1\"\nname: PG\nversion: 1.0.0\nauthor: t\n\
             description: postgres\ntype: mcp\nmcp_server:\n  command: npx\n  args: [\"-y\", \"pg\"]\n",
        ).unwrap();
        let block = spec.mcp_server.clone().unwrap();
        let cfg = build_mcp_config("pg-mcp", &spec, &block, &serde_json::json!({}));
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args, vec!["-y", "pg"]);
        assert_eq!(cfg.name, "PG");
        assert!(!cfg.id.is_empty());
    }
}
```

Register the module in `mod.rs` — add `mod standalone_install;` next to the existing `mod skill_install;` / `mod cache;` cluster.

- [ ] **Step 2: Run unit tests**

Run: `cd src-tauri && cargo test --lib standalone_install`
Expected: 6 PASS.

- [ ] **Step 3: Build check**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors. `#[allow(dead_code)]` on the module's `pub` fns is acceptable here — Task 4 wires them in. Add it to the module (`#![allow(dead_code)]` at the top, or per-fn) if the compiler warns; Task 4 removes it.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/marketplace/standalone_install.rs src-tauri/src/automation/marketplace/mod.rs
git commit -m "feat(marketplace): standalone_install module — skill/mcp translation

render_skill_md (spec → SKILL.md text), install_skill_files (staging +
atomic rename into _marketplace/_standalone/<slug>/), substitute_env
({{config.key}} → user config value), build_mcp_config (mcp_server block
→ McpServerConfig). Pure translation + fs staging; the install dispatcher
wires these in next."
```

---

### Task 4: Install dispatcher — `install_marketplace_item` + 3 flat functions

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (rename `install_human` → `install_automation`; add `install_marketplace_item`, `install_standalone_skill`, `install_standalone_mcp`)
- Modify: `src-tauri/src/automation/marketplace/types.rs` (add `InstallOutcome`)
- Modify: `src-tauri/src/tauri_commands.rs` (update the one call site in `install_marketplace_human`)
- Test: `src-tauri/src/automation/marketplace/mod.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Add `InstallOutcome` to types.rs**

```rust
/// Result of installing any marketplace item. The automation path carries the
/// installed spec row; skill/mcp paths carry a lighter confirmation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum InstallOutcome {
    Automation { spec: crate::automation::manager::HumaneSpecRow },
    Skill { slug: String, install_path: String },
    Mcp { slug: String, mcp_server_id: String },
}
```

(If `HumaneSpecRow`'s path differs, fix the `use` — grep `pub struct HumaneSpecRow`.)

- [ ] **Step 2: Rename `install_human` → `install_automation`, return `InstallOutcome`**

Rename the function `install_human` to `install_automation`. Its body is **unchanged** except the final return: wrap the `HumaneSpecRow` it produces in `Ok(InstallOutcome::Automation { spec: row })`. Update its return type to `Result<InstallOutcome>`.

- [ ] **Step 3: Add the `install_marketplace_item` dispatcher**

```rust
/// Install dispatcher — resolves the registry item, routes by type.
pub async fn install_marketplace_item(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    space_id: Option<String>,
    user_config: Option<serde_json::Value>,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
    mcp_manager: crate::mcp::SharedMcpManager,
    progress_channel: Option<String>,
) -> Result<InstallOutcome> {
    let source = RegistrySource::default();
    let _ = cache::sync_registry(&runtime.db, &source, false).await;
    let item = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found in registry: {}", slug))?
            .0
    };
    match item.app_type.as_str() {
        "automation" => {
            install_automation(runtime, app_handle, slug, space_id, user_config, skills_registry, progress_channel).await
        }
        "skill" => {
            install_standalone_skill(runtime, app_handle, slug, skills_registry, progress_channel).await
        }
        "mcp" => {
            install_standalone_mcp(runtime, app_handle, slug, user_config, mcp_manager, progress_channel).await
        }
        other => Err(anyhow!("marketplace item type '{}' is not installable", other)),
    }
}
```

Note: `install_automation` keeps its own internal `sync_registry` + `get_item_with_spec` call — the dispatcher's pre-resolve is only to read `app_type`. That's a tiny duplicate fetch (cache hit, cheap); acceptable for keeping `install_automation`'s body verbatim. (If the duplicate is distasteful, the plan's reviewer can suggest threading the resolved item through — but verbatim-body is the safer bisectable choice.)

- [ ] **Step 4: Add `install_standalone_skill`**

```rust
async fn install_standalone_skill(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    skills_registry: Arc<RwLock<SkillsRegistry>>,
    progress_channel: Option<String>,
) -> Result<InstallOutcome> {
    use tauri::Emitter;
    let source = RegistrySource::default();
    let emit = |phase: &str, percent: u8, message: Option<&str>| {
        if let Some(ch) = &progress_channel {
            let _ = app_handle.emit(ch, MarketplaceInstallProgress {
                phase: phase.into(), slug: slug.to_string(), percent,
                message: message.map(String::from),
            });
        }
    };

    emit("fetching_spec", 20, Some("拉取 spec.yaml"));
    let (item, cached_yaml, _, _) = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found: {}", slug))?
    };
    let yaml = match cached_yaml {
        Some(y) => y,
        None => {
            let entry = registry_entry_for(slug, &item);
            halo_adapter::fetch_spec_yaml(&source, &entry).await?
        }
    };

    emit("parsing", 40, Some("解析 skill spec"));
    let spec: crate::automation::protocol::humane_v1::HumaneAutomationSpec =
        serde_yml::from_str(&yaml).with_context(|| format!("parse spec.yaml for skill {}", slug))?;
    crate::automation::protocol::humane_v1::validate_common(&spec)
        .map_err(|e| anyhow!("invalid skill spec for {}: {}", slug, e))?;

    emit("installing", 70, Some("写入 SKILL.md"));
    let skill_md = standalone_install::render_skill_md(&spec);
    let skills_root = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?
        .join(".uclaw").join("skills");
    let install_dir = standalone_install::install_skill_files(slug, &skill_md, &skills_root)?;

    emit("registering_skills", 85, Some("注册 skill 扫描目录"));
    {
        let standalone_root = skills_root.join("_marketplace").join("_standalone");
        let mut reg = skills_registry.write().await;
        reg.add_scan_dir(standalone_root, crate::skills::SkillProvenance::Marketplace);
        let _ = reg.discover();
    }

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    {
        let conn = runtime.db.lock().unwrap();
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) VALUES (?,?,?,?,NULL)",
            rusqlite::params![slug, "skill", item.version, now_secs],
        ) {
            tracing::error!(slug = %slug, error = %e, "failed to record standalone skill install");
        }
    }

    emit("complete", 100, Some("完成"));
    Ok(InstallOutcome::Skill { slug: slug.to_string(), install_path: install_dir.to_string_lossy().to_string() })
}
```

`registry_entry_for(slug, &item)` — there's already an inline `RegistryEntry { ... }` builder duplicated in `install_human` (now `install_automation`); extract it as a small free helper `fn registry_entry_for(slug: &str, item: &MarketplaceItem) -> RegistryEntry` and reuse it in both `install_automation` and the standalone fns. (This dedups the 3 copies the 3b-α reviewer flagged.)

- [ ] **Step 5: Add `install_standalone_mcp`**

```rust
async fn install_standalone_mcp(
    runtime: &AppRuntimeService,
    app_handle: tauri::AppHandle,
    slug: &str,
    user_config: Option<serde_json::Value>,
    mcp_manager: crate::mcp::SharedMcpManager,
    progress_channel: Option<String>,
) -> Result<InstallOutcome> {
    use tauri::Emitter;
    let source = RegistrySource::default();
    let emit = |phase: &str, percent: u8, message: Option<&str>| {
        if let Some(ch) = &progress_channel {
            let _ = app_handle.emit(ch, MarketplaceInstallProgress {
                phase: phase.into(), slug: slug.to_string(), percent,
                message: message.map(String::from),
            });
        }
    };

    emit("fetching_spec", 20, Some("拉取 spec.yaml"));
    let (item, cached_yaml, _, _) = {
        let conn = runtime.db.lock().unwrap();
        cache::get_item_with_spec(&conn, &source.id, slug)?
            .ok_or_else(|| anyhow!("slug not found: {}", slug))?
    };
    let yaml = match cached_yaml {
        Some(y) => y,
        None => {
            let entry = registry_entry_for(slug, &item);
            halo_adapter::fetch_spec_yaml(&source, &entry).await?
        }
    };

    emit("parsing", 50, Some("解析 mcp spec"));
    let spec: crate::automation::protocol::humane_v1::HumaneAutomationSpec =
        serde_yml::from_str(&yaml).with_context(|| format!("parse spec.yaml for mcp {}", slug))?;
    crate::automation::protocol::humane_v1::validate_common(&spec)
        .map_err(|e| anyhow!("invalid mcp spec for {}: {}", slug, e))?;
    let block = spec.mcp_server.clone()
        .ok_or_else(|| anyhow!("mcp spec {} missing mcp_server block", slug))?;

    emit("installing", 75, Some("注册 MCP server"));
    let cfg = standalone_install::build_mcp_config(
        slug, &spec, &block, &user_config.unwrap_or(serde_json::Value::Null),
    );
    let mcp_server_id = cfg.id.clone();
    {
        let mut mgr = mcp_manager.write().await;
        mgr.add_server(cfg).map_err(|e| anyhow!("MCP manager add_server failed: {}", e))?;
    }

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    {
        let conn = runtime.db.lock().unwrap();
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) VALUES (?,?,?,?,?)",
            rusqlite::params![slug, "mcp", item.version, now_secs, mcp_server_id],
        ) {
            tracing::error!(slug = %slug, error = %e, "failed to record standalone mcp install");
        }
    }

    emit("complete", 100, Some("完成"));
    Ok(InstallOutcome::Mcp { slug: slug.to_string(), mcp_server_id })
}
```

Confirm `McpManager::add_server`'s exact signature/return by reading `mcp.rs` — adjust the `.map_err` if it returns something other than a `String`-displayable error.

- [ ] **Step 6: Update the Tauri command call site**

In `tauri_commands.rs`, `install_marketplace_human` currently calls `install_human(...)`. Change it to call `install_marketplace_item(...)`, passing the new `mcp_manager` arg (`state.mcp_manager.clone()`), and adjust the return type — the command now returns `InstallOutcome` instead of `HumaneSpecRow`. Update the command's `Result<..., Error>` signature accordingly.

- [ ] **Step 7: Write the dispatcher routing test**

Add to `mod.rs` test module:

```rust
#[tokio::test]
async fn install_marketplace_item_rejects_extension_type() {
    // We assert the type-routing decision directly: a known item_type that
    // isn't installable must produce an error. Full install paths need a live
    // HTTP server + runtime, so this test targets the routing contract via a
    // seeded cache item of type 'extension'.
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    // Seed an 'extension' item into the V23a cache.
    // (Use the same INSERT shape the existing cache tests use — read cache.rs
    // tests for the marketplace_items column list.)
    // ... insert item_type='extension' row ...
    // Then assert get_item_with_spec returns it and the dispatcher's match arm
    // for a non-(automation|skill|mcp) type is the Err arm.
}
```

If wiring a full async dispatcher test is too heavy, instead extract the type-routing decision into a tiny pure helper `fn route_install_type(app_type: &str) -> InstallRoute` (enum `Automation | Skill | Mcp | Unsupported`) and unit-test that directly — cleaner and matches the "test the contract, not the plumbing" approach 3b-α used for `validating_caps`. **Prefer this** — add `route_install_type` + test its 4 cases.

- [ ] **Step 8: Verify**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib marketplace 2>&1 | tail -15
```

Expected: zero errors; all marketplace tests green (existing automation install/uninstall tests must still pass — `install_automation` is the verbatim old body).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs src-tauri/src/automation/marketplace/types.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(marketplace): install dispatcher + standalone skill/mcp paths

install_human → install_automation (body verbatim). New
install_marketplace_item dispatches by item type:
- skill → fetch spec, validate_common, render SKILL.md, stage into
  _marketplace/_standalone/<slug>/, register scan dir, write V25 row.
- mcp → fetch spec, read mcp_server block, build McpServerConfig,
  register with the MCP manager, write V25 row linking mcp_server_id.
- extension/unknown → Err. InstallOutcome enum carries the per-type
  result. registry_entry_for dedups the RegistryEntry builder."
```

---

### Task 5: `validating_caps` recognises installed MCPs

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (the `validating_caps` phase inside `install_automation`)
- Test: `src-tauri/src/automation/marketplace/mod.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Add to `mod.rs` test module:

```rust
#[test]
fn capability_check_recognises_installed_mcp() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    conn.execute(
        "INSERT INTO marketplace_standalone_installs \
            (slug, item_type, version, installed_at, mcp_server_id) \
            VALUES ('postgres-mcp', 'mcp', '1.0.0', 0, 'srv-1')",
        [],
    ).unwrap();

    // ai-browser resolves via capability_map; postgres-mcp via installed table;
    // unknown-mcp resolves nowhere → reported missing.
    let missing = super::missing_capabilities(
        &conn,
        &["ai-browser".to_string(), "postgres-mcp".to_string(), "unknown-mcp".to_string()],
    );
    assert_eq!(missing, vec!["unknown-mcp".to_string()]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib capability_check_recognises_installed_mcp`
Expected: FAIL — `missing_capabilities` does not exist.

- [ ] **Step 3: Extract `missing_capabilities` + use it in `validating_caps`**

Add a free function in `mod.rs`:

```rust
/// Given the MCP ids an automation requires, return the ones uClaw cannot
/// satisfy — i.e. not resolvable via the built-in capability_map AND not
/// present as an installed standalone MCP. (3b-δ replaces capability_map with
/// a configurable table; this function's installed-MCP check is additive.)
fn missing_capabilities(conn: &rusqlite::Connection, mcp_ids: &[String]) -> Vec<String> {
    let installed: std::collections::HashSet<String> = conn
        .prepare("SELECT slug FROM marketplace_standalone_installs WHERE item_type = 'mcp'")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();
    mcp_ids
        .iter()
        .filter(|id| {
            crate::automation::capability_map::resolve_capability(id).is_none()
                && !installed.contains(*id)
        })
        .cloned()
        .collect()
}
```

In `install_automation`'s `validating_caps` phase, replace the existing inline "filter by `resolve_capability(...).is_none()`" loop with a call to `missing_capabilities(&conn, &mcp_ids)`. Read the current `validating_caps` block (3b-α added it) to see the exact local variable names — keep the warning-emit behaviour unchanged, just swap the missing-list computation.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib capability_check_recognises_installed_mcp`
Expected: PASS.

Run: `cd src-tauri && cargo test --lib marketplace 2>&1 | tail -10`
Expected: green — the existing `validating_caps`/`capability_validation` test still passes (ai-browser still resolves; the change only *adds* the installed-MCP fallback).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs
git commit -m "feat(marketplace): validating_caps recognises installed MCPs

An automation requiring an MCP now passes capability validation if that
MCP is installed as a standalone marketplace MCP — not only if it's a
built-in (capability_map). Makes installing an MCP meaningful. The
capability_map rewrite to a configurable table stays deferred to 3b-δ;
this is purely an additive read against marketplace_standalone_installs."
```

---

### Task 6: Un-filter store queries + `uninstall_marketplace_item` + `list_standalone_inner`

**Files:**
- Modify: `src-tauri/src/automation/marketplace/mod.rs` (`list_humans`, `query_marketplace_cached`, rename `uninstall_human` → add dispatcher, add `list_standalone_inner`)
- Modify: `src-tauri/src/automation/marketplace/types.rs` (add `StandaloneInstall` DTO)
- Test: `src-tauri/src/automation/marketplace/mod.rs` inline `#[cfg(test)]`

- [ ] **Step 1: Add `StandaloneInstall` DTO**

In `types.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandaloneInstall {
    pub slug: String,
    pub item_type: String,   // "skill" | "mcp"
    pub version: String,
    pub installed_at: i64,
    pub mcp_server_id: Option<String>,
}
```

- [ ] **Step 2: Write the failing tests**

Add to `mod.rs` test module:

```rust
#[test]
fn list_standalone_inner_returns_rows() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    conn.execute(
        "INSERT INTO marketplace_standalone_installs VALUES ('s1','skill','1.0.0',100,NULL)", [],
    ).unwrap();
    conn.execute(
        "INSERT INTO marketplace_standalone_installs VALUES ('m1','mcp','2.0.0',200,'srv-9')", [],
    ).unwrap();
    let list = super::list_standalone_inner(&conn).unwrap();
    assert_eq!(list.len(), 2);
    // ordered by installed_at DESC — m1 first
    assert_eq!(list[0].slug, "m1");
    assert_eq!(list[0].mcp_server_id.as_deref(), Some("srv-9"));
    assert_eq!(list[1].slug, "s1");
    assert_eq!(list[1].mcp_server_id, None);
}

#[test]
fn uninstall_standalone_skill_removes_files_and_row() {
    let tmp = tempfile::tempdir().unwrap();
    let skills_root = tmp.path().join("skills");
    let dir = skills_root.join("_marketplace").join("_standalone").join("s1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), b"---\nname: s1\n---\n").unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    conn.execute(
        "INSERT INTO marketplace_standalone_installs VALUES ('s1','skill','1.0.0',0,NULL)", [],
    ).unwrap();

    super::uninstall_standalone_skill_inner(&conn, &skills_root, "s1").unwrap();

    assert!(!dir.exists(), "skill dir removed");
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM marketplace_standalone_installs WHERE slug='s1'", [], |r| r.get(0),
    ).unwrap();
    assert_eq!(n, 0, "V25 row removed");
}
```

- [ ] **Step 3: Un-filter the store queries**

In `list_humans` (mod.rs:35) remove `.filter(|e| e.app_type == "automation")`. In `query_marketplace_cached`, find any equivalent `WHERE item_type = 'automation'` or post-filter and remove it (the V23a cache `item_type` column + the StoreHeader type-tabs already handle type filtering; the unfiltered query should return all types, and the caller's `item_type` param does the narrowing). Read the function to confirm where the automation-only assumption lives.

- [ ] **Step 4: Implement `list_standalone_inner` + the uninstall functions**

```rust
pub fn list_standalone_inner(conn: &rusqlite::Connection) -> Result<Vec<types::StandaloneInstall>> {
    let mut stmt = conn.prepare(
        "SELECT slug, item_type, version, installed_at, mcp_server_id \
            FROM marketplace_standalone_installs ORDER BY installed_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(types::StandaloneInstall {
            slug: r.get(0)?,
            item_type: r.get(1)?,
            version: r.get(2)?,
            installed_at: r.get(3)?,
            mcp_server_id: r.get(4)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Sync core of standalone-skill uninstall — testable without runtime handles.
pub fn uninstall_standalone_skill_inner(
    conn: &rusqlite::Connection,
    skills_root: &std::path::Path,
    slug: &str,
) -> Result<()> {
    let dir = skills_root.join("_marketplace").join("_standalone").join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
    }
    conn.execute(
        "DELETE FROM marketplace_standalone_installs WHERE slug = ?1",
        rusqlite::params![slug],
    )?;
    Ok(())
}
```

Then the async `uninstall_marketplace_item(runtime, skills_registry, mcp_manager, slug)` dispatcher:
1. Look up the slug in `marketplace_standalone_installs`. If found:
   - `item_type == "skill"` → `uninstall_standalone_skill_inner(&conn, &skills_root, slug)` + (under the registry lock) `discover()` to drop the now-missing skill.
   - `item_type == "mcp"` → read `mcp_server_id`, call `mcp_manager.write().await.remove_server(&id)`, then `DELETE FROM marketplace_standalone_installs WHERE slug = ?`.
2. If no standalone row → fall through to the existing automation uninstall logic (`uninstall_human` — keep that function, the dispatcher calls it).

`uninstall_marketplace_human` (Tauri command) keeps its name; its body now calls `uninstall_marketplace_item(...)` with the extra `mcp_manager` handle.

- [ ] **Step 5: Run tests + build**

```bash
cd src-tauri && cargo test --lib list_standalone_inner_returns_rows uninstall_standalone_skill_removes_files_and_row
cd src-tauri && cargo test --lib marketplace 2>&1 | tail -10
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

Expected: new tests PASS, all marketplace tests green, zero build errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/marketplace/mod.rs src-tauri/src/automation/marketplace/types.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(marketplace): un-filter store query + uninstall dispatcher

list_humans / query_marketplace_cached stop filtering to automation —
skill/mcp cards now reach the store (the V23a item_type column + the
StoreHeader type-tabs already do the narrowing). uninstall_marketplace_item
dispatches: standalone skill → rm _marketplace/_standalone/<slug>/ +
discover; standalone mcp → MCP-manager remove; otherwise → automation
uninstall. list_standalone_inner backs the AppsTab standalone section."
```

---

### Task 7: Tauri command + bridge — `list_standalone_installs`

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add `list_standalone_installs` command; confirm install/uninstall command bodies from Tasks 4 & 6 are correct)
- Modify: `src-tauri/src/main.rs` (register `list_standalone_installs`)
- Modify: `ui/src/lib/tauri-bridge.ts` (add `StandaloneInstall` type + `listStandaloneInstalls`; update `installMarketplaceHuman` return type to `InstallOutcome`)
- Modify: `ui/src/lib/types.ts` (sync stale `SkillInfo.provenance` union — Step 3b)

- [ ] **Step 1: Add the Tauri command**

In `tauri_commands.rs`:

```rust
#[tauri::command]
pub async fn list_standalone_installs(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::automation::marketplace::types::StandaloneInstall>, Error> {
    let conn = state.runtime_service.db.lock().unwrap();
    crate::automation::marketplace::list_standalone_inner(&conn)
        .map_err(|e| Error::Internal(format!("{:#}", e)))
}
```

(Confirm the `AppState` path to the DB connection — read how `install_marketplace_human` reaches `runtime`.)

- [ ] **Step 2: Register in `main.rs`**

Add `uclaw_core::tauri_commands::list_standalone_installs,` to the `invoke_handler!` macro next to `install_marketplace_human` / `uninstall_marketplace_human` (~main.rs:452).

- [ ] **Step 3: TS bridge**

In `ui/src/lib/tauri-bridge.ts`:

```ts
export interface StandaloneInstall {
  slug: string
  itemType: string // 'skill' | 'mcp'
  version: string
  installedAt: number
  mcpServerId: string | null
}

export const listStandaloneInstalls = (): Promise<StandaloneInstall[]> =>
  invoke<StandaloneInstall[]>('list_standalone_installs')
```

Also: `installMarketplaceHuman` now returns `InstallOutcome` (the Rust dispatcher's return type), not `HumaneSpecRow`. Add an `InstallOutcome` TS type mirroring the serde enum (`{ kind: 'automation', spec: HumaneSpecRow } | { kind: 'skill', slug, installPath } | { kind: 'mcp', slug, mcpServerId }`) and update `installMarketplaceHuman`'s `invoke<...>` generic. Grep for callers of `installMarketplaceHuman` — `InstallWizard` and `UpgradeModal`. `UpgradeModal` ignores the return (Task 4 of 3b-β: `await installMarketplaceHuman(slug)`). `InstallWizard` — check whether it reads the returned `HumaneSpecRow`; if it does, adapt it to handle `InstallOutcome` (the automation case still carries `.spec`). Keep the change minimal.

- [ ] **Step 3b: Sync the stale `SkillInfo.provenance` TS union**

In `ui/src/lib/types.ts`, `SkillInfo.provenance` is typed `'bundled' | 'user' | 'project'` — but the Rust `SkillProvenance` enum gained a `Marketplace` variant in 3b-α (serde-serialised as `"marketplace"`), and γ produces more marketplace-provenance skills (the standalone `_marketplace/_standalone/<slug>/` ones). Add `'marketplace'` to the union:

```ts
  provenance?: 'bundled' | 'user' | 'project' | 'marketplace';
```

This is a one-line type-correctness fix — the runtime value already flows through `list_skills`; the type was just left stale. Do **not** modify the PR #169 Skills / Integrations modules — γ's standalone items already surface there via `SkillsRegistry` / `mcp_manager` discovery (spec § 4.9); their marketplace lifecycle is owned by AppsTab (Task 10).

- [ ] **Step 4: Verify**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: zero errors both sides.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts ui/src/lib/types.ts
git commit -m "feat(marketplace): list_standalone_installs command + bridge

list_standalone_installs Tauri command reads marketplace_standalone_installs
for the AppsTab standalone section. Bridge gains StandaloneInstall +
listStandaloneInstalls; installMarketplaceHuman's return type widens to
the InstallOutcome enum. Also syncs the stale SkillInfo.provenance TS
union to include 'marketplace' (the Rust variant shipped in 3b-α)."
```

---

### Task 8: `StoreDetail` type-aware layout

**Files:**
- Modify: `ui/src/components/automation/StoreDetail.tsx`
- Test: `ui/src/components/automation/StoreDetail.test.tsx` (extend if it exists; else create)

- [ ] **Step 1: Write the failing test**

Read `StoreDetail.tsx` + any existing `StoreDetail.test.tsx` first. Add tests covering the new type branches. Sketch (adapt to the existing test harness — `renderWithProviders`, atom seeding for `marketplaceDetailAtom`):

```tsx
// type:skill — shows system_prompt + 安装技能, no 依赖 tab
test('skill layout: system_prompt + 安装技能 button, no subscriptions tab', async () => {
  // seed marketplaceDetailAtom with a detail whose item.appType === 'skill'
  // and parsedSpecJson.system_prompt set
  // assert: 安装技能 button present; 依赖 tab absent
})

// type:mcp — shows mcp_server panel + 安装 MCP
test('mcp layout: mcp_server command panel + 安装 MCP button', async () => {
  // seed detail with item.appType === 'mcp', parsedSpecJson.mcp_server = {command:'npx', args:[...]}
  // assert: command 'npx' rendered; 安装 MCP button present
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run StoreDetail 2>&1 | tail -8`
Expected: FAIL — new branches not implemented.

- [ ] **Step 3: Implement the type-aware layout**

In `StoreDetail.tsx`:
- Replace the `item.appType === 'automation' ? (...) : <stub>` branch (line ~147-178) with a three-way: `automation` (unchanged), `skill` (a `安装技能` button opening the InstallWizard), `mcp` (a `安装 MCP` button opening the wizard). The upgrade-button logic from 3b-β stays for the automation case; skill/mcp can reuse the same install/upgrade affordance (a standalone skill/mcp is also re-installable).
- Make the sub-tab strip type-aware: build the `TABS` list conditionally — `automation` → all four (`概览/配置/依赖/提示词`); `skill` → `概览` + (`配置` if `config_schema` non-empty) + `提示词`; `mcp` → `概览` + (`配置` if `config_schema` non-empty). Add an `mcp` overview panel that renders the `mcp_server` block (`command`, `args` joined, `env` keys) in a read-only `rounded-xl border border-border/50` panel.
- The `parsedSpecJson` access for `mcp_server` / `system_prompt`: extend the existing cautious narrowing (`spec` is already typed `{ i18n?, config_schema? }` — add `system_prompt?: string` and `mcp_server?: { command: string; args?: string[]; env?: Record<string,string> }`).

Theme tokens only; match the existing StoreDetail visual language.

- [ ] **Step 4: Run tests + tsc**

```bash
cd ui && npm test -- --run StoreDetail 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: new tests PASS, zero TS errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/automation/StoreDetail.tsx ui/src/components/automation/StoreDetail.test.tsx
git commit -m "feat(marketplace): StoreDetail type-aware layout for skill/mcp

Replaces the '{type} 安装在 Phase 3b 开放' stub. skill → system_prompt
preview + 安装技能; mcp → mcp_server command/args panel + 安装 MCP. The
sub-tab strip is now type-aware (skill/mcp have no 依赖 tab; 配置 shows
only when config_schema is present)."
```

---

### Task 9: `InstallWizard` type-aware steps

**Files:**
- Modify: `ui/src/components/automation/InstallWizard.tsx`
- Test: `ui/src/components/automation/InstallWizard.test.tsx` (extend if it exists; else create)

- [ ] **Step 1: Write the failing test**

Read `InstallWizard.tsx` first. Add:

```tsx
test('skill item skips the scope step', async () => {
  // open the wizard for an item whose appType === 'skill'
  // assert: the stepper does not show 'scope'; first step is 'config' or 'confirm'
})

test('automation item still shows the scope step', async () => {
  // open the wizard for an appType === 'automation' item
  // assert: 'scope' step present
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run InstallWizard 2>&1 | tail -8`
Expected: FAIL.

- [ ] **Step 3: Implement type-aware steps**

In `InstallWizard.tsx`:
- The wizard needs to know the item's `appType`. It has the `slug` — it can read the item type from the marketplace items atom / detail atom, or the opener (StoreDetail) can pass `appType` into the `installWizardAtom` state. Prefer passing `appType` through the atom state (StoreDetail already knows it) — smaller, no extra fetch.
- Compute the step sequence from `appType`: `automation` → `['scope','config','confirm','progress']`; `skill`/`mcp` → `['config','confirm','progress']`. If the item has no `config_schema`, `config` can also be skipped (optional — check whether the current wizard already skips empty config; if so, mirror it).
- The stepper render (`['scope','config','confirm'].map(...)`) becomes `steps.filter(s => s !== 'progress').map(...)` using the computed sequence.
- The "next step" transitions must use the computed sequence, not the hardcoded `STEPS`.

- [ ] **Step 4: Run tests + tsc**

```bash
cd ui && npm test -- --run InstallWizard 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: PASS, zero TS errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/automation/InstallWizard.tsx ui/src/components/automation/InstallWizard.test.tsx
git commit -m "feat(marketplace): InstallWizard type-aware step sequence

Skills and MCPs aren't workspace-scoped — the wizard skips the 'scope'
step for them. The step sequence is computed from the item's appType
(threaded through installWizardAtom) instead of the hardcoded STEPS."
```

---

### Task 10: `AppsTab` standalone-install section

> **Why AppsTab and not the new Skills/Integrations modules** (PR #169, now merged): standalone skills/MCPs *already* surface in those Kaleidoscope modules automatically — a `_marketplace/_standalone/` skill is discovered by `SkillsRegistry` → `list_skills`, a registered MCP shows in `listMcpServers`. Those are the **operational** views ("all my skills / MCPs"). AppsTab is the **marketplace-lifecycle** view (version, slug-keyed uninstall via `uninstall_marketplace_item`) — exactly parallel to how an installed automation lives in AppsTab while its bundled skills also show in the Skills module. This task does **not** modify the #169 modules; do not duplicate uninstall affordances into them.

**Files:**
- Modify: `ui/src/components/automation/AppsTab.tsx`
- Test: `ui/src/components/automation/AppsTab.test.tsx` (extend)

- [ ] **Step 1: Write the failing test**

Add to `AppsTab.test.tsx` (it mocks `@/lib/tauri-bridge` — add `listStandaloneInstalls` + `uninstallMarketplaceHuman` to the mock factory if not already there):

```tsx
test('renders standalone installs section with uninstall', async () => {
  ;(listStandaloneInstalls as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
    { slug: 'summariser', itemType: 'skill', version: '1.0.0', installedAt: 0, mcpServerId: null },
    { slug: 'pg-mcp', itemType: 'mcp', version: '2.0.0', installedAt: 0, mcpServerId: 'srv-1' },
  ])
  // listInstalledMarketplaceAutomations mock → []
  const { findByText, getAllByText } = renderWithProviders(<AppsTab />)
  expect(await findByText('summariser')).toBeInTheDocument()
  expect(await findByText('pg-mcp')).toBeInTheDocument()
  // 卸载 buttons present for standalone items
  fireEvent.click(getAllByText('卸载')[0])
  await waitFor(() => expect(uninstallMarketplaceHuman).toHaveBeenCalled())
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run AppsTab 2>&1 | tail -8`
Expected: FAIL.

- [ ] **Step 3: Implement the standalone section**

In `AppsTab.tsx`:
- On mount (alongside the existing `listInstalledMarketplaceAutomations` load), call `listStandaloneInstalls()` and keep the result in state.
- Render a labelled group — e.g. a `独立技能 / MCP` section header — below the automations list, one `rounded-xl border border-border/50 bg-card` card per `StandaloneInstall`: an `AppTypeBadge` (the `itemType` maps to 技能/MCP), the `slug` as the name, `v{version}`, and a `卸载` button calling `uninstallMarketplaceHuman(slug)` then refreshing both lists.
- If `listStandaloneInstalls()` returns `[]`, render nothing for the section (no empty-state noise — the automations list has its own empty state).

Theme tokens only; match the existing AppsTab card visual language.

- [ ] **Step 4: Run tests + tsc + full vitest**

```bash
cd ui && npm test -- --run AppsTab 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | head -5
cd ui && npm test -- --run 2>&1 | tail -8
```

Expected: AppsTab tests PASS, zero TS errors, no new failures vs baseline.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/automation/AppsTab.tsx ui/src/components/automation/AppsTab.test.tsx
git commit -m "feat(marketplace): AppsTab standalone skill/MCP section

Below the installed-automations list, a 独立技能 / MCP section lists
standalone marketplace installs (from listStandaloneInstalls) with an
AppTypeBadge + 卸载. Hidden entirely when there are none."
```

---

### Task 11: CLAUDE.md migration registry + PR

**Files:**
- Modify: `CLAUDE.md` (*Active migration registry* table — add V25 row)

- [ ] **Step 1: Add the V25 row**

In `CLAUDE.md`'s *Active migration registry* table, after the V23a row:

```markdown
| V25 | marketplace_standalone_installs (standalone skill/MCP install tracking) | **this PR** (Phase 3b-γ) |
```

- [ ] **Step 2: Full verification**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd ui && npx tsc --noEmit 2>&1 | head -3
cd ui && npm test -- --run 2>&1 | tail -5
```

Expected: Rust all green, zero TS errors, Vitest all green.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): register V25 marketplace_standalone_installs migration"
```

- [ ] **Step 4: Push + open PR**

```bash
git push -u origin worktree-phase3b-gamma-standalone-items

gh pr create --title "feat(marketplace): Phase 3b-γ — standalone skill / MCP entries" --body "$(cat <<'EOF'
## Summary

Makes `type: skill` and `type: mcp` marketplace packages installable. The DHP protocol already specifies these types; this is the uClaw-side consumption gap.

- **Install dispatcher.** `install_human` → `install_automation` (body verbatim). New `install_marketplace_item` matches `item.app_type` → `install_automation` / `install_standalone_skill` / `install_standalone_mcp` / Err(extension|unknown).
- **Standalone skill** → translate `spec.yaml` to a `SKILL.md`, stage + atomic-rename into `~/.uclaw/skills/_marketplace/_standalone/<slug>/`, register the scan dir.
- **Standalone MCP** → translate the `mcp_server` block to an `McpServerConfig` (with `{{config.key}}` env substitution), register with the MCP manager.
- **V25 `marketplace_standalone_installs`** tracks both; `mcp_server_id` links an MCP install to its server entry.
- **`validating_caps`** now recognises installed standalone MCPs — an automation depending on one stops warning (`capability_map` itself untouched — that rewrite is 3b-δ).
- **Store query un-filtered** — the 技能/MCP tabs show real cards. `StoreDetail` / `InstallWizard` / `AppsTab` are now type-aware.

DHP `index.json` has no skill/mcp entries yet — tested against synthetic fixtures; no DHP-repo changes. The feature works the moment DHP publishes entries.

## Commits (bisectable)

| # | Commit | Scope |
|---|--------|-------|
| 1 | feat(db): V25 — marketplace_standalone_installs | DB |
| 2 | feat(protocol): McpServerBlock field + validate_common | protocol |
| 3 | feat(marketplace): standalone_install module — skill/mcp translation | backend |
| 4 | feat(marketplace): install dispatcher + standalone skill/mcp paths | backend |
| 5 | feat(marketplace): validating_caps recognises installed MCPs | backend |
| 6 | feat(marketplace): un-filter store query + uninstall dispatcher | backend |
| 7 | feat(marketplace): list_standalone_installs command + bridge | backend + bridge |
| 8 | feat(marketplace): StoreDetail type-aware layout for skill/mcp | UI |
| 9 | feat(marketplace): InstallWizard type-aware step sequence | UI |
| 10 | feat(marketplace): AppsTab standalone skill/MCP section | UI |
| 11 | docs(claude): register V25 migration | docs |

Spec: docs/superpowers/specs/2026-05-14-phase3b-gamma-standalone-items-design.md
Plan: docs/superpowers/plans/2026-05-14-phase3b-gamma-standalone-items.md

## Test plan

- [ ] `cargo test --lib` — all green (new: V25 migration, validate_common, standalone_install module, dispatcher routing, validating_caps installed-MCP, list/uninstall standalone)
- [ ] `npm test -- --run` — all green (new: StoreDetail skill/mcp layouts, InstallWizard scope-skip, AppsTab standalone section)
- [ ] `npx tsc --noEmit` — zero errors
- [ ] Manual (fixture): a synthetic `type: skill` package installs → `SKILL.md` under `_marketplace/_standalone/`, SkillsRegistry discovers it
- [ ] Manual (fixture): a synthetic `type: mcp` package installs → MCP server registered, V25 row links it
- [ ] Manual: `type: extension` install fails with a clear error

## Follow-ups (Phase 3b-δ / ε / ζ)

- 3b-δ — multi-registry + `capability_map` → DB table
- 3b-ε — proxy adapters (Smithery / official MCP Registry / SkillHub)
- 3b-ζ — local hello-halo workspace as a registry source
EOF
)"
```

---

## Self-review

### Spec coverage

| Spec § | Where in plan |
|---|---|
| § 4.1 install dispatcher | Task 4 |
| § 4.2 `install_standalone_skill` (translation + staging + scan dir) | Task 3 (translation/staging) + Task 4 (orchestration) |
| § 4.3 `install_standalone_mcp` (`mcp_server` → `McpServerConfig`, `{{config.key}}`) | Task 2 (`McpServerBlock`) + Task 3 (`build_mcp_config`, `substitute_env`) + Task 4 (orchestration) |
| § 4.4 V25 `marketplace_standalone_installs` | Task 1 |
| § 4.5 `validating_caps` recognises installed MCPs | Task 5 |
| § 4.6 un-filter query + uninstall dispatcher | Task 6 |
| § 4.7 `StoreDetail` type-aware layout | Task 8 |
| § 4.8 `InstallWizard` type-aware steps | Task 9 |
| § 4.9 `AppsTab` standalone section + `list_standalone_installs` | Task 6 (`list_standalone_inner`) + Task 7 (command + bridge) + Task 10 (UI) |
| § 5 error handling | Distributed: Task 2 (`validate_common` rejects), Task 3 (staging rollback, literal-on-miss substitution), Task 4 (dispatcher Err for extension), Task 6 (best-effort uninstall) |
| § 6.1 Rust tests | Tasks 1, 2, 3, 4, 5, 6 each ship their listed tests |
| § 6.2 Vitest tests | Tasks 8, 9, 10 |
| § 7 V25 migration | Task 1 + Task 11 (registry doc) |
| § 10 done criteria | Task 11 PR test plan mirrors each item |

No gaps.

### Placeholder scan

No "TODO"/"TBD". Tasks 3, 7, 8, 9 say "read the file first" to confirm exact local names (existing test harness shape, `installWizardAtom` fields, `McpManager::add_server` signature, `validating_caps` locals) — directed investigation with the surrounding code shown, not placeholders. Task 4 Step 7 offers two test approaches and **picks one** ("Prefer this — add `route_install_type`"). Every code step ships complete code.

### Type consistency

- `McpServerBlock` `{ command, args, env, cwd }` — defined Task 2, consumed Task 3 (`build_mcp_config`), read Task 4 (`install_standalone_mcp`).
- `validate_common(&HumaneAutomationSpec) -> Result<(), String>` — defined + tested Task 2, called Task 4 (both standalone fns).
- `InstallOutcome` enum (`Automation { spec } | Skill { slug, install_path } | Mcp { slug, mcp_server_id }`) — defined Task 4, returned by all three install fns + the dispatcher, surfaced in the bridge Task 7.
- `StandaloneInstall` `{ slug, item_type, version, installed_at, mcp_server_id }` — defined Task 6, produced by `list_standalone_inner` (Task 6), exposed via `list_standalone_installs` (Task 7), consumed by `AppsTab` (Task 10). Rust snake_case ↔ TS camelCase via serde rename — `itemType` / `installedAt` / `mcpServerId` in the bridge.
- `standalone_install` module fns (`render_skill_md`, `install_skill_files`, `substitute_env`, `build_mcp_config`) — defined Task 3, called Task 4.
- `missing_capabilities(&Connection, &[String]) -> Vec<String>` — defined + tested Task 5.
- `list_standalone_inner` / `uninstall_standalone_skill_inner` — defined + tested Task 6, `list_standalone_inner` called by Task 7's command.
- `route_install_type` (if chosen in Task 4 Step 7) — local to Task 4, tested there.

Naming consistent across tasks.
