//! Derived session/task projection stubs and compact journal entries.
//!
//! This module only reads rollout JSONL records. It does not own runtime truth,
//! mutate task events, or write to the SQLite rollout mirror.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::runtime::contracts::{TaskEvent, TaskEventSource, TaskVerdict};
use crate::runtime::rollout::RolloutRecord;

pub const PROJECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProjectionStatus {
    Running,
    Waiting,
    Checkpointed,
    Completed,
    Cancelled,
    Failed,
    BudgetExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProjectionSummary {
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    pub source: TaskEventSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_kind: Option<String>,
    pub status: TaskProjectionStatus,
    pub is_terminal: bool,
    pub event_count: u64,
    pub last_sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary_reason: Option<String>,
    pub warning_count: u64,
    pub source_rollout_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjectionStub {
    pub schema_version: u32,
    pub generated_at: String,
    pub source_rollout_file: PathBuf,
    pub last_sequence: u64,
    pub malformed_line_count: u64,
    pub tasks: Vec<TaskProjectionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionJournalEntry {
    pub sequence: u64,
    pub task_id: String,
    pub ts: String,
    pub kind: String,
    pub source: TaskEventSource,
    pub status: TaskProjectionStatus,
    pub is_terminal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectionJournalStore {
    pub root_dir: PathBuf,
}

impl SessionProjectionStub {
    pub fn from_records(records: &[RolloutRecord], generated_at: impl Into<String>) -> Self {
        Self::from_records_with_malformed(records, generated_at, 0)
    }

    fn from_records_with_malformed(
        records: &[RolloutRecord],
        generated_at: impl Into<String>,
        malformed_line_count: u64,
    ) -> Self {
        let source_rollout_file = records
            .first()
            .map(|record| record.rollout_file.clone())
            .unwrap_or_default();
        let mut by_task: HashMap<String, TaskProjectionSummary> = HashMap::new();
        let mut last_sequence = 0;

        for (index, record) in records.iter().enumerate() {
            let projection_sequence = (index as u64) + 1;
            last_sequence = projection_sequence;
            let task_id = record.event.task_id().to_string();
            let ts = record.event.ts().to_string();
            let kind = record.event.kind().to_string();
            let source = record.event.source();
            let summary = by_task
                .entry(task_id.clone())
                .or_insert_with(|| TaskProjectionSummary {
                    task_id,
                    intent_id: None,
                    source,
                    first_ts: Some(ts.clone()),
                    last_ts: None,
                    last_kind: None,
                    status: TaskProjectionStatus::Running,
                    is_terminal: false,
                    event_count: 0,
                    last_sequence: 0,
                    checkpoint_ref: None,
                    boundary_reason: None,
                    warning_count: 0,
                    source_rollout_file: record.rollout_file.clone(),
                });

            summary.source = source;
            if summary.first_ts.is_none() {
                summary.first_ts = Some(ts.clone());
            }
            summary.last_ts = Some(ts);
            summary.last_kind = Some(kind);
            summary.event_count += 1;
            summary.last_sequence = projection_sequence;
            summary.source_rollout_file = record.rollout_file.clone();

            if summary.intent_id.is_none() {
                summary.intent_id = record.intent_id.clone();
            }

            match &record.event {
                TaskEvent::TaskStarted { intent_id, .. } => {
                    summary.intent_id = Some(intent_id.clone());
                    if !summary.is_terminal {
                        summary.status = TaskProjectionStatus::Running;
                    }
                }
                TaskEvent::Checkpoint { checkpoint_ref, .. } => {
                    summary.status = TaskProjectionStatus::Checkpointed;
                    summary.is_terminal = false;
                    summary.checkpoint_ref = Some(checkpoint_ref.clone());
                }
                TaskEvent::BoundaryYield { reason, .. } => {
                    summary.status = TaskProjectionStatus::Waiting;
                    summary.is_terminal = false;
                    summary.boundary_reason = Some(reason.clone());
                }
                TaskEvent::Warning { .. } => {
                    summary.warning_count += 1;
                }
                TaskEvent::TaskFinished { verdict, .. } => {
                    summary.status = status_from_verdict(verdict);
                    summary.is_terminal = true;
                }
                _ => {}
            }
        }

        let mut tasks: Vec<_> = by_task.into_values().collect();
        tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));

        Self {
            schema_version: PROJECTION_SCHEMA_VERSION,
            generated_at: generated_at.into(),
            source_rollout_file,
            last_sequence,
            malformed_line_count,
            tasks,
        }
    }
}

impl ProjectionJournalEntry {
    fn from_record(sequence: u64, record: &RolloutRecord) -> Self {
        let (status, is_terminal, checkpoint_ref, boundary_reason) = match &record.event {
            TaskEvent::Checkpoint { checkpoint_ref, .. } => (
                TaskProjectionStatus::Checkpointed,
                false,
                Some(checkpoint_ref.clone()),
                None,
            ),
            TaskEvent::BoundaryYield { reason, .. } => (
                TaskProjectionStatus::Waiting,
                false,
                None,
                Some(reason.clone()),
            ),
            TaskEvent::TaskFinished { verdict, .. } => {
                (status_from_verdict(verdict), true, None, None)
            }
            _ => (TaskProjectionStatus::Running, false, None, None),
        };

        Self {
            sequence,
            task_id: record.event.task_id().to_string(),
            ts: record.event.ts().to_string(),
            kind: record.event.kind().to_string(),
            source: record.event.source(),
            status,
            is_terminal,
            checkpoint_ref,
            boundary_reason,
        }
    }
}

impl ProjectionJournalStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn stub_path_for_rollout(&self, rollout_file: &Path) -> PathBuf {
        self.root_dir
            .join("projection-stubs")
            .join(format!("{}.stub.json", rollout_file_stem(rollout_file)))
    }

    pub fn journal_path_for_rollout(&self, rollout_file: &Path) -> PathBuf {
        self.root_dir
            .join("projection-journals")
            .join(format!("{}.journal.jsonl", rollout_file_stem(rollout_file)))
    }

    pub fn write_stub(&self, stub: &SessionProjectionStub) -> std::io::Result<()> {
        let path = self.stub_path_for_rollout(&stub.source_rollout_file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(stub).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }

    pub fn read_stub(&self, rollout_file: &Path) -> std::io::Result<SessionProjectionStub> {
        let path = self.stub_path_for_rollout(rollout_file);
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(std::io::Error::other)
    }

    pub fn derive_journal_entries(&self, records: &[RolloutRecord]) -> Vec<ProjectionJournalEntry> {
        records
            .iter()
            .enumerate()
            .map(|(index, record)| ProjectionJournalEntry::from_record((index as u64) + 1, record))
            .collect()
    }

    pub fn append_journal_entries(
        &self,
        rollout_file: &Path,
        entries: &[ProjectionJournalEntry],
    ) -> std::io::Result<()> {
        let path = self.journal_path_for_rollout(rollout_file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        for entry in entries {
            let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
        }
        Ok(())
    }

    pub fn read_journal_entries_lossy(
        &self,
        rollout_file: &Path,
    ) -> std::io::Result<Vec<ProjectionJournalEntry>> {
        let path = self.journal_path_for_rollout(rollout_file);
        let file = fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        for line in reader.split(b'\n') {
            let line = match line {
                Ok(line) => line,
                Err(_) => continue,
            };
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let Ok(line) = std::str::from_utf8(&line) else {
                continue;
            };
            if let Ok(entry) = serde_json::from_str::<ProjectionJournalEntry>(line) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    pub fn build_stub_from_rollout_jsonl(
        &self,
        rollout_file: &Path,
        generated_at: impl Into<String>,
    ) -> std::io::Result<SessionProjectionStub> {
        let file = fs::File::open(rollout_file)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();
        let mut malformed_line_count = 0;

        for line in reader.split(b'\n') {
            let line = line?;
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let Ok(line) = std::str::from_utf8(&line) else {
                malformed_line_count += 1;
                continue;
            };
            match serde_json::from_str::<RolloutRecord>(line) {
                Ok(record) => records.push(record),
                Err(_) => malformed_line_count += 1,
            }
        }

        let mut stub = SessionProjectionStub::from_records_with_malformed(
            &records,
            generated_at,
            malformed_line_count,
        );

        stub.source_rollout_file = rollout_file.to_path_buf();
        for task in &mut stub.tasks {
            task.source_rollout_file = rollout_file.to_path_buf();
        }
        Ok(stub)
    }
}

fn status_from_verdict(verdict: &TaskVerdict) -> TaskProjectionStatus {
    match verdict {
        TaskVerdict::Completed { .. } => TaskProjectionStatus::Completed,
        TaskVerdict::Cancelled { .. } => TaskProjectionStatus::Cancelled,
        TaskVerdict::Failed { .. } => TaskProjectionStatus::Failed,
        TaskVerdict::BudgetExhausted { .. } => TaskProjectionStatus::BudgetExhausted,
    }
}

fn rollout_file_stem(rollout_file: &Path) -> String {
    rollout_file
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("rollout")
        .replace(['/', '\\', ':'], "_")
}

#[cfg(test)]
#[path = "projection_journal_tests.rs"]
mod tests;
