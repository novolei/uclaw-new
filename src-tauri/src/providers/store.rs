//! Provider configuration persistence layer.
//!
//! Reads and writes provider configurations to `providers.json`
//! in the uclaw data directory. Uses atomic writes (temp file + rename)
//! to prevent file corruption.

use std::path::{Path, PathBuf};

use super::types::ProviderConfigs;

/// Load provider configs from disk.
///
/// Returns default empty configs if the file doesn't exist.
pub fn load_provider_configs(path: &Path) -> Result<ProviderConfigs, ConfigStoreError> {
    if !path.exists() {
        return Ok(ProviderConfigs::new());
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConfigStoreError::Io(e, format!("read: {}", path.display())))?;
    let configs: ProviderConfigs = serde_json::from_str(&content)
        .map_err(|e| ConfigStoreError::Deserialize(format!("parse: {e}")))?;
    Ok(configs)
}

/// Save provider configs to disk atomically.
///
/// Writes to a temp file first, then renames to the target path.
/// Sets file permissions to 0600 for security (API keys).
pub fn save_provider_configs(
    configs: &ProviderConfigs,
    path: &Path,
) -> Result<(), ConfigStoreError> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ConfigStoreError::Io(e, format!("mkdir: {}", parent.display())))?;
    }

    let content = serde_json::to_string_pretty(configs)
        .map_err(|e| ConfigStoreError::Serialize(format!("serialize: {e}")))?;

    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, content)
        .map_err(|e| ConfigStoreError::Io(e, format!("write temp: {}", temp_path.display())))?;

    // Set restrictive permissions on unix (0600 — owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&temp_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(&temp_path, perms);
        }
    }

    std::fs::rename(&temp_path, path)
        .map_err(|e| ConfigStoreError::Io(e, format!("rename: {}", path.display())))?;

    Ok(())
}

/// Delete the providers config file.
pub fn delete_provider_configs(path: &Path) -> Result<(), ConfigStoreError> {
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| ConfigStoreError::Io(e, format!("delete: {}", path.display())))?;
    }
    Ok(())
}

/// Get the default path for providers config.
#[must_use]
pub fn default_providers_path(data_dir: &Path) -> PathBuf {
    data_dir.join("providers.json")
}

// ── Error type ──────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ConfigStoreError {
    #[error("I/O error at {1}: {0}")]
    Io(#[source] std::io::Error, String),
    #[error("serialize error: {0}")]
    Serialize(String),
    #[error("deserialize error: {0}")]
    Deserialize(String),
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::types::{ModelSelection, ProviderConfig};

    fn temp_file() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_providers.json");
        (dir, path)
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let (_dir, path) = temp_file();
        let configs = load_provider_configs(&path).unwrap();
        assert!(configs.providers.is_empty());
        assert!(configs.active_model.is_none());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let (_dir, path) = temp_file();
        let mut configs = ProviderConfigs::new();
        configs.upsert_provider(ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: Some("sk-test".into()),
            base_url: Some("https://api.openai.com/v1".into()),
            api: None,
        });
        configs.active_model = Some(ModelSelection {
            provider_id: "openai".into(),
            model_id: "gpt-4o".into(),
        });

        save_provider_configs(&configs, &path).unwrap();
        let loaded = load_provider_configs(&path).unwrap();

        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].provider_id, "openai");
        assert_eq!(loaded.providers[0].api_key.as_deref(), Some("sk-test"));
        assert!(loaded.active_model.is_some());
        assert_eq!(loaded.active_model.as_ref().unwrap().model_id, "gpt-4o");
    }

    #[test]
    fn test_save_creates_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subdir").join("providers.json");
        let configs = ProviderConfigs::new();
        save_provider_configs(&configs, &path).unwrap();
        assert!(path.exists());
    }
}
