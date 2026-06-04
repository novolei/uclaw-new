//! Pi-3b — install a plugin (git clone or local-dir copy) into
//! `$DATA_DIR/plugins/<id>/`. The plugin activates on next boot (registration is
//! boot-only). Files only — nothing is executed here; the subprocess runs
//! sandboxed (#669) at boot.

use std::path::Path;

use crate::plugin_manifest::schema::PluginManifest;

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub display_name: String,
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("plugin '{0}' is already installed")]
    AlreadyInstalled(String),
    #[error("git clone failed: {0}")]
    GitFailed(String),
    #[error("plugin.toml not found in source")]
    ManifestMissing,
    #[error("invalid plugin.toml: {0}")]
    ManifestInvalid(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Validate a source dir contains a parseable plugin.toml with a safe id.
fn validate_manifest_dir(dir: &Path) -> Result<PluginManifest, InstallError> {
    let manifest_path = dir.join("plugin.toml");
    if !manifest_path.exists() {
        return Err(InstallError::ManifestMissing);
    }
    let body = std::fs::read_to_string(&manifest_path).map_err(|e| InstallError::Io(e.to_string()))?;
    let manifest: PluginManifest =
        toml::from_str(&body).map_err(|e| InstallError::ManifestInvalid(e.to_string()))?;
    let id = manifest.id.trim();
    if id.is_empty() {
        return Err(InstallError::ManifestInvalid("empty id".into()));
    }
    // The id becomes a directory name under plugins/ — reject path traversal.
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(InstallError::ManifestInvalid(format!("unsafe id: {id}")));
    }
    Ok(manifest)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn finish(manifest: PluginManifest) -> InstalledPlugin {
    InstalledPlugin { id: manifest.id, display_name: manifest.display_name, version: manifest.version }
}

/// Copy a local plugin dir (containing plugin.toml) into plugins/<id>/.
pub fn install_from_local_dir(src: &Path, plugins_root: &Path) -> Result<InstalledPlugin, InstallError> {
    let manifest = validate_manifest_dir(src)?;
    let target = plugins_root.join(&manifest.id);
    if target.exists() {
        return Err(InstallError::AlreadyInstalled(manifest.id));
    }
    std::fs::create_dir_all(plugins_root).map_err(|e| InstallError::Io(e.to_string()))?;
    copy_dir_recursive(src, &target).map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(finish(manifest))
}

/// git clone (repo root = the plugin) into a staging dir, validate, promote to
/// plugins/<id>/. Cleans up staging on any failure.
pub async fn install_from_git(git_url: &str, plugins_root: &Path) -> Result<InstalledPlugin, InstallError> {
    std::fs::create_dir_all(plugins_root).map_err(|e| InstallError::Io(e.to_string()))?;
    let staging = plugins_root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let output = tokio::process::Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(git_url)
        .arg(&staging)
        .output()
        .await
        .map_err(|e| InstallError::GitFailed(format!("git unavailable: {e}")))?;
    if !output.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("clone failed");
        return Err(InstallError::GitFailed(tail.to_string()));
    }
    let manifest = match validate_manifest_dir(&staging) {
        Ok(m) => m,
        Err(e) => { let _ = std::fs::remove_dir_all(&staging); return Err(e); }
    };
    let target = plugins_root.join(&manifest.id);
    if target.exists() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(InstallError::AlreadyInstalled(manifest.id));
    }
    let _ = std::fs::remove_dir_all(staging.join(".git")); // drop clone history (best-effort)
    std::fs::rename(&staging, &target).map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging);
        InstallError::Io(e.to_string())
    })?;
    Ok(finish(manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn write_plugin(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), format!(
            "id = \"{id}\"\nversion = \"0.1.0\"\ndisplay_name = \"Demo\"\n\n[author]\nname = \"t\"\n\n[runtime]\nmin_uclaw_version = \"0.1.0\"\n"
        )).unwrap();
        std::fs::write(dir.join("server.mjs"), "// demo").unwrap();
    }
    #[test]
    fn local_install_copies_and_returns_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        write_plugin(&src, "demo");
        let root = tmp.path().join("plugins");
        let info = install_from_local_dir(&src, &root).unwrap();
        assert_eq!(info.id, "demo");
        assert!(root.join("demo/plugin.toml").exists());
        assert!(root.join("demo/server.mjs").exists());
    }
    #[test]
    fn local_install_rejects_already_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src"); write_plugin(&src, "demo");
        let root = tmp.path().join("plugins");
        install_from_local_dir(&src, &root).unwrap();
        let err = install_from_local_dir(&src, &root).unwrap_err();
        assert!(matches!(err, InstallError::AlreadyInstalled(_)));
    }
    #[test]
    fn local_install_rejects_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src"); std::fs::create_dir_all(&src).unwrap();
        let root = tmp.path().join("plugins");
        assert!(matches!(install_from_local_dir(&src, &root).unwrap_err(), InstallError::ManifestMissing));
        assert!(!root.join("demo").exists());
    }
    #[test]
    fn rejects_unsafe_id() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("plugin.toml"), "id = \"../evil\"\nversion=\"0.1.0\"\ndisplay_name=\"x\"\n[author]\nname=\"t\"\n[runtime]\nmin_uclaw_version=\"0.1.0\"\n").unwrap();
        let root = tmp.path().join("plugins");
        assert!(matches!(install_from_local_dir(&src, &root).unwrap_err(), InstallError::ManifestInvalid(_)));
    }
    #[tokio::test]
    async fn git_install_from_local_file_repo() {
        // Skip if git is unavailable.
        if std::process::Command::new("git").arg("--version").output().is_err() { return; }
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        write_plugin(&repo, "gitdemo");
        for args in [vec!["init","-q"], vec!["add","-A"], vec!["-c","user.email=t@t","-c","user.name=t","commit","-qm","x"]] {
            let ok = std::process::Command::new("git").current_dir(&repo).args(&args).output().unwrap().status.success();
            if !ok { return; } // env without git identity → skip
        }
        let root = tmp.path().join("plugins");
        let url = format!("file://{}", repo.display());
        let info = install_from_git(&url, &root).await.unwrap();
        assert_eq!(info.id, "gitdemo");
        assert!(root.join("gitdemo/plugin.toml").exists());
        assert!(!root.join("gitdemo/.git").exists()); // history dropped
        // no leftover staging dirs
        let staging = std::fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with(".staging-")).count();
        assert_eq!(staging, 0);
    }
}
