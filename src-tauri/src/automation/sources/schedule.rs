use super::{SubscriptionSource, TriggerCallback};
use async_trait::async_trait;
use cron::Schedule;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

use crate::automation::protocol::humane_v1::Subscription;

pub struct ScheduleSource {
    tasks: Arc<Mutex<HashMap<(String, String), JoinHandle<()>>>>,
}

impl ScheduleSource {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Drop for ScheduleSource {
    fn drop(&mut self) {
        // Best-effort abort all running task handles to prevent leaks.
        // Callers should call detach() for each key before drop in normal
        // operation; this is a safety net for abnormal teardown.
        if let Ok(mut map) = self.tasks.try_lock() {
            for (_, handle) in map.drain() {
                handle.abort();
            }
        }
    }
}

impl Default for ScheduleSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SubscriptionSource for ScheduleSource {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        on_fire: TriggerCallback,
    ) -> anyhow::Result<()> {
        let Subscription::Schedule(s) = sub else {
            anyhow::bail!("not a schedule subscription");
        };

        let cron_expr = if let Some(c) = &s.cron {
            c.clone()
        } else if let Some(every) = &s.every {
            parse_every_to_cron(every)?
        } else {
            anyhow::bail!("schedule requires cron or every");
        };

        // Normalise 5-field cron to 6-field (prepend seconds "0 ")
        let normalized = if cron_expr.split_whitespace().count() == 5 {
            format!("0 {}", cron_expr)
        } else {
            cron_expr
        };

        let schedule = Schedule::from_str(&normalized)
            .map_err(|e| anyhow::anyhow!("invalid cron: {}", e))?;

        let spec_id = spec_id.to_string();
        let sub_id = sub_id.to_string();
        let key = (spec_id.clone(), sub_id.clone());

        let handle = tokio::spawn(async move {
            loop {
                let Some(next) = schedule.upcoming(chrono::Utc).next() else {
                    break;
                };
                let now = chrono::Utc::now();
                let dur = (next - now).to_std().unwrap_or(std::time::Duration::ZERO);
                tokio::time::sleep(dur).await;
                let payload = serde_json::json!({
                    "fired_at": chrono::Utc::now().to_rfc3339()
                });
                on_fire(spec_id.clone(), sub_id.clone(), payload);
            }
        });

        self.tasks.lock().unwrap().insert(key, handle);
        Ok(())
    }

    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()> {
        let key = (spec_id.to_string(), sub_id.to_string());
        if let Some(h) = self.tasks.lock().unwrap().remove(&key) {
            h.abort();
        }
        Ok(())
    }
}

/// Convert a human "every" string to a 6-field cron expression.
/// "10s" → "*/10 * * * * *"
/// "30m" → "0 */30 * * * *"
/// "1h"  → "0 0 */1 * * *"
fn parse_every_to_cron(every: &str) -> anyhow::Result<String> {
    if every.is_empty() {
        anyhow::bail!("empty every string");
    }
    let (num_str, unit) = every.split_at(every.len() - 1);
    let n: u32 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid number in every: {}", every))?;
    match unit {
        "s" => Ok(format!("*/{} * * * * *", n)),
        "m" => Ok(format!("0 */{} * * * *", n)),
        "h" => Ok(format!("0 0 */{} * * *", n)),
        _ => anyhow::bail!("unsupported every unit: {}", unit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::ScheduleSubscription;

    #[test]
    fn parse_every_handles_seconds() {
        assert_eq!(parse_every_to_cron("10s").unwrap(), "*/10 * * * * *");
    }

    #[test]
    fn parse_every_handles_minutes() {
        assert_eq!(parse_every_to_cron("30m").unwrap(), "0 */30 * * * *");
    }

    #[test]
    fn parse_every_handles_hours() {
        assert_eq!(parse_every_to_cron("1h").unwrap(), "0 0 */1 * * *");
    }

    #[test]
    fn parse_every_rejects_bad_unit() {
        assert!(parse_every_to_cron("5x").is_err());
    }

    #[tokio::test]
    async fn schedule_source_fires_at_least_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(0));
        let c2 = counter.clone();
        let cb: TriggerCallback = Arc::new(move |_, _, _| {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        let src = ScheduleSource::new();
        let sub = Subscription::Schedule(ScheduleSubscription {
            cron: None,
            every: Some("1s".into()),
        });

        src.attach("spec", "sub", &sub, cb).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        src.detach("spec", "sub").await.unwrap();

        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "expected at least 1 fire, got {}",
            counter.load(Ordering::SeqCst)
        );
    }
}
