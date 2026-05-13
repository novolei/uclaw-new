use super::{SubscriptionSource, TriggerCallback};
use async_trait::async_trait;

use crate::automation::protocol::humane_v1::Subscription;

/// Phase 1 stub. Logs a warning on attach; Phase 2 will introduce a
/// plug-in registry that dispatches to the named provider.
pub struct CustomSource;

impl CustomSource {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SubscriptionSource for CustomSource {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        _on_fire: TriggerCallback,
    ) -> anyhow::Result<()> {
        let Subscription::Custom(c) = sub else {
            anyhow::bail!("not a custom subscription");
        };
        tracing::warn!(
            spec_id = %spec_id,
            sub_id = %sub_id,
            provider = %c.provider,
            key = %c.key,
            "Custom subscription source registered but inert in Phase 1; \
             plug-in registry arrives in Phase 2"
        );
        Ok(())
    }

    async fn detach(&self, _spec_id: &str, _sub_id: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn custom_attach_is_inert() {
        let src = CustomSource::new();
        let sub = Subscription::Custom(
            crate::automation::protocol::humane_v1::CustomSubscription {
                provider: "stub".into(),
                key: "k".into(),
                config: serde_json::json!({}),
            },
        );
        let cb: TriggerCallback =
            Arc::new(|_, _, _| panic!("inert custom must not invoke callback"));
        assert!(src.attach("s", "u", &sub, cb).await.is_ok());
        assert!(src.detach("s", "u").await.is_ok());
    }
}
