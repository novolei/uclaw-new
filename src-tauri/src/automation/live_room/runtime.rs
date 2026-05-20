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
        let poll_interval_seconds = runtime
            .get("poll_interval_seconds")
            .and_then(|v| v.as_u64())
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
}
