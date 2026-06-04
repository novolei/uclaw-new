# Plugin Subprocess Sandbox Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b plugin system. Third 3b slice (after lifecycle PR #667, skills PR #668). Sandboxes plugin subprocess (MCP server) execution — plugins currently run third-party code with ZERO isolation, the biggest live security gap of the now-functional plugin system.

## Problem

A plugin's MCP server is spawned from `manifest.runtime.executable` via `mcp/mod.rs::StdioTransport::spawn` (`tokio::process::Command::new(command)`) with **no isolation**: it inherits the FULL parent environment (PATH, HOME, secrets like GITHUB_TOKEN/API keys), the parent cwd, full filesystem read+write, full network, and the user's uid. `kill_on_drop(true)` is the only control. A malicious or compromised plugin = arbitrary code as the user (secret exfiltration, FS destruction, network exfil, persistence, lateral movement). The `PluginPermissions { network, filesystem_read, filesystem_write, run_subprocess, … }` vocabulary exists but only `run_subprocess` is enforced (a registration-time block); `network`/`filesystem_*` are declared-but-ignored. No OS sandbox primitive is used anywhere in the codebase.

## Decision (approved 2026-06-04)

**Hybrid: macOS `sandbox-exec` (permission-driven seatbelt) + a cross-platform floor; permission-driven policy; macOS fail-closed.** Applies to PLUGIN MCP servers only — built-in MCP servers / bash / browser are unchanged.

- **Cross-platform floor (always, all platforms):** `env_clear()` + a small env allowlist; mandatory cwd-jail to the plugin dir; Unix resource limits (`setrlimit` via `pre_exec`, using the already-present `libc` — no new crate).
- **macOS strong layer:** wrap the plugin command in `sandbox-exec -p <profile>`; the seatbelt profile is generated from the plugin's `PluginPermissions` (deny-by-default; network/external-FS allowed only when declared). **fail-closed**: if the sandbox wrap can't be applied (sandbox-exec missing / error), do NOT spawn the plugin.
- **Permission-driven**: the profile reflects declared perms (`network`, `filesystem_read`, `filesystem_write`).
- **Honest threat model**: macOS gets real OS-level isolation; Linux/Windows get the floor only (env-scrub + cwd-jail + rlimits) — which blocks secret-exfil-via-env, cwd-relative FS traversal, and resource DoS, but NOT absolute-path FS access or network. seccomp/landlock are a documented v2.

## Design

### §1 `PluginSandboxPolicy` (the per-plugin sandbox descriptor)
```rust
pub struct PluginSandboxPolicy {
    pub plugin_dir: PathBuf,      // cwd-jail + profile allow-path (absolute)
    pub allow_network: bool,      // = manifest.permissions.network
    pub allow_fs_read: bool,      // = manifest.permissions.filesystem_read
    pub allow_fs_write: bool,     // = manifest.permissions.filesystem_write
}
```
Home: a new `plugins/sandbox.rs` (or `mcp/sandbox.rs` — plan pins; keep it near the spawn it serves). Built by `plugins/registration.rs` from the manifest when constructing a plugin's `McpServerConfig`.

### §2 `McpServerConfig.sandbox: Option<PluginSandboxPolicy>`
Add the field to `McpServerConfig`. **`None` = no sandbox** (built-in MCP servers — unchanged behavior). **`Some(policy)` = plugin** — spawn applies the floor + (macOS) sandbox-exec. registration.rs sets `Some(...)` for plugin configs; all existing builtin config construction defaults `None` (the field is `#[serde(default)]` + `Default` so serialized configs round-trip).

### §3 Cross-platform floor — `configure_sandbox_floor(cmd, policy)`
A function that mutates a `tokio::process::Command` for the floor:
- `cmd.env_clear()` then re-add an allowlist: `PATH, HOME, LANG, LC_ALL, LC_CTYPE, TZ, USER, LOGNAME, SHELL, TMPDIR` (those present in the parent env). (Plugin-declared extra env, if any in the manifest, is added — but the current manifest has no env field, so allowlist-only for v1.)
- `cmd.current_dir(&policy.plugin_dir)` — mandatory cwd-jail.
- Unix only (`#[cfg(unix)]`): `unsafe { cmd.pre_exec(|| { libc::setrlimit(RLIMIT_AS, 512MB); libc::setrlimit(RLIMIT_NOFILE, 256); libc::setrlimit(RLIMIT_NPROC, 64); Ok(()) }) }` via `std::os::unix::process::CommandExt`. (`pre_exec` runs in the forked child before exec; keep the closure async-signal-safe — only `setrlimit` calls, no allocation.)

### §4 macOS sandbox-exec wrap — `sandbox_exec_wrap(command, args, policy) -> (String, Vec<String>)`
On macOS only: returns the rewritten `(command="sandbox-exec", args=["-p", profile, original_command, ...original_args])`. The profile (a pure `fn build_seatbelt_profile(policy) -> String`) is deny-by-default Seatbelt:
```
(version 1)
(deny default)
(allow process-fork) (allow process-exec*)        ; the plugin needs to exec itself
(allow file-read*  (subpath "/usr/lib") (subpath "/System") (subpath "/Library") (literal "/dev/null") (subpath "<TMPDIR>"))
(allow file-read* file-write* (subpath "<plugin_dir>") (subpath "<TMPDIR>"))
(allow file-read-metadata)
; conditional:
(allow network*)                ; iff allow_network
(allow file-read* (subpath "/")) ; iff allow_fs_read
(allow file-write* (subpath "/")) ; iff allow_fs_write
```
(The exact minimal allow-set — process, sysctl-read, mach-lookup for the dynamic linker, stdio — is pinned in the plan against a working profile; the profile must let a normal Node/script MCP server boot + speak stdio while denying the rest.) **fail-closed**: if `which sandbox-exec` fails or the wrap can't build, spawn returns an error (the plugin's MCP server doesn't start; logged).

### §5 Spawn integration (`mcp/mod.rs::spawn`)
`spawn` (or its plugin-config caller) consults the config's `sandbox: Option<PluginSandboxPolicy>`:
- `None` → today's behavior (builtins).
- `Some(policy)` → `configure_sandbox_floor(&mut cmd, &policy)`; on macOS, before building the Command, rewrite `(command, args)` via `sandbox_exec_wrap` (fail-closed). The `spawn` signature currently takes `command/args/env/working_dir` — the plan pins whether the policy threads via the `McpServerConfig` reaching spawn or as a new spawn param (the config is the natural carrier).

### §6 registration.rs — build the policy
When `plugins/registration.rs` builds a plugin's `McpServerConfig`, set `sandbox: Some(PluginSandboxPolicy { plugin_dir: loaded.plugin_dir.clone(), allow_network: manifest.permissions.network, allow_fs_read: manifest.permissions.filesystem_read, allow_fs_write: manifest.permissions.filesystem_write })`.

## Data flow (after this slice)

```
plugin load → registration builds McpServerConfig{ sandbox: Some(policy from permissions+plugin_dir) }
boot/connect → mcp spawn:
   if sandbox Some:
      floor: env_clear+allowlist, cwd=plugin_dir, rlimits (Unix pre_exec)
      macOS: command/args = sandbox-exec -p <profile(policy)> <plugin_exe> <args>   (fail-closed)
   else (builtin): unchanged
→ plugin subprocess runs jailed (macOS: OS-enforced; others: floor)
```

## Out of scope

Linux seccomp / landlock (v2 — floor only on Linux for now); Windows job objects (v2); a per-plugin permission-edit UI / consent prompt (separate install+UI slice); sandboxing built-in MCP servers / bash / browser (only plugin servers); WASM plugin runtime (`runtime.kind="wasm"` future); network-level egress filtering beyond on/off; a manifest `env` allowlist field (none exists; floor uses a fixed allowlist).

## Error handling

macOS fail-closed: `sandbox-exec` not found OR profile build error → spawn returns `McpError` (plugin server not started; warn-logged with the reason). Floor is best-effort-but-applied: `env_clear`+allowlist + `current_dir` always succeed; `pre_exec` setrlimit failures are logged inside the closure but don't abort (a failed rlimit shouldn't kill an otherwise-fine plugin — but `pre_exec` returning Err WOULD abort the spawn, so the closure swallows setrlimit errors and returns Ok). Non-macOS/non-Unix: floor applies what it can (env+cwd everywhere; rlimits Unix-only). `sandbox: None` path is byte-for-byte today's behavior.

## Testing

1. **Profile generation** (pure, cross-platform-buildable): `build_seatbelt_profile(policy)` — deny default present; plugin_dir allowed read+write; network rule present iff `allow_network`; broad FS rules present iff the fs flags; the profile string is well-formed.
2. **PluginSandboxPolicy from manifest**: registration builds a policy with the right flags from `PluginPermissions`.
3. **floor env allowlist**: a helper that computes the allowlist from a given parent-env map keeps only the allowed keys (test the pure filter; `env_clear`/`current_dir` on a Command are hard to assert, so extract the allowlist computation into a testable fn).
4. **McpServerConfig round-trip**: `sandbox: None` (builtin) serializes/deserializes; a config with `Some` policy too (or mark policy `#[serde(skip)]` if it shouldn't persist — plan decides, since plugin configs are rebuilt at boot from the manifest, `#[serde(skip, default)]` is cleanest so on-disk MCP config files don't carry it).
5. **fail-closed (macOS, if testable)**: a unit test of the wrap fn returning the sandbox-exec form; the "sandbox-exec missing → error" path is hard to unit-test (don't remove sandbox-exec) — assert the wrap builds the right argv + note the runtime fail-closed.
6. `cargo build` (incl. macOS) + clippy clean; `cargo test --lib mcp plugins` + broad dependent run; the `pre_exec` unsafe block compiles on Unix + is `#[cfg]`-gated.

## Scope / files

| File | Change |
|---|---|
| `plugins/sandbox.rs` (new) | `PluginSandboxPolicy`, `build_seatbelt_profile`, `sandbox_exec_wrap` (macOS), `configure_sandbox_floor`, env-allowlist helper |
| `mcp/mod.rs` | `McpServerConfig.sandbox: Option<PluginSandboxPolicy>` (`#[serde(skip, default)]`); `spawn` applies floor + (macOS) sandbox-exec wrap when `Some`, fail-closed |
| `plugins/registration.rs` | build `Some(PluginSandboxPolicy{...})` from manifest permissions + plugin_dir for plugin MCP configs |
| `plugins/mod.rs` | `pub mod sandbox;` |

## Risk

Med-high (security + macOS-specific + spawn-path). Main risks: (1) **seatbelt profile correctness** — too tight breaks legit plugins (a Node MCP server needs dyld, mach-lookup, sysctl-read, stdio; the profile must allow those), too loose defeats the purpose; the plan pins a tested minimal allow-set + the verification soaks the example plugin under sandbox. (2) **fail-closed UX** — a mis-built profile would silently stop all macOS plugins; mitigate with clear logging + a tested default profile + (if needed) a config escape hatch noted but not built. (3) **`pre_exec` safety** — the closure must be async-signal-safe (only `setrlimit`, no alloc/log inside); errors swallowed→Ok so a failed rlimit doesn't abort spawn. (4) **only plugin configs sandboxed** — the `sandbox: None` default must be preserved for ALL builtin MCP configs (a stray `Some` would sandbox a builtin); grep-verify. (5) sandbox-exec deprecation (still functional; documented). Bisectable: policy+profile → floor → spawn-wire+registration → verify. After this slice, a plugin's subprocess on macOS runs under an OS-enforced, permission-scoped sandbox (fail-closed), and on all platforms with a scrubbed env + jailed cwd + capped resources — closing the plugin system's worst security gap.
