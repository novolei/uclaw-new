// SPDX-License-Identifier: Apache-2.0
//! Persisted first-run onboarding state for the local model, stored as a small
//! JSON file under the data dir (no DB migration). Tri-state per the spec:
//! pending → (completed | deferred | skipped). `deferred` is re-promptable;
//! `skipped` is permanent ("不再提示").

use std::path::{Path, PathBuf};

const FILENAME: &str = "local_model_onboarding.json";

/// Valid onboarding states. Unknown strings are rejected at the command layer.
pub const VALID_STATES: &[&str] = &["pending", "completed", "deferred", "skipped"];

pub fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILENAME)
}

/// Read the stored state; defaults to "pending" if the file is absent/garbage.
pub fn read_state(data_dir: &Path) -> String {
    let path = state_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
            v.get("minicpm")
                .and_then(|x| x.as_str())
                .filter(|s| VALID_STATES.contains(s))
                .unwrap_or("pending")
                .to_string()
        }
        Err(_) => "pending".to_string(),
    }
}

/// Persist the state. Rejects unknown values.
pub fn write_state(data_dir: &Path, state: &str) -> Result<(), String> {
    if !VALID_STATES.contains(&state) {
        return Err(format!("invalid onboarding state: {state}"));
    }
    std::fs::create_dir_all(data_dir).map_err(|e| format!("mkdir: {e}"))?;
    let body = serde_json::json!({ "minicpm": state });
    std::fs::write(state_path(data_dir), body.to_string()).map_err(|e| format!("write: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_pending_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read_state(tmp.path()), "pending");
    }
    #[test]
    fn write_then_read_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        write_state(tmp.path(), "completed").unwrap();
        assert_eq!(read_state(tmp.path()), "completed");
        write_state(tmp.path(), "deferred").unwrap();
        assert_eq!(read_state(tmp.path()), "deferred");
    }
    #[test]
    fn rejects_invalid_state() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(write_state(tmp.path(), "bogus").is_err());
    }
    #[test]
    fn garbage_file_reads_as_pending() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(state_path(tmp.path()), b"not json").unwrap();
        assert_eq!(read_state(tmp.path()), "pending");
    }
}
