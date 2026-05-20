use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinAutomationApp {
    pub id: String,
    pub spec_yaml: String,
    pub spec_path: PathBuf,
    pub skills: Vec<BuiltinAutomationSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinAutomationSkill {
    pub id: String,
    pub root: PathBuf,
    pub index_js: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
struct BuiltinManifest {
    apps: Vec<String>,
}

pub fn load_builtin_apps(root: impl AsRef<Path>) -> anyhow::Result<Vec<BuiltinAutomationApp>> {
    let root = root.as_ref();
    let manifest_path = root.join("manifest.json");
    let manifest_json = fs::read_to_string(&manifest_path)
        .map_err(|e| anyhow::anyhow!("read builtin manifest {}: {}", manifest_path.display(), e))?;
    let manifest: BuiltinManifest = serde_json::from_str(&manifest_json).map_err(|e| {
        anyhow::anyhow!("parse builtin manifest {}: {}", manifest_path.display(), e)
    })?;

    manifest
        .apps
        .into_iter()
        .map(|id| load_builtin_app(root, id))
        .collect()
}

pub fn sync_builtin_skills(
    app: &BuiltinAutomationApp,
    workspace_root: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let workspace_root = workspace_root.as_ref();
    let skills_root = workspace_root.join(".claude").join("skills");

    for skill in &app.skills {
        ensure_safe_segment(&skill.id, "builtin skill id")?;
        let destination = skills_root.join(&skill.id);
        fs::create_dir_all(&destination).map_err(|e| {
            anyhow::anyhow!("create builtin skill dir {}: {}", destination.display(), e)
        })?;
        copy_skill_dir(&skill.root, &destination)?;
    }

    Ok(())
}

fn load_builtin_app(root: &Path, id: String) -> anyhow::Result<BuiltinAutomationApp> {
    ensure_safe_segment(&id, "builtin app id")?;

    let app_root = root.join(&id);
    let spec_path = app_root.join("spec.yaml");
    let spec_yaml = fs::read_to_string(&spec_path)
        .map_err(|e| anyhow::anyhow!("read builtin spec {}: {}", spec_path.display(), e))?;
    let skills = load_builtin_skills(&app_root.join("skills"))?;

    Ok(BuiltinAutomationApp {
        id,
        spec_yaml,
        spec_path,
        skills,
    })
}

fn load_builtin_skills(skills_root: &Path) -> anyhow::Result<Vec<BuiltinAutomationSkill>> {
    if !skills_root.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(skills_root)
        .map_err(|e| anyhow::anyhow!("read builtin skills dir {}: {}", skills_root.display(), e))?
    {
        let entry = entry.map_err(|e| anyhow::anyhow!("read builtin skill entry: {}", e))?;
        let file_type = entry.file_type().map_err(|e| {
            anyhow::anyhow!("read builtin skill type {}: {}", entry.path().display(), e)
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("builtin skill id is not valid UTF-8"))?;
        ensure_safe_segment(&id, "builtin skill id")?;

        let root = entry.path();
        let index_js = root.join("index.js");
        if index_js.is_file() {
            skills.push(BuiltinAutomationSkill { id, root, index_js });
        }
    }
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(skills)
}

fn copy_skill_dir(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let bundle_root = source.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "canonicalize builtin skill root {}: {}",
            source.display(),
            e
        )
    })?;
    copy_dir_inner(source, destination, &bundle_root)
}

fn copy_dir_inner(source: &Path, destination: &Path, bundle_root: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(source)
        .map_err(|e| anyhow::anyhow!("read builtin skill dir {}: {}", source.display(), e))?
    {
        let entry = entry.map_err(|e| anyhow::anyhow!("read builtin skill entry: {}", e))?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        ensure_safe_file_name(&file_name)?;
        let destination_path = destination.join(file_name);
        let file_type = entry.file_type().map_err(|e| {
            anyhow::anyhow!("read builtin skill type {}: {}", source_path.display(), e)
        })?;

        if file_type.is_symlink() {
            let target = source_path.canonicalize().map_err(|e| {
                anyhow::anyhow!(
                    "canonicalize builtin skill symlink {}: {}",
                    source_path.display(),
                    e
                )
            })?;
            if !target.starts_with(bundle_root) {
                anyhow::bail!(
                    "builtin skill symlink escapes bundle root: {} -> {}",
                    source_path.display(),
                    target.display()
                );
            }
            if target.is_file() {
                fs::copy(&target, &destination_path).map_err(|e| {
                    anyhow::anyhow!(
                        "copy builtin skill symlink target {} to {}: {}",
                        target.display(),
                        destination_path.display(),
                        e
                    )
                })?;
            } else {
                anyhow::bail!(
                    "builtin skill symlink must point to a file: {}",
                    source_path.display()
                );
            }
        } else if file_type.is_dir() {
            fs::create_dir_all(&destination_path).map_err(|e| {
                anyhow::anyhow!(
                    "create builtin skill dir {}: {}",
                    destination_path.display(),
                    e
                )
            })?;
            copy_dir_inner(&source_path, &destination_path, bundle_root)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|e| {
                anyhow::anyhow!(
                    "copy builtin skill file {} to {}: {}",
                    source_path.display(),
                    destination_path.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

fn ensure_safe_segment(value: &str, label: &str) -> anyhow::Result<()> {
    let mut components = Path::new(value).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) if !value.is_empty() => Ok(()),
        _ => anyhow::bail!("rejecting unsafe {}: {}", label, value),
    }
}

fn ensure_safe_file_name(name: &OsStr) -> anyhow::Result<()> {
    let path = Path::new(name);
    match path.components().next() {
        Some(Component::Normal(_)) if path.components().count() == 1 => Ok(()),
        _ => anyhow::bail!("rejecting unsafe builtin skill file name: {:?}", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_builtin_apps_discovers_manifest_spec_and_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(
            root.join("manifest.json"),
            r#"{"apps":["bilibili-comment-auto-reply"]}"#,
        )
        .unwrap();
        let app_root = root.join("bilibili-comment-auto-reply");
        fs::create_dir_all(app_root.join("skills/bili-get-messages")).unwrap();
        fs::write(app_root.join("spec.yaml"), "type: automation\nname: bili\n").unwrap();
        fs::write(
            app_root.join("skills/bili-get-messages/index.js"),
            "module.exports = {};",
        )
        .unwrap();

        let apps = load_builtin_apps(root).unwrap();

        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "bilibili-comment-auto-reply");
        assert_eq!(apps[0].spec_yaml, "type: automation\nname: bili\n");
        assert_eq!(apps[0].skills.len(), 1);
        assert_eq!(apps[0].skills[0].id, "bili-get-messages");
    }

    #[test]
    fn sync_builtin_skills_rejects_symlink_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("skill");
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&root).unwrap();
        let outside = tmp.path().join("outside.js");
        fs::write(&outside, "escape").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("index.js")).unwrap();

        #[cfg(unix)]
        {
            let app = BuiltinAutomationApp {
                id: "test-app".into(),
                spec_yaml: String::new(),
                spec_path: tmp.path().join("spec.yaml"),
                skills: vec![BuiltinAutomationSkill {
                    id: "test-skill".into(),
                    root,
                    index_js: tmp.path().join("skill/index.js"),
                }],
            };

            let err = sync_builtin_skills(&app, workspace)
                .unwrap_err()
                .to_string();
            assert!(err.contains("escapes bundle root"), "{err}");
        }
    }
}
