use super::*;
use serde_json::json;

#[test]
fn queue_drains_in_fifo_order_and_counts_urgent_messages() {
    let queue = SoftInterruptQueue::default();
    queue.push(SoftInterruptMessage::user("first"));
    queue.push(SoftInterruptMessage::urgent_system("second"));

    let drained = queue.drain();

    assert_eq!(drained.messages.len(), 2);
    assert_eq!(drained.messages[0].content, "first");
    assert_eq!(drained.messages[1].content, "second");
    assert_eq!(drained.urgent_count, 1);
    assert_eq!("firstsecond".len(), drained.total_content_bytes);
    assert!(queue.is_empty());
}

#[test]
fn snapshot_preserves_pending_messages() {
    let queue = SoftInterruptQueue::default();
    assert!(queue.is_empty());

    let pending = queue.push(SoftInterruptMessage::system("watch"));

    assert_eq!(pending, 1);
    assert_eq!(queue.len(), 1);
    assert!(!queue.is_empty());
    assert!(!queue.has_urgent());

    let snapshot = queue.snapshot();

    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].source, SoftInterruptSource::System);
    assert_eq!(snapshot[0].content, "watch");
    assert_eq!(queue.len(), 1);
}

#[test]
fn clear_returns_removed_count() {
    let queue = SoftInterruptQueue::default();
    queue.push(SoftInterruptMessage::user("one"));
    queue.push(SoftInterruptMessage::system("two"));

    assert_eq!(queue.clear(), 2);
    assert_eq!(queue.clear(), 0);
    assert!(queue.is_empty());
}

#[test]
fn serde_uses_expected_wire_shape() {
    let message =
        SoftInterruptMessage::new(SoftInterruptSource::BackgroundTask, "refresh context", true);

    let value = serde_json::to_value(&message).expect("serialize soft interrupt message");

    assert_eq!(
        value,
        json!({
            "source": "background_task",
            "content": "refresh context",
            "urgent": true,
        })
    );
}
