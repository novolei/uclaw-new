use super::{SubscriptionSource, TriggerCallback};
use async_trait::async_trait;
use globset::Glob;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::automation::protocol::humane_v1::Subscription;

struct FileSubEntry {
    /// Held to keep the watcher alive; dropped in detach.
    _watcher: RecommendedWatcher,
}

pub struct FileSource {
    subs: Arc<Mutex<HashMap<(String, String), FileSubEntry>>>,
}

impl FileSource {
    pub fn new() -> Self {
        Self {
            subs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for FileSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SubscriptionSource for FileSource {
    async fn attach(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        on_fire: TriggerCallback,
    ) -> anyhow::Result<()> {
        let Subscription::File(fs) = sub else {
            anyhow::bail!("not a file subscription");
        };

        let glob = Glob::new(&fs.pattern)?.compile_matcher();
        let root = root_from_glob(&fs.pattern);

        let spec_id_s = spec_id.to_string();
        let sub_id_s = sub_id.to_string();

        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    if matches!(
                        ev.kind,
                        EventKind::Create(_) | EventKind::Modify(_)
                    ) {
                        for path in &ev.paths {
                            if glob.is_match(path) {
                                let payload = serde_json::json!({
                                    "path": path.to_string_lossy(),
                                    "event": format!("{:?}", ev.kind),
                                });
                                on_fire(spec_id_s.clone(), sub_id_s.clone(), payload);
                            }
                        }
                    }
                }
            })?;

        watcher.watch(&root, RecursiveMode::Recursive)?;

        self.subs.lock().await.insert(
            (spec_id.into(), sub_id.into()),
            FileSubEntry { _watcher: watcher },
        );
        Ok(())
    }

    async fn detach(&self, spec_id: &str, sub_id: &str) -> anyhow::Result<()> {
        self.subs
            .lock()
            .await
            .remove(&(spec_id.into(), sub_id.into()));
        Ok(())
    }
}

/// Derive the longest non-glob prefix directory from a glob pattern.
/// "/tmp/foo/*.txt" → "/tmp/foo"
/// "/tmp/bar/**/*.rs" → "/tmp/bar"
fn root_from_glob(pattern: &str) -> PathBuf {
    let first_meta = pattern.find(|c: char| matches!(c, '*' | '?' | '['));
    let prefix = match first_meta {
        Some(i) => &pattern[..i],
        None => pattern,
    };
    let last_slash = prefix.rfind('/').map(|i| &prefix[..i]).unwrap_or(".");
    PathBuf::from(if last_slash.is_empty() { "/" } else { last_slash })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::FileSubscription;

    #[test]
    fn root_from_glob_extracts_base_dir() {
        assert_eq!(root_from_glob("/tmp/foo/*.txt"), PathBuf::from("/tmp/foo"));
        assert_eq!(
            root_from_glob("/tmp/bar/**/*.rs"),
            PathBuf::from("/tmp/bar")
        );
    }

    #[tokio::test]
    async fn file_source_fires_on_pattern_match() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tmp = tempfile::TempDir::new().unwrap();
        // Canonicalize to resolve /tmp → /private/tmp on macOS so that the
        // glob pattern matches the real paths that FSEvents delivers.
        let real_path = std::fs::canonicalize(tmp.path()).unwrap();
        let pattern = format!("{}/*.txt", real_path.to_string_lossy());

        let counter = Arc::new(AtomicUsize::new(0));
        let c2 = counter.clone();
        let cb: TriggerCallback = Arc::new(move |_, _, _| {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        let src = FileSource::new();
        let sub = Subscription::File(FileSubscription { pattern });
        src.attach("s", "u", &sub, cb).await.unwrap();

        // Let the watcher arm before writing
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        std::fs::write(real_path.join("hello.txt"), "hi").unwrap();

        // Wait for FSEvents to deliver (FSEvents on macOS can batch-coalesce)
        tokio::time::sleep(std::time::Duration::from_millis(3000)).await;

        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "expected file event, got {}",
            counter.load(Ordering::SeqCst)
        );

        src.detach("s", "u").await.unwrap();
    }
}
