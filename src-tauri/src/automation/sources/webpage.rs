use super::{SubscriptionSource, TriggerCallback};
use async_trait::async_trait;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::automation::protocol::humane_v1::Subscription;

/// Hard-coded poll interval for Phase 1 (configurable in Phase 2).
const POLL_INTERVAL_SECS: u64 = 300;

pub struct WebpageSource {
    tasks: Arc<Mutex<HashMap<(String, String), JoinHandle<()>>>>,
    /// Last-seen SHA-256 hash per (spec_id, sub_id).
    seen: Arc<Mutex<HashMap<(String, String), String>>>,
}

impl WebpageSource {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            seen: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SubscriptionSource for WebpageSource {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        on_fire: TriggerCallback,
    ) -> anyhow::Result<()> {
        let Subscription::Webpage(wp) = sub else {
            anyhow::bail!("not a webpage subscription");
        };

        let url = wp.url.clone();
        let selector_str = wp.selector.clone();
        let spec_id_s = spec_id.to_string();
        let sub_id_s = sub_id.to_string();
        let seen = self.seen.clone();
        let key = (spec_id_s.clone(), sub_id_s.clone());

        let handle = tokio::spawn(async move {
            let client = match reqwest::Client::builder().build() {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "webpage client build failed");
                    return;
                }
            };
            let selector = match Selector::parse(&selector_str) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = ?e, selector = %selector_str, "invalid css selector");
                    return;
                }
            };

            loop {
                tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

                let body = match client.get(&url).send().await {
                    Ok(resp) => match resp.text().await {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(error = %e, url = %url, "webpage body read failed");
                            continue;
                        }
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, url = %url, "webpage fetch failed");
                        continue;
                    }
                };

                let text = extract_text(&body, &selector);
                let hash = hex::encode(Sha256::digest(text.as_bytes()));

                let mut seen_map = seen.lock().await;
                let prev = seen_map.get(&key).cloned();

                if prev.as_deref() != Some(&hash) {
                    let prev_hash = prev.unwrap_or_default();
                    seen_map.insert(key.clone(), hash.clone());
                    drop(seen_map);

                    // Skip first fetch — that establishes baseline, not a change.
                    if !prev_hash.is_empty() {
                        on_fire(
                            spec_id_s.clone(),
                            sub_id_s.clone(),
                            serde_json::json!({
                                "url": url,
                                "selected_text": text,
                                "prev_hash": prev_hash,
                                "new_hash": hash,
                            }),
                        );
                    }
                }
            }
        });

        let tasks_key = (spec_id.to_string(), sub_id.to_string());
        self.tasks.lock().await.insert(tasks_key, handle);
        Ok(())
    }

    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()> {
        let key = (spec_id.to_string(), sub_id.to_string());
        if let Some(h) = self.tasks.lock().await.remove(&key) {
            h.abort();
        }
        self.seen.lock().await.remove(&key);
        Ok(())
    }
}

/// Extract and concatenate text from all elements matching `selector` in `html`.
pub fn extract_text(html: &str, selector: &Selector) -> String {
    Html::parse_document(html)
        .select(selector)
        .map(|el| el.text().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_picks_matching_element() {
        let html = r#"<html><body><h1>Title</h1><p class="x">Hello</p></body></html>"#;
        let sel = Selector::parse("p.x").unwrap();
        assert_eq!(extract_text(html, &sel), "Hello");
    }

    #[test]
    fn extract_text_concatenates_multiple_matches() {
        let html = r#"<ul><li>one</li><li>two</li></ul>"#;
        let sel = Selector::parse("li").unwrap();
        let out = extract_text(html, &sel);
        assert!(out.contains("one"));
        assert!(out.contains("two"));
    }

    #[test]
    fn extract_text_returns_empty_for_no_match() {
        let html = r#"<p>nothing here</p>"#;
        let sel = Selector::parse("span.missing").unwrap();
        assert_eq!(extract_text(html, &sel), "");
    }
}
