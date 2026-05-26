//! Bundle 27-A + 27-C — Reply recovery after unclean shutdown.
//!
//! On boot, if Bundle 27-C reports `PreviousShutdown::Unclean` AND
//! Bundle 27-A's flight-recorder file (`last_active_run.json`) is
//! still present, the previous process died mid-agent-run with
//! buffered streamed text that never reached chat history. This
//! module is the bridge: read the flight record + persist its
//! `partial_text` as an `[interrupted-recovered]` assistant message
//! so the user actually sees what the agent was saying when it died,
//! and the next "继续" command has the same context as if the run
//! had completed normally.
//!
//! Only emits an event when there's something meaningful to recover —
//! empty partial buffers (process died before LLM produced any text)
//! are not noisy.

use std::path::Path;

use serde::Serialize;
use tauri::Emitter;

use crate::agent::heartbeat::FlightRecord;
use crate::observability::shutdown::ProcessLock;

/// Summary returned to the caller (main.rs) for logging.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryReport {
    pub recovered: bool,
    pub conversation_id: Option<String>,
    pub iteration: Option<u32>,
    pub stage: Option<String>,
    pub partial_chars: usize,
    pub dead_pid: Option<u32>,
}

impl RecoveryReport {
    pub fn none() -> Self {
        Self {
            recovered: false,
            conversation_id: None,
            iteration: None,
            stage: None,
            partial_chars: 0,
            dead_pid: None,
        }
    }
}

/// Inspect the flight record (and Bundle 27-C's prior ProcessLock for
/// PID context) and emit an `agent:interrupted-recovered` event with
/// enough payload for the UI to render a "上一轮被中断" banner +
/// inline the recovered text as a new assistant message.
///
/// Persistence of the recovered message into the agent_messages
/// table is intentionally deferred to a follow-up: it requires
/// access to the DB and session_manager state which is awkward at
/// the boot-time call site. The event payload carries everything the
/// UI needs to display the message immediately; the next user turn
/// will compact-fold it into history alongside other messages.
pub fn recover_unclean_shutdown<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    flight_path: &Path,
    dead_lock: Option<&ProcessLock>,
    pending_recovery_store: Option<std::sync::Arc<std::sync::Mutex<Option<serde_json::Value>>>>,
) -> std::io::Result<RecoveryReport> {
    let record = match crate::agent::heartbeat::read_flight(flight_path)? {
        Some(r) => r,
        None => {
            tracing::info!("[Bundle 27-A] no flight record on disk — nothing to recover");
            return Ok(RecoveryReport::none());
        }
    };

    // Only treat as recoverable if the record actually belongs to the
    // dead process (PID cross-check). A stray flight file from a
    // long-ago boot is conservatively ignored — emitting a stale
    // recovery banner would be worse than missing one.
    if let Some(lock) = dead_lock {
        if record.pid != lock.pid {
            tracing::warn!(
                flight_pid = record.pid,
                lock_pid = lock.pid,
                "[Bundle 27-A] flight record PID mismatch with process lock — ignoring stale flight"
            );
            // Clean it up so it doesn't false-positive on the NEXT boot.
            crate::agent::heartbeat::clear_flight(flight_path);
            return Ok(RecoveryReport::none());
        }
    }

    // Bundle 27-A2 fix (2026-05-22) — surface the banner EVEN WHEN
    // partial_text is empty. The user's primary signal is "my last run
    // didn't complete cleanly" — that's valuable on its own. When the
    // user kills during a tool-call phase (before any LLM text has
    // streamed), partial_text IS empty, but the user still benefits
    // from knowing where the agent stopped (which tool / which
    // iteration). Synthesize a short status note for the partial_text
    // payload in that case so the existing UI banner still has
    // something meaningful to render.
    let partial_chars = record.partial_text.chars().count();
    let partial_text_for_payload = if partial_chars > 0 {
        record.partial_text.clone()
    } else {
        format!(
            "⚠️ 上一轮 Agent 任务被异常中断 (iter={}, stage={})。LLM 还没开始流式输出，无文本可恢复 — 重新发送即可。",
            record.iteration, record.stage
        )
    };

    tracing::warn!(
        conversation_id = %record.conversation_id,
        iteration = record.iteration,
        stage = %record.stage,
        partial_chars = partial_chars,
        "[Bundle 27-A] recovering interrupted run (partial_chars={partial_chars})"
    );

    let payload = serde_json::json!({
        "conversationId": record.conversation_id,
        "spaceId": record.space_id,
        "iteration": record.iteration,
        "stage": record.stage,
        "startedAt": record.started_at,
        "lastActivityAt": record.last_activity_at,
        "partialText": partial_text_for_payload,
        "partialChars": partial_chars,
        "deadPid": record.pid,
    });
    // Bundle 27-A2 pull-model — store in AppState so UI can fetch on
    // mount via `consume_pending_recovery` Tauri command. The push-via-
    // event below is still emitted as belt-and-suspenders (covers the
    // case where the listener was already registered when boot
    // happened, e.g. when dev runs are restarted quickly).
    if let Some(store) = pending_recovery_store {
        if let Ok(mut guard) = store.lock() {
            *guard = Some(payload.clone());
            tracing::info!("[Bundle 27-A2] pending_recovery payload stored in AppState");
        }
    }
    let _ = app_handle.emit("agent:interrupted-recovered", &payload);

    // Clear the file so we don't re-recover the same record on the
    // next boot.
    crate::agent::heartbeat::clear_flight(flight_path);

    Ok(RecoveryReport {
        recovered: true,
        conversation_id: Some(record.conversation_id),
        iteration: Some(record.iteration),
        stage: Some(record.stage),
        partial_chars,
        dead_pid: Some(record.pid),
    })
}

/// Bundle 27-A — assemble the FULL FlightRecord-shaped payload above.
/// Exported as a synchronous helper for use sites that already
/// hold the FlightRecord (tests, future plan-mode recovery).
pub fn report_from_record(rec: &FlightRecord) -> RecoveryReport {
    RecoveryReport {
        recovered: true,
        conversation_id: Some(rec.conversation_id.clone()),
        iteration: Some(rec.iteration),
        stage: Some(rec.stage.clone()),
        partial_chars: rec.partial_text.chars().count(),
        dead_pid: Some(rec.pid),
    }
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::heartbeat::FlightRecord;

    fn sample_record(partial: &str) -> FlightRecord {
        FlightRecord {
            schema_version: 1,
            conversation_id: "c1".into(),
            space_id: "default".into(),
            iteration: 3,
            stage: "llm_stream".into(),
            started_at: 1,
            last_activity_at: 2,
            partial_text: partial.into(),
            pid: 4321,
        }
    }

    #[test]
    fn report_from_record_captures_partial_chars() {
        let rec = sample_record("hello 你好");
        let report = report_from_record(&rec);
        assert!(report.recovered);
        assert_eq!(report.conversation_id.as_deref(), Some("c1"));
        assert_eq!(report.iteration, Some(3));
        assert_eq!(report.stage.as_deref(), Some("llm_stream"));
        // "hello 你好" = 5 ASCII + 1 space + 2 CJK = 8 chars
        assert_eq!(report.partial_chars, 8);
        assert_eq!(report.dead_pid, Some(4321));
    }

    #[test]
    fn none_helper_returns_inactive_report() {
        let r = RecoveryReport::none();
        assert!(!r.recovered);
        assert!(r.conversation_id.is_none());
        assert_eq!(r.partial_chars, 0);
    }
}
