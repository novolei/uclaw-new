use std::path::PathBuf;
use tokio::fs;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

pub struct MemoryStore {
    root: PathBuf,
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl MemoryStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root, locks: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn path(&self, spec_id: &str) -> PathBuf {
        self.root.join(spec_id).join("memory.md")
    }

    async fn lock_for(&self, spec_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.entry(spec_id.to_string()).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
    }

    pub async fn read(&self, spec_id: &str) -> std::io::Result<String> {
        let lock = self.lock_for(spec_id).await;
        let _g = lock.lock().await;
        match fs::read_to_string(self.path(spec_id)).await {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }

    pub async fn write(&self, spec_id: &str, content: &str) -> std::io::Result<()> {
        let lock = self.lock_for(spec_id).await;
        let _g = lock.lock().await;
        let p = self.path(spec_id);
        if let Some(parent) = p.parent() { fs::create_dir_all(parent).await?; }
        fs::write(&p, content).await
    }

    pub async fn append(&self, spec_id: &str, content: &str) -> std::io::Result<()> {
        let existing = self.read(spec_id).await?;
        self.write(spec_id, &(existing + content)).await
    }

    pub async fn compact(&self, spec_id: &str) -> std::io::Result<PathBuf> {
        let lock = self.lock_for(spec_id).await;
        let _g = lock.lock().await;
        let main = self.path(spec_id);
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let archive = main.parent().unwrap().join("archives").join(format!("{}.md", timestamp));
        if let Some(parent) = archive.parent() { fs::create_dir_all(parent).await?; }
        if main.exists() {
            fs::rename(&main, &archive).await?;
            fs::write(&main, "").await?;
        }
        Ok(archive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn full_cycle() {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::new(tmp.path().to_path_buf());

        // read empty
        assert_eq!(store.read("s1").await.unwrap(), "");
        // write
        store.write("s1", "hello").await.unwrap();
        assert_eq!(store.read("s1").await.unwrap(), "hello");
        // append
        store.append("s1", "\nworld").await.unwrap();
        assert_eq!(store.read("s1").await.unwrap(), "hello\nworld");
        // compact
        let archive = store.compact("s1").await.unwrap();
        assert!(archive.exists());
        assert_eq!(store.read("s1").await.unwrap(), "");
    }
}
