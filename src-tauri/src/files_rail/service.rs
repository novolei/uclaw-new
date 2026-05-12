//! `FilesRailService` — owns the `FilesRailWatcher` and implements `ManagedService`.

use super::watcher::FilesRailWatcher;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::RwLock;

pub struct FilesRailService {
    watcher: Arc<FilesRailWatcher>,
    status: Arc<RwLock<ServiceStatus>>,
}

impl FilesRailService {
    pub fn new(app: AppHandle) -> Self {
        Self {
            watcher: Arc::new(FilesRailWatcher::new(app)),
            status: Arc::new(RwLock::new(ServiceStatus::Stopped)),
        }
    }

    pub fn watcher(&self) -> Arc<FilesRailWatcher> {
        self.watcher.clone()
    }

    pub async fn register_mount(&self, mount_id: String, root: PathBuf) -> anyhow::Result<()> {
        self.watcher
            .register_mount(mount_id, root)
            .await
            .map_err(|e| anyhow::anyhow!("watcher register_mount: {}", e))
    }

    pub async fn unregister_mount(&self, mount_id: &str) -> anyhow::Result<()> {
        self.watcher
            .unregister_mount(mount_id)
            .await
            .map_err(|e| anyhow::anyhow!("watcher unregister_mount: {}", e))
    }
}

#[async_trait]
impl ManagedService for FilesRailService {
    fn name(&self) -> &str {
        "files_rail"
    }

    async fn start(&self) -> anyhow::Result<()> {
        *self.status.write().await = ServiceStatus::Starting;
        // Watcher.start() MUST be called before any register_mount() call —
        // otherwise the notify subscription is a silent no-op.
        self.watcher
            .start()
            .await
            .map_err(|e| anyhow::anyhow!("watcher start: {}", e))?;
        *self.status.write().await = ServiceStatus::Running;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        *self.status.write().await = ServiceStatus::Stopping;
        // TODO(files-rail-stop): the watcher's tokio::spawn flush loop is
        // orphaned here — it dies when the tauri runtime tears down, which
        // is fine for the only stop path today (WindowEvent::Destroyed →
        // service_manager.stop_all → process exit). If we ever support
        // hot-restart, store the JoinHandle in Inner and call abort() here.
        // Watcher is held in Arc and drops naturally when service drops.
        // Tauri's WindowEvent::Destroyed flow releases AppState which drops here.
        *self.status.write().await = ServiceStatus::Stopped;
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        self.status
            .try_read()
            .map(|s| s.clone())
            .unwrap_or(ServiceStatus::Stopped)
    }

    fn health(&self) -> ServiceHealth {
        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs: None,
            last_error: None,
            metrics: serde_json::json!({}),
        }
    }
}
