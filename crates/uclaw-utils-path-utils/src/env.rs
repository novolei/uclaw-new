// SPDX-License-Identifier: Apache-2.0
// Derived from codex-rs/utils/path-utils/src/env.rs (https://github.com/openai/codex).
// Copyright (c) OpenAI. Licensed under Apache License 2.0.
// See NOTICE in the repository root.

//! Functions for environment detection that need to be shared across crates.

/// Returns true if the current process is running under Windows Subsystem for Linux.
pub fn is_wsl() -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WSL_DISTRO_NAME").is_some() {
            return true;
        }
        match std::fs::read_to_string("/proc/version") {
            Ok(version) => version.to_lowercase().contains("microsoft"),
            Err(_) => false,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
