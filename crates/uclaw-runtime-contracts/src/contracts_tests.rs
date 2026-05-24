use super::*;

// ── AutonomyLevel ───────────────────────────────────────────────

#[test]
fn autonomy_rungs_monotonic() {
    let levels = [
        AutonomyLevel::ChatAssist,
        AutonomyLevel::AssistedAction,
        AutonomyLevel::SupervisedTask,
        AutonomyLevel::DelegatedTask,
        AutonomyLevel::ScheduledWorker,
        AutonomyLevel::AgentTeam,
        AutonomyLevel::DistributedCluster,
    ];
    for w in levels.windows(2) {
        assert!(w[0].rung() < w[1].rung(), "{:?} < {:?}", w[0], w[1]);
    }
}

#[test]
fn risk_caps_high_at_supervised() {
    assert_eq!(
        AutonomyLevel::DistributedCluster.cap_for_risk(RiskClass::High),
        AutonomyLevel::SupervisedTask
    );
    assert_eq!(
        AutonomyLevel::DelegatedTask.cap_for_risk(RiskClass::High),
        AutonomyLevel::SupervisedTask
    );
    // High doesn't elevate below-the-cap levels.
    assert_eq!(
        AutonomyLevel::ChatAssist.cap_for_risk(RiskClass::High),
        AutonomyLevel::ChatAssist
    );
}

#[test]
fn risk_caps_restricted_at_assisted() {
    assert_eq!(
        AutonomyLevel::DelegatedTask.cap_for_risk(RiskClass::Restricted),
        AutonomyLevel::AssistedAction
    );
    // Restricted caps everything above L1 down to L1.
    assert_eq!(
        AutonomyLevel::SupervisedTask.cap_for_risk(RiskClass::Restricted),
        AutonomyLevel::AssistedAction
    );
}

#[test]
fn risk_low_and_medium_pass_through() {
    for level in [
        AutonomyLevel::DelegatedTask,
        AutonomyLevel::AgentTeam,
        AutonomyLevel::DistributedCluster,
    ] {
        assert_eq!(level.cap_for_risk(RiskClass::Low), level);
        assert_eq!(level.cap_for_risk(RiskClass::Medium), level);
    }
}

// ── Serde round-trips ──────────────────────────────────────────

#[test]
fn intent_spec_roundtrip() {
    let intent = IntentSpec {
        id: "01HJZ8QK9X".into(),
        origin: IntentOrigin::Chat,
        user_goal: "summarize today's emails".into(),
        acceptance_criteria: vec!["≤ 200 words".into()],
        constraints: vec![Constraint {
            key: "deadline".into(),
            value: "2026-05-20T18:00:00Z".into(),
        }],
        autonomy_target: AutonomyLevel::DelegatedTask,
        risk_class: RiskClass::Low,
        context_refs: vec![ContextRef {
            source: "mail".into(),
            id: "thread/42".into(),
            label: None,
        }],
        requested_capabilities: vec![CapabilityQuery {
            kind: "tool".into(),
            name: Some("read_mail".into()),
            tags: Default::default(),
        }],
    };
    let json = serde_json::to_string(&intent).expect("serialize");
    let parsed: IntentSpec = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed, intent);
    // Spot-check the camelCase wire shape.
    assert!(json.contains("\"userGoal\""));
    assert!(json.contains("\"autonomyTarget\""));
    assert!(json.contains("\"riskClass\":\"low\""));
}

#[test]
fn task_spec_roundtrip_with_skipped_none_plan_ref() {
    let task = TaskSpec {
        id: "task-001".into(),
        intent_id: "intent-001".into(),
        goal: "Step 2: fetch the data".into(),
        plan_ref: None,
        policy: PolicySpec {
            effective_autonomy: AutonomyLevel::SupervisedTask,
            require_step_approval: false,
            tool_permission_rule_ids: vec![],
        },
        budget: BudgetSpec {
            max_input_tokens: Some(40_000),
            max_output_tokens: Some(2_000),
            max_wallclock_seconds: Some(120),
            max_cost_usd_micros: None,
        },
        capability_profile: "default".into(),
        output_contract: OutputContract::Markdown,
        checkpoint_policy: CheckpointPolicy::PerTurn,
    };
    let json = serde_json::to_string(&task).expect("serialize");
    let parsed: TaskSpec = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed, task);
    // None plan_ref should not appear in the wire form.
    assert!(!json.contains("planRef"));
}

#[test]
fn task_event_kind_strings() {
    let started = TaskEvent::TaskStarted {
        ts: "2026-05-20T12:00:00Z".into(),
        source: TaskEventSource::AgentLoop,
        task_id: "task-001".into(),
        intent_id: "intent-001".into(),
    };
    assert_eq!(started.kind(), "task_started");
    assert_eq!(started.source(), TaskEventSource::AgentLoop);
    assert_eq!(started.task_id(), "task-001");
    assert_eq!(started.ts(), "2026-05-20T12:00:00Z");
}

#[test]
fn task_event_finished_with_completed_verdict_roundtrips() {
    let e = TaskEvent::TaskFinished {
        ts: "2026-05-20T12:30:00Z".into(),
        source: TaskEventSource::AgentLoop,
        task_id: "task-001".into(),
        verdict: TaskVerdict::Completed {
            summary: Some("done".into()),
        },
    };
    let json = serde_json::to_string(&e).expect("serialize");
    let parsed: TaskEvent = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed, e);
    assert!(json.contains("\"kind\":\"task_finished\""));
    assert!(json.contains("\"outcome\":\"completed\""));
}

#[test]
fn task_event_budget_exhausted_carries_dimension() {
    let e = TaskEvent::TaskFinished {
        ts: "2026-05-20T12:30:00Z".into(),
        source: TaskEventSource::AgentLoop,
        task_id: "task-001".into(),
        verdict: TaskVerdict::BudgetExhausted {
            dimension: "max_output_tokens".into(),
        },
    };
    let json = serde_json::to_string(&e).expect("serialize");
    let parsed: TaskEvent = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed, e);
    assert!(json.contains("budget_exhausted"));
}

#[test]
fn task_event_kind_covers_all_fourteen_variants() {
    let ts = "2026-05-20T12:00:00Z".to_string();
    let src = TaskEventSource::AgentLoop;
    let tid = "task-001".to_string();
    let variants: Vec<TaskEvent> = vec![
        TaskEvent::TaskStarted {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            intent_id: "i".into(),
        },
        TaskEvent::ModelTurn {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            token_usage: TokenUsage::default(),
        },
        TaskEvent::ToolCall {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            tool_name: "read".into(),
            input_ref: "in/1".into(),
        },
        TaskEvent::ToolResult {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            tool_name: "read".into(),
            output_ref: "out/1".into(),
            ok: true,
        },
        TaskEvent::PermissionRequested {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            request_id: "r/1".into(),
            reason: "shell".into(),
        },
        TaskEvent::PermissionDecided {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            request_id: "r/1".into(),
            decision: PermissionDecision::Granted,
        },
        TaskEvent::ContextAccess {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            op: ContextOp::Read,
            context_ref: ContextRef {
                source: "mail".into(),
                id: "thread/42".into(),
                label: None,
            },
        },
        TaskEvent::MemoryWrite {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            target: "gbrain".into(),
            artifact_ref: "art/1".into(),
        },
        TaskEvent::MemoryRecall {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            target: "gbrain".into(),
            artifact_ref: "art/1".into(),
        },
        TaskEvent::Checkpoint {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            checkpoint_ref: "ck/1".into(),
        },
        TaskEvent::BoundaryYield {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            reason: "needs approval".into(),
        },
        TaskEvent::Warning {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            code: "cost_warning".into(),
            message: "approaching cap".into(),
        },
        TaskEvent::Signal {
            ts: ts.clone(),
            source: src,
            task_id: tid.clone(),
            code: "browser.provider.selected".into(),
            message: "{\"providerId\":\"browser.local_chromium\"}".into(),
        },
        TaskEvent::TaskFinished {
            ts,
            source: src,
            task_id: tid,
            verdict: TaskVerdict::Completed { summary: None },
        },
    ];
    assert_eq!(variants.len(), 14);
    let kinds: Vec<&'static str> = variants.iter().map(|v| v.kind()).collect();
    // All distinct + snake-case shape.
    let mut sorted = kinds.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        14,
        "all 14 variants must have distinct kind() strings"
    );
    for k in &kinds {
        assert!(
            k.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "kind() must be snake_case: {k}"
        );
    }
}

#[test]
fn token_usage_default_serializes_zeros() {
    let t = TokenUsage::default();
    let json = serde_json::to_string(&t).unwrap();
    // costUsdMicros is Option<u64>, None → skipped.
    assert!(!json.contains("costUsdMicros"));
    assert!(json.contains("\"inputTokens\":0"));
    assert!(json.contains("\"reasoningOutputTokens\":0"));
}

// ── HookDecision ────────────────────────────────────────────────

#[test]
fn hook_decision_classifiers() {
    assert!(HookDecision::Allow.is_allow());
    assert!(!HookDecision::Allow.is_deny());
    assert!(!HookDecision::Allow.requires_user());

    let deny = HookDecision::Deny {
        reason: "blocked by policy".into(),
    };
    assert!(deny.is_deny());
    assert!(!deny.is_allow());

    let ask = HookDecision::AskUser {
        prompt: "execute rm -rf /?".into(),
        risk_class: Some(RiskClass::Restricted),
    };
    assert!(ask.requires_user());
    assert!(!ask.is_allow());
    assert!(!ask.is_deny());
}

#[test]
fn hook_decision_serde_uses_decision_tag_snake_case() {
    let v = serde_json::to_value(HookDecision::Allow).unwrap();
    assert_eq!(v["decision"], "allow");

    let v = serde_json::to_value(HookDecision::Deny { reason: "x".into() }).unwrap();
    assert_eq!(v["decision"], "deny");
    assert_eq!(v["reason"], "x");

    let v = serde_json::to_value(HookDecision::AskUser {
        prompt: "p".into(),
        risk_class: None,
    })
    .unwrap();
    assert_eq!(v["decision"], "ask_user");
    assert_eq!(v["prompt"], "p");
}

#[test]
fn hook_decision_roundtrips_with_risk_class() {
    let d = HookDecision::AskUser {
        prompt: "?".into(),
        risk_class: Some(RiskClass::High),
    };
    let json = serde_json::to_string(&d).unwrap();
    let back: HookDecision = serde_json::from_str(&json).unwrap();
    assert_eq!(d, back);
}

// ── BoundaryRef ─────────────────────────────────────────────────

#[test]
fn boundary_ref_factories() {
    let b = BoundaryRef::new("budget", "input_tokens");
    assert!(b.is_budget());
    assert!(!b.is_policy());
    assert!(b.note.is_none());

    let b = BoundaryRef::with_note("policy", "no_external_net", "blocked by org policy");
    assert!(b.is_policy());
    assert_eq!(b.note.as_deref(), Some("blocked by org policy"));
}

#[test]
fn boundary_ref_serde_camel_case_note_skipped_when_none() {
    let b = BoundaryRef::new("role", "research-only");
    let json = serde_json::to_string(&b).unwrap();
    // camelCase = same as lowercase for single-word fields; verify no "note" key.
    assert!(!json.contains("note"));
    assert!(json.contains("\"kind\":\"role\""));
}

#[test]
fn boundary_ref_roundtrip() {
    let b = BoundaryRef::with_note("world", "edge.git", "git remote unreachable");
    let json = serde_json::to_string(&b).unwrap();
    let back: BoundaryRef = serde_json::from_str(&json).unwrap();
    assert_eq!(b, back);
}

// ── WorkerId ────────────────────────────────────────────────────

#[test]
fn worker_id_constructors_and_display() {
    let a = WorkerId::new("w1");
    assert_eq!(a.as_str(), "w1");
    assert_eq!(a.to_string(), "w1");

    let b: WorkerId = "w2".into();
    let c: WorkerId = String::from("w3").into();
    assert_eq!(b, WorkerId::new("w2"));
    assert_eq!(c, WorkerId::new("w3"));
}

#[test]
fn worker_id_serializes_transparently() {
    // #[serde(transparent)] means it serializes as a bare string,
    // not {"0": "w1"}.
    let w = WorkerId::new("w1");
    let json = serde_json::to_string(&w).unwrap();
    assert_eq!(json, "\"w1\"");
    let back: WorkerId = serde_json::from_str(&json).unwrap();
    assert_eq!(w, back);
}

#[test]
fn worker_id_hashable_in_hashmap() {
    use std::collections::HashMap;
    let mut map: HashMap<WorkerId, i32> = HashMap::new();
    map.insert(WorkerId::new("w1"), 1);
    map.insert(WorkerId::new("w2"), 2);
    assert_eq!(map.get(&WorkerId::new("w1")), Some(&1));
    assert_eq!(map.len(), 2);
}
