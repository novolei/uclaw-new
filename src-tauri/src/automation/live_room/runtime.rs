use std::sync::{Arc, Mutex as StdMutex};

use crate::automation::runtime::service::AppRuntimeService;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiveRunKey {
    pub spec_id: String,
    pub run_id: String,
    pub platform: String,
    pub room_id: String,
}

impl LiveRunKey {
    pub fn new(spec_id: &str, run_id: &str, platform: &str, room_id: &str) -> Self {
        Self {
            spec_id: spec_id.to_string(),
            run_id: run_id.to_string(),
            platform: platform.to_string(),
            room_id: room_id.to_string(),
        }
    }
}

impl std::fmt::Display for LiveRunKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}",
            self.spec_id, self.run_id, self.platform, self.room_id
        )
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LiveRunState {
    pub platform: String,
    pub room_id: String,
    pub host_id: Option<String>,
    pub tab_id: Option<String>,
    pub comment_cursor: Option<String>,
    pub last_tick_ms: Option<i64>,
    pub ended_signal_count: u8,
    pub stop_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRoomStatus {
    pub status: String,
    pub signals: Vec<String>,
    pub reason: Option<String>,
}

pub fn should_stop_for_room_status(
    status: &LiveRoomStatus,
    state: &mut LiveRunState,
) -> Option<LiveStopReason> {
    match status.status.as_str() {
        "ended" => {
            state.ended_signal_count = state.ended_signal_count.saturating_add(1);
            if state.ended_signal_count >= 2 {
                Some(LiveStopReason::RoomEnded)
            } else {
                None
            }
        }
        "login_required" => Some(LiveStopReason::LoginRequired),
        "blocked" => Some(LiveStopReason::Blocked),
        _ => {
            state.ended_signal_count = 0;
            None
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveRuntimeMetadata {
    pub kind: String,
    pub poll_interval_seconds: u64,
}

impl LiveRuntimeMetadata {
    pub fn from_spec_json(spec: &serde_json::Value) -> Result<Self, String> {
        let runtime = spec
            .get("x_uclaw_runtime")
            .ok_or_else(|| "missing_live_runtime_metadata".to_string())?;
        let kind = runtime
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing_live_runtime_kind".to_string())?
            .to_string();
        let poll_interval_seconds = spec
            .get("config")
            .and_then(|v| v.get("poll_interval_seconds"))
            .and_then(|v| v.as_u64())
            .or_else(|| {
                runtime
                    .get("poll_interval_seconds")
                    .and_then(|v| v.as_u64())
            })
            .unwrap_or(30);
        Ok(Self {
            kind,
            poll_interval_seconds,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveStopReason {
    UserStopped,
    RoomEnded,
    LoginRequired,
    InsufficientPermissions,
    Blocked,
    FatalAdapterError,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveRunReport {
    pub platform: String,
    pub room_id: String,
    pub room_title: Option<String>,
    pub live_url: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub stop_reason: LiveStopReason,
    pub comments_scanned: u64,
    pub replies_sent: u64,
    pub warnings_sent: u64,
    pub mutes_executed: u64,
    pub removals_executed: u64,
    pub gbrain_recalls: u64,
    pub gbrain_writes: u64,
    pub gbrain_slugs: Vec<String>,
    pub error_kinds: Vec<String>,
}

impl LiveRunReport {
    pub fn to_markdown(&self) -> String {
        let stop_reason = serde_json::to_value(self.stop_reason)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let slugs = if self.gbrain_slugs.is_empty() {
            "- none".to_string()
        } else {
            self.gbrain_slugs
                .iter()
                .map(|slug| format!("- `{slug}`"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "# Live Room Run Report\n\n- Platform: `{}`\n- Room: `{}`\n- Stop reason: `{}`\n- Comments scanned: {}\n- Replies sent: {}\n- Warnings: {}\n- Mutes: {}\n- Removals: {}\n- gbrain recalls: {}\n- gbrain writes: {}\n\n## gbrain pages\n{}\n",
            self.platform,
            self.room_id,
            stop_reason,
            self.comments_scanned,
            self.replies_sent,
            self.warnings_sent,
            self.mutes_executed,
            self.removals_executed,
            self.gbrain_recalls,
            self.gbrain_writes,
            slugs
        )
    }
}

pub async fn execute_live_room_run(
    service: &AppRuntimeService,
    spec_id: &str,
    activity_id: String,
    session_id: String,
    spec_value: serde_json::Value,
    payload: serde_json::Value,
    _workspace_root: std::path::PathBuf,
) -> anyhow::Result<()> {
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let platform = live_value(&spec_value, &payload, "platform").unwrap_or_else(|| "douyin".into());
    let room_id = live_value(&spec_value, &payload, "room_id")
        .or_else(|| live_value(&spec_value, &payload, "roomId"))
        .unwrap_or_else(|| "unknown-room".into());
    let live_url = live_value(&spec_value, &payload, "live_url")
        .or_else(|| live_value(&spec_value, &payload, "liveUrl"))
        .unwrap_or_default();

    let report = LiveRunReport {
        platform,
        room_id,
        room_title: None,
        live_url,
        started_at_ms,
        ended_at_ms: chrono::Utc::now().timestamp_millis(),
        stop_reason: LiveStopReason::FatalAdapterError,
        comments_scanned: 0,
        replies_sent: 0,
        warnings_sent: 0,
        mutes_executed: 0,
        removals_executed: 0,
        gbrain_recalls: 0,
        gbrain_writes: 0,
        gbrain_slugs: Vec::new(),
        error_kinds: vec!["live_room_executor_not_connected".to_string()],
    };
    persist_final_report(&service.db, &activity_id, &report).await?;
    {
        let conn = service
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        conn.execute(
            "UPDATE automation_activities
             SET status = 'failed', error_text = ?2, completed_at = ?3
             WHERE id = ?1 AND status != 'cancelled'",
            rusqlite::params![
                activity_id,
                "live_room_executor_not_connected",
                report.ended_at_ms
            ],
        )?;
        crate::automation::runtime::run_session::persist_transcript(
            &conn,
            &session_id,
            &[crate::agent::types::ChatMessage::user(&format!(
                "Live-room automation `{}` ended before adapter execution was connected.",
                spec_id
            ))],
        )
        .ok();
    }
    Ok(())
}

pub async fn persist_final_report(
    db: &Arc<StdMutex<rusqlite::Connection>>,
    activity_id: &str,
    report: &LiveRunReport,
) -> anyhow::Result<()> {
    let text = report.to_markdown();
    let artifacts = serde_json::json!([{
        "kind": "live_room_report",
        "platform": report.platform,
        "room_id": report.room_id,
        "stop_reason": report.stop_reason,
    }]);
    let conn = db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
    conn.execute(
        "UPDATE automation_activities
         SET report_text = ?1, report_artifacts_json = ?2
         WHERE id = ?3 AND status != 'cancelled'",
        rusqlite::params![text, artifacts.to_string(), activity_id],
    )?;
    Ok(())
}

fn live_value(
    spec_value: &serde_json::Value,
    payload: &serde_json::Value,
    key: &str,
) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .or_else(|| {
            spec_value
                .get("config")
                .and_then(|v| v.get(key))
                .and_then(|v| v.as_str())
        })
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_key_includes_spec_run_platform_and_room() {
        let key = LiveRunKey::new("spec-a", "run-1", "douyin", "room-9");
        assert_eq!(key.to_string(), "spec-a:run-1:douyin:room-9");
    }

    #[test]
    fn different_rooms_do_not_share_keys() {
        let a = LiveRunKey::new("spec-a", "run-1", "douyin", "room-a");
        let b = LiveRunKey::new("spec-a", "run-1", "douyin", "room-b");
        assert_ne!(a, b);
    }

    #[test]
    fn live_runtime_metadata_is_explicit() {
        let spec = serde_json::json!({
            "x_uclaw_runtime": {
                "kind": "live_room_moderator",
                "poll_interval_seconds": 30
            }
        });
        let runtime = LiveRuntimeMetadata::from_spec_json(&spec).unwrap();
        assert_eq!(runtime.kind, "live_room_moderator");
        assert_eq!(runtime.poll_interval_seconds, 30);
    }

    #[test]
    fn live_runtime_metadata_prefers_config_poll_interval_override() {
        let spec = serde_json::json!({
            "x_uclaw_runtime": {
                "kind": "live_room_moderator",
                "poll_interval_seconds": 30
            },
            "config": {
                "poll_interval_seconds": 12
            }
        });
        let runtime = LiveRuntimeMetadata::from_spec_json(&spec).unwrap();
        assert_eq!(runtime.poll_interval_seconds, 12);
    }

    #[test]
    fn final_report_contains_terminal_reason_and_counts() {
        let report = LiveRunReport {
            platform: "douyin".into(),
            room_id: "room-1".into(),
            room_title: Some("Launch Room".into()),
            live_url: "https://www.douyin.com/live/room-1".into(),
            started_at_ms: 1000,
            ended_at_ms: 61000,
            stop_reason: LiveStopReason::RoomEnded,
            comments_scanned: 42,
            replies_sent: 3,
            warnings_sent: 2,
            mutes_executed: 1,
            removals_executed: 0,
            gbrain_recalls: 4,
            gbrain_writes: 2,
            gbrain_slugs: vec!["live/douyin/room-1/facts/topic".into()],
            error_kinds: vec![],
        };
        let text = report.to_markdown();
        assert!(text.contains("Stop reason: `room_ended`"));
        assert!(text.contains("Comments scanned: 42"));
        assert!(text.contains("live/douyin/room-1/facts/topic"));
    }

    #[test]
    fn room_ended_requires_two_consecutive_signals() {
        let mut state = LiveRunState::default();
        let ended = LiveRoomStatus {
            status: "ended".into(),
            signals: vec!["ended_text".into(), "no_comment_input".into()],
            reason: Some("room ended".into()),
        };
        assert_eq!(should_stop_for_room_status(&ended, &mut state), None);
        assert_eq!(
            should_stop_for_room_status(&ended, &mut state),
            Some(LiveStopReason::RoomEnded)
        );
    }

    #[test]
    fn live_status_resets_ended_signal_count() {
        let mut state = LiveRunState {
            ended_signal_count: 1,
            ..LiveRunState::default()
        };
        let live = LiveRoomStatus {
            status: "live".into(),
            signals: vec![],
            reason: None,
        };
        assert_eq!(should_stop_for_room_status(&live, &mut state), None);
        assert_eq!(state.ended_signal_count, 0);
    }

    #[tokio::test]
    async fn persist_final_report_updates_activity_report_fields() {
        let db = Arc::new(StdMutex::new(rusqlite::Connection::open_in_memory().unwrap()));
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "CREATE TABLE automation_activities (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL DEFAULT 'running',
                    report_text TEXT,
                    report_artifacts_json TEXT NOT NULL DEFAULT '[]'
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO automation_activities (id, report_artifacts_json) VALUES ('a1', '[]')",
                [],
            )
            .unwrap();
        }
        let report = LiveRunReport {
            platform: "douyin".into(),
            room_id: "room-1".into(),
            room_title: None,
            live_url: "".into(),
            started_at_ms: 1000,
            ended_at_ms: 2000,
            stop_reason: LiveStopReason::UserStopped,
            comments_scanned: 1,
            replies_sent: 0,
            warnings_sent: 0,
            mutes_executed: 0,
            removals_executed: 0,
            gbrain_recalls: 0,
            gbrain_writes: 0,
            gbrain_slugs: Vec::new(),
            error_kinds: Vec::new(),
        };
        persist_final_report(&db, "a1", &report).await.unwrap();
        let conn = db.lock().unwrap();
        let (text, artifacts): (String, String) = conn
            .query_row(
                "SELECT report_text, report_artifacts_json FROM automation_activities WHERE id='a1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(text.contains("Stop reason: `user_stopped`"));
        assert!(artifacts.contains("live_room_report"));
    }
}
