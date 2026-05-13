use super::{SubscriptionSource, TriggerCallback};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::automation::protocol::humane_v1::Subscription;

/// Hard-coded poll interval for Phase 1 (configurable in Phase 2).
const POLL_INTERVAL_SECS: u64 = 300;

pub struct RssSource {
    tasks: Arc<Mutex<HashMap<(String, String), JoinHandle<()>>>>,
    /// Seen GUIDs per (spec_id, sub_id).
    seen: Arc<Mutex<HashMap<(String, String), HashSet<String>>>>,
}

impl RssSource {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            seen: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SubscriptionSource for RssSource {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        on_fire: TriggerCallback,
    ) -> anyhow::Result<()> {
        let Subscription::Rss(r) = sub else {
            anyhow::bail!("not an rss subscription");
        };

        let url = r.url.clone();
        let spec_id_s = spec_id.to_string();
        let sub_id_s = sub_id.to_string();
        let seen = self.seen.clone();
        let key = (spec_id_s.clone(), sub_id_s.clone());

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

                let body = match reqwest::get(&url).await {
                    Ok(resp) => match resp.bytes().await {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::warn!(error = %e, url = %url, "rss body read failed");
                            continue;
                        }
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, url = %url, "rss fetch failed");
                        continue;
                    }
                };

                let new_items = {
                    let mut seen_guard = seen.lock().await;
                    match parse_new_items(&body, &mut *seen_guard, &key) {
                        Ok(items) => items,
                        Err(e) => {
                            tracing::warn!(error = %e, url = %url, "rss parse failed");
                            continue;
                        }
                    }
                };

                for item in new_items {
                    on_fire(spec_id_s.clone(), sub_id_s.clone(), item);
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

/// Parse RSS bytes and return new items whose GUIDs are not yet in `seen`.
///
/// On first fetch (key not yet in `seen`), silently records all GUIDs as
/// baseline and returns an empty vec — no backfill.
pub fn parse_new_items(
    body: &[u8],
    seen: &mut HashMap<(String, String), HashSet<String>>,
    key: &(String, String),
) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let channel = rss::Channel::read_from(body)?;
    let first_fetch = !seen.contains_key(key);
    let seen_set = seen.entry(key.clone()).or_insert_with(HashSet::new);

    let mut out = vec![];
    for item in channel.items() {
        // Derive a stable identifier: prefer explicit guid, fall back to link, then title.
        let guid = item
            .guid()
            .map(|g| g.value().to_string())
            .or_else(|| item.link().map(|s| s.to_string()))
            .unwrap_or_else(|| item.title().unwrap_or("").to_string());

        if !seen_set.contains(&guid) {
            seen_set.insert(guid.clone());
            if !first_fetch {
                out.push(serde_json::json!({
                    "guid": guid,
                    "title": item.title(),
                    "link": item.link(),
                    "content": item.content(),
                    "pub_date": item.pub_date(),
                }));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FEED: &[u8] = br#"<?xml version="1.0"?><rss version="2.0"><channel>
        <title>Test</title><link>http://x</link><description>d</description>
        <item><title>A</title><guid>g1</guid><link>http://a</link></item>
        <item><title>B</title><guid>g2</guid><link>http://b</link></item>
    </channel></rss>"#;

    #[test]
    fn parse_new_items_backfills_silently_on_first_fetch() {
        let mut seen = HashMap::new();
        let key = ("s".to_string(), "u".to_string());
        let items = parse_new_items(SAMPLE_FEED, &mut seen, &key).unwrap();
        assert_eq!(items.len(), 0, "first fetch should not emit items");
        assert_eq!(seen[&key].len(), 2, "but should record both as seen");
    }

    #[test]
    fn parse_new_items_emits_only_new_after_first_fetch() {
        let mut seen = HashMap::new();
        let key = ("s".to_string(), "u".to_string());
        // Prime the seen set with the initial feed.
        parse_new_items(SAMPLE_FEED, &mut seen, &key).unwrap();

        let updated: &[u8] = br#"<?xml version="1.0"?><rss version="2.0"><channel>
            <title>Test</title><link>http://x</link><description>d</description>
            <item><title>A</title><guid>g1</guid><link>http://a</link></item>
            <item><title>B</title><guid>g2</guid><link>http://b</link></item>
            <item><title>C</title><guid>g3</guid><link>http://c</link></item>
        </channel></rss>"#;
        let items = parse_new_items(updated, &mut seen, &key).unwrap();
        assert_eq!(items.len(), 1, "only new item should fire");
        assert_eq!(items[0]["guid"], "g3");
    }
}
