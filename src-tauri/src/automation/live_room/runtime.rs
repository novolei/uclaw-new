use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::automation::runtime::service::AppRuntimeService;
use crate::browser::identity::BrowserAuthProfileBroker;

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
        let errors = if self.error_kinds.is_empty() {
            "- none".to_string()
        } else {
            self.error_kinds
                .iter()
                .map(|kind| format!("- `{kind}`"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "# Live Room Run Report\n\n- Platform: `{}`\n- Room: `{}`\n- Stop reason: `{}`\n- Comments scanned: {}\n- Replies sent: {}\n- Warnings: {}\n- Mutes: {}\n- Removals: {}\n- gbrain recalls: {}\n- gbrain writes: {}\n\n## gbrain pages\n{}\n\n## errors\n{}\n",
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
            slugs,
            errors
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

    let mut report = LiveRunReport {
        platform: platform.clone(),
        room_id: room_id.clone(),
        room_title: None,
        live_url: live_url.clone(),
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
        error_kinds: Vec::new(),
    };
    let result =
        run_live_room_adapter_loop(service, spec_id, &activity_id, &spec_value, &mut report).await;
    if let Err(error) = result {
        report.stop_reason = LiveStopReason::FatalAdapterError;
        report.error_kinds.push(error.to_string());
    }
    report.ended_at_ms = chrono::Utc::now().timestamp_millis();
    persist_final_report(&service.db, &activity_id, &report).await?;
    let (status, error_text) = match report.stop_reason {
        LiveStopReason::FatalAdapterError
        | LiveStopReason::LoginRequired
        | LiveStopReason::Blocked
        | LiveStopReason::InsufficientPermissions => (
            "failed",
            report
                .error_kinds
                .first()
                .map(String::as_str)
                .or(Some("live_room_adapter_error")),
        ),
        LiveStopReason::UserStopped | LiveStopReason::RoomEnded => ("completed", None),
    };
    {
        let conn = service
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        conn.execute(
            "UPDATE automation_activities
             SET status = ?2, error_text = ?3, completed_at = ?4
             WHERE id = ?1 AND status != 'cancelled'",
            rusqlite::params![activity_id, status, error_text, report.ended_at_ms],
        )?;
        crate::automation::runtime::run_session::persist_transcript(
            &conn,
            &session_id,
            &[crate::agent::types::ChatMessage::user(&format!(
                "Live-room automation `{}` ended with {:?}.",
                spec_id, report.stop_reason
            ))],
        )
        .ok();
    }
    if let Some(ctx_mgr) = service.browser_context_manager.as_ref() {
        ctx_mgr
            .destroy(
                &crate::automation::runtime::service::automation_browser_session_id(
                    spec_id,
                    &activity_id,
                ),
            )
            .await;
    }
    Ok(())
}

async fn run_live_room_adapter_loop(
    service: &AppRuntimeService,
    spec_id: &str,
    activity_id: &str,
    spec_value: &serde_json::Value,
    report: &mut LiveRunReport,
) -> anyhow::Result<()> {
    if report.platform != "douyin" {
        anyhow::bail!("unsupported_live_room_platform:{}", report.platform);
    }
    if report.live_url.trim().is_empty() {
        anyhow::bail!("missing_live_url");
    }
    let ctx_mgr = service
        .browser_context_manager
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("browser_context_manager_unavailable"))?;
    let app_handle = ctx_mgr.app_handle();
    let browser_session_id =
        crate::automation::runtime::service::automation_browser_session_id(spec_id, activity_id);
    let ctx = ctx_mgr.get_or_create(&browser_session_id).await?;
    let mut tab_id = ctx.navigate("new", &report.live_url, app_handle).await?;

    if let Some(profile_id) = configured_auth_profile_id(spec_value) {
        let broker = BrowserAuthProfileBroker::system_default()
            .map_err(|e| anyhow::anyhow!("browser_auth_profile_broker:{e}"))?;
        let (_profile, state) = broker
            .load_storage_state_for_profile(&profile_id)
            .map_err(|e| anyhow::anyhow!("browser_auth_profile_load:{e}"))?;
        ctx.apply_storage_state(&tab_id, &state, app_handle).await?;
        tab_id = ctx.navigate(&tab_id, &report.live_url, app_handle).await?;
    } else {
        anyhow::bail!("browser_auth_profile_missing");
    }

    let script_root = service.browser_builtin_root.join(&report.platform);
    let enter = run_script(
        &ctx,
        &tab_id,
        &script_root,
        "enter_room.js",
        serde_json::json!({
            "configuredRoomId": report.room_id,
            "liveUrl": report.live_url,
        }),
    )
    .await?;
    report.room_title = enter
        .get("roomTitle")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let runtime = LiveRuntimeMetadata::from_spec_json(spec_value).unwrap_or(LiveRuntimeMetadata {
        kind: "live_room_moderator".into(),
        poll_interval_seconds: 30,
    });
    let poll_interval = runtime.poll_interval_seconds.clamp(5, 300);
    let mut state = LiveRunState {
        platform: report.platform.clone(),
        room_id: report.room_id.clone(),
        host_id: enter
            .get("hostId")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        tab_id: Some(tab_id.clone()),
        ..LiveRunState::default()
    };

    loop {
        if activity_cancelled(&service.db, activity_id)? {
            report.stop_reason = LiveStopReason::UserStopped;
            break;
        }

        let raw_status = run_script(
            &ctx,
            &tab_id,
            &script_root,
            "check_room_status.js",
            serde_json::json!({}),
        )
        .await?;
        let room_status = parse_room_status(raw_status)?;
        if let Some(reason) = should_stop_for_room_status(&room_status, &mut state) {
            report.stop_reason = reason;
            if let Some(detail) = room_status.reason {
                report.error_kinds.push(detail);
            }
            break;
        }

        let raw_comments = run_script(
            &ctx,
            &tab_id,
            &script_root,
            "scan_comments.js",
            serde_json::json!({ "cursor": state.comment_cursor }),
        )
        .await?;
        let batch =
            crate::automation::live_room::adapters::douyin::parse_scan_comments(raw_comments)
                .map_err(|e| anyhow::anyhow!("scan_comments_parse:{e}"))?;
        report.comments_scanned = report
            .comments_scanned
            .saturating_add(batch.comments.len() as u64);
        state.comment_cursor = batch.next_cursor;
        state.last_tick_ms = Some(chrono::Utc::now().timestamp_millis());

        tokio::time::sleep(Duration::from_secs(poll_interval)).await;
    }

    Ok(())
}

async fn run_script(
    ctx: &crate::browser::context::BrowserContext,
    tab_id: &str,
    script_root: &std::path::Path,
    script_name: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let path = script_root.join(script_name);
    let source = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("read_live_room_script:{}:{e}", path.display()))?;
    ctx.evaluate_script_with_params(tab_id, &source, params, 15_000)
        .await
        .map_err(|e| anyhow::anyhow!("run_live_room_script:{script_name}:{e}"))
}

fn configured_auth_profile_id(spec_value: &serde_json::Value) -> Option<String> {
    let config = spec_value.get("config")?;
    let profiles = config.get("browser_login_profiles")?.as_object()?;
    profiles.values().find_map(|value| {
        if value.get("status").and_then(|v| v.as_str()) == Some("live") {
            value
                .get("profileId")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        } else {
            None
        }
    })
}

fn parse_room_status(raw: serde_json::Value) -> anyhow::Result<LiveRoomStatus> {
    Ok(LiveRoomStatus {
        status: raw
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        signals: raw
            .get("signals")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        reason: raw
            .get("reason")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .map(str::to_string),
    })
}

fn activity_cancelled(
    db: &Arc<StdMutex<rusqlite::Connection>>,
    activity_id: &str,
) -> anyhow::Result<bool> {
    let conn = db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
    conn.query_row(
        "SELECT status = 'cancelled' FROM automation_activities WHERE id = ?1",
        rusqlite::params![activity_id],
        |row| row.get::<_, bool>(0),
    )
    .map_err(|e| anyhow::anyhow!("activity status lookup: {e}"))
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
    fn final_report_includes_adapter_error_details() {
        let report = LiveRunReport {
            platform: "douyin".into(),
            room_id: "room-1".into(),
            room_title: None,
            live_url: "https://live.douyin.com/room-1".into(),
            started_at_ms: 1000,
            ended_at_ms: 2000,
            stop_reason: LiveStopReason::FatalAdapterError,
            comments_scanned: 0,
            replies_sent: 0,
            warnings_sent: 0,
            mutes_executed: 0,
            removals_executed: 0,
            gbrain_recalls: 0,
            gbrain_writes: 0,
            gbrain_slugs: Vec::new(),
            error_kinds: vec!["browser_auth_profile_missing".into()],
        };
        let text = report.to_markdown();
        assert!(text.contains("## errors"));
        assert!(text.contains("browser_auth_profile_missing"));
    }

    #[test]
    fn configured_auth_profile_prefers_live_profile() {
        let spec = serde_json::json!({
            "config": {
                "browser_login_profiles": {
                    "https://www.douyin.com/": {
                        "status": "expired",
                        "profileId": "old-profile"
                    },
                    "https://live.douyin.com/": {
                        "status": "live",
                        "profileId": "live-profile"
                    }
                }
            }
        });
        assert_eq!(
            configured_auth_profile_id(&spec).as_deref(),
            Some("live-profile")
        );
    }

    #[test]
    fn parse_room_status_keeps_signals_and_reason() {
        let status = parse_room_status(serde_json::json!({
            "status": "ended",
            "signals": ["ended_text", "no_comment_input"],
            "reason": "room ended text detected"
        }))
        .unwrap();
        assert_eq!(status.status, "ended");
        assert_eq!(status.signals, vec!["ended_text", "no_comment_input"]);
        assert_eq!(status.reason.as_deref(), Some("room ended text detected"));
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
        let db = Arc::new(StdMutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
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
