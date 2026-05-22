use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Emitter;

/// Notification severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A single notification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub message: String,
    pub level: NotificationLevel,
    pub source: String,
    pub timestamp: String,
    pub duration_ms: Option<u64>,
}

impl Notification {
    pub fn new(title: impl Into<String>, message: impl Into<String>, level: NotificationLevel, source: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            message: message.into(),
            level,
            source: source.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: None,
        }
    }

    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }
}

/// Notification manager — in-memory queue with Tauri event emission
pub struct NotificationManager {
    history: VecDeque<Notification>,
    app_handle: tauri::AppHandle,
    max_history: usize,
}

impl NotificationManager {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            history: VecDeque::new(),
            app_handle,
            max_history: 100,
        }
    }

    /// Push a notification and emit it to the frontend
    pub fn push(&mut self, notification: Notification) {
        let _ = self.app_handle.emit("notification:new", &notification);
        self.history.push_front(notification);

        // Trim history
        while self.history.len() > self.max_history {
            self.history.pop_back();
        }
    }

    /// Convenience: info notification
    pub fn info(&mut self, title: &str, message: &str, source: &str) {
        self.push(Notification::new(title, message, NotificationLevel::Info, source));
    }

    /// Convenience: success notification
    pub fn success(&mut self, title: &str, message: &str, source: &str) {
        self.push(Notification::new(title, message, NotificationLevel::Success, source));
    }

    /// Convenience: warning notification
    pub fn warning(&mut self, title: &str, message: &str, source: &str) {
        self.push(Notification::new(title, message, NotificationLevel::Warning, source));
    }

    /// Convenience: error notification
    pub fn error(&mut self, title: &str, message: &str, source: &str) {
        self.push(Notification::new(title, message, NotificationLevel::Error, source));
    }

    /// Get recent history
    pub fn history(&self) -> Vec<Notification> {
        self.history.iter().cloned().collect()
    }

    /// Clear history
    pub fn clear(&mut self) {
        self.history.clear();
    }
}

/// Shared notification manager type for Tauri state
pub type SharedNotificationManager = Arc<Mutex<NotificationManager>>;

pub fn notify_pipeline_done(
    app: &tauri::AppHandle,
    issue_number: u64,
    issue_title: &str,
    _notification_sound: bool,
) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<crate::app::AppState>() {
        let notifications = state.notifications.clone();
        let issue_title = issue_title.to_string();
        tauri::async_runtime::spawn(async move {
            let mut mgr = notifications.lock().await;
            let title = format!("Pipeline Done (Issue #{})", issue_number);
            let message = format!("Issue #{}: {} has been successfully processed and completed.", issue_number, issue_title);
            mgr.success(&title, &message, "symphony");
        });
    }
}

pub fn notify_pipeline_failed(
    app: &tauri::AppHandle,
    issue_number: u64,
    stage_label: &str,
    _notification_sound: bool,
) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<crate::app::AppState>() {
        let notifications = state.notifications.clone();
        let stage_label = stage_label.to_string();
        tauri::async_runtime::spawn(async move {
            let mut mgr = notifications.lock().await;
            let title = format!("Pipeline Failed (Issue #{})", issue_number);
            let message = format!("Stage '{}' failed for Issue #{}.", stage_label, issue_number);
            mgr.error(&title, &message, "symphony");
        });
    }
}

pub fn notify_awaiting_approval(
    app: &tauri::AppHandle,
    issue_number: u64,
    stage_label: &str,
    _notification_sound: bool,
) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<crate::app::AppState>() {
        let notifications = state.notifications.clone();
        let stage_label = stage_label.to_string();
        tauri::async_runtime::spawn(async move {
            let mut mgr = notifications.lock().await;
            let title = format!("Awaiting Approval (Issue #{})", issue_number);
            let message = format!("Stage '{}' of Issue #{} is awaiting your approval.", stage_label, issue_number);
            mgr.warning(&title, &message, "symphony");
        });
    }
}

pub fn notify_all_processed(
    app: &tauri::AppHandle,
    _notification_sound: bool,
) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<crate::app::AppState>() {
        let notifications = state.notifications.clone();
        tauri::async_runtime::spawn(async move {
            let mut mgr = notifications.lock().await;
            mgr.info("All Issues Processed", "The Symphony orchestrator has completed all scheduled tasks.", "symphony");
        });
    }
}
