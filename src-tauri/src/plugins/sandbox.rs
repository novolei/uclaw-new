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
/// stdio, read+write of the plugin dir + temp. Conditionally allows broad
/// network / FS read / FS write per declared permissions.
pub fn build_seatbelt_profile(policy: &PluginSandboxPolicy) -> String {
    let dir = policy.plugin_dir.display();
    let mut p = String::new();
    p.push_str("(version 1)\n(deny default)\n");
    p.push_str("(allow process-fork)\n(allow process-exec*)\n");
    p.push_str("(allow sysctl-read)\n");
    p.push_str("(allow mach-lookup)\n");
    p.push_str("(allow file-read-metadata)\n");
    p.push_str("(allow file-read* (subpath \"/usr/lib\") (subpath \"/usr/bin\") (subpath \"/System\") (subpath \"/Library/Frameworks\") (subpath \"/usr/local\") (subpath \"/opt/homebrew\") (subpath \"/private/var/select\") (subpath \"/etc\") (literal \"/dev/null\") (literal \"/dev/random\") (literal \"/dev/urandom\"))\n");
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
    // Trust boundary: `extra_env` is the plugin's manifest-declared `env`
    // (static, user-reviewed at install) — trusted by definition, so it merges
    // over the allowlist. It can only set literal values the manifest already
    // contains; it cannot read host secrets. (Today plugin config.env is empty
    // — no manifest env field yet — so this is currently a no-op.)
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
    fn profile_deny_default_and_plugin_dir() {
        let p = build_seatbelt_profile(&pol(false, false, false));
        assert!(p.contains("(deny default)"));
        assert!(p.contains("(subpath \"/tmp/plug\")"));
        assert!(!p.contains("(allow network*)"));
        assert!(!p.contains("(allow file-read* (subpath \"/\"))"));
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
