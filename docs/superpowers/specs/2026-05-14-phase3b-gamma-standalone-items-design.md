# Phase 3b-γ — Standalone Skill / MCP Marketplace Entries — Design

**Status:** Draft → ready to plan
**Author:** Claude (brainstormed with Ryan, 2026-05-14)
**Date:** 2026-05-14
**Scope:** Phase 3b third slice. Depends on 3b-α (PR #160) + 3b-β (PR #168), both merged. δ / ε / ζ stay deferred — see § 9.

---

## 1. Problem

The marketplace UI already has type-filter tabs (`全部 / 数字人 / 技能 / MCP`) in `StoreHeader`, but the 技能 / MCP tabs are permanently empty and `StoreDetail` stubs every non-automation item with `{type} 安装在 Phase 3b 开放`. The install path hard-rejects them:

- `install_human` ([marketplace/mod.rs:488](../../../src-tauri/src/automation/marketplace/mod.rs)): `if item.app_type != "automation" { return Err("only 'automation' installable in Phase 3a") }`.
- `list_humans` ([marketplace/mod.rs:35](../../../src-tauri/src/automation/marketplace/mod.rs)): `.filter(|e| e.app_type == "automation")`.

The DHP protocol **already specifies** the non-automation types — this is not a format-design problem, it's a uClaw-side consumption gap:

- `type: skill` — "a capability invoked on demand by the user." Requires `system_prompt`, no `subscriptions` (`spec/app-spec.md` §14).
- `type: mcp` — "wraps an external MCP server." Requires an `mcp_server: { command, args, env, cwd }` block, no `system_prompt`.
- `type: extension` — "UI extension or theme." uClaw has no extension system — **out of scope** (§ 9).

All DHP packages — regardless of type — live under `packages/digital-humans/<slug>/`, so the hardcoded spec path in `install_human` does **not** need to change.

The DHP `index.json` currently has **zero** non-automation entries. This slice builds the uClaw-side consumption capability and tests it against synthetic fixtures; it makes no changes to the DHP repo. When DHP publishes real skill/mcp entries, the feature works immediately.

## 2. Goals

1. **Installable standalone skills.** A `type: skill` package installs into uClaw's skill system — its `system_prompt` becomes a discoverable skill.
2. **Installable standalone MCPs.** A `type: mcp` package installs into uClaw's MCP manager — its `mcp_server` block becomes a registered MCP server.
3. **Visible in the store.** The 技能 / MCP filter tabs show real cards; `StoreDetail` renders a type-appropriate layout + install affordance.
4. **Tracked + uninstallable.** Standalone installs are recorded, shown in AppsTab, and cleanly removable.
5. **MCP dependencies resolve.** An automation declaring `requires.mcps: [some-mcp]` no longer warns once `some-mcp` is installed as a standalone MCP — installing an MCP becomes meaningful.

## 3. Non-goals (stay deferred)

- **`type: extension`.** uClaw has no UI-extension/theme system. The dispatcher rejects it with a clear error; nothing else.
- **Standalone skill packages that bundle their own script files.** A `type: skill` whose `spec.yaml` declares `requires.skills[].bundled` files. MVP handles the pure-`system_prompt` skill. The bundled-files case can later reuse 3b-α's `skill_install::fetch_bundled_skills` — noted, not built.
- **Proxy adapters** (Smithery / official MCP Registry / SkillHub) — 3b-ε.
- **Multi-registry sources** + the `capability_map` → DB-table rewrite — 3b-δ. This slice keeps `capability_map` exactly as-is; it only *adds* an installed-MCP check alongside it (see § 4.5).
- **`{{config.key}}` substitution edge cases** beyond simple `env` value replacement (e.g. nested references, defaults) — MVP does literal `{{config.key}}` → user-config-value replacement in `env` strings; anything fancier is deferred.

## 4. Design

### 4.1 Backend — install dispatcher

`install_human` is automation-shaped: it fetches the spec, writes an `automation_specs` row, and activates subscriptions. Skills and MCPs have none of that. Per the brainstorm decision, restructure into a **dispatcher + three flat install functions** (matches CLAUDE.md's "flat enumeration over generic dispatchers" preference — the dispatcher is a 3-arm match, each arm a self-contained function):

```rust
pub async fn install_marketplace_item(
    runtime, app_handle, slug, space_id, user_config, skills_registry, progress_channel,
) -> Result<InstallOutcome> {
    let item = /* resolve from cache as install_human does today */;
    match item.app_type.as_str() {
        "automation" => install_automation(...).await,   // existing install_human body, lifted verbatim
        "skill"      => install_standalone_skill(...).await,
        "mcp"        => install_standalone_mcp(...).await,
        other        => Err(anyhow!("type '{}' is not installable", other)),
    }
}
```

- `install_automation` — the **entire current `install_human` body**, moved into its own function unchanged. The Tauri command `install_marketplace_human` is renamed to `install_marketplace_item` (the bridge keeps the old export name aliased, or the frontend is updated — decide in the plan; the frontend already only calls one install fn).
- `InstallOutcome` — a small enum or struct so the three paths can return type-appropriate results. The automation path returns the `HumaneSpecRow` it returns today; skill/mcp return a lighter confirmation. The plan task confirms whether the existing return type can be widened or a new enum is cleaner.

The `app_type != "automation"` rejection at mod.rs:488 is **deleted** (the dispatcher's `match` replaces it).

### 4.2 Backend — `install_standalone_skill`

1. Fetch `packages/digital-humans/<slug>/spec.yaml` via the existing `halo_adapter::fetch_spec_yaml`.
2. Parse with serde into `HumaneAutomationSpec`. **Do NOT call `.validate()`** — `humane_v1.rs`'s `must_be_automation` garde validator hard-requires `kind == "automation"` and would reject every skill/mcp spec. Instead apply a lightweight common-field check: `name` / `version` / `description` non-empty, and (for skill) `system_prompt` non-empty. The plan adds a small `validate_common(&spec) -> Result<()>` helper in `humane_v1.rs` or `parse.rs`.
3. Translate to a `SKILL.md`:
   - Frontmatter: `name` and `description` from the spec.
   - Body: the spec's `system_prompt`.
4. Write to `~/.uclaw/skills/_marketplace/_standalone/<slug>/SKILL.md`. Use a staging + atomic-rename pattern consistent with 3b-α's `skill_install` (stage at `.staging/_standalone/<slug>/`, rename on success).
5. Register `~/.uclaw/skills/_marketplace/_standalone/` as a SkillsRegistry scan dir (`SkillProvenance::Marketplace`) if not already registered, then `discover()`. The 3b-α boot scan already walks `_marketplace/`'s direct children, so `_standalone` is picked up on restart automatically — SkillsRegistry's recursive scan finds `_standalone/<slug>/SKILL.md`.
6. Write the `marketplace_standalone_installs` row (§ 4.4).

### 4.3 Backend — `install_standalone_mcp`

1. Fetch + parse the `spec.yaml` (same parse-without-`.validate()` approach; common-field check requires the `mcp_server` block present).
2. Read the `mcp_server: { command, args, env, cwd }` block. In `HumaneAutomationSpec` this is not a typed field today — it'll be read from the lenient JSON the parser already keeps, or a typed `Option<McpServerBlock>` is added to the spec struct (plan decides — a typed field is cleaner since the shape is fixed by the DHP spec).
3. `{{config.key}}` substitution: if the package declares `config_schema` and the InstallWizard collected `user_config`, replace `{{config.<key>}}` occurrences in each `env` value with the user-provided value. No config → pass `env` through literally.
4. Translate to `crate::mcp::McpServerConfig` (`{ id, name, description, transport_type: Stdio, command, args, env, url: None, enabled: true, auto_approve: false }`). The `id` is a fresh UUID; the `slug` is what links it back (stored in the V24 row).
5. Register via the MCP manager — the same path `add_mcp_server` uses (`state.mcp_manager.write().await.add_server(config)`). `install_standalone_mcp` takes the `mcp_manager` handle as a parameter (plumbed from the Tauri command, same way 3b-α plumbed `skills_registry`).
6. Write the `marketplace_standalone_installs` row with `mcp_server_id` = the registered server's id.

### 4.4 Backend — V24 migration: `marketplace_standalone_installs`

```sql
CREATE TABLE IF NOT EXISTS marketplace_standalone_installs (
    slug          TEXT PRIMARY KEY,
    item_type     TEXT NOT NULL,        -- 'skill' | 'mcp'
    version       TEXT NOT NULL,
    installed_at  INTEGER NOT NULL,
    mcp_server_id TEXT                  -- NULL for skills; the mcp_servers.json id for mcp installs
);
```

Migration **V24** — next free per the CLAUDE.md registry (V22 merged, V23/V23a is the marketplace-cache pair; V24 is unambiguous). The PR updates the registry table.

`slug` is the PK — a standalone item is installed at most once. Re-installing (upgrade, à la 3b-β) is `INSERT OR REPLACE`.

### 4.5 Backend — `validating_caps` recognises installed MCPs

Today `install_automation`'s `validating_caps` phase resolves each `requires.mcps[].id` only through the hardcoded `capability_map::resolve_capability` (`ai-browser → builtin`). Misses warn.

γ adds a second check: if `resolve_capability` returns `None`, also check whether the id matches an **installed MCP server** (query the MCP manager's registered server list, or `marketplace_standalone_installs WHERE item_type='mcp'`). A match → no warning. This makes installing an MCP meaningful — automations depending on it stop warning.

This is a *read* against existing state, **not** a `capability_map` rewrite (that's δ). `capability_map.rs` is untouched.

### 4.6 Backend — un-filter the store query + uninstall dispatcher

- `list_humans` and `query_marketplace_cached` (`marketplace/mod.rs`) drop the `app_type == "automation"` filter. The V23a cache already has the `item_type` column; the store type-tabs already filter on it.
- `uninstall_marketplace_human` becomes `uninstall_marketplace_item(slug)`, dispatching on the item's type:
  - `automation` → existing `uninstall_human` logic.
  - standalone `skill` → `rm -rf ~/.uclaw/skills/_marketplace/_standalone/<slug>/` + `SkillsRegistry::remove_scan_dir` (only if `_standalone` now has no children) + `discover()` + `DELETE FROM marketplace_standalone_installs WHERE slug = ?`.
  - standalone `mcp` → remove from the MCP manager by `mcp_server_id` + `DELETE FROM marketplace_standalone_installs WHERE slug = ?`.

  The type is looked up from `marketplace_standalone_installs` (for standalone items) or `automation_specs` (for automations) — the plan picks the cleanest dispatch (probably: try `marketplace_standalone_installs` first; if no row, fall through to the automation path).

### 4.7 Frontend — `StoreDetail` type-aware layout

`StoreDetail` currently shows `{appType} 安装在 Phase 3b 开放` for non-automation. Replace:

- **`skill`** — show the `system_prompt` (collapsible, same as the existing automation prompt tab) + a `安装技能` button. Hide the `配置` tab unless `config_schema` is present; hide `依赖` / subscriptions-related content (skills have no subscriptions).
- **`mcp`** — show the `mcp_server` block (`command`, `args`, `env` keys) in a small read-only panel + a `安装 MCP` button. Hide `提示词` and `配置` tabs unless `config_schema` is present.
- The sub-tab strip becomes type-aware: `automation` keeps all four tabs; `skill` shows `概览` + (`配置` if present) + `提示词`; `mcp` shows `概览` + (`配置` if present).

### 4.8 Frontend — `InstallWizard` type-aware steps

Skills and MCPs are not workspace-scoped. The wizard's `scope` step is automation-only:

- `automation` — `scope` → (`config` if `config_schema`) → install (unchanged).
- `skill` / `mcp` — skip `scope`; go straight to (`config` if `config_schema`) → install.

The wizard reads the item's `appType` and branches its step sequence accordingly.

### 4.9 Frontend — `AppsTab` standalone-install section

`AppsTab` currently lists installed automations + their bundled skills. γ adds a section for standalone installs:

- New bridge `listStandaloneInstalls(): Promise<StandaloneInstall[]>` backed by a `list_standalone_installs` Tauri command reading `marketplace_standalone_installs`.
- Render each as a card with the `AppTypeBadge` (技能 / MCP), name, version, and an `卸载` button calling `uninstallMarketplaceItem(slug)`.
- Placement: a labelled group below the automations list (or a second list), per the same uClaw Design DNA — `rounded-xl` cards, theme tokens.

## 5. Error handling

| Failure | Behaviour |
|---|---|
| `type: extension` (or unknown type) reaches the dispatcher | `Err("type '<x>' is not installable")` — surfaced as an install-failure toast. |
| Skill spec.yaml missing `system_prompt` | The common-field check fails → install aborts before any file is written. |
| MCP spec.yaml missing the `mcp_server` block | Common-field check fails → abort before touching the MCP manager. |
| Skill staging fetch fails | Staging dir cleaned, no `_standalone/<slug>/` created, no V24 row — consistent with 3b-α's rollback boundary. |
| MCP manager `add_server` fails | No V24 row written; install returns the error. (The fetch produced no on-disk artifacts for MCPs, so there's nothing to roll back.) |
| `{{config.key}}` references a key not in `config_schema` / `user_config` | Leave the literal `{{config.key}}` in place + log a warning; don't abort (the MCP server may still start, or fail loudly itself — better than silently dropping the var). |
| Uninstall of a standalone item whose files/server are already gone | Best-effort: log + continue, still delete the V24 row (mirrors 3b-α uninstall semantics). |

## 6. Tests

### 6.1 Rust (inline `#[cfg(test)]`, fixtures for the synthetic skill/mcp specs)

- `install_standalone_skill_writes_skill_md` — fixture `type: skill` spec.yaml served by a mock HTTP server; assert `_marketplace/_standalone/<slug>/SKILL.md` exists with the right frontmatter + body, and a `marketplace_standalone_installs` row with `item_type='skill'`, `mcp_server_id` NULL.
- `install_standalone_mcp_translates_mcp_server` — fixture `type: mcp` spec.yaml; assert the translated `McpServerConfig` has the right `command` / `args` / `env`, and a V24 row with `item_type='mcp'` + a non-null `mcp_server_id`. (Drive the translation + row write directly; the actual `mcp_manager.add_server` can be exercised via a thin seam or asserted at the config-translation boundary.)
- `mcp_env_config_substitution` — `env` value `"{{config.token}}"` + `user_config {token: "abc"}` → resolves to `"abc"`; a missing key stays literal `{{config.token}}`.
- `validate_common_accepts_skill_and_mcp` — `validate_common` passes a `type: skill` spec (rejected by the automation `.validate()`); rejects a skill with empty `system_prompt`.
- `dispatcher_routes_by_type` — `install_marketplace_item` routes `skill` / `mcp` / `automation` / `extension` correctly (extension → Err).
- `validating_caps_recognises_installed_mcp` — seed a `marketplace_standalone_installs` row `item_type='mcp', slug='postgres-mcp'`; assert an automation requiring `postgres-mcp` produces no missing-capability warning.
- `uninstall_standalone_skill_and_mcp` — install then uninstall each; assert files/rows gone, automations' user-written skills untouched.
- `v24_creates_marketplace_standalone_installs` + `v24_is_idempotent` — migration tests, same shape as 3b-α's V22 tests.

### 6.2 Vitest

- `StoreDetail.test.tsx` extension — renders the `skill` layout (system_prompt + 安装技能, no 依赖 tab) and the `mcp` layout (mcp_server panel + 安装 MCP) from mocked detail data.
- `InstallWizard.test.tsx` extension — a `skill` item skips the `scope` step; an `automation` item still shows it.
- `AppsTab.test.tsx` extension — renders the standalone-installs section from a mocked `listStandaloneInstalls`; 卸载 calls `uninstallMarketplaceItem`.

## 7. Migration impact

One new migration: **V24** `marketplace_standalone_installs`. The PR updates the CLAUDE.md *Active migration registry* table with the V24 row.

## 8. Compatibility

- `install_marketplace_human` → `install_marketplace_item` rename: the only caller is the frontend bridge. The plan either keeps `install_marketplace_human` as the Tauri command name (just widening its behaviour) or renames both sides in one task — whichever is the smaller diff. No external consumers.
- `uninstall_marketplace_human` → `uninstall_marketplace_item`: same — single frontend caller.
- The automation install/uninstall behaviour is **unchanged** — `install_automation` is the current `install_human` body lifted verbatim; the existing 3b-α/β tests must still pass untouched.
- No change to `capability_map.rs`, the runtime, the protocol parser's automation rules, or the Kaleidoscope surface wiring.

## 9. Phase 3b roadmap (context, not in scope here)

| Sub-slice | Status |
|---|---|
| 3b-α | merged (PR #160) — bundled skill install + capability map + AppsTab |
| 3b-β | merged (PR #168) — skill bundle updates + UpgradeModal |
| **3b-γ (THIS PR)** | standalone skill / MCP marketplace entries — install dispatcher + non-bundled paths |
| 3b-δ | multi-registry + `capability_map` → DB table |
| 3b-ε | proxy adapters (Smithery / official MCP Registry / SkillHub) |
| 3b-ζ | local hello-halo workspace as a registry source |

## 10. Done criteria

- [ ] A fixture `type: skill` package installs → `~/.uclaw/skills/_marketplace/_standalone/<slug>/SKILL.md` exists, SkillsRegistry discovers it, a `marketplace_standalone_installs` row is written.
- [ ] A fixture `type: mcp` package installs → an MCP server is registered with the manager, the V24 row links it via `mcp_server_id`.
- [ ] An automation requiring an installed standalone MCP produces no missing-capability warning.
- [ ] The store 技能 / MCP tabs show fixture cards; `StoreDetail` renders a type-appropriate layout + install button.
- [ ] `InstallWizard` skips the `scope` step for skill/mcp items.
- [ ] `AppsTab` lists standalone installs with working uninstall.
- [ ] `type: extension` install fails with a clear error.
- [ ] All existing 3b-α/β automation tests still pass; new Rust + Vitest tests pass.
- [ ] V24 migration is idempotent; CLAUDE.md migration registry updated.
