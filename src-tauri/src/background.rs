use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Background task status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Background task record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    pub id: String,
    pub name: String,
    pub status: TaskStatus,
    pub progress: Option<u32>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl BackgroundTask {
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            status: TaskStatus::Pending,
            progress: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// Background task manager
pub struct BackgroundTaskManager {
    tasks: HashMap<String, BackgroundTask>,
    max_tasks: usize,
}

impl BackgroundTaskManager {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            max_tasks: 100,
        }
    }

    /// Create a new task
    pub fn create(&mut self, name: &str) -> &BackgroundTask {
        let task = BackgroundTask::new(name);
        let id = task.id.clone();
        self.tasks.insert(id.clone(), task);
        self.cleanup();
        self.tasks.get(&id).unwrap()
    }

    /// Update task status
    pub fn set_status(&mut self, id: &str, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = status;
            task.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }

    /// Update task progress
    pub fn set_progress(&mut self, id: &str, progress: u32) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.progress = Some(progress);
            task.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }

    /// Mark task as completed
    pub fn complete(&mut self, id: &str) {
        self.set_status(id, TaskStatus::Completed);
    }

    /// Mark task as failed
    pub fn fail(&mut self, id: &str, error: impl Into<String>) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = TaskStatus::Failed;
            task.error = Some(error.into());
            task.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }

    /// Cancel a task
    pub fn cancel(&mut self, id: &str) {
        self.set_status(id, TaskStatus::Cancelled);
    }

    /// Get a task
    pub fn get(&self, id: &str) -> Option<&BackgroundTask> {
        self.tasks.get(id)
    }

    /// List all tasks
    pub fn list(&self) -> Vec<&BackgroundTask> {
        let mut tasks: Vec<&BackgroundTask> = self.tasks.values().collect();
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        tasks
    }

    /// Remove old tasks beyond max
    fn cleanup(&mut self) {
        while self.tasks.len() > self.max_tasks {
            let mut oldest: Vec<(&String, &BackgroundTask)> = self.tasks.iter().collect();
            oldest.sort_by(|a, b| a.1.created_at.cmp(&b.1.created_at));
            if let Some((id, _)) = oldest.first() {
                let id = id.to_string();
                self.tasks.remove(&id);
            }
        }
    }
}

/// Shared type for Tauri state
pub type SharedBackgroundManager = Arc<Mutex<BackgroundTaskManager>>;
