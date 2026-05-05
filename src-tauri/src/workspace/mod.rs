use std::path::{Path, PathBuf};
use crate::error::Error;
use crate::ipc::{ArtifactTreeNodeResponse, FileChangeEvent};
use tauri::Emitter;
use tauri::AppHandle;

/// Build a list of artifact tree nodes for a given directory path.
pub async fn list_artifact_tree(
    space_dir: &Path,
    relative_path: &str,
) -> Result<Vec<ArtifactTreeNodeResponse>, Error> {
    let dir = if relative_path.is_empty() || relative_path == "/" {
        space_dir.to_path_buf()
    } else {
        let clean = relative_path.trim_start_matches('/');
        let resolved = space_dir.join(clean);
        if resolved.exists() {
            let canonical = resolved.canonicalize().map_err(|e| {
                Error::NotFound(format!("Path not found: {} ({})", clean, e))
            })?;
            // Basic traversal check
            if !canonical.starts_with(space_dir) && !canonical.starts_with(space_dir.canonicalize().unwrap_or_default()) {
                return Err(Error::InvalidInput("Path traversal detected".into()));
            }
            canonical
        } else {
            space_dir.to_path_buf()
        }
    };

    let mut nodes: Vec<ArtifactTreeNodeResponse> = Vec::new();

    if !dir.exists() {
        return Ok(nodes);
    }

    let mut entries = tokio::fs::read_dir(&dir).await.map_err(Error::Io)?;

    while let Some(entry) = entries.next_entry().await.map_err(Error::Io)? {
        let path = entry.path();
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Skip hidden files and common ignore dirs
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        let relative = path.strip_prefix(space_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        let metadata = entry.metadata().await.map_err(Error::Io)?;
        let is_dir = metadata.is_dir();

        let parent_path = Path::new(&relative)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        nodes.push(ArtifactTreeNodeResponse {
            path: relative,
            name,
            is_dir,
            parent_path,
            size_bytes: if is_dir { None } else { Some(metadata.len()) },
            mime_type: if is_dir { None } else { mime_from_path(&path) },
            modified_at: metadata.modified().ok().map(|t| {
                chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
            }),
            children: if is_dir { Some(vec![]) } else { None },
        });
    }

    // Sort: dirs first, then by name case-insensitive
    nodes.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    Ok(nodes)
}

/// Load children of a specific directory path.
pub async fn load_artifact_children(
    space_dir: &Path,
    relative_path: &str,
) -> Result<Vec<ArtifactTreeNodeResponse>, Error> {
    list_artifact_tree(space_dir, relative_path).await
}

/// Determine MIME type from file extension.
pub fn mime_from_path(path: &std::path::Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "ts" | "tsx" => "text/typescript",
        "js" | "jsx" => "text/javascript",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "go" => "text/x-go",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "json" => "application/json",
        "md" => "text/markdown",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svelte" => "text/x-svelte",
        "yaml" | "yml" => "text/yaml",
        "toml" => "text/toml",
        "sql" => "text/x-sql",
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        _ => "application/octet-stream",
    }.to_string())
}

/// File Watcher using notify crate.
pub struct FileWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl FileWatcher {
    pub fn new(
        watch_dir: PathBuf,
        space_id: String,
        app_handle: AppHandle,
    ) -> Result<Self, Error> {
        use notify::{Event, EventKind, RecursiveMode, Watcher};

        let app = app_handle.clone();
        let sid = space_id.clone();
        let wd = watch_dir.clone();

        let mut watcher = notify::RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let changes: Vec<FileChangeEvent> = event.paths.iter().map(|p| {
                        let relative = p.strip_prefix(&wd)
                            .unwrap_or(p)
                            .to_string_lossy()
                            .to_string();
                        FileChangeEvent {
                            space_id: sid.clone(),
                            change_type: match event.kind {
                                EventKind::Create(_) => "create",
                                EventKind::Modify(_) => "modify",
                                EventKind::Remove(_) => "delete",
                                _ => "modify",
                            }.to_string(),
                            path: relative,
                            old_path: None,
                            is_dir: false,
                        }
                    }).collect();

                    if !changes.is_empty() {
                        let _ = app.emit("artifact:tree_update", &changes[0]);
                    }
                }
            },
            notify::Config::default(),
        ).map_err(|e| Error::Internal(format!("Failed to create file watcher: {}", e)))?;

        watcher
            .watch(&watch_dir, RecursiveMode::Recursive)
            .map_err(|e| Error::Internal(format!("Failed to watch directory: {}", e)))?;

        Ok(FileWatcher { _watcher: watcher })
    }
}
