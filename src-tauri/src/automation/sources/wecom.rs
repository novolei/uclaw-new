use super::{SubscriptionSource, TriggerCallback};
use super::webhook::{global_registry, WebhookEntry};
use async_trait::async_trait;
use std::sync::Arc;

use crate::automation::protocol::humane_v1::Subscription;

pub struct WecomSource;

impl WecomSource {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SubscriptionSource for WecomSource {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        on_fire: TriggerCallback,
    ) -> anyhow::Result<()> {
        let Subscription::Wecom(wc) = sub else {
            anyhow::bail!("not a wecom subscription");
        };

        let chat_id_filter = wc.chat_id.clone();
        let user_cb = on_fire;

        // Wrap the callback to enforce optional chat_id matching before
        // forwarding to the user-provided handler.
        let wrapped: TriggerCallback = Arc::new(move |sid, sub_id, payload| {
            if let Some(want) = &chat_id_filter {
                let got = payload
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if got != want {
                    return;
                }
            }
            user_cb(sid, sub_id, payload);
        });

        // Register under a synthesised path so the existing axum wildcard
        // route `/api/automation/webhook/*tail` handles delivery without a
        // new route.  No secret verification for Phase 1 (deferred to § 9).
        let path = format!("wecom/{}/{}", spec_id, sub_id);
        global_registry().write().await.insert(
            (spec_id.to_string(), sub_id.to_string()),
            WebhookEntry {
                path,
                secret: None,
                callback: wrapped,
            },
        );
        Ok(())
    }

    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()> {
        global_registry()
            .write()
            .await
            .remove(&(spec_id.to_string(), sub_id.to_string()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn wecom_filters_by_chat_id() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c2 = counter.clone();
        let cb: TriggerCallback = Arc::new(move |_, _, _| {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        let src = WecomSource::new();
        let sub = Subscription::Wecom(
            crate::automation::protocol::humane_v1::WecomSubscription {
                chat_id: Some("expected".into()),
            },
        );

        // Use a unique key to avoid collisions with other tests sharing the
        // global registry.
        src.attach("test_spec_wecom_filter", "u", &sub, cb)
            .await
            .unwrap();

        // Retrieve the wrapped callback from the registry and exercise it.
        let entry_cb = {
            let registry = global_registry();
            let reg = registry.read().await;
            let entry = reg
                .get(&(
                    "test_spec_wecom_filter".to_string(),
                    "u".to_string(),
                ))
                .unwrap();
            entry.callback.clone()
        };

        entry_cb(
            "test_spec_wecom_filter".into(),
            "u".into(),
            serde_json::json!({"chat_id": "wrong"}),
        );
        entry_cb(
            "test_spec_wecom_filter".into(),
            "u".into(),
            serde_json::json!({"chat_id": "expected"}),
        );
        entry_cb(
            "test_spec_wecom_filter".into(),
            "u".into(),
            serde_json::json!({}), // missing chat_id
        );

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "only the matching chat_id should fire the callback"
        );

        src.detach("test_spec_wecom_filter", "u").await.unwrap();
    }
}
