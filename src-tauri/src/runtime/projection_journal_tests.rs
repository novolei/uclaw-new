use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;
use crate::runtime::contracts::{TaskEvent, TaskEventSource, TaskVerdict, TokenUsage};
use crate::runtime::rollout::RolloutRecord;

fn rollout_file() -> PathBuf {
    PathBuf::from("/tmp/rollout-pr5.jsonl")
}

fn record(sequence: u64, event: TaskEvent, intent_id: Option<&str>) -> RolloutRecord {
    RolloutRecord {
        sequence,
        event,
        intent_id: intent_id.map(str::to_string),
        rollout_file: rollout_file(),
    }
}

fn started(task_id: &str, intent_id: &str, ts: &str) -> TaskEvent {
    TaskEvent::TaskStarted {
        ts: ts.into(),
        source: TaskEventSource::AgentLoop,
        task_id: task_id.into(),
        intent_id: intent_id.into(),
    }
}

fn model_turn(task_id: &str, ts: &str) -> TaskEvent {
    TaskEvent::ModelTurn {
        ts: ts.into(),
        source: TaskEventSource::AgentLoop,
        task_id: task_id.into(),
        provider: "openai".into(),
        model: "gpt-5".into(),
        token_usage: TokenUsage {
            input_tokens: 8,
            cached_input_tokens: 2,
            output_tokens: 5,
            reasoning_output_tokens: 1,
            total_tokens: 14,
            cost_usd_micros: Some(42),
        },
    }
}

fn finished(task_id: &str, ts: &str, verdict: TaskVerdict) -> TaskEvent {
    TaskEvent::TaskFinished {
        ts: ts.into(),
        source: TaskEventSource::AgentLoop,
        task_id: task_id.into(),
        verdict,
    }
}

fn boundary_yield(task_id: &str, ts: &str, reason: &str) -> TaskEvent {
    TaskEvent::BoundaryYield {
        ts: ts.into(),
        source: TaskEventSource::AgentLoop,
        task_id: task_id.into(),
        reason: reason.into(),
    }
}

fn checkpoint(task_id: &str, ts: &str, checkpoint_ref: &str) -> TaskEvent {
    TaskEvent::Checkpoint {
        ts: ts.into(),
        source: TaskEventSource::Browser,
        task_id: task_id.into(),
        checkpoint_ref: checkpoint_ref.into(),
    }
}

fn warning(task_id: &str, ts: &str) -> TaskEvent {
    TaskEvent::Warning {
        ts: ts.into(),
        source: TaskEventSource::AgentLoop,
        task_id: task_id.into(),
        code: "model_stall".into(),
        message: "model stalled once".into(),
    }
}

fn write_jsonl(path: &Path, records: &[RolloutRecord], extra_lines: &[&str]) {
    let mut body = String::new();
    for record in records {
        body.push_str(&serde_json::to_string(record).unwrap());
        body.push('\n');
    }
    for line in extra_lines {
        body.push_str(line);
        body.push('\n');
    }
    std::fs::write(path, body).unwrap();
}

#[test]
fn completed_task_from_started_model_finished() {
    let records = vec![
        record(
            1,
            started("task-1", "intent-1", "2026-05-23T01:00:00Z"),
            None,
        ),
        record(
            2,
            model_turn("task-1", "2026-05-23T01:00:01Z"),
            Some("intent-1"),
        ),
        record(
            3,
            finished(
                "task-1",
                "2026-05-23T01:00:02Z",
                TaskVerdict::Completed {
                    summary: Some("done".into()),
                },
            ),
            Some("intent-1"),
        ),
    ];

    let stub = SessionProjectionStub::from_records(&records, "2026-05-23T02:00:00Z");
    assert_eq!(stub.schema_version, PROJECTION_SCHEMA_VERSION);
    assert_eq!(stub.last_sequence, 3);
    assert_eq!(stub.tasks.len(), 1);
    let task = &stub.tasks[0];
    assert_eq!(task.task_id, "task-1");
    assert_eq!(task.intent_id.as_deref(), Some("intent-1"));
    assert_eq!(task.status, TaskProjectionStatus::Completed);
    assert!(task.is_terminal);
    assert_eq!(task.event_count, 3);
    assert_eq!(task.last_sequence, 3);
    assert_eq!(task.first_ts.as_deref(), Some("2026-05-23T01:00:00Z"));
    assert_eq!(task.last_ts.as_deref(), Some("2026-05-23T01:00:02Z"));
    assert_eq!(task.last_kind.as_deref(), Some("task_finished"));
}

#[test]
fn boundary_yield_is_waiting_and_not_terminal() {
    let records = vec![
        record(
            1,
            started("task-2", "intent-2", "2026-05-23T01:00:00Z"),
            None,
        ),
        record(
            2,
            boundary_yield("task-2", "2026-05-23T01:00:01Z", "waiting for user"),
            Some("intent-2"),
        ),
    ];

    let stub = SessionProjectionStub::from_records(&records, "2026-05-23T02:00:00Z");
    let task = &stub.tasks[0];
    assert_eq!(task.status, TaskProjectionStatus::Waiting);
    assert_eq!(task.boundary_reason.as_deref(), Some("waiting for user"));
    assert!(!task.is_terminal);
    assert_eq!(task.last_kind.as_deref(), Some("boundary_yield"));
}

#[test]
fn checkpoint_marks_checkpointed_and_stores_ref() {
    let records = vec![
        record(
            1,
            started("task-3", "intent-3", "2026-05-23T01:00:00Z"),
            None,
        ),
        record(
            2,
            checkpoint("task-3", "2026-05-23T01:00:01Z", "checkpoint-123"),
            Some("intent-3"),
        ),
    ];

    let stub = SessionProjectionStub::from_records(&records, "2026-05-23T02:00:00Z");
    let task = &stub.tasks[0];
    assert_eq!(task.status, TaskProjectionStatus::Checkpointed);
    assert_eq!(task.checkpoint_ref.as_deref(), Some("checkpoint-123"));
    assert_eq!(task.source, TaskEventSource::Browser);
    assert!(!task.is_terminal);
}

#[test]
fn projection_sequence_tracks_rollout_order_across_tasks() {
    let records = vec![
        record(
            1,
            started("task-a", "intent-a", "2026-05-23T01:00:00Z"),
            None,
        ),
        record(
            1,
            started("task-b", "intent-b", "2026-05-23T01:00:01Z"),
            None,
        ),
        record(
            2,
            model_turn("task-a", "2026-05-23T01:00:02Z"),
            Some("intent-a"),
        ),
    ];

    let store = ProjectionJournalStore::new("/tmp/projection-pr5");
    let stub = SessionProjectionStub::from_records(&records, "2026-05-23T02:00:00Z");
    let entries = store.derive_journal_entries(&records);

    assert_eq!(stub.last_sequence, 3);
    assert_eq!(stub.tasks.len(), 2);
    assert_eq!(stub.tasks[0].task_id, "task-a");
    assert_eq!(stub.tasks[0].last_sequence, 3);
    assert_eq!(stub.tasks[1].task_id, "task-b");
    assert_eq!(stub.tasks[1].last_sequence, 2);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn malformed_rollout_jsonl_does_not_panic_and_counts_lines() {
    let temp = TempDir::new().unwrap();
    let rollout = temp.path().join("rollout.jsonl");
    let records = vec![record(
        1,
        started("task-4", "intent-4", "2026-05-23T01:00:00Z"),
        Some("intent-4"),
    )];
    write_jsonl(&rollout, &records, &["{not valid json", "   "]);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    file.write_all(b"\xff\xfe\n").unwrap();

    let store = ProjectionJournalStore::new(temp.path().join("projection"));
    let stub = store
        .build_stub_from_rollout_jsonl(&rollout, "2026-05-23T02:00:00Z")
        .unwrap();

    assert_eq!(stub.malformed_line_count, 2);
    assert_eq!(stub.tasks.len(), 1);
    assert_eq!(stub.source_rollout_file, rollout);
    assert_eq!(stub.tasks[0].source_rollout_file, stub.source_rollout_file);
}

#[test]
fn stub_json_roundtrip_is_compact() {
    let temp = TempDir::new().unwrap();
    let store = ProjectionJournalStore::new(temp.path());
    let records = vec![
        record(
            1,
            started("task-5", "intent-5", "2026-05-23T01:00:00Z"),
            None,
        ),
        record(
            2,
            model_turn("task-5", "2026-05-23T01:00:01Z"),
            Some("intent-5"),
        ),
        record(
            3,
            warning("task-5", "2026-05-23T01:00:02Z"),
            Some("intent-5"),
        ),
    ];
    let stub = SessionProjectionStub::from_records(&records, "2026-05-23T02:00:00Z");

    store.write_stub(&stub).unwrap();
    let json = std::fs::read_to_string(store.stub_path_for_rollout(&rollout_file())).unwrap();
    assert!(!json.contains("tokenUsage"));
    assert!(!json.contains("inputTokens"));
    assert!(!json.contains("provider"));

    let roundtrip = store.read_stub(&rollout_file()).unwrap();
    assert_eq!(roundtrip, stub);
    assert_eq!(roundtrip.tasks[0].warning_count, 1);
}

#[test]
fn journal_entry_roundtrip_uses_lossy_reader() {
    let temp = TempDir::new().unwrap();
    let store = ProjectionJournalStore::new(temp.path());
    let records = vec![
        record(
            1,
            started("task-6", "intent-6", "2026-05-23T01:00:00Z"),
            None,
        ),
        record(
            2,
            boundary_yield("task-6", "2026-05-23T01:00:01Z", "waiting for user"),
            Some("intent-6"),
        ),
        record(
            3,
            finished(
                "task-6",
                "2026-05-23T01:00:02Z",
                TaskVerdict::Failed {
                    error_code: "boom".into(),
                    message: "failed".into(),
                },
            ),
            Some("intent-6"),
        ),
    ];
    let entries = store.derive_journal_entries(&records);

    store
        .append_journal_entries(&rollout_file(), &entries)
        .unwrap();
    let journal_path = store.journal_path_for_rollout(&rollout_file());
    let mut body = std::fs::read_to_string(&journal_path).unwrap();
    body.push_str("{malformed\n\n");
    std::fs::write(&journal_path, body).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .unwrap();
    file.write_all(b"\xff\xfe\n").unwrap();

    let roundtrip = store.read_journal_entries_lossy(&rollout_file()).unwrap();
    assert_eq!(roundtrip, entries);
    assert_eq!(roundtrip[1].status, TaskProjectionStatus::Waiting);
    assert_eq!(
        roundtrip[1].boundary_reason.as_deref(),
        Some("waiting for user")
    );
    assert_eq!(roundtrip[2].status, TaskProjectionStatus::Failed);
    assert!(roundtrip[2].is_terminal);
}
