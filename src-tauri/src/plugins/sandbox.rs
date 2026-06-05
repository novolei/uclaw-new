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

/// Pure: crown-jewels read-denials that OVERRIDE the broad `(allow file-read*)`.
///
/// macOS seatbelt is last-matching-rule-wins, so these MUST be appended AFTER
/// `(allow file-read*)`. They deny reads of clear secret stores (no plugin
/// legitimately needs them) + uClaw's own data dir (which holds the plugin_env
/// secret store / API keys / llm config), then RE-ALLOW the plugin's own dir
/// (which lives under data_dir) so the plugin can still read its own files.
///
/// Chosen to NOT break npx/uvx runtimes: `~/.npmrc`/`~/.pypirc` are left readable
/// (registry config) — their token risk is narrower than ssh/cloud creds.
/// Fail-open per-rule: an empty `home`/`data_dir` simply omits that rule (the
/// profile stays syntactically valid).
fn crown_jewel_read_denials(home: &str, data_dir: Option<&str>, plugin_dir: &str) -> String {
    let mut p = String::new();
    if !home.is_empty() {
        p.push_str(&format!(
            "(deny file-read* (subpath \"{home}/.ssh\") (subpath \"{home}/.aws\") (subpath \"{home}/.gnupg\") (subpath \"{home}/.config/gcloud\") (subpath \"{home}/.kube\") (subpath \"{home}/.docker\") (subpath \"{home}/.netrc\") (subpath \"{home}/Library/Keychains\") (subpath \"{home}/Library/Cookies\"))\n"
        ));
    }
    if let Some(dd) = data_dir {
        if !dd.is_empty() {
            // uClaw's own data dir — DB (plugin_env secrets), llm config, etc.
            p.push_str(&format!("(deny file-read* (subpath \"{dd}\"))\n"));
        }
    }
    // Re-allow the plugin's own dir (lives under data_dir) — last match wins.
    p.push_str(&format!("(allow file-read* (subpath \"{plugin_dir}\"))\n"));
    p
}

/// Pure: build a deny-by-default macOS Seatbelt profile from the policy.
///
/// Enforces **write-isolation + network-isolation** (v1) PLUS **read-isolation
/// via a crown-jewels denylist** (v2), validated against a real Node MCP server:
/// - WRITE is denied outside the plugin dir + temp (broad write only if
///   `allow_fs_write`).
/// - NETWORK is denied unless `allow_network`.
/// - READ is broad (`allow file-read*`) — runtimes (Node/Python/etc.) read many
///   install-specific paths (dyld cache, runtime libs) at startup; a narrow
///   read-set aborts them (SIGABRT). v2 keeps broad read but appends a
///   crown-jewels denylist (`crown_jewel_read_denials`) so a plugin can boot yet
///   cannot read SSH/cloud credentials or uClaw's own secret store. The
///   cross-platform floor additionally scrubs secrets from the ENV.
///   (`allow_fs_read` is retained on the policy; granular per-permission read
///   allowlists remain future work — the denylist is unconditional.)
/// `process-info*`/`signal (target self)`/`file-ioctl` are required for the
/// runtime to boot + speak stdio (omitting them aborts Node).
pub fn build_seatbelt_profile(policy: &PluginSandboxPolicy) -> String {
    let dir = policy.plugin_dir.display();
    let mut p = String::new();
    p.push_str("(version 1)\n(deny default)\n");
    p.push_str("(allow process-fork)\n(allow process-exec*)\n(allow process-info*)\n");
    p.push_str("(allow signal (target self))\n");
    p.push_str("(allow sysctl-read)\n(allow mach-lookup)\n(allow file-ioctl)\n(allow file-read-metadata)\n");
    // Broad read — required for runtime boot (see doc above). Write stays jailed.
    p.push_str("(allow file-read*)\n");
    // v2 read-isolation: deny crown jewels (override the broad read above).
    // data_dir = plugin_dir's grandparent (<data_dir>/plugins/<id>).
    let home = std::env::var("HOME").unwrap_or_default();
    let data_dir = policy
        .plugin_dir
        .parent()
        .and_then(|pp| pp.parent())
        .map(|dd| dd.display().to_string());
    p.push_str(&crown_jewel_read_denials(
        &home,
        data_dir.as_deref(),
        &dir.to_string(),
    ));
    p.push_str(&format!("(allow file-write* (subpath \"{dir}\"))\n"));
    p.push_str("(allow file-write* (subpath \"/private/tmp\") (subpath \"/private/var/folders\"))\n");
    p.push_str("(allow file-write-data (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\"))\n");
    if policy.allow_network {
        p.push_str("(allow network*)\n");
    }
    if policy.allow_fs_write {
        p.push_str("(allow file-write* (subpath \"/\"))\n");
    }
    p
}

/// Apply the cross-platform floor to a Command for a sandboxed plugin: clear
/// the inherited env and re-add only the allowlist (merged with `extra_env`),
/// jail cwd to the plugin dir, and (Unix) cap resources via pre_exec.
pub fn apply_floor(
    cmd: &mut tokio::process::Command,
    policy: &PluginSandboxPolicy,
    extra_env: &HashMap<String, String>,
) {
    let parent: HashMap<String, String> = std::env::vars().collect();
    let mut env = allowlisted_env(&parent);
    // Trust boundary: `extra_env` is user-set per-plugin env vars stored in the
    // V61 `plugin_env` DB table (API keys, tokens, etc.) injected into
    // `McpServerConfig.env` at boot by app.rs Phase 3. They merge over the
    // allowlist so they survive env_clear() and reach the MCP subprocess.
    // Values are set explicitly by the user in the plugin env editor — never
    // read from the host environment, so they cannot exfiltrate ambient secrets.
    for (k, v) in extra_env {
        env.insert(k.clone(), v.clone());
    }
    cmd.env_clear();
    cmd.envs(&env);
    cmd.current_dir(&policy.plugin_dir);
    #[cfg(unix)]
    {
        // tokio::process::Command exposes pre_exec as an inherent method;
        // CommandExt trait import is not required.
        unsafe {
            cmd.pre_exec(|| {
                // Best-effort rlimits; never abort the spawn (return Ok).
                // Async-signal-safe: only setrlimit, no alloc/log.
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
    let rl = libc::rlimit {
        rlim_cur: limit as libc::rlim_t,
        rlim_max: limit as libc::rlim_t,
    };
    unsafe {
        libc::setrlimit(resource, &rl);
    }
}

/// macOS: rewrite (command, args) to run under sandbox-exec with the policy's
/// seatbelt profile. Err if sandbox-exec is unavailable (caller fail-closes).
#[cfg(target_os = "macos")]
pub fn sandbox_exec_wrap(
    command: &str,
    args: &[String],
    policy: &PluginSandboxPolicy,
) -> Result<(String, Vec<String>), String> {
    const SBX: &str = "/usr/bin/sandbox-exec";
    if !std::path::Path::new(SBX).exists() {
        return Err("sandbox-exec not found".to_string());
    }
    let profile = build_seatbelt_profile(policy);
    let mut new_args = vec!["-p".to_string(), profile, command.to_string()];
    new_args.extend(args.iter().cloned());
    Ok((SBX.to_string(), new_args))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pol(net: bool, fr: bool, fw: bool) -> PluginSandboxPolicy {
        PluginSandboxPolicy { plugin_dir: PathBuf::from("/tmp/plug"), allow_network: net, allow_fs_read: fr, allow_fs_write: fw }
    }
    #[test]
    fn profile_deny_default_jails_write_and_gates_network() {
        let p = build_seatbelt_profile(&pol(false, false, false));
        assert!(p.contains("(deny default)"));
        assert!(p.contains("(allow file-read*)")); // broad read (runtime boot)
        assert!(p.contains("(allow file-write* (subpath \"/tmp/plug\"))")); // write jailed to plugin dir
        assert!(!p.contains("(allow network*)")); // network denied
        assert!(!p.contains("(allow file-write* (subpath \"/\"))")); // no broad write
    }
    #[test]
    fn profile_conditional_perms() {
        // network + fs_write granted → network + broad write present. (fs_read is
        // NOT enforced granularly — read is broad-minus-crown-jewels — so no
        // separate per-permission read rule.)
        let p = build_seatbelt_profile(&pol(true, true, true));
        assert!(p.contains("(allow network*)"));
        assert!(p.contains("(allow file-write* (subpath \"/\"))"));
    }

    #[test]
    fn crown_jewel_denials_deny_secrets_and_reallow_plugin_dir() {
        let out = crown_jewel_read_denials("/Users/x", Some("/Users/x/.uclaw"), "/Users/x/.uclaw/plugins/p");
        // secret stores denied
        assert!(out.contains("(deny file-read*"));
        assert!(out.contains("/Users/x/.ssh"));
        assert!(out.contains("/Users/x/.aws"));
        assert!(out.contains("/Users/x/Library/Keychains"));
        // uClaw's own data dir denied
        assert!(out.contains("(deny file-read* (subpath \"/Users/x/.uclaw\"))"));
        // the plugin's own dir re-allowed AFTER the data_dir deny (last-match-wins)
        let data_deny = out.find("(deny file-read* (subpath \"/Users/x/.uclaw\"))").unwrap();
        let plugin_allow = out.find("(allow file-read* (subpath \"/Users/x/.uclaw/plugins/p\"))").unwrap();
        assert!(plugin_allow > data_deny, "plugin-dir re-allow must come after the data_dir deny");
    }

    #[test]
    fn crown_jewel_denials_fail_open_on_empty() {
        // empty home → no HOME-based denials; empty/None data_dir → no data deny;
        // but the plugin-dir re-allow is always present + profile stays valid.
        let out = crown_jewel_read_denials("", None, "/tmp/p");
        assert!(!out.contains("/.ssh"));
        assert!(out.contains("(allow file-read* (subpath \"/tmp/p\"))"));
    }

    #[test]
    fn profile_v2_includes_crown_jewel_read_denials() {
        // The full profile keeps the v1 invariants AND adds the read denylist.
        let p = build_seatbelt_profile(&pol(false, false, false));
        assert!(p.contains("(allow file-read*)")); // broad read still present (runtime boot)
        assert!(p.contains("(deny file-read*")); // crown-jewels denylist present
        assert!(p.contains(".ssh")); // a known crown jewel (HOME is set in the test env)
        // the broad read allow comes BEFORE the deny (ordering: deny overrides)
        let broad = p.find("(allow file-read*)\n").unwrap();
        let deny = p.find("(deny file-read*").unwrap();
        assert!(deny > broad, "crown-jewel deny must come after the broad read allow");
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

    #[cfg(target_os = "macos")]
    #[test]
    fn sandbox_exec_wrap_structure() {
        // Skip if sandbox-exec is missing (CI without SIP); if present, verify shape.
        const SBX: &str = "/usr/bin/sandbox-exec";
        if !std::path::Path::new(SBX).exists() {
            return;
        }
        let policy = pol(false, false, false);
        let args = vec!["--foo".to_string(), "bar".to_string()];
        let (cmd, new_args) = super::sandbox_exec_wrap("node", &args, &policy).unwrap();
        assert_eq!(cmd, SBX);
        assert_eq!(new_args[0], "-p");
        // new_args[1] is the seatbelt profile (non-empty string)
        assert!(!new_args[1].is_empty());
        // new_args[2] is the original command
        assert_eq!(new_args[2], "node");
        // new_args[3..] are the original args
        assert_eq!(&new_args[3..], args.as_slice());
    }
}
