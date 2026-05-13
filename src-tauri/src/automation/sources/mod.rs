use async_trait::async_trait;
use crate::automation::protocol::humane_v1::Subscription;

/// Cloneable callback invoked when a subscription fires.
/// Args: (spec_id, sub_id, payload)
pub type TriggerCallback = std::sync::Arc<dyn Fn(String, String, serde_json::Value) + Send + Sync>;

/// A source that can attach/detach subscription listeners.
#[async_trait]
pub trait SubscriptionSource: Send + Sync {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        on_fire: TriggerCallback,
    ) -> anyhow::Result<()>;

    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()>;
}

pub mod file;
pub mod schedule;
pub mod webhook;

pub use file::FileSource;
pub use schedule::ScheduleSource;
pub use webhook::{global_registry, WebhookRegistry, WebhookSource};
