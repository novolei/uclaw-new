# Plugin Subprocess Sandbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Sandbox plugin MCP-server subprocesses (only plugins; builtins unchanged). Cross-platform floor (env-scrub + cwd-jail + rlimits) + macOS `sandbox-exec` permission-driven seatbelt (fail-closed).

**Spec:** `docs/superpowers/specs/2026-06-04-plugin-subprocess-sandbox-design.md`

---

## Pinned facts (verbatim — do not re-derive)

- **CRITICAL env behavior:** `tokio::process::Command` (like std) **INHERITS the parent env by default**; `.envs(map)` ADDS on top (does NOT clear). So `.envs(HashMap::new())` for plugins = inherit FULL parent env (the leak). The floor must `cmd.env_clear()` THEN re-add an allowlist (PATH/HOME/etc. read from the parent) or Node won't find its runtime.
- **`McpServerConfig`** (mcp/mod.rs:390): `#[derive(Debug, Clone, Serialize, Deserialize)] #[serde(rename_all="camelCase")]` — **NO `Default` derive**. Fields: id, name, description, transport_type, command, args, env, url, enabled, auto_approve, tool_allowlist. **3 construction sites** need an explicit new-field value: `builtin_playwright_mcp_config` (mcp/mod.rs:421 → `sandbox: None`), test `cfg()` (mcp/mod.rs:2122 → `sandbox: None`), `plugins/registration.rs:127` (→ `sandbox: Some(...)`).
- **`StdioTransport::spawn`** (mcp/mod.rs:483) takes `(name, command: &str, args: &[String], env: &HashMap, working_dir: Option<&Path>, server_id, notification_tx)` — NOT the config. Add a param `sandbox_policy: Option<&PluginSandboxPolicy>`. Spawn body builds `cmd = tokio::process::Command::new(command)` then `.current_dir(working_dir?)`, `.args(args).envs(env).stdin/out/err(piped).kill_on_drop(true)` (mcp/mod.rs:499-507).
- **Caller** `connect_server_shared` (mcp/mod.rs:1930-1943) has `config` (cloned, no lock held) and calls `StdioTransport::spawn(&config.name, &config.command, &config.args, &config.env, runtime_working_dir.as_deref(), id, notification_tx.clone())` — add `config.sandbox.as_ref()` as the new last-ish arg.
- **pre_exec pattern** (shell.rs:456): `#[cfg(unix)] { use std::os::unix::process::CommandExt; unsafe { cmd.pre_exec(|| { if libc::setsid() == -1 { return Err(std::io::Error::last_os_error()); } Ok(()) }); } }`. Mirror for setrlimit. `libc` is in `[target.'cfg(unix)'.dependencies]` (Cargo.toml:209), has `RLIMIT_AS`/`RLIMIT_NOFILE`/`RLIMIT_NPROC`/`setrlimit`/`rlimit{rlim_cur,rlim_max}`.
- **macOS sandbox-exec**: `/usr/bin/sandbox-exec` is a builtin; `-p '<inline profile>'` then the command + args. To sandbox, rewrite `(command, args)` → `("/usr/bin/sandbox-exec", ["-p", profile, original_command, ...original_args])` BEFORE `Command::new`.
- **registration.rs build site** (plugins/registration.rs:127): in scope `loaded.plugin_dir: PathBuf`, `loaded.manifest.permissions.{network, filesystem_read, filesystem_write}`.
- **`plugins/mod.rs`** (line ~12-17): add `pub mod sandbox;`.
- Example plugin `examples/plugins/hello-uclaw/server.mjs` is a Node stdio server (reads process.stdin; needs PATH to find node + HOME). Use it for the macOS soak.
- **NEW file `plugins/sandbox.rs` needs explicit `git add`.**

---

## Task 1: `plugins/sandbox.rs` — policy + pure profile/env builders

**Files:** Create `plugins/sandbox.rs`; modify `plugins/mod.rs`

- [ ] **Step 1: Create `plugins/sandbox.rs`**
```rust
//! Pi-3b — plugin subprocess sandbox. Cross-platform floor (env allowlist +
//! cwd-jail + rlimits) + macOS sandbox-exec seatbelt (permission-driven,
//! fail-closed). Applies to PLUGIN MCP servers only.

use std::collections::HashMap;
use std::path::PathBuf;

/// Per-plugin sandbox descriptor, built from the manifest permissions + dir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSandboxPolicy {
    pub plugin_dir: PathBuf,
    pub allow_network: bool,
    pub allow_fs_read: bool,
    pub allow_fs_write: bool,
}

/// Env var names kept when sandboxing a plugin subprocess. Everything else
/// (secrets like GITHUB_TOKEN/API keys, the rest of the user env) is dropped.
pub const ENV_ALLOWLIST: &[&str] = &[
    "PATH", "HOME", "LANG", "LC_ALL", "LC_CTYPE", "TZ", "USER", "LOGNAME", "SHELL", "TMPDIR",
];

/// Pure: from a parent-env map, keep only allowlisted keys. Testable.
pub fn allowlisted_env(parent: &HashMap<String, String>) -> HashMap<String, String> {
    ENV_ALLOWLIST
        .iter()
        .filter_map(|k| parent.get(*k).map(|v| ((*k).to_string(), v.clone())))
        .collect()
}

/// Pure: build a deny-by-default macOS Seatbelt profile from the policy.
/// Allows: process exec/fork, system dylib reads, sysctl/mach essentials,
/// stdio, read+write of the plugin dir + TMPDIR. Conditionally allows broad
/// network / FS read / FS write per declared permissions.
pub fn build_seatbelt_profile(policy: &PluginSandboxPolicy) -> String {
    let dir = policy.plugin_dir.display();
    let mut p = String::new();
    p.push_str("(version 1)\n(deny default)\n");
    p.push_str("(allow process-fork)\n(allow process-exec*)\n");
    p.push_str("(allow sysctl-read)\n");
    p.push_str("(allow mach-lookup)\n");
    p.push_str("(allow file-read-metadata)\n");
    // system libs / runtimes needed to boot node/scripts
    p.push_str("(allow file-read* (subpath \"/usr/lib\") (subpath \"/usr/bin\") (subpath \"/System\") (subpath \"/Library/Frameworks\") (subpath \"/usr/local\") (subpath \"/opt/homebrew\") (subpath \"/private/var/select\") (subpath \"/etc\") (literal \"/dev/null\") (literal \"/dev/random\") (literal \"/dev/urandom\"))\n");
    // plugin dir + temp: read + write
    p.push_str(&format!("(allow file-read* file-write* (subpath \"{dir}\"))\n"));
    p.push_str("(allow file-read* file-write* (subpath \"/private/tmp\") (subpath \"/private/var/folders\"))\n");
    if policy.allow_network {
        p.push_str("(allow network*)\n");
    }
    if policy.allow_fs_read {
        p.push_str("(allow file-read* (subpath \"/\"))\n");
    }
    if policy.allow_fs_write {
        p.push_str("(allow file-write* (subpath \"/\"))\n");
    }
    p
}
```

- [ ] **Step 2: Add `pub mod sandbox;` to `plugins/mod.rs`**

- [ ] **Step 3: Tests (pure, cross-platform)**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn pol(net: bool, fr: bool, fw: bool) -> PluginSandboxPolicy {
        PluginSandboxPolicy { plugin_dir: PathBuf::from("/tmp/plug"), allow_network: net, allow_fs_read: fr, allow_fs_write: fw }
    }
    #[test]
    fn profile_deny_default_and_plugin_dir() {
        let p = build_seatbelt_profile(&pol(false, false, false));
        assert!(p.contains("(deny default)"));
        assert!(p.contains("(subpath \"/tmp/plug\")"));
        assert!(!p.contains("(allow network*)"));
        assert!(!p.contains("(subpath \"/\")")); // no broad FS
    }
    #[test]
    fn profile_conditional_perms() {
        let p = build_seatbelt_profile(&pol(true, true, true));
        assert!(p.contains("(allow network*)"));
        assert!(p.contains("(allow file-read* (subpath \"/\"))"));
        assert!(p.contains("(allow file-write* (subpath \"/\"))"));
    }
    #[test]
    fn env_allowlist_drops_secrets() {
        let mut parent = HashMap::new();
        parent.insert("PATH".into(), "/usr/bin".into());
        parent.insert("HOME".into(), "/Users/x".into());
        parent.insert("GITHUB_TOKEN".into(), "secret".into());
        let out = allowlisted_env(&parent);
        assert_eq!(out.get("PATH").map(String::as_str), Some("/usr/bin"));
        assert_eq!(out.get("HOME").map(String::as_str), Some("/Users/x"));
        assert!(!out.contains_key("GITHUB_TOKEN"));
    }
}
```

- [ ] **Step 4: Build + commit**
`cd src-tauri && cargo test --lib plugins::sandbox 2>&1 | tail` → green. `cargo build 2>&1 | grep -E "^error"` → empty.
```bash
git add src-tauri/src/plugins/sandbox.rs src-tauri/src/plugins/mod.rs
git commit -m "feat(plugins): PluginSandboxPolicy + seatbelt profile + env allowlist (pure builders) for subprocess sandbox"
```
Verify `git show HEAD --stat` lists `plugins/sandbox.rs` (new).

---

## Task 2: `McpServerConfig.sandbox` field + registration builds the policy

**Files:** Modify `mcp/mod.rs` (field + 2 builtin/test sites), `plugins/registration.rs`

- [ ] **Step 1: Add the field**
In `McpServerConfig` (mcp/mod.rs:390), after `tool_allowlist`:
```rust
/// Pi-3b — sandbox policy for PLUGIN subprocesses (None = built-in, no
/// sandbox). Rebuilt from the manifest at boot, so not persisted.
#[serde(skip, default)]
pub sandbox: Option<crate::plugins::sandbox::PluginSandboxPolicy>,
```
(`#[serde(skip)]` → on deserialize the field is `Default::default()` = `None`; `Option` has Default so no struct-level Default needed.)

- [ ] **Step 2: Add `sandbox: None` at the builtin + test construction sites**
`builtin_playwright_mcp_config` (mcp/mod.rs:421) + the test `cfg()` helper (mcp/mod.rs:2122): add `sandbox: None,`. Grep `McpServerConfig {` across src-tauri for any OTHER literal (config-load deserialization uses serde so it's covered by skip/default; only explicit literals need the field) — add `sandbox: None` to each non-plugin one. Report the full list found.

- [ ] **Step 3: registration.rs builds `Some(policy)`**
In `plugins/registration.rs:127` `McpServerConfig { ... }`, add:
```rust
sandbox: Some(crate::plugins::sandbox::PluginSandboxPolicy {
    plugin_dir: loaded.plugin_dir.clone(),
    allow_network: loaded.manifest.permissions.network,
    allow_fs_read: loaded.manifest.permissions.filesystem_read,
    allow_fs_write: loaded.manifest.permissions.filesystem_write,
}),
```
(Built on ALL platforms; the spawn applies it platform-appropriately. Don't gate on cfg here.)

- [ ] **Step 4: Test**
In registration.rs tests, extend/add: a plugin with `permissions.network=true, filesystem_write=false` → `summary.mcp_configs[0].sandbox` is `Some` with `allow_network==true, allow_fs_write==false, plugin_dir==loaded.plugin_dir`. (Reuse the `fixture_plugin` builder; check if it sets permissions — if not, set them on the fixture's manifest.)

- [ ] **Step 5: Build + commit**
`cargo build 2>&1 | grep -E "^error"` → empty (missing-field errors flag any construction site you missed). `cargo test --lib mcp plugins::registration 2>&1 | tail` → green.
```bash
git add src-tauri/src/mcp/mod.rs src-tauri/src/plugins/registration.rs
git commit -m "feat(plugins): McpServerConfig.sandbox field + registration builds PluginSandboxPolicy from manifest permissions"
```

---

## Task 3: spawn integration — floor + macOS sandbox-exec (fail-closed)

**Files:** Modify `mcp/mod.rs` (spawn signature + body + call site), `plugins/sandbox.rs` (the Command-mutating helpers)

- [ ] **Step 1: Add Command-mutating helpers to `plugins/sandbox.rs`**
```rust
/// Apply the cross-platform floor to a Command: env_clear + allowlist (from the
/// real parent env) merged with `extra_env`, mandatory cwd-jail to plugin_dir,
/// and (Unix) resource limits via pre_exec.
pub fn apply_floor(cmd: &mut tokio::process::Command, policy: &PluginSandboxPolicy, extra_env: &HashMap<String, String>) {
    let parent: HashMap<String, String> = std::env::vars().collect();
    let mut env = allowlisted_env(&parent);
    for (k, v) in extra_env { env.insert(k.clone(), v.clone()); }
    cmd.env_clear();
    cmd.envs(&env);
    cmd.current_dir(&policy.plugin_dir);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                // Best-effort rlimits; a failure must NOT abort the spawn, so
                // swallow errors and return Ok. Async-signal-safe (no alloc/log).
                set_rlimit(libc::RLIMIT_AS, 512 * 1024 * 1024);
                set_rlimit(libc::RLIMIT_NOFILE, 256);
                set_rlimit(libc::RLIMIT_NPROC, 64);
                Ok(())
            });
        }
    }
}

#[cfg(unix)]
fn set_rlimit(resource: libc::c_int, limit: u64) {
    let rl = libc::rlimit { rlim_cur: limit as libc::rlim_t, rlim_max: limit as libc::rlim_t };
    unsafe { libc::setrlimit(resource, &rl); } // ignore result (best-effort)
}

/// macOS: rewrite (command, args) to run under sandbox-exec with the policy's
/// profile. Returns Err if sandbox-exec is unavailable (caller fail-closes).
#[cfg(target_os = "macos")]
pub fn sandbox_exec_wrap(command: &str, args: &[String], policy: &PluginSandboxPolicy) -> Result<(String, Vec<String>), String> {
    const SBX: &str = "/usr/bin/sandbox-exec";
    if !std::path::Path::new(SBX).exists() {
        return Err("sandbox-exec not found".to_string());
    }
    let profile = build_seatbelt_profile(policy);
    let mut new_args = vec!["-p".to_string(), profile, command.to_string()];
    new_args.extend(args.iter().cloned());
    Ok((SBX.to_string(), new_args))
}
```
(`RLIMIT_NPROC` exists on macOS+Linux in libc. If clippy/portability flags `RLIMIT_NPROC` on some target, `#[cfg]`-guard it; report.)

- [ ] **Step 2: Thread `sandbox_policy` into `spawn`**
Add param `sandbox_policy: Option<&crate::plugins::sandbox::PluginSandboxPolicy>` to `StdioTransport::spawn`. In the body, BEFORE `Command::new(command)`:
```rust
// Pi-3b — plugin sandbox. macOS: wrap in sandbox-exec (fail-closed). All: floor.
let (command, args_vec): (String, Vec<String>);
let (eff_command, eff_args): (&str, &[String]) = if let Some(policy) = sandbox_policy {
    #[cfg(target_os = "macos")]
    {
        match crate::plugins::sandbox::sandbox_exec_wrap(command, args, policy) {
            Ok((c, a)) => { command = c; args_vec = a; (&command, &args_vec) }
            Err(e) => {
                return Err(McpError::Transport(format!("plugin sandbox unavailable (fail-closed): {e}")));
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    { (command, args) }
} else {
    (command, args)
};
let mut cmd = tokio::process::Command::new(eff_command);
// ... current_dir(working_dir) ... but note: floor sets cwd to plugin_dir, so
// for sandboxed servers prefer the policy's plugin_dir. Apply floor AFTER the
// base env/args/stdio setup, BEFORE spawn:
cmd.args(eff_args).envs(env).stdin(...).stdout(...).stderr(...).kill_on_drop(true);
if let Some(policy) = sandbox_policy {
    crate::plugins::sandbox::apply_floor(&mut cmd, policy, env); // env_clear+allowlist+cwd+rlimits (overrides the .envs(env)/current_dir above)
}
```
ADAPT to the real spawn body (the recon shows the exact `.args().envs()...` chain). Order matters: `apply_floor` calls `env_clear()` so it must run AFTER any base `.envs(env)` (it re-applies allowlist+extra). The existing `working_dir` `current_dir` for builtins stays for the `None` path; for `Some`, `apply_floor` sets cwd to plugin_dir. Keep the non-sandbox (`None`) path byte-identical to today. The variable shadowing of `command`/`args` for the macOS wrap is the fiddly part — implement cleanly (maybe compute `(eff_command, eff_args)` as owned Strings/Vec up front to avoid lifetime knots). Report the final shape.

- [ ] **Step 3: Pass policy at the call site**
`connect_server_shared` (mcp/mod.rs:1943): add `config.sandbox.as_ref()` to the `StdioTransport::spawn(...)` args (matching the new param position). grep for ANY other `StdioTransport::spawn(` call site (tests?) + pass `None`.

- [ ] **Step 4: Build (all the cfg-gating + unsafe is the compile risk)**
`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty. `cargo clippy --lib 2>&1 | grep -iE "sandbox|mcp/mod" | grep -iE "warning|error"` → no new (esp. the unsafe pre_exec + cfg).
`cargo test --lib mcp plugins 2>&1 | tail` → green.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/mcp/mod.rs src-tauri/src/plugins/sandbox.rs
git commit -m "feat(plugins): apply sandbox floor + macOS sandbox-exec to plugin MCP spawn (fail-closed); builtins unchanged"
```

---

## Task 4: Whole-slice verification + ship

- [ ] **Step 1**: `cargo build` (+ if possible a macOS build) + `cargo clippy --lib` clean.
- [ ] **Step 2**: tests — `plugins::sandbox`, `plugins::registration`, `mcp`, + broad dependent run. Green.
- [ ] **Step 3**: grep gates — `sandbox: None` at ALL builtin/test McpServerConfig literals (no builtin gets `Some`); `sandbox: Some` only in registration.rs; `apply_floor`/`sandbox_exec_wrap` only on the plugin spawn path; the `None` spawn path unchanged.
- [ ] **Step 4**: **macOS soak (manual, document in PR)**: run the example plugin (`examples/plugins/hello-uclaw`) — confirm its Node MCP server BOOTS under the seatbelt profile (the profile must allow node + stdio) and its `hello` tool works; confirm a profile that's too tight is caught (the verification proves the default profile is functional, not just deny-everything). If running the app isn't feasible in this env, at minimum craft a shell test: `/usr/bin/sandbox-exec -p "<profile>" node examples/plugins/hello-uclaw/server.mjs` with a probe stdin line, and confirm it responds (not killed by sandbox). Report the result.
- [ ] **Step 5**: `npx gitnexus analyze`.
- [ ] **Step 6**: PR with `## Commits (bisectable)` table. Note: plugins-only (builtins `None`), macOS fail-closed, cross-platform floor, permission-driven profile, sandbox-exec deprecation noted, seccomp/landlock = v2. **Verify `git show <commit> --stat` includes `plugins/sandbox.rs`.**
- [ ] **Step 7**: rebase onto latest origin/main, rebase-merge, sync main, cleanup worktree+branch, reindex, update memory ([[project-pi-lightweight-vs-agent-os]]: sandbox shipped; next 3b = install+UI / commands-dispatch / Linux seccomp v2).

---

## Self-Review

**Spec coverage:** §1 policy → T1; §2 field → T2; §3 floor → T1(builders)+T3(apply); §4 sandbox-exec → T1(wrap)+T3; §5 spawn → T3; §6 registration → T2. ✓
**Placeholder scan:** the spawn-body shaping (T3 Step 2 "adapt to real chain + report final shape") + the `McpServerConfig {` literal grep (T2 Step 2) + RLIMIT_NPROC cfg-guard are flagged-with-fallback engineering details, not TODOs. ✓
**Type consistency:** `PluginSandboxPolicy{plugin_dir,allow_network,allow_fs_read,allow_fs_write}` used in T1/T2/T3; `McpServerConfig.sandbox: Option<PluginSandboxPolicy>` `#[serde(skip,default)]`; spawn param `Option<&PluginSandboxPolicy>`; same builders. ✓
**Builtins-unchanged invariant:** `sandbox: None` on every builtin/test config + the `None` spawn path identical to today → grep-verified in T4. ✓
**Security correctness:** env_clear+allowlist (drops secrets), cwd-jail, rlimits (Unix), macOS seatbelt deny-default permission-driven, fail-closed on macOS. Threat model honest (floor-only on Linux/Windows). ✓
**New-file safety:** T1 + T4 verify `plugins/sandbox.rs` in `git show --stat`. ✓
