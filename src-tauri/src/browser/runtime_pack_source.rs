//! Source resolution for app-managed Browser runtime packs.
//!
//! A valid source is a complete `browser-runtime-pack-v1/` directory whose
//! layout matches the installed pack layout. The installer copies from one of
//! these sources into uClaw-managed storage; it never falls back to global npm
//! or user-managed browser caches.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::runtime_pack::{
    load_runtime_pack_manifest, BrowserRuntimePackManifest, BrowserRuntimePackManifestLoadStatus,
};

const ENV_SOURCE_VAR: &str = "UCLAW_BROWSER_RUNTIME_PACK_SOURCE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackSourceResolutionStatus {
    Found,
    Missing,
    Invalid,
    UnsupportedPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackSourceKind {
    EnvOverride,
    BundleResource,
    DevStaging,
}

impl BrowserRuntimePackSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EnvOverride => "env_override",
            Self::BundleResource => "bundle_resource",
            Self::DevStaging => "dev_staging",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackSourceResolution {
    pub status: BrowserRuntimePackSourceResolutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<BrowserRuntimePackSourceKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<BrowserRuntimePackManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BrowserRuntimePackSourceResolver {
    env_override_dir: Option<PathBuf>,
    bundle_resource_dir: Option<PathBuf>,
    dev_staging_dir: Option<PathBuf>,
    supported_platform: bool,
}

impl BrowserRuntimePackSourceResolver {
    pub fn new(bundle_resource_dir: Option<PathBuf>, dev_staging_dir: Option<PathBuf>) -> Self {
        Self {
            env_override_dir: std::env::var_os(ENV_SOURCE_VAR).map(PathBuf::from),
            bundle_resource_dir,
            dev_staging_dir,
            supported_platform: is_supported_platform(),
        }
    }

    pub fn from_runtime_context(bundle_resource_dir: Option<PathBuf>) -> Self {
        let dev_staging_dir = Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".runtime-pack-staging")
                .join(BrowserRuntimePackManifest::v1_default().pack_version),
        );
        Self::new(bundle_resource_dir, dev_staging_dir)
    }

    pub fn for_test(
        env_override_dir: Option<PathBuf>,
        bundle_resource_dir: Option<PathBuf>,
        dev_staging_dir: Option<PathBuf>,
    ) -> Self {
        Self::for_test_with_platform(true, env_override_dir, bundle_resource_dir, dev_staging_dir)
    }

    pub fn for_test_with_platform(
        supported_platform: bool,
        env_override_dir: Option<PathBuf>,
        bundle_resource_dir: Option<PathBuf>,
        dev_staging_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            env_override_dir,
            bundle_resource_dir,
            dev_staging_dir,
            supported_platform,
        }
    }

    pub fn resolve(
        &self,
        expected: &BrowserRuntimePackManifest,
    ) -> BrowserRuntimePackSourceResolution {
        if !self.supported_platform {
            return BrowserRuntimePackSourceResolution {
                status: BrowserRuntimePackSourceResolutionStatus::UnsupportedPlatform,
                source_kind: None,
                source_dir: None,
                manifest: None,
                validation_errors: vec![
                    "Browser runtime pack v1 supports macOS arm64 only.".to_string()
                ],
            };
        }

        for (kind, candidate) in self.candidates() {
            let Some(source_dir) = candidate else {
                continue;
            };
            if !source_dir.exists() {
                continue;
            }

            let validation = validate_source(expected, &source_dir);
            return BrowserRuntimePackSourceResolution {
                status: if validation.errors.is_empty() {
                    BrowserRuntimePackSourceResolutionStatus::Found
                } else {
                    BrowserRuntimePackSourceResolutionStatus::Invalid
                },
                source_kind: Some(kind),
                source_dir: Some(source_dir),
                manifest: validation.manifest,
                validation_errors: validation.errors,
            };
        }

        BrowserRuntimePackSourceResolution {
            status: BrowserRuntimePackSourceResolutionStatus::Missing,
            source_kind: None,
            source_dir: None,
            manifest: None,
            validation_errors: vec![
                "Runtime pack source not found. Generate the dev pack or install an app bundle that includes it.".to_string(),
            ],
        }
    }

    fn candidates(&self) -> [(BrowserRuntimePackSourceKind, Option<PathBuf>); 3] {
        [
            (
                BrowserRuntimePackSourceKind::EnvOverride,
                self.env_override_dir.clone(),
            ),
            (
                BrowserRuntimePackSourceKind::BundleResource,
                self.bundle_resource_dir.clone(),
            ),
            (
                BrowserRuntimePackSourceKind::DevStaging,
                self.dev_staging_dir.clone(),
            ),
        ]
    }
}

struct SourceValidation {
    manifest: Option<BrowserRuntimePackManifest>,
    errors: Vec<String>,
}

pub fn validate_runtime_pack_source(
    expected: &BrowserRuntimePackManifest,
    source_dir: &Path,
) -> BrowserRuntimePackSourceResolution {
    let validation = validate_source(expected, source_dir);
    BrowserRuntimePackSourceResolution {
        status: if validation.errors.is_empty() {
            BrowserRuntimePackSourceResolutionStatus::Found
        } else {
            BrowserRuntimePackSourceResolutionStatus::Invalid
        },
        source_kind: None,
        source_dir: Some(source_dir.to_path_buf()),
        manifest: validation.manifest,
        validation_errors: validation.errors,
    }
}

fn validate_source(expected: &BrowserRuntimePackManifest, source_dir: &Path) -> SourceValidation {
    let mut errors = Vec::new();
    let manifest_path = source_dir.join("runtime-pack.manifest.json");
    let manifest_load = load_runtime_pack_manifest(&manifest_path);
    let manifest = manifest_load.manifest;

    match manifest_load.status {
        BrowserRuntimePackManifestLoadStatus::Loaded => {}
        BrowserRuntimePackManifestLoadStatus::Missing => {
            errors.push("missing runtime-pack.manifest.json".to_string());
        }
        BrowserRuntimePackManifestLoadStatus::InvalidJson => {
            errors.push("invalid runtime-pack.manifest.json".to_string());
        }
        BrowserRuntimePackManifestLoadStatus::IoError => {
            errors.push("could not read runtime-pack.manifest.json".to_string());
        }
    }

    if let Some(installed) = manifest.as_ref() {
        push_version_error(
            &mut errors,
            "packVersion",
            &expected.pack_version,
            &installed.pack_version,
        );
        push_version_error(
            &mut errors,
            "nodeVersion",
            &expected.node_version,
            &installed.node_version,
        );
        push_version_error(
            &mut errors,
            "playwrightVersion",
            &expected.playwright_version,
            &installed.playwright_version,
        );
        push_version_error(
            &mut errors,
            "playwrightMcpVersion",
            &expected.playwright_mcp_version,
            &installed.playwright_mcp_version,
        );
        push_version_error(
            &mut errors,
            "workerVersion",
            &expected.worker_version,
            &installed.worker_version,
        );
        push_version_error(
            &mut errors,
            "chromiumRevision",
            &expected.chromium_revision,
            &installed.chromium_revision,
        );
    }

    for required_path in required_source_paths(expected) {
        let path = source_dir.join(required_path);
        if !path.exists() {
            errors.push(format!(
                "missing required runtime pack path: {}",
                path.display()
            ));
        }
    }

    SourceValidation { manifest, errors }
}

fn push_version_error(errors: &mut Vec<String>, field: &str, expected: &str, actual: &str) {
    if expected != actual {
        errors.push(format!(
            "{field} mismatch: expected {expected}, got {actual}"
        ));
    }
}

fn required_source_paths(manifest: &BrowserRuntimePackManifest) -> Vec<PathBuf> {
    vec![
        PathBuf::from("runtime-pack.manifest.json"),
        PathBuf::from("node/bin/node"),
        PathBuf::from("node_modules/playwright"),
        PathBuf::from("node_modules/@playwright/mcp"),
        PathBuf::from("worker/uclaw-playwright-worker.mjs"),
        chromium_binary_relative_path(&manifest.chromium_revision),
    ]
}

fn chromium_binary_relative_path(revision: &str) -> PathBuf {
    let chromium_root = PathBuf::from("ms-playwright").join(format!("chromium-{revision}"));
    match std::env::consts::OS {
        "macos" => chromium_root
            .join("chrome-mac")
            .join("Chromium.app")
            .join("Contents")
            .join("MacOS")
            .join("Chromium"),
        "windows" => chromium_root.join("chrome-win").join("chrome.exe"),
        _ => chromium_root.join("chrome-linux").join("chrome"),
    }
}

fn is_supported_platform() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write_pack_fixture(root: &std::path::Path, manifest: &BrowserRuntimePackManifest) {
        fs::create_dir_all(root.join("node/bin")).expect("node bin");
        fs::write(root.join("node/bin/node"), "node").expect("node");
        fs::create_dir_all(root.join("node_modules/playwright")).expect("playwright");
        fs::create_dir_all(root.join("node_modules/@playwright/mcp")).expect("mcp");
        fs::create_dir_all(root.join("worker")).expect("worker dir");
        fs::write(
            root.join("worker/uclaw-playwright-worker.mjs"),
            "console.log('worker')\n",
        )
        .expect("worker");
        fs::create_dir_all(
            root.join("ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS"),
        )
        .expect("chromium dir");
        fs::write(
            root.join(
                "ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS/Chromium",
            ),
            "chromium",
        )
        .expect("chromium");
        fs::write(
            root.join("runtime-pack.manifest.json"),
            serde_json::to_string_pretty(manifest).expect("manifest json"),
        )
        .expect("manifest");
    }

    #[test]
    fn env_override_wins_over_bundle_and_dev_candidates() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let env = temp.path().join("env/browser-runtime-pack-v1");
        let bundle = temp.path().join("bundle/browser-runtime-pack-v1");
        let dev = temp.path().join("dev/browser-runtime-pack-v1");
        write_pack_fixture(&env, &manifest);
        write_pack_fixture(&bundle, &manifest);
        write_pack_fixture(&dev, &manifest);

        let resolver =
            BrowserRuntimePackSourceResolver::for_test(Some(env.clone()), Some(bundle), Some(dev));
        let resolution = resolver.resolve(&manifest);

        assert_eq!(
            resolution.status,
            BrowserRuntimePackSourceResolutionStatus::Found
        );
        assert_eq!(
            resolution.source_kind,
            Some(BrowserRuntimePackSourceKind::EnvOverride)
        );
        assert_eq!(resolution.source_dir.as_deref(), Some(env.as_path()));
        assert!(resolution.validation_errors.is_empty());
    }

    #[test]
    fn dev_staging_is_used_when_env_and_bundle_are_missing() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let dev = temp
            .path()
            .join("src-tauri/.runtime-pack-staging/browser-runtime-pack-v1");
        write_pack_fixture(&dev, &manifest);

        let resolver = BrowserRuntimePackSourceResolver::for_test(None, None, Some(dev.clone()));
        let resolution = resolver.resolve(&manifest);

        assert_eq!(
            resolution.status,
            BrowserRuntimePackSourceResolutionStatus::Found
        );
        assert_eq!(
            resolution.source_kind,
            Some(BrowserRuntimePackSourceKind::DevStaging)
        );
        assert_eq!(resolution.source_dir.as_deref(), Some(dev.as_path()));
    }

    #[test]
    fn unsupported_platform_returns_clear_status() {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let resolver =
            BrowserRuntimePackSourceResolver::for_test_with_platform(false, None, None, None);
        let resolution = resolver.resolve(&manifest);

        assert_eq!(
            resolution.status,
            BrowserRuntimePackSourceResolutionStatus::UnsupportedPlatform
        );
        assert!(resolution.validation_errors[0].contains("macOS arm64"));
    }

    #[test]
    fn missing_source_reports_generation_guidance() {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let resolver = BrowserRuntimePackSourceResolver::for_test(None, None, None);
        let resolution = resolver.resolve(&manifest);

        assert_eq!(
            resolution.status,
            BrowserRuntimePackSourceResolutionStatus::Missing
        );
        assert!(resolution.validation_errors[0].contains("Generate the dev pack"));
    }

    #[test]
    fn invalid_source_reports_missing_required_paths() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let source = temp.path().join("broken/browser-runtime-pack-v1");
        fs::create_dir_all(&source).expect("source");
        fs::write(
            source.join("runtime-pack.manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("manifest json"),
        )
        .expect("manifest");

        let resolver = BrowserRuntimePackSourceResolver::for_test(Some(source), None, None);
        let resolution = resolver.resolve(&manifest);

        assert_eq!(
            resolution.status,
            BrowserRuntimePackSourceResolutionStatus::Invalid
        );
        assert!(resolution
            .validation_errors
            .iter()
            .any(|error| error.contains("node/bin/node")));
        assert!(resolution
            .validation_errors
            .iter()
            .any(|error| error.contains("node_modules/playwright")));
    }

    #[test]
    fn version_mismatch_reports_expected_and_actual_values() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let mut old_manifest = manifest.clone();
        old_manifest.node_version = "20.0.0".to_string();
        let source = temp.path().join("source/browser-runtime-pack-v1");
        write_pack_fixture(&source, &old_manifest);

        let resolution = validate_runtime_pack_source(&manifest, &source);

        assert_eq!(
            resolution.status,
            BrowserRuntimePackSourceResolutionStatus::Invalid
        );
        assert!(resolution
            .validation_errors
            .iter()
            .any(|error| error.contains("nodeVersion mismatch: expected 22.16.0, got 20.0.0")));
    }
}
