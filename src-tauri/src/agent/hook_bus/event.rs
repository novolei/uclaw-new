//! `HookEvent` — the 13 events the agent emits across a turn.

use serde::{Deserialize, Serialize};

/// Lifecycle phase the hook bus is firing. Every variant carries
/// just enough context for subscribers to make a decision (or log)
/// without requiring access to the full SessionTask.
///
/// `task_id` is on every variant so observability tooling can group
/// events by owning task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookEvent {
    PreToolUse {
        task_id: String,
        tool_name: String,
        args_json: String,
    },
    PostToolUse {
        task_id: String,
        tool_name: String,
        success: bool,
        result_preview: String,
    },
    PreLlmCall {
        task_id: String,
        provider: String,
        model: String,
        prompt_tokens_estimate: usize,
    },
    PostLlmCall {
        task_id: String,
        provider: String,
        model: String,
        input_tokens: u64,
        output_tokens: u64,
    },
    PrePermission {
        task_id: String,
        action: String,
        target: String,
    },
    PostPermission {
        task_id: String,
        action: String,
        granted: bool,
    },
    PreContextInject {
        task_id: String,
        fragment_ids: Vec<String>,
        total_tokens_estimate: usize,
    },
    PostContextInject {
        task_id: String,
        fragments_injected: usize,
        tokens_used: usize,
    },
    TaskStart {
        task_id: String,
        intent_id: String,
    },
    TaskEnd {
        task_id: String,
        outcome: String, // matches TaskVerdict::* discriminant string
    },
    MemoryWrite {
        task_id: String,
        topic: String,
        size_bytes: usize,
    },
    MemoryRecall {
        task_id: String,
        query: String,
        hit_count: usize,
    },
    Checkpoint {
        task_id: String,
        checkpoint_id: String,
        /// Checkpoint flavor (auto / manual / boundary). Named with
        /// the `checkpoint_` prefix to avoid colliding with the
        /// enum's `#[serde(tag = "kind")]` discriminator.
        checkpoint_kind: String,
    },
}

/// Compact discriminant for indexing / filtering. Subscribers
/// declare `interest_in()` returning a slice of these so the bus can
/// short-circuit dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventKind {
    PreToolUse,
    PostToolUse,
    PreLlmCall,
    PostLlmCall,
    PrePermission,
    PostPermission,
    PreContextInject,
    PostContextInject,
    TaskStart,
    TaskEnd,
    MemoryWrite,
    MemoryRecall,
    Checkpoint,
}

impl HookEventKind {
    /// All 13 kinds in canonical order. Used by tests + UI to
    /// enumerate "what hooks exist".
    pub const ALL: [HookEventKind; 13] = [
        Self::PreToolUse,
        Self::PostToolUse,
        Self::PreLlmCall,
        Self::PostLlmCall,
        Self::PrePermission,
        Self::PostPermission,
        Self::PreContextInject,
        Self::PostContextInject,
        Self::TaskStart,
        Self::TaskEnd,
        Self::MemoryWrite,
        Self::MemoryRecall,
        Self::Checkpoint,
    ];

    /// `true` if subscribers can influence behaviour (return Deny /
    /// AskUser) for this event. `false` means the event is observe-
    /// only and `HookBus::dispatch_with_decision` always allows it.
    pub const fn is_decision_capable(self) -> bool {
        matches!(
            self,
            Self::PreToolUse
                | Self::PreLlmCall
                | Self::PrePermission
                | Self::PreContextInject
                | Self::MemoryWrite
        )
    }
}

impl HookEvent {
    pub fn kind(&self) -> HookEventKind {
        match self {
            Self::PreToolUse { .. } => HookEventKind::PreToolUse,
            Self::PostToolUse { .. } => HookEventKind::PostToolUse,
            Self::PreLlmCall { .. } => HookEventKind::PreLlmCall,
            Self::PostLlmCall { .. } => HookEventKind::PostLlmCall,
            Self::PrePermission { .. } => HookEventKind::PrePermission,
            Self::PostPermission { .. } => HookEventKind::PostPermission,
            Self::PreContextInject { .. } => HookEventKind::PreContextInject,
            Self::PostContextInject { .. } => HookEventKind::PostContextInject,
            Self::TaskStart { .. } => HookEventKind::TaskStart,
            Self::TaskEnd { .. } => HookEventKind::TaskEnd,
            Self::MemoryWrite { .. } => HookEventKind::MemoryWrite,
            Self::MemoryRecall { .. } => HookEventKind::MemoryRecall,
            Self::Checkpoint { .. } => HookEventKind::Checkpoint,
        }
    }

    pub fn task_id(&self) -> &str {
        match self {
            Self::PreToolUse { task_id, .. }
            | Self::PostToolUse { task_id, .. }
            | Self::PreLlmCall { task_id, .. }
            | Self::PostLlmCall { task_id, .. }
            | Self::PrePermission { task_id, .. }
            | Self::PostPermission { task_id, .. }
            | Self::PreContextInject { task_id, .. }
            | Self::PostContextInject { task_id, .. }
            | Self::TaskStart { task_id, .. }
            | Self::TaskEnd { task_id, .. }
            | Self::MemoryWrite { task_id, .. }
            | Self::MemoryRecall { task_id, .. }
            | Self::Checkpoint { task_id, .. } => task_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_13_kinds_present_and_distinct() {
        assert_eq!(HookEventKind::ALL.len(), 13);
        let mut sorted = HookEventKind::ALL.to_vec();
        sorted.sort_unstable_by_key(|k| format!("{k:?}"));
        sorted.dedup();
        assert_eq!(sorted.len(), 13, "all kinds must be distinct");
    }

    #[test]
    fn decision_capable_set_matches_adr() {
        let capable: Vec<_> = HookEventKind::ALL
            .iter()
            .filter(|k| k.is_decision_capable())
            .copied()
            .collect();
        assert_eq!(capable.len(), 5);
        assert!(capable.contains(&HookEventKind::PreToolUse));
        assert!(capable.contains(&HookEventKind::PreLlmCall));
        assert!(capable.contains(&HookEventKind::PrePermission));
        assert!(capable.contains(&HookEventKind::PreContextInject));
        assert!(capable.contains(&HookEventKind::MemoryWrite));
    }

    #[test]
    fn post_events_are_observe_only() {
        assert!(!HookEventKind::PostToolUse.is_decision_capable());
        assert!(!HookEventKind::PostLlmCall.is_decision_capable());
        assert!(!HookEventKind::PostPermission.is_decision_capable());
        assert!(!HookEventKind::PostContextInject.is_decision_capable());
    }

    #[test]
    fn task_lifecycle_events_observe_only() {
        assert!(!HookEventKind::TaskStart.is_decision_capable());
        assert!(!HookEventKind::TaskEnd.is_decision_capable());
        assert!(!HookEventKind::Checkpoint.is_decision_capable());
    }

    #[test]
    fn memory_recall_observe_only_but_write_decision_capable() {
        assert!(!HookEventKind::MemoryRecall.is_decision_capable());
        assert!(HookEventKind::MemoryWrite.is_decision_capable());
    }

    #[test]
    fn event_kind_matches_variant() {
        let e = HookEvent::PreToolUse {
            task_id: "t1".into(),
            tool_name: "shell".into(),
            args_json: "{}".into(),
        };
        assert_eq!(e.kind(), HookEventKind::PreToolUse);
        assert_eq!(e.task_id(), "t1");
    }

    #[test]
    fn task_id_accessor_covers_all_variants() {
        for k in HookEventKind::ALL {
            let e = make_default_event(k, "task-x");
            assert_eq!(e.task_id(), "task-x", "task_id missing for {k:?}");
        }
    }

    #[test]
    fn serde_tag_snake_case() {
        let e = HookEvent::PreLlmCall {
            task_id: "t".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet".into(),
            prompt_tokens_estimate: 5000,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "pre_llm_call");
        assert_eq!(v["provider"], "anthropic");
    }

    #[test]
    fn serde_roundtrip_each_variant() {
        for k in HookEventKind::ALL {
            let e = make_default_event(k, "rt");
            let json = serde_json::to_string(&e).unwrap();
            let back: HookEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(e, back);
        }
    }

    fn make_default_event(kind: HookEventKind, task_id: &str) -> HookEvent {
        match kind {
            HookEventKind::PreToolUse => HookEvent::PreToolUse {
                task_id: task_id.into(),
                tool_name: "x".into(),
                args_json: "{}".into(),
            },
            HookEventKind::PostToolUse => HookEvent::PostToolUse {
                task_id: task_id.into(),
                tool_name: "x".into(),
                success: true,
                result_preview: String::new(),
            },
            HookEventKind::PreLlmCall => HookEvent::PreLlmCall {
                task_id: task_id.into(),
                provider: "p".into(),
                model: "m".into(),
                prompt_tokens_estimate: 0,
            },
            HookEventKind::PostLlmCall => HookEvent::PostLlmCall {
                task_id: task_id.into(),
                provider: "p".into(),
                model: "m".into(),
                input_tokens: 0,
                output_tokens: 0,
            },
            HookEventKind::PrePermission => HookEvent::PrePermission {
                task_id: task_id.into(),
                action: "a".into(),
                target: "t".into(),
            },
            HookEventKind::PostPermission => HookEvent::PostPermission {
                task_id: task_id.into(),
                action: "a".into(),
                granted: true,
            },
            HookEventKind::PreContextInject => HookEvent::PreContextInject {
                task_id: task_id.into(),
                fragment_ids: vec![],
                total_tokens_estimate: 0,
            },
            HookEventKind::PostContextInject => HookEvent::PostContextInject {
                task_id: task_id.into(),
                fragments_injected: 0,
                tokens_used: 0,
            },
            HookEventKind::TaskStart => HookEvent::TaskStart {
                task_id: task_id.into(),
                intent_id: "i".into(),
            },
            HookEventKind::TaskEnd => HookEvent::TaskEnd {
                task_id: task_id.into(),
                outcome: "completed".into(),
            },
            HookEventKind::MemoryWrite => HookEvent::MemoryWrite {
                task_id: task_id.into(),
                topic: "t".into(),
                size_bytes: 0,
            },
            HookEventKind::MemoryRecall => HookEvent::MemoryRecall {
                task_id: task_id.into(),
                query: "q".into(),
                hit_count: 0,
            },
            HookEventKind::Checkpoint => HookEvent::Checkpoint {
                task_id: task_id.into(),
                checkpoint_id: "c".into(),
                checkpoint_kind: "auto".into(),
            },
        }
    }
}
