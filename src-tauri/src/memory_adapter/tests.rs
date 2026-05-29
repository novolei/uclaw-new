//! Smoke tests for memory_adapter owned types. The trait has no
//! implementors in PR1; trait behavior is locked by later PRs.

use super::*;

#[test]
fn memory_category_display_round_trip() {
    assert_eq!(MemoryCategory::Core.to_string(), "core");
    assert_eq!(MemoryCategory::Daily.to_string(), "daily");
    assert_eq!(MemoryCategory::Conversation.to_string(), "conversation");
    assert_eq!(
        MemoryCategory::Custom("foo".to_string()).to_string(),
        "foo"
    );
}

#[test]
fn memory_category_serde_round_trip() {
    let core_json = serde_json::to_string(&MemoryCategory::Core).unwrap();
    assert_eq!(core_json, "\"core\"");
    let back: MemoryCategory = serde_json::from_str(&core_json).unwrap();
    assert_eq!(back, MemoryCategory::Core);

    let custom = MemoryCategory::Custom("topic_x".to_string());
    let json = serde_json::to_string(&custom).unwrap();
    let back: MemoryCategory = serde_json::from_str(&json).unwrap();
    assert_eq!(back, custom);
}

#[test]
fn memory_entry_serde_round_trip() {
    let entry = MemoryEntry {
        id: "abc".into(),
        key: "topic".into(),
        content: "Ryan likes coffee.".into(),
        namespace: Some("user_profile".into()),
        category: MemoryCategory::Core,
        timestamp: "2026-05-29T10:00:00Z".into(),
        session_id: Some("sess-42".into()),
        score: Some(0.87),
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: MemoryEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, entry.id);
    assert_eq!(back.namespace, entry.namespace);
    assert_eq!(back.score, entry.score);
}

#[test]
fn recall_opts_defaults_to_all_none() {
    let opts = RecallOpts::default();
    assert!(opts.namespace.is_none());
    assert!(opts.category.is_none());
    assert!(opts.session_id.is_none());
    assert!(opts.min_score.is_none());
}

#[test]
fn namespace_summary_serde_round_trip() {
    let s = NamespaceSummary {
        namespace: "user_profile".into(),
        count: 17,
        last_updated: Some("2026-05-29T09:30:00Z".into()),
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: NamespaceSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(back.count, 17);
    assert_eq!(back.last_updated.as_deref(), Some("2026-05-29T09:30:00Z"));
}
