use super::{SubscriptionSource, TriggerCallback};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

use crate::automation::protocol::humane_v1::Subscription;

/// Shared registry mapping (spec_id, sub_id) → WebhookEntry.
/// The axum route handler reads from this to dispatch incoming requests.
pub type WebhookRegistry = Arc<RwLock<HashMap<(String, String), WebhookEntry>>>;

/// Process-global registry accessed by the axum handler.
/// Initialised once by `WebhookSource::new()` (or `global_registry()`).
static GLOBAL_WEBHOOK_REGISTRY: OnceLock<WebhookRegistry> = OnceLock::new();

/// Return the global webhook registry, creating it on first call.
pub fn global_registry() -> WebhookRegistry {
    GLOBAL_WEBHOOK_REGISTRY
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

pub struct WebhookEntry {
    pub path: String,
    pub secret: Option<String>,
    pub callback: TriggerCallback,
}

pub struct WebhookSource {
    registry: WebhookRegistry,
}

impl WebhookSource {
    pub fn new(registry: WebhookRegistry) -> Self {
        Self { registry }
    }

    /// Create a `WebhookSource` backed by the process-global registry.
    /// The axum handler at `POST /automation/webhook/:spec_id/:sub_id/*tail`
    /// resolves entries through the same global registry.
    pub fn with_global_registry() -> Self {
        Self::new(global_registry())
    }
}

#[async_trait]
impl SubscriptionSource for WebhookSource {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        on_fire: TriggerCallback,
    ) -> anyhow::Result<()> {
        let Subscription::Webhook(wh) = sub else {
            anyhow::bail!("not a webhook subscription");
        };

        self.registry.write().await.insert(
            (spec_id.into(), sub_id.into()),
            WebhookEntry {
                path: wh.path.clone(),
                secret: wh.secret.clone(),
                callback: on_fire,
            },
        );
        Ok(())
    }

    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()> {
        self.registry
            .write()
            .await
            .remove(&(spec_id.into(), sub_id.into()));
        Ok(())
    }
}

/// Verify an HMAC-SHA256 signature.
///
/// `signature_header` must be in the form `"sha256=<hex-digest>"`.
pub fn verify_signature(secret: &str, body: &[u8], signature_header: &str) -> bool {
    let Some(hex_digest) = signature_header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(got) = hex::decode(hex_digest) else {
        return false;
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    mac.verify_slice(&got).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::WebhookSubscription;

    #[test]
    fn verify_signature_accepts_correct_hmac() {
        let secret = "topsecret";
        let body = b"{\"x\":1}";
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let hex_sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("sha256={}", hex_sig);
        assert!(verify_signature(secret, body, &header));
    }

    #[test]
    fn verify_signature_rejects_bad_hmac() {
        assert!(!verify_signature("s", b"body", "sha256=deadbeef"));
    }

    #[test]
    fn verify_signature_rejects_malformed_header() {
        assert!(!verify_signature("s", b"body", "wrong-format"));
    }

    #[tokio::test]
    async fn attach_then_detach_registry_roundtrip() {
        let reg: WebhookRegistry = Arc::new(RwLock::new(HashMap::new()));
        let src = WebhookSource::new(reg.clone());
        let cb: TriggerCallback = Arc::new(|_, _, _| {});

        let sub = Subscription::Webhook(WebhookSubscription {
            path: "test".into(),
            secret: None,
        });

        src.attach("spec", "sub", &sub, cb).await.unwrap();
        assert!(
            reg.read()
                .await
                .contains_key(&("spec".into(), "sub".into()))
        );

        src.detach("spec", "sub").await.unwrap();
        assert!(
            !reg.read()
                .await
                .contains_key(&("spec".into(), "sub".into()))
        );
    }
}
