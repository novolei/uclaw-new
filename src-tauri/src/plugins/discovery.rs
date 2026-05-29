//! Manifest discovery — scans `$DATA_DIR/plugins/<id>/plugin.toml` files.

use std::path::{Path, PathBuf};

use crate::plugin_manifest::schema::PluginManifest;

/// A successfully-loaded plugin manifest with its on-disk path.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    pub manifest_path: PathBuf,
}

/// Errors discovery may surface.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("plugins directory does not exist: {0}")]
    DirectoryMissing(PathBuf),
    #[error("failed to read plugin directory entry {path}: {source}")]
    DirRead { path: PathBuf, source: std::io::Error },
    #[error("failed to read manifest at {path}: {source}")]
    ManifestRead { path: PathBuf, source: std::io::Error },
    #[error("failed to parse manifest at {path}: {source}")]
    ManifestParse { path: PathBuf, source: toml::de::Error },
    #[error("manifest at {path} validation failed: {reason}")]
    ManifestInvalid { path: PathBuf, reason: String },
}

/// Discovery scans `plugins/<id>/plugin.toml` files under a given root and
/// returns parsed manifests. NO side effects (doesn't spawn subprocesses,
/// doesn't register anything) — pure scan + parse.
pub struct PluginDiscovery {
    plugins_root: PathBuf,
}

impl PluginDiscovery {
    /// Construct a discovery rooted at the given directory. Use
    /// `$DATA_DIR/plugins/` for the production wiring.
    pub fn new(plugins_root: impl AsRef<Path>) -> Self {
        Self {
            plugins_root: plugins_root.as_ref().to_path_buf(),
        }
    }

    /// Get the plugins directory this discovery scans.
    pub fn plugins_root(&self) -> &Path {
        &self.plugins_root
    }

    /// Scan the plugins root and parse each `<plugin_id>/plugin.toml`.
    ///
    /// Returns a vector of `Result<LoadedPlugin, DiscoveryError>` so the
    /// caller can decide how to handle per-plugin failures (typically
    /// log + skip, not abort the whole boot).
    pub fn discover(&self) -> Result<Vec<Result<LoadedPlugin, DiscoveryError>>, DiscoveryError> {
        if !self.plugins_root.exists() {
            // Empty plugins dir is fine — return empty list, not error.
            return Ok(Vec::new());
        }
        let mut results = Vec::new();
        let entries = std::fs::read_dir(&self.plugins_root).map_err(|e| DiscoveryError::DirRead {
            path: self.plugins_root.clone(),
            source: e,
        })?;
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    results.push(Err(DiscoveryError::DirRead {
                        path: self.plugins_root.clone(),
                        source: e,
                    }));
                    continue;
                }
            };
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let manifest_path = plugin_dir.join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }
            results.push(Self::load_manifest(&manifest_path, &plugin_dir));
        }
        Ok(results)
    }

    fn load_manifest(
        manifest_path: &Path,
        plugin_dir: &Path,
    ) -> Result<LoadedPlugin, DiscoveryError> {
        let body = std::fs::read_to_string(manifest_path).map_err(|e| DiscoveryError::ManifestRead {
            path: manifest_path.to_path_buf(),
            source: e,
        })?;
        let manifest: PluginManifest = toml::from_str(&body).map_err(|e| DiscoveryError::ManifestParse {
            path: manifest_path.to_path_buf(),
            source: e,
        })?;
        // Basic validation — id matches directory name.
        if let Some(dir_name) = plugin_dir.file_name().and_then(|s| s.to_str()) {
            if manifest.id != dir_name {
                return Err(DiscoveryError::ManifestInvalid {
                    path: manifest_path.to_path_buf(),
                    reason: format!(
                        "manifest id {:?} does not match directory name {:?}",
                        manifest.id, dir_name
                    ),
                });
            }
        }
        Ok(LoadedPlugin {
            manifest,
            plugin_dir: plugin_dir.to_path_buf(),
            manifest_path: manifest_path.to_path_buf(),
        })
    }
}
